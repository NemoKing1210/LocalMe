//! Typed errors for every layer.
//!
//! Production code in this crate never panics: fallible operations return one of these
//! types, and the `clippy::unwrap_used`/`clippy::expect_used` denials in `lib.rs` keep it
//! that way. [`CoreError`] is the umbrella the Tauri layer maps onto its own `ApiError`.

use std::io;

/// Convenient result alias for the crate's umbrella error.
pub type Result<T, E = CoreError> = std::result::Result<T, E>;

/// Errors produced while validating domain values.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DomainError {
    /// A nickname was empty after trimming.
    #[error("nickname is empty")]
    EmptyNickname,
    /// A nickname exceeded [`crate::protocol::limits::MAX_NICKNAME_CHARS`] characters.
    #[error("nickname has {actual} characters, the limit is {max}")]
    NicknameTooLong {
        /// Observed character count.
        actual: usize,
        /// Configured maximum.
        max: usize,
    },
    /// A nickname contained a control character.
    #[error("nickname contains a control character")]
    NicknameControlChar,
    /// A message body was empty after trimming.
    #[error("message body is empty")]
    EmptyBody,
    /// A message body exceeded [`crate::protocol::limits::MAX_BODY_CHARS`] characters.
    #[error("message body has {actual} characters, the limit is {max}")]
    BodyTooLong {
        /// Observed character count.
        actual: usize,
        /// Configured maximum.
        max: usize,
    },
    /// A message body contained a control character other than `\n` or `\t`.
    #[error("message body contains a disallowed control character")]
    BodyControlChar,
    /// An avatar seed was empty or too long.
    #[error("avatar seed has {actual} characters, the limit is 1..={max}")]
    BadAvatarSeed {
        /// Observed character count.
        actual: usize,
        /// Configured maximum.
        max: usize,
    },
    /// A UUID string could not be parsed.
    #[error("{kind} is not a valid UUID: {value}")]
    InvalidId {
        /// Which identifier failed to parse.
        kind: &'static str,
        /// The offending value, truncated for log safety.
        value: String,
    },
}

/// Errors from the wire protocol: framing, decoding and semantic validation.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// A declared frame length was outside the accepted range.
    #[error("frame length {len} is outside the accepted range 1..={max}")]
    FrameSize {
        /// Declared length.
        len: usize,
        /// Configured maximum.
        max: usize,
    },
    /// A frame was not valid UTF-8.
    #[error("frame payload is not valid UTF-8")]
    NotUtf8,
    /// A frame was not valid JSON, or did not match the envelope schema.
    #[error("frame payload is malformed: {0}")]
    Malformed(String),
    /// The peer speaks a protocol version this build does not support.
    #[error("peer speaks protocol version {got}, this build speaks {supported}")]
    VersionMismatch {
        /// Version the peer announced.
        got: u16,
        /// Version this build supports.
        supported: u16,
    },
    /// A validated value was rejected.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// The peer exceeded its frame budget.
    #[error("peer exceeded the inbound rate limit")]
    RateLimited,
    /// The framing layer rejected the stream.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Errors from the storage adapter.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// SQLite reported an error.
    #[error("sqlite error: {0}")]
    Sqlite(String),
    /// The database file was not usable and has been preserved under this name.
    #[error("database was corrupt and has been preserved as {preserved_path}")]
    Corrupted {
        /// Where the unusable file was moved.
        preserved_path: String,
    },
    /// The storage thread is gone; the process is shutting down.
    #[error("storage is unavailable")]
    Unavailable,
    /// A row could not be converted back into a domain value.
    #[error("stored value is invalid: {0}")]
    InvalidRow(String),
    /// A schema migration failed.
    #[error("migration {version} failed: {reason}")]
    Migration {
        /// Version being applied.
        version: u32,
        /// Underlying reason.
        reason: String,
    },
}

