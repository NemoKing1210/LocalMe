//! Wire format: framing, the validated frame type, and the raw JSON envelope it is built from.
//!
//! The public surface is deliberately small: `Frame --encode--> bytes --decode--> Frame`.
//! [`decode`] is the only way bytes become a [`Frame`], and the raw wire shapes inside
//! [`wire`] are never handed out, so unvalidated data cannot be mistaken for a domain value.

pub mod framing;
pub mod limits;
pub mod ratelimit;
mod wire;

pub use framing::{FrameDecoder, encode_frame};
pub use limits::{
    DEFAULT_BEACON_PORT, DEFAULT_TCP_PORT, HEARTBEAT_INTERVAL, HEARTBEAT_TIMEOUT, MAX_BODY_CHARS,
    MAX_FRAME_BYTES, MAX_NICKNAME_CHARS, MAX_PEERS, PROTOCOL_VERSION, SERVICE_TYPE,
};
pub use ratelimit::TokenBucket;

use serde::Deserialize;

use crate::domain::attachment::{AttachmentMeta, Sha256};
use crate::domain::ids::{AttachmentId, AvatarSeed, MessageId};
use crate::domain::message::MessageBody;
use crate::domain::nickname::Nickname;
use crate::domain::peer::Handshake;
use crate::error::ProtocolError;

use limits::MAX_ERROR_TEXT_CHARS;
use wire::WireFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoodbyeReason {
    /// The peer is quitting. Treated as an immediate offline transition.
    Shutdown,
    /// The peer is dropping this connection because it kept the other one it had with us
    /// (the simultaneous-connect rule).
    Superseded,
    Error,
    /// A reason this build does not know.
    Unknown,
}

impl GoodbyeReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shutdown => "shutdown",
            Self::Superseded => "superseded",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }

    /// An unrecognised reason degrades to [`GoodbyeReason::Unknown`] instead of failing the
    /// frame: a peer that adds a reason in a later version must still be able to say goodbye.
    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "shutdown" => Self::Shutdown,
            "superseded" => Self::Superseded,
            "error" => Self::Error,
            _ => Self::Unknown,
        }
    }
}

/// A machine-readable error code.
///
/// The accompanying human-readable text is bounded and only ever logged: the UI renders the
/// code, never attacker-controlled prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Malformed,
    BadNickname,
    BadBody,
    /// An attachment list, chunk or acknowledgement that could not be trusted.
    BadAttachment,
    RateLimited,
    VersionMismatch,
    PeerLimit,
    SelfConnection,
    HandshakeTimeout,
    Internal,
}

impl ErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::BadNickname => "bad_nickname",
            Self::BadBody => "bad_body",
            Self::BadAttachment => "bad_attachment",
            Self::RateLimited => "rate_limited",
            Self::VersionMismatch => "version_mismatch",
            Self::PeerLimit => "peer_limit",
            Self::SelfConnection => "self_connection",
            Self::HandshakeTimeout => "handshake_timeout",
            Self::Internal => "internal",
        }
    }

    /// Anything unrecognised maps to [`ErrorCode::Malformed`] so a peer from the future cannot
    /// smuggle an arbitrary string into our logs as a code we appear to understand.
    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "malformed" => Self::Malformed,
            "bad_nickname" => Self::BadNickname,
            "bad_body" => Self::BadBody,
            "bad_attachment" => Self::BadAttachment,
            "rate_limited" => Self::RateLimited,
            "version_mismatch" => Self::VersionMismatch,
            "peer_limit" => Self::PeerLimit,
            "self_connection" => Self::SelfConnection,
            "handshake_timeout" => Self::HandshakeTimeout,
            "internal" => Self::Internal,
            _ => Self::Malformed,
        }
    }
}

