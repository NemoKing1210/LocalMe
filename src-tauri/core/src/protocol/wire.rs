//! Raw wire shapes.
//!
//! Everything here uses plain JSON-friendly types (`String`, `u16`, `u64`) so that the
//! schema is exactly what the documentation describes, and so that malformed input fails in
//! one predictable place. These types are never exposed outside [`super`]: the only way to
//! obtain a [`Frame`](super::Frame) is [`WireFrame::into_frame`], which validates every
//! field.
//!
//! Keeping a separate wire shape is what lets the envelope evolve (a field renamed in v2,
//! a codec that is not JSON) without the domain types noticing.

use serde::{Deserialize, Serialize};

use crate::domain::ids::{AvatarSeed, DeviceId, MessageId};
use crate::domain::message::MessageBody;
use crate::domain::nickname::Nickname;
use crate::domain::peer::Handshake;
use crate::error::ProtocolError;
use crate::protocol::{ErrorCode, Frame, GoodbyeReason};

/// A complete envelope: the protocol version plus the tagged payload.
///
/// Serialised flat, so a heartbeat is exactly `{"v":1,"t":"heartbeat","seq":7}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WireFrame {
    /// Protocol version.
    pub v: u16,
    /// Tagged payload.
    #[serde(flatten)]
    pub payload: WirePayload,
}

/// The tagged payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(super) enum WirePayload {
    /// Opening frame from the dialer.
    Hello(WireHandshake),
    /// The acceptor's reply.
    Welcome(WireHandshake),
    /// Liveness probe.
    Heartbeat {
        /// Counter.
        seq: u64,
    },
    /// Liveness confirmation.
    HeartbeatAck {
        /// Counter being confirmed.
        seq: u64,
    },
    /// A chat message.
    Chat {
        /// Message identifier.
        id: String,
        /// Message text.
        body: String,
    },
    /// Delivery confirmation.
    ChatAck {
        /// Message identifier.
        id: String,
    },
    /// Profile change.
    Profile {
        /// New nickname.
        nickname: String,
        /// New avatar seed.
        avatar_seed: String,
    },
    /// Graceful close.
    Goodbye {
        /// Reason string.
        reason: String,
    },
    /// Protocol error.
    Error {
        /// Machine-readable code.
        code: String,
        /// Human-readable detail.
        message: String,
    },
}

/// Identity exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WireHandshake {
    /// Stable device id.
    pub device_id: String,
    /// Display name.
    pub nickname: String,
    /// Avatar seed.
    pub avatar_seed: String,
    /// Listening TCP port.
    pub listen_port: u16,
    /// Protocol version.
    pub protocol_version: u16,
}

impl WireFrame {
    /// Projects a validated frame onto its wire shape.
    pub(super) fn from_frame(frame: &Frame) -> Self {
        let payload = match frame {
            Frame::Hello(handshake) => WirePayload::Hello(WireHandshake::from_handshake(handshake)),
            Frame::Welcome(handshake) => {
                WirePayload::Welcome(WireHandshake::from_handshake(handshake))
            }
            Frame::Heartbeat { seq } => WirePayload::Heartbeat { seq: *seq },
            Frame::HeartbeatAck { seq } => WirePayload::HeartbeatAck { seq: *seq },
            Frame::Chat { id, body } => WirePayload::Chat {
                id: id.to_string(),
                body: body.as_str().to_owned(),
            },
            Frame::ChatAck { id } => WirePayload::ChatAck { id: id.to_string() },
            Frame::Profile {
                nickname,
                avatar_seed,
            } => WirePayload::Profile {
                nickname: nickname.as_str().to_owned(),
                avatar_seed: avatar_seed.as_str().to_owned(),
            },
            Frame::Goodbye { reason } => WirePayload::Goodbye {
                reason: reason.as_str().to_owned(),
            },
            Frame::Error { code, message } => WirePayload::Error {
                code: code.as_str().to_owned(),
                message: message.clone(),
            },
        };
        Self {
            v: crate::protocol::PROTOCOL_VERSION,
            payload,
        }
    }

