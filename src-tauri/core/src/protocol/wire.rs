//! Raw wire shapes.
//!
//! Plain JSON-friendly types so malformed input fails in one predictable place. Never exposed
//! outside [`super`]: the only way to obtain a [`Frame`](super::Frame) is
//! [`WireFrame::into_frame`], which validates every field.

use std::collections::HashSet;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::domain::attachment::{AttachmentMeta, FileName, Sha256};
use crate::domain::ids::{AttachmentId, AvatarSeed, DeviceId, MessageId};
use crate::domain::message::MessageBody;
use crate::domain::nickname::Nickname;
use crate::domain::peer::Handshake;
use crate::error::{DomainError, ProtocolError};
use crate::protocol::limits::{
    FILE_CHUNK_BYTES, MAX_ATTACHMENT_BYTES, MAX_ATTACHMENTS_PER_MESSAGE,
};
use crate::protocol::{ErrorCode, FileAckState, FileCancelReason, Frame, GoodbyeReason};

/// A complete envelope: the protocol version plus the tagged payload.
///
/// Serialised flat, so a heartbeat is exactly `{"v":1,"t":"heartbeat","seq":7}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WireFrame {
    pub v: u16,
    #[serde(flatten)]
    pub payload: WirePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(super) enum WirePayload {
    Hello(WireHandshake),
    Welcome(WireHandshake),
    Heartbeat {
        seq: u64,
    },
    Chat {
        id: String,
        /// Absent for a message that is nothing but files.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default)]
        attachments: Vec<WireAttachment>,
    },
    ChatAck {
        id: String,
    },
    FileChunk {
        attachment: String,
        offset: u64,
        /// Base64: the frame is JSON, and an array of numbers would be larger than the file.
        data: String,
    },
    FileDone {
        attachment: String,
        sha256: String,
    },
    FileAck {
        attachment: String,
        received: u64,
        state: String,
    },
    FileCancel {
        attachment: String,
        reason: String,
    },
    FileRequest {
        attachment: String,
    },
    Profile {
        nickname: String,
        avatar_seed: String,
    },
    Goodbye {
        reason: String,
    },
    Error {
        code: String,
        message: String,
    },
}

/// What a `chat` frame announces about one file.
///
/// The kind (image or file) is *not* on the wire: it follows from the name, and a field that
/// repeats a derivable fact is a field that can disagree with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WireAttachment {
    pub id: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WireHandshake {
    pub device_id: String,
    pub nickname: String,
    pub avatar_seed: String,
    pub listen_port: u16,
    pub protocol_version: u16,
}