/// A validated protocol frame.
///
/// Every variant carries domain values, so anything reaching the service layer has already
/// passed framing, schema and semantic validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Hello(Handshake),
    Welcome(Handshake),
    /// `seq` is a monotonic counter, useful in a log when a peer claims not to have heard us.
    Heartbeat {
        seq: u64,
    },
    /// A chat message: the identifier and the deduplication key, the text when there is any, and
    /// the files it carries. A message with neither text nor files cannot be built, which is why
    /// the frame is the one place that rule is enforced.
    Chat {
        id: MessageId,
        text: Option<MessageBody>,
        attachments: Vec<AttachmentMeta>,
    },
    ChatAck {
        id: MessageId,
    },
    /// A slice of a file, at an absolute offset.
    ///
    /// `offset` is part of the frame rather than implied by arrival order because a transfer has
    /// to resume after a disconnect, and the recipient — whose file on disk is the only authority
    /// on how much it really has — is the one that decides where the sender continues.
    FileChunk {
        attachment: AttachmentId,
        offset: u64,
        data: Vec<u8>,
    },
    /// The sender's last chunk is out and `sha256` is the digest of the whole file.
    FileDone {
        attachment: AttachmentId,
        sha256: Sha256,
    },
    /// The recipient's position and what it means.
    FileAck {
        attachment: AttachmentId,
        received: u64,
        state: FileAckState,
    },
    /// The transfer is over and will not be resumed by itself.
    FileCancel {
        attachment: AttachmentId,
        reason: FileCancelReason,
    },
    /// "Send me this file", used by a recipient that has the metadata but not the bytes —
    /// after a failed transfer, or when the user asks for it again.
    FileRequest {
        attachment: AttachmentId,
    },
    /// Re-broadcast to every live connection.
    Profile {
        nickname: Nickname,
        avatar_seed: AvatarSeed,
    },
    /// `reason` decides whether to redial.
    Goodbye {
        reason: GoodbyeReason,
    },
    /// The connection is closed after it is sent; `message` is for logs only.
    Error {
        code: ErrorCode,
        message: String,
    },
}

/// What a [`Frame::FileAck`] says about the transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAckState {
    /// The recipient is still writing; `received` is the append position of the next chunk.
    Receiving,
    /// The file is stored, verified and renamed; the transfer is over.
    Complete,
}

impl FileAckState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Receiving => "receiving",
            Self::Complete => "complete",
        }
    }

    /// An unrecognised state would leave the sender unable to tell whether its file arrived, so
    /// it is a malformed frame rather than a degraded one.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "receiving" => Some(Self::Receiving),
            "complete" => Some(Self::Complete),
            _ => None,
        }
    }
}

/// Why a transfer will not continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCancelReason {
    /// The user pressed cancel on the sending side.
    Cancelled,
    /// A terminal failure: the source disappeared, the disk refused the write, the file was too
    /// large, the digest did not match.
    Failed,
    /// The offer is above [`MAX_ATTACHMENT_BYTES`](crate::protocol::limits::MAX_ATTACHMENT_BYTES).
    TooLarge,
    /// The recipient has no record of that attachment: the message was cleared, or a retry
    /// arrived after the history it belonged to was deleted.
    Unknown,
    /// The bytes arrived but the digest disagrees with the sender's.
    Checksum,
    /// A reason this build does not know.
    UnknownReason,
}

impl FileCancelReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::TooLarge => "too_large",
            Self::Unknown => "unknown",
            Self::Checksum => "checksum",
            Self::UnknownReason => "other",
        }
    }

    /// An unrecognised reason degrades to [`FileCancelReason::UnknownReason`] rather than failing
    /// the frame: a peer that adds a reason in a later version must still be able to cancel.
    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            "too_large" => Self::TooLarge,
            "unknown" => Self::Unknown,
            "checksum" => Self::Checksum,
            _ => Self::UnknownReason,
        }
    }

    /// Whether the interruption says something about the *file* rather than about the moment:
    /// these are worth offering a retry for.
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        !matches!(self, Self::Cancelled)
    }
}

impl Frame {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::Welcome(_) => "welcome",
            Self::Heartbeat { .. } => "heartbeat",
            Self::Chat { .. } => "chat",
            Self::ChatAck { .. } => "chat_ack",
            Self::FileChunk { .. } => "file_chunk",
            Self::FileDone { .. } => "file_done",
            Self::FileAck { .. } => "file_ack",
            Self::FileCancel { .. } => "file_cancel",
            Self::FileRequest { .. } => "file_request",
            Self::Profile { .. } => "profile",
            Self::Goodbye { .. } => "goodbye",
            Self::Error { .. } => "error",
        }
    }

    #[must_use]
    pub const fn is_handshake(&self) -> bool {
        matches!(self, Self::Hello(_) | Self::Welcome(_))
    }

    /// Truncates the detail so a peer cannot use the error channel to make us log unbounded
    /// text.
    #[must_use]
    pub fn error(code: ErrorCode, message: impl AsRef<str>) -> Self {
        let text: String = message
            .as_ref()
            .chars()
            .take(MAX_ERROR_TEXT_CHARS)
            .collect();
        Self::Error {
            code,
            message: text,
        }
    }
}