    /// Validates the wire shape into a frame.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::Domain`] for structurally valid but semantically invalid
    /// values (bad UUIDs, empty nicknames, oversized bodies).
    pub(super) fn into_frame(self) -> Result<Frame, ProtocolError> {
        let _version = self.v;
        let frame = match self.payload {
            WirePayload::Hello(handshake) => Frame::Hello(handshake.into_handshake()?),
            WirePayload::Welcome(handshake) => Frame::Welcome(handshake.into_handshake()?),
            WirePayload::Heartbeat { seq } => Frame::Heartbeat { seq },
            WirePayload::HeartbeatAck { seq } => Frame::HeartbeatAck { seq },
            WirePayload::Chat { id, body } => Frame::Chat {
                id: id.parse::<MessageId>()?,
                body: MessageBody::parse(&body)?,
            },
            WirePayload::ChatAck { id } => Frame::ChatAck {
                id: id.parse::<MessageId>()?,
            },
            WirePayload::Profile {
                nickname,
                avatar_seed,
            } => Frame::Profile {
                nickname: Nickname::parse(&nickname)?,
                avatar_seed: AvatarSeed::parse(&avatar_seed)?,
            },
            WirePayload::Goodbye { reason } => Frame::Goodbye {
                reason: GoodbyeReason::from_wire(&reason),
            },
            WirePayload::Error { code, message } => {
                Frame::error(ErrorCode::from_wire(&code), message)
            }
        };
        Ok(frame)
    }
}

impl WireHandshake {
    fn from_handshake(handshake: &Handshake) -> Self {
        Self {
            device_id: handshake.device_id.to_string(),
            nickname: handshake.nickname.as_str().to_owned(),
            avatar_seed: handshake.avatar_seed.as_str().to_owned(),
            listen_port: handshake.listen_port,
            protocol_version: handshake.protocol_version,
        }
    }

    fn into_handshake(self) -> Result<Handshake, ProtocolError> {
        Ok(Handshake {
            protocol_version: self.protocol_version,
            device_id: self.device_id.parse::<DeviceId>()?,
            nickname: Nickname::parse(&self.nickname)?,
            avatar_seed: AvatarSeed::parse(&self.avatar_seed)?,
            listen_port: self.listen_port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::limits::MAX_FRAME_BYTES;
    use crate::protocol::{decode, encode};

    #[test]
    fn the_wire_shape_is_flat_and_versioned() {
        let frame = Frame::Profile {
            nickname: Nickname::parse("Аня").expect("valid"),
            avatar_seed: AvatarSeed::parse("seed").expect("valid"),
        };
        let json = serde_json::to_value(WireFrame::from_frame(&frame)).expect("serialise");
        assert_eq!(json["v"], 1);
        assert_eq!(json["t"], "profile");
        assert_eq!(json["nickname"], "Аня");
        assert!(json.get("payload").is_none(), "payload must not be nested");
    }

    #[test]
    fn a_handshake_announcing_another_version_decodes() {
        // Version enforcement is the connection loop's job, not the codec's: the codec must
        // hand back what the peer said so the mismatch can be reported precisely.
        let hello = br#"{"v":9,"t":"hello","device_id":"018f2b9c-0000-7000-8000-000000000000","nickname":"n","avatar_seed":"s","listen_port":1,"protocol_version":9}"#;
        match decode(hello).expect("decodes") {
            Frame::Hello(handshake) => assert_eq!(handshake.protocol_version, 9),
            other => panic!("expected hello, got {other:?}"),
        }
    }

    #[test]
    fn oversized_bodies_are_rejected_before_they_reach_the_domain() {
        let body = "a".repeat(crate::protocol::limits::MAX_BODY_CHARS + 1);
        let payload = serde_json::to_vec(&WireFrame {
            v: 1,
            payload: WirePayload::Chat {
                id: MessageId::generate().to_string(),
                body,
            },
        })
        .expect("serialise");
        assert!(payload.len() < MAX_FRAME_BYTES);
        assert!(decode(&payload).is_err());
    }

    #[test]
    fn every_field_of_a_handshake_must_be_present() {
        for missing in [
            "device_id",
            "nickname",
            "avatar_seed",
            "listen_port",
            "protocol_version",
        ] {
            let mut value = serde_json::json!({
                "v": 1,
                "t": "hello",
                "device_id": "018f2b9c-0000-7000-8000-000000000000",
                "nickname": "n",
                "avatar_seed": "s",
                "listen_port": 1,
                "protocol_version": 1,
            });
            value.as_object_mut().expect("object").remove(missing);
            let payload = serde_json::to_vec(&value).expect("serialise");
            assert!(
                decode(&payload).is_err(),
                "a handshake without `{missing}` must be refused"
            );
        }
    }

    #[test]
    fn encoding_and_decoding_agree_on_the_same_bytes() {
        let frame = Frame::Heartbeat { seq: u64::MAX };
        let bytes = encode(&frame).expect("encode");
        let (_prefix, payload) = bytes.split_at(4);
        assert_eq!(decode(payload).expect("decode"), frame);
    }
}