impl WireFrame {
    pub(super) fn from_frame(frame: &Frame) -> Self {
        let payload = match frame {
            Frame::Hello(handshake) => WirePayload::Hello(WireHandshake::from_handshake(handshake)),
            Frame::Welcome(handshake) => {
                WirePayload::Welcome(WireHandshake::from_handshake(handshake))
            }
            Frame::Heartbeat { seq } => WirePayload::Heartbeat { seq: *seq },
            Frame::Chat {
                id,
                text,
                attachments,
            } => WirePayload::Chat {
                id: id.to_string(),
                text: text.as_ref().map(|body| body.as_str().to_owned()),
                attachments: attachments.iter().map(WireAttachment::from_meta).collect(),
            },
            Frame::ChatAck { id } => WirePayload::ChatAck { id: id.to_string() },
            Frame::FileChunk {
                attachment,
                offset,
                data,
            } => WirePayload::FileChunk {
                attachment: attachment.to_string(),
                offset: *offset,
                data: STANDARD.encode(data),
            },
            Frame::FileDone { attachment, sha256 } => WirePayload::FileDone {
                attachment: attachment.to_string(),
                sha256: sha256.to_hex(),
            },
            Frame::FileAck {
                attachment,
                received,
                state,
            } => WirePayload::FileAck {
                attachment: attachment.to_string(),
                received: *received,
                state: state.as_str().to_owned(),
            },
            Frame::FileCancel { attachment, reason } => WirePayload::FileCancel {
                attachment: attachment.to_string(),
                reason: reason.as_str().to_owned(),
            },
            Frame::FileRequest { attachment } => WirePayload::FileRequest {
                attachment: attachment.to_string(),
            },
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

    /// # Errors
    ///
    /// Returns [`ProtocolError::Domain`] for structurally valid but semantically invalid
    /// values (bad UUIDs, empty nicknames, oversized bodies) and [`ProtocolError::Malformed`]
    /// for a payload that cannot be decoded at all (bad base64, an unknown acknowledgement
    /// state).
    pub(super) fn into_frame(self) -> Result<Frame, ProtocolError> {
        let _version = self.v;
        let frame = match self.payload {
            WirePayload::Hello(handshake) => Frame::Hello(handshake.into_handshake()?),
            WirePayload::Welcome(handshake) => Frame::Welcome(handshake.into_handshake()?),
            WirePayload::Heartbeat { seq } => Frame::Heartbeat { seq },
            WirePayload::Chat {
                id,
                text,
                attachments,
            } => {
                let id = id.parse::<MessageId>()?;
                let text = match text {
                    Some(text) => Some(MessageBody::parse(&text)?),
                    None => None,
                };
                let attachments = into_attachments(attachments)?;
                if text.is_none() && attachments.is_empty() {
                    return Err(ProtocolError::Domain(DomainError::EmptyMessage));
                }
                Frame::Chat {
                    id,
                    text,
                    attachments,
                }
            }
            WirePayload::ChatAck { id } => Frame::ChatAck {
                id: id.parse::<MessageId>()?,
            },
            WirePayload::FileChunk {
                attachment,
                offset,
                data,
            } => {
                let attachment = attachment.parse::<AttachmentId>()?;
                let data = STANDARD.decode(data.as_bytes()).map_err(|error| {
                    ProtocolError::Malformed(format!("file chunk is not base64: {error}"))
                })?;
                check_chunk(&data, offset)?;
                Frame::FileChunk {
                    attachment,
                    offset,
                    data,
                }
            }
            WirePayload::FileDone { attachment, sha256 } => Frame::FileDone {
                attachment: attachment.parse::<AttachmentId>()?,
                sha256: Sha256::from_hex(&sha256)?,
            },
            WirePayload::FileAck {
                attachment,
                received,
                state,
            } => Frame::FileAck {
                attachment: attachment.parse::<AttachmentId>()?,
                received,
                state: FileAckState::from_wire(&state).ok_or_else(|| {
                    ProtocolError::Malformed(format!("unknown acknowledgement state `{state}`"))
                })?,
            },
            WirePayload::FileCancel { attachment, reason } => Frame::FileCancel {
                attachment: attachment.parse::<AttachmentId>()?,
                reason: FileCancelReason::from_wire(&reason),
            },
            WirePayload::FileRequest { attachment } => Frame::FileRequest {
                attachment: attachment.parse::<AttachmentId>()?,
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

impl WireAttachment {
    fn from_meta(meta: &AttachmentMeta) -> Self {
        Self {
            id: meta.id.to_string(),
            name: meta.name.as_str().to_owned(),
            size: meta.size,
        }
    }
}

/// The attachment list of a `chat` frame.
///
/// The size cap is deliberately *not* checked here: an over-sized offer is refused by the
/// session with a `file_cancel`, which keeps the message visible and the connection up, while a
/// list that is too long or names the same file twice is a broken peer and closes it.
fn into_attachments(list: Vec<WireAttachment>) -> Result<Vec<AttachmentMeta>, ProtocolError> {
    if list.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        return Err(ProtocolError::Domain(DomainError::BadAttachment {
            reason: format!("{} files in one message", list.len()),
        }));
    }
    let mut seen = HashSet::with_capacity(list.len());
    let mut attachments = Vec::with_capacity(list.len());
    for raw in list {
        let id = raw.id.parse::<AttachmentId>()?;
        if !seen.insert(id) {
            return Err(ProtocolError::Domain(DomainError::BadAttachment {
                reason: "the same attachment id appears twice".to_owned(),
            }));
        }
        attachments.push(AttachmentMeta::new(
            id,
            FileName::sanitise(&raw.name),
            raw.size,
        ));
    }
    Ok(attachments)
}

/// A chunk must be non-empty and bounded, and its offset must stay inside a file.
fn check_chunk(data: &[u8], offset: u64) -> Result<(), ProtocolError> {
    if data.is_empty() || data.len() > FILE_CHUNK_BYTES {
        return Err(ProtocolError::Domain(DomainError::BadAttachment {
            reason: format!("a chunk of {} bytes", data.len()),
        }));
    }
    let end = offset.checked_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
    match end {
        Some(end) if end <= MAX_ATTACHMENT_BYTES => Ok(()),
        _ => Err(ProtocolError::Domain(DomainError::BadAttachment {
            reason: format!("a chunk at offset {offset} leaves the file's bounds"),
        })),
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
    use crate::protocol::decode;
    use crate::protocol::limits::MAX_FRAME_BYTES;

    #[test]
    fn the_wire_shape_is_flat_and_versioned() {
        let frame = Frame::Profile {
            nickname: Nickname::parse("Аня").expect("valid"),
            avatar_seed: AvatarSeed::parse("seed").expect("valid"),
        };
        let json = serde_json::to_value(WireFrame::from_frame(&frame)).expect("serialise");
        assert_eq!(json["v"], 2);
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
            v: 2,
            payload: WirePayload::Chat {
                id: MessageId::generate().to_string(),
                text: Some(body),
                attachments: Vec::new(),
            },
        })
        .expect("serialise");
        assert!(payload.len() < MAX_FRAME_BYTES);
        assert!(decode(&payload).is_err());
    }

    #[test]
    fn a_message_with_neither_text_nor_files_is_refused() {
        let payload = serde_json::to_vec(&WireFrame {
            v: 2,
            payload: WirePayload::Chat {
                id: MessageId::generate().to_string(),
                text: None,
                attachments: Vec::new(),
            },
        })
        .expect("serialise");
        assert!(matches!(
            decode(&payload),
            Err(ProtocolError::Domain(DomainError::EmptyMessage))
        ));
    }

    #[test]
    fn a_file_only_message_round_trips_and_keeps_its_name() {
        let id = AttachmentId::generate();
        let frame = Frame::Chat {
            id: MessageId::generate(),
            text: None,
            attachments: vec![AttachmentMeta::new(
                id,
                FileName::sanitise("отчёт.pdf"),
                4096,
            )],
        };
        let json = serde_json::to_value(WireFrame::from_frame(&frame)).expect("serialise");
        assert_eq!(json["t"], "chat");
        assert_eq!(json["text"], serde_json::Value::Null);
        assert_eq!(json["attachments"][0]["name"], "отчёт.pdf");
        assert!(
            json["attachments"][0].get("kind").is_none(),
            "the kind is derived, not sent"
        );
        assert_eq!(
            crate::protocol::round_trip(&frame).expect("round trip"),
            frame
        );
    }

    #[test]
    fn a_hostile_file_name_is_sanitised_on_the_way_in() {
        let payload = serde_json::to_vec(&WireFrame {
            v: 2,
            payload: WirePayload::Chat {
                id: MessageId::generate().to_string(),
                text: None,
                attachments: vec![WireAttachment {
                    id: AttachmentId::generate().to_string(),
                    name: "../../../etc/passwd".to_owned(),
                    size: 10,
                }],
            },
        })
        .expect("serialise");
        match decode(&payload).expect("decodes") {
            Frame::Chat { attachments, .. } => {
                assert_eq!(attachments[0].name.as_str(), "passwd");
            }
            other => panic!("expected chat, got {other:?}"),
        }
    }

    #[test]
    fn a_repeated_attachment_id_is_refused() {
        let id = AttachmentId::generate().to_string();
        let attachment = || WireAttachment {
            id: id.clone(),
            name: "a.bin".to_owned(),
            size: 1,
        };
        let payload = serde_json::to_vec(&WireFrame {
            v: 2,
            payload: WirePayload::Chat {
                id: MessageId::generate().to_string(),
                text: None,
                attachments: vec![attachment(), attachment()],
            },
        })
        .expect("serialise");
        assert!(decode(&payload).is_err());
    }

    #[test]
    fn a_chunk_round_trips_and_an_impossible_one_is_refused() {
        let attachment = AttachmentId::generate();
        let frame = Frame::FileChunk {
            attachment,
            offset: 0,
            data: vec![7_u8; FILE_CHUNK_BYTES],
        };
        assert_eq!(
            crate::protocol::round_trip(&frame).expect("round trip"),
            frame
        );

        let huge = Frame::FileChunk {
            attachment,
            offset: 0,
            data: vec![0_u8; FILE_CHUNK_BYTES + 1],
        };
        assert!(crate::protocol::round_trip(&huge).is_err());

        let past_the_end = Frame::FileChunk {
            attachment,
            offset: MAX_ATTACHMENT_BYTES,
            data: vec![1_u8],
        };
        assert!(crate::protocol::round_trip(&past_the_end).is_err());

        let not_base64 = br#"{"v":2,"t":"file_chunk","attachment":"018f2b9c-0000-7000-8000-000000000000","offset":0,"data":"!!!"}"#;
        assert!(decode(not_base64).is_err());
    }

    #[test]
    fn the_transfer_control_frames_round_trip() {
        let attachment = AttachmentId::generate();
        let frames = vec![
            Frame::FileAck {
                attachment,
                received: 4096,
                state: FileAckState::Receiving,
            },
            Frame::FileAck {
                attachment,
                received: 8192,
                state: FileAckState::Complete,
            },
            Frame::FileDone {
                attachment,
                sha256: Sha256::of(b"payload"),
            },
            Frame::FileCancel {
                attachment,
                reason: FileCancelReason::TooLarge,
            },
            Frame::FileRequest { attachment },
        ];
        for frame in frames {
            assert_eq!(
                crate::protocol::round_trip(&frame).expect("round trip"),
                frame
            );
        }
    }

    #[test]
    fn an_unknown_cancellation_reason_degrades_but_an_unknown_ack_does_not() {
        let attachment = AttachmentId::generate();
        let cancel = format!(
            r#"{{"v":2,"t":"file_cancel","attachment":"{attachment}","reason":"from_the_future"}}"#
        );
        match decode(cancel.as_bytes()).expect("decodes") {
            Frame::FileCancel { reason, .. } => {
                assert_eq!(reason, FileCancelReason::UnknownReason);
            }
            other => panic!("expected a cancel, got {other:?}"),
        }

        // An unknown acknowledgement state would leave the sender unable to tell whether its
        // bytes arrived, so it is not something to guess at.
        let ack = format!(
            r#"{{"v":2,"t":"file_ack","attachment":"{attachment}","received":1,"state":"maybe"}}"#
        );
        assert!(decode(ack.as_bytes()).is_err());
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
                "v": 2,
                "t": "hello",
                "device_id": "018f2b9c-0000-7000-8000-000000000000",
                "nickname": "n",
                "avatar_seed": "s",
                "listen_port": 1,
                "protocol_version": 2,
            });
            value.as_object_mut().expect("object").remove(missing);
            let encoded = serde_json::to_vec(&value).expect("serialise");
            assert!(decode(&encoded).is_err(), "`{missing}` was not required");
        }
    }
}