/// # Errors
///
/// Returns [`ProtocolError::FrameSize`] if the encoded frame would exceed [`MAX_FRAME_BYTES`].
/// With the domain limits in place this is unreachable for frames we construct, but it is
/// checked rather than assumed.
pub fn encode(frame: &Frame) -> Result<Vec<u8>, ProtocolError> {
    let json = serde_json::to_vec(&WireFrame::from_frame(frame))
        .map_err(|err| ProtocolError::Malformed(format!("encode failed: {err}")))?;
    if json.is_empty() || json.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameSize {
            len: json.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    encode_frame(&json)
}

/// Decodes and validates one frame body: the payload that follows the length prefix.
///
/// This is the trust boundary. The announced protocol version is *not* checked here — it is
/// returned inside `Hello`/`Welcome` so the connection loop can answer a mismatched peer with a
/// precise error.
///
/// # Errors
///
/// * [`ProtocolError::FrameSize`] — the payload is empty or larger than [`MAX_FRAME_BYTES`].
/// * [`ProtocolError::NotUtf8`] — the payload is not valid UTF-8.
/// * [`ProtocolError::Malformed`] — the payload is not a valid envelope.
/// * [`ProtocolError::Domain`] — a field was structurally fine but semantically invalid.
pub fn decode(payload: &[u8]) -> Result<Frame, ProtocolError> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameSize {
            len: payload.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    let text = std::str::from_utf8(payload).map_err(|_| ProtocolError::NotUtf8)?;
    let wire: WireFrame =
        serde_json::from_str(text).map_err(|err| ProtocolError::Malformed(err.to_string()))?;
    wire.into_frame()
}

/// Round-trips a frame through the encoder and decoder.
///
/// # Errors
///
/// Propagates encoder and decoder failures.
pub fn round_trip(frame: &Frame) -> Result<Frame, ProtocolError> {
    let encoded = encode(frame)?;
    let (_prefix, payload) = encoded.split_at(limits::LENGTH_PREFIX_BYTES);
    decode(payload)
}

/// The version an envelope announces, read cheaply from the raw text.
///
/// Used to answer a peer that announces an unsupported version *before* its frame is
/// interpreted: if the schemas differ, a full decode would fail with a confusing "malformed"
/// instead of "we do not speak your version".
///
/// # Errors
///
/// Returns [`ProtocolError::NotUtf8`] or [`ProtocolError::Malformed`].
pub fn peek_version(payload: &[u8]) -> Result<u16, ProtocolError> {
    #[derive(Deserialize)]
    struct VersionOnly {
        v: u16,
    }

    let text = std::str::from_utf8(payload).map_err(|_| ProtocolError::NotUtf8)?;
    serde_json::from_str::<VersionOnly>(text)
        .map(|parsed| parsed.v)
        .map_err(|err| ProtocolError::Malformed(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::attachment::{AttachmentMeta, FileName};
    use crate::domain::ids::{AttachmentId, DeviceId};
    use crate::domain::peer::PeerProfile;
    use crate::error::DomainError;

    fn handshake() -> Handshake {
        let device_id = DeviceId::from_uuid(uuid::Uuid::from_u128(7));
        let nickname = Nickname::parse("Аня").expect("valid");
        Handshake::new(
            PROTOCOL_VERSION,
            &PeerProfile::new(device_id, nickname),
            47820,
        )
    }

    #[test]
    fn every_frame_variant_round_trips() {
        let message_id = MessageId::generate();
        let frames = vec![
            Frame::Hello(handshake()),
            Frame::Welcome(handshake()),
            Frame::Heartbeat { seq: 42 },
            Frame::Chat {
                id: message_id,
                text: Some(MessageBody::parse("Привет 👋\nвторая строка").expect("valid")),
                attachments: Vec::new(),
            },
            Frame::Chat {
                id: message_id,
                text: None,
                attachments: vec![AttachmentMeta::new(
                    AttachmentId::generate(),
                    FileName::sanitise("photo.jpg"),
                    2048,
                )],
            },
            Frame::ChatAck { id: message_id },
            Frame::Profile {
                nickname: Nickname::parse("Аня").expect("valid"),
                avatar_seed: AvatarSeed::parse("seed:Аня").expect("valid"),
            },
            Frame::Goodbye {
                reason: GoodbyeReason::Shutdown,
            },
            Frame::error(ErrorCode::BadBody, "too long"),
        ];

        for frame in frames {
            let decoded = round_trip(&frame).expect("round trip");
            assert_eq!(decoded, frame, "frame {frame:?} did not survive the wire");
        }
    }

    #[test]
    fn the_wire_form_is_the_documented_shape() {
        let frame = Frame::Heartbeat { seq: 7 };
        let encoded = encode(&frame).expect("encode");
        let (prefix, payload) = encoded.split_at(limits::LENGTH_PREFIX_BYTES);
        let len = u32::from_be_bytes(prefix.try_into().expect("four bytes")) as usize;
        assert_eq!(len, payload.len());

        let json: serde_json::Value = serde_json::from_slice(payload).expect("json");
        assert_eq!(json["v"], PROTOCOL_VERSION);
        assert_eq!(json["t"], "heartbeat");
        assert_eq!(json["seq"], 7);
    }

    #[test]
    fn chat_frames_carry_no_sender_field() {
        // The connection is the sender; a field saying so would be redundant and forgeable.
        let frame = Frame::Chat {
            id: MessageId::generate(),
            text: Some(MessageBody::parse("hi").expect("valid")),
            attachments: Vec::new(),
        };
        let encoded = encode(&frame).expect("encode");
        let (_prefix, payload) = encoded.split_at(limits::LENGTH_PREFIX_BYTES);
        let json: serde_json::Value = serde_json::from_slice(payload).expect("json");
        let object = json.as_object().expect("object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["attachments", "id", "t", "text", "v"]);
    }

    #[test]
    fn the_handshake_carries_every_field_the_peer_needs() {
        let encoded = encode(&Frame::Hello(handshake())).expect("encode");
        let (_prefix, payload) = encoded.split_at(limits::LENGTH_PREFIX_BYTES);
        let json: serde_json::Value = serde_json::from_slice(payload).expect("json");
        for field in [
            "v",
            "t",
            "device_id",
            "nickname",
            "avatar_seed",
            "listen_port",
            "protocol_version",
        ] {
            assert!(!json[field].is_null(), "missing field {field}: {json}");
        }
    }

    #[test]
    fn proportional_shaped_payloads_are_rejected_rather_than_allocated() {
        // A declared length beyond the cap is refused on the payload alone, without the
        // decoder ever allocating for the claim.
        let payload = vec![b'a'; MAX_FRAME_BYTES + 1];
        assert!(matches!(
            decode(&payload),
            Err(ProtocolError::FrameSize { .. })
        ));
    }

    #[test]
    fn decoding_rejects_empty_and_non_utf8_payloads() {
        assert!(matches!(
            decode(&[]),
            Err(ProtocolError::FrameSize { len: 0, .. })
        ));
        assert!(matches!(
            decode(&[0xff, 0xfe, 0xfd]),
            Err(ProtocolError::NotUtf8)
        ));
    }

    #[test]
    fn decoding_rejects_unknown_frame_types() {
        let err = decode(br#"{"v":1,"t":"self_destruct"}"#).expect_err("must fail");
        assert!(matches!(err, ProtocolError::Malformed(_)), "{err}");
    }

    #[test]
    fn decoding_rejects_missing_and_mistyped_fields() {
        assert!(decode(br#"{"v":1,"t":"heartbeat"}"#).is_err());
        assert!(decode(br#"{"v":1,"t":"heartbeat","seq":"seven"}"#).is_err());
        assert!(decode(br#"{"t":"heartbeat","seq":1}"#).is_err());
        assert!(decode(b"[]").is_err());
        assert!(decode(b"null").is_err());
    }

    #[test]
    fn decoding_validates_nicknames_and_bodies() {
        let hello = br#"{"v":1,"t":"hello","device_id":"018f2b9c-0000-7000-8000-000000000000","nickname":"   ","avatar_seed":"s","listen_port":1,"protocol_version":1}"#;
        assert!(matches!(
            decode(hello),
            Err(ProtocolError::Domain(DomainError::EmptyNickname))
        ));

        let oversized_nickname = format!(
            r#"{{"v":1,"t":"hello","device_id":"018f2b9c-0000-7000-8000-000000000000","nickname":"{}","avatar_seed":"s","listen_port":1,"protocol_version":1}}"#,
            "x".repeat(MAX_NICKNAME_CHARS + 1)
        );
        assert!(matches!(
            decode(oversized_nickname.as_bytes()),
            Err(ProtocolError::Domain(DomainError::NicknameTooLong { .. }))
        ));

        let chat =
            br#"{"v":2,"t":"chat","id":"018f2b9c-0000-7000-8000-000000000001","text":"   "}"#;
        assert!(matches!(
            decode(chat),
            Err(ProtocolError::Domain(DomainError::EmptyBody))
        ));

        // A chat frame with neither text nor files cannot describe a message.
        let empty = br#"{"v":2,"t":"chat","id":"018f2b9c-0000-7000-8000-000000000001"}"#;
        assert!(matches!(
            decode(empty),
            Err(ProtocolError::Domain(DomainError::EmptyMessage))
        ));

        let bad_id = br#"{"v":2,"t":"chat","id":"not-a-uuid","text":"hi"}"#;
        assert!(matches!(
            decode(bad_id),
            Err(ProtocolError::Domain(DomainError::InvalidId { .. }))
        ));

        let bad_seed = br#"{"v":1,"t":"profile","nickname":"ok","avatar_seed":""}"#;
        assert!(matches!(
            decode(bad_seed),
            Err(ProtocolError::Domain(DomainError::BadAvatarSeed { .. }))
        ));
    }

    #[test]
    fn decoding_truncates_hostile_error_text() {
        let payload = format!(
            r#"{{"v":1,"t":"error","code":"internal","message":"{}"}}"#,
            "x".repeat(MAX_ERROR_TEXT_CHARS * 4)
        );
        let frame = decode(payload.as_bytes()).expect("decodes");
        match frame {
            Frame::Error { message, .. } => {
                assert_eq!(message.chars().count(), MAX_ERROR_TEXT_CHARS);
            }
            other => panic!("expected an error frame, got {other:?}"),
        }
    }

    #[test]
    fn unknown_error_codes_and_goodbye_reasons_degrade_to_known_values() {
        let unknown_error = br#"{"v":1,"t":"error","code":"from_the_future","message":"hi"}"#;
        assert_eq!(
            decode(unknown_error).expect("decodes"),
            Frame::Error {
                code: ErrorCode::Malformed,
                message: "hi".to_owned()
            }
        );

        let unknown_goodbye = br#"{"v":1,"t":"goodbye","reason":"because"}"#;
        assert_eq!(
            decode(unknown_goodbye).expect("decodes"),
            Frame::Goodbye {
                reason: GoodbyeReason::Unknown
            }
        );
    }

    #[test]
    fn error_codes_round_trip_through_their_wire_names() {
        for code in [
            ErrorCode::Malformed,
            ErrorCode::BadNickname,
            ErrorCode::BadBody,
            ErrorCode::BadAttachment,
            ErrorCode::RateLimited,
            ErrorCode::VersionMismatch,
            ErrorCode::PeerLimit,
            ErrorCode::SelfConnection,
            ErrorCode::HandshakeTimeout,
            ErrorCode::Internal,
        ] {
            assert_eq!(ErrorCode::from_wire(code.as_str()), code);
        }
        assert_eq!(ErrorCode::from_wire("bogus"), ErrorCode::Malformed);
    }

    #[test]
    fn peek_version_reads_the_envelope_version() {
        assert_eq!(
            peek_version(br#"{"v":3,"t":"heartbeat","seq":1}"#).expect("peek"),
            3
        );
        assert!(peek_version(b"not json").is_err());
        assert!(peek_version(br#"{"t":"heartbeat"}"#).is_err());
        assert!(peek_version(&[0xff]).is_err());
    }

    #[test]
    fn error_text_is_bounded_at_construction() {
        match Frame::error(ErrorCode::Internal, "y".repeat(10_000)) {
            Frame::Error { message, .. } => {
                assert_eq!(message.chars().count(), MAX_ERROR_TEXT_CHARS);
            }
            other => panic!("expected an error frame, got {other:?}"),
        }
    }

    #[test]
    fn frame_kinds_are_stable_labels() {
        assert_eq!(Frame::Heartbeat { seq: 0 }.kind(), "heartbeat");
        assert_eq!(
            Frame::ChatAck {
                id: MessageId::generate()
            }
            .kind(),
            "chat_ack"
        );
        assert!(Frame::Hello(handshake()).is_handshake());
        assert!(
            !Frame::ChatAck {
                id: MessageId::generate()
            }
            .is_handshake()
        );
    }
}
