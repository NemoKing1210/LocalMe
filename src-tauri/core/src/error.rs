//! Typed errors for every layer.

use std::io;

pub type Result<T, E = CoreError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("nickname is empty")]
    EmptyNickname,
    #[error("nickname has {actual} characters, the limit is {max}")]
    NicknameTooLong { actual: usize, max: usize },
    #[error("nickname contains a control character")]
    NicknameControlChar,
    /// A message body was empty after trimming.
    #[error("message body is empty")]
    EmptyBody,
    /// A message body exceeded [`crate::protocol::limits::MAX_BODY_CHARS`] characters.
    #[error("message body has {actual} characters, the limit is {max}")]
    BodyTooLong { actual: usize, max: usize },
    #[error("message body contains a disallowed control character")]
    BodyControlChar,
    /// An avatar seed was empty or too long.
    #[error("avatar seed has {actual} characters, the limit is 1..={max}")]
    BadAvatarSeed { actual: usize, max: usize },
    #[error("{kind} is not a valid UUID: {value}")]
    InvalidId { kind: &'static str, value: String },
    /// A digest that is not 64 hexadecimal characters.
    #[error("`{value}` is not a SHA-256 digest")]
    InvalidDigest { value: String },
    /// A file above [`crate::protocol::limits::MAX_ATTACHMENT_BYTES`].
    #[error("attachment of {size} bytes is above the limit of {max}")]
    AttachmentTooLarge { size: u64, max: u64 },
    /// A message that carries neither text nor a file.
    #[error("a message needs text or at least one attachment")]
    EmptyMessage,
    /// A file that cannot be offered: missing, not a regular file, or unreadable.
    #[error("the file could not be attached: {reason}")]
    BadAttachment { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("frame length {len} is outside the accepted range 1..={max}")]
    FrameSize { len: usize, max: usize },
    #[error("frame payload is not valid UTF-8")]
    NotUtf8,
    #[error("frame payload is malformed: {0}")]
    Malformed(String),
    #[error("peer speaks protocol version {got}, this build speaks {supported}")]
    VersionMismatch { got: u16, supported: u16 },
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("peer exceeded the inbound rate limit")]
    RateLimited,
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(String),
    #[error("database was corrupt and has been preserved as {preserved_path}")]
    Corrupted { preserved_path: String },
    #[error("storage is unavailable")]
    Unavailable,
    #[error("stored value is invalid: {0}")]
    InvalidRow(String),
    #[error("migration {version} failed: {reason}")]
    Migration { version: u32, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("mdns daemon error: {0}")]
    Mdns(String),
    #[error("beacon socket error: {0}")]
    Beacon(#[source] io::Error),
    #[error("failed to announce this device: {0}")]
    Announce(String),
    #[error("discovery is shut down")]
    Shutdown,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("peer closed the connection with {buffered} bytes of a partial frame buffered")]
    TruncatedFrame { buffered: usize },
    #[error("peer closed the connection during the handshake")]
    HandshakeClosed,
    #[error("handshake timed out")]
    HandshakeTimeout,
    #[error("peer identified as {got} but we dialled {expected}")]
    IdentityMismatch { expected: String, got: String },
    #[error("peer announced our own device id")]
    SelfConnection,
    #[error("expected a handshake frame, got {got}")]
    UnexpectedFrame { got: &'static str },
    #[error("connection refused: peer limit of {max} reached")]
    PeerLimit { max: usize },
    #[error("outbound queue for the peer is full")]
    BackPressure,
    #[error("connection is closed")]
    Closed,
    /// Refusals a peer reported back, and failures whose distinction only matters in the log:
    /// every caller treats them the same way, by marking the peer offline and retrying later.
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("unknown device {0}")]
    UnknownPeer(String),
    #[error("the application is shutting down")]
    ShuttingDown,
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