/// Errors from the discovery adapter.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The mDNS daemon could not be created or reached.
    #[error("mdns daemon error: {0}")]
    Mdns(String),
    /// The UDP beacon socket could not be created.
    #[error("beacon socket error: {0}")]
    Beacon(#[source] io::Error),
    /// Our own service could not be advertised.
    #[error("failed to announce this device: {0}")]
    Announce(String),
    /// The discovery supervisor has been shut down.
    #[error("discovery is shut down")]
    Shutdown,
}

/// Errors from the TCP transport.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// A socket operation failed.
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    /// A frame was rejected by the protocol layer.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// The peer closed the connection in the middle of a frame.
    #[error("peer closed the connection with {buffered} bytes of a partial frame buffered")]
    TruncatedFrame {
        /// Bytes of the incomplete frame that had arrived.
        buffered: usize,
    },
    /// The peer closed the connection before the handshake completed.
    #[error("peer closed the connection during the handshake")]
    HandshakeClosed,
    /// The handshake did not complete within the deadline.
    #[error("handshake timed out")]
    HandshakeTimeout,
    /// The peer announced a different identity than the one we dialled.
    #[error("peer identified as {got} but we dialled {expected}")]
    IdentityMismatch {
        /// Identity we dialled.
        expected: String,
        /// Identity the peer announced.
        got: String,
    },
    /// The peer tried to talk to itself, or two instances share a device id.
    #[error("peer announced our own device id")]
    SelfConnection,
    /// The first frame on a connection was not the handshake.
    #[error("expected a handshake frame, got {got}")]
    UnexpectedFrame {
        /// What arrived instead.
        got: &'static str,
    },
    /// Too many peers are already connected.
    #[error("connection refused: peer limit of {max} reached")]
    PeerLimit {
        /// Configured maximum.
        max: usize,
    },
    /// The outbound queue for this peer is full.
    #[error("outbound queue for the peer is full")]
    BackPressure,
    /// The connection task has stopped.
    #[error("connection is closed")]
    Closed,
    /// The connection failed for a reason that has no more specific variant.
    ///
    /// Used for refusals a peer reported back (a version mismatch, a full peer table) and for
    /// failures whose distinction only matters in the log: every caller treats them the same
    /// way, by marking the peer offline and retrying later.
    #[error("{0}")]
    Failed(String),
}

/// Errors surfaced to the caller of the service layer.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Domain validation failed.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// The wire protocol rejected a frame.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// Storage failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Discovery failed.
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    /// Transport failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// No peer with that device id is known.
    #[error("unknown device {0}")]
    UnknownPeer(String),
    /// The peer exists but is not reachable right now.
    #[error("device {0} is offline")]
    PeerOffline(String),
    /// The service has been told to shut down.
    #[error("the application is shutting down")]
    ShuttingDown,
    /// A background task ended unexpectedly.
    #[error("background task failed: {0}")]
    Task(String),
}

impl From<tokio::sync::mpsc::error::SendError<()>> for CoreError {
    fn from(_: tokio::sync::mpsc::error::SendError<()>) -> Self {
        CoreError::ShuttingDown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_errors_are_comparable_and_descriptive() {
        let err = DomainError::NicknameTooLong {
            actual: 33,
            max: 32,
        };
        assert_eq!(
            err,
            DomainError::NicknameTooLong {
                actual: 33,
                max: 32
            }
        );
        assert_eq!(
            err.to_string(),
            "nickname has 33 characters, the limit is 32"
        );
    }

    #[test]
    fn protocol_error_converts_from_domain_error() {
        let err: ProtocolError = DomainError::EmptyBody.into();
        assert!(matches!(err, ProtocolError::Domain(DomainError::EmptyBody)));
    }

    #[test]
    fn core_error_exposes_the_underlying_layer() {
        let err: CoreError = ProtocolError::RateLimited.into();
        assert!(matches!(
            err,
            CoreError::Protocol(ProtocolError::RateLimited)
        ));
    }
}
