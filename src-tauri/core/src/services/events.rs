//! Events the core emits for the host application to forward to the interface.

use crate::domain::attachment::Attachment;
use crate::domain::clock::UnixMillis;
use crate::domain::ids::{AvatarSeed, DeviceId, MessageId};
use crate::domain::message::{ChatMessage, MessageStatus};
use crate::domain::nickname::Nickname;
use crate::domain::peer::PeerView;

/// Serialised untagged with camelCase fields: this shape is the interface's API, matched by
/// `src/ipc/types.ts`, so a field renamed here breaks the front-end build. `untagged` puts the
/// variant's content at the top level instead of wrapping it in the variant name, which
/// [`CoreEvent::name`] already supplies as the Tauri event name.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all_fields = "camelCase", untagged)]
pub enum CoreEvent {
    /// The user list already filtered, searched and ordered as the interface should show it.
    Peers {
        peers: Vec<PeerView>,
    },
    /// A stored message. Carries the peer's row too, because the interface needs it to label a
    /// notification and move the conversation up the list.
    Message {
        peer: PeerView,
        message: ChatMessage,
    },
    MessageStatus {
        peer: DeviceId,
        id: MessageId,
        status: MessageStatus,
        /// The second date the interface prints for a message that waited in the outbox; `None`
        /// while it is queued or in flight.
        delivered_at: Option<UnixMillis>,
    },
    /// An attachment moved: progress, completion, or a failure worth showing.
    ///
    /// The whole row travels rather than a delta, so the interface has one shape to apply and a
    /// dropped event costs a repaint rather than a wrong bar.
    Attachment {
        peer: DeviceId,
        attachment: Attachment,
    },
    OwnProfile {
        nickname: Nickname,
        avatar_seed: AvatarSeed,
    },
    Notice {
        level: NoticeLevel,
        message: String,
    },
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

impl CoreEvent {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Peers { .. } => "peers",
            Self::Message { .. } => "message",
            Self::MessageStatus { .. } => "message_status",
            Self::Attachment { .. } => "attachment",
            Self::OwnProfile { .. } => "own_profile",
            Self::Notice { .. } => "notice",
            Self::Stopped => "stopped",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::clock::UnixMillis;
    use crate::domain::message::{Direction, MessageBody};
    use serde_json::json;

    fn peer_view(device_id: DeviceId) -> PeerView {
        PeerView {
            device_id,
            nickname: Nickname::parse("Nemo").expect("valid nickname"),
            avatar_seed: AvatarSeed::parse("seed").expect("valid seed"),
            online: true,
            last_seen_ms: None,
            unread: 0,
            notify_muted: false,
            last_activity_ms: None,
            last_message: None,
        }
    }

    fn emitted(event: &CoreEvent) -> (String, serde_json::Value) {
        (
            event.name().to_owned(),
            serde_json::to_value(event).expect("serialises"),
        )
    }

    #[test]
    fn an_event_carries_its_fields_and_not_a_second_copy_of_its_name() {
        // This shape is the interface's API: without the untagged representation the front end
        // would receive `{ "peers": { "peers": [...] } }` and read `undefined` from every field.
        let (name, payload) = emitted(&CoreEvent::Peers { peers: Vec::new() });
        assert_eq!(name, "peers");
        assert_eq!(payload, json!({ "peers": [] }));
        assert!(
            payload.get("peers").is_some(),
            "the field must be at the top level: {payload}"
        );
    }

    #[test]
    fn the_message_status_payload_matches_the_front_end_declaration() {
        let peer = DeviceId::generate();
        let id = MessageId::generate();
        let (name, payload) = emitted(&CoreEvent::MessageStatus {
            peer,
            id,
            status: MessageStatus::Delivered,
            delivered_at: Some(UnixMillis(1_790_000_000_002)),
        });

        assert_eq!(name, "message_status");
        assert_eq!(payload["peer"], json!(peer.to_string()));
        assert_eq!(payload["id"], json!(id.to_string()));
        assert_eq!(payload["status"], json!("delivered"));
        assert_eq!(payload["deliveredAt"], json!(1_790_000_000_002_i64));
        assert_eq!(payload.as_object().map(serde_json::Map::len), Some(4));

        // A row that goes back to the outbox carries no delivery time to unset.
        let (_, payload) = emitted(&CoreEvent::MessageStatus {
            peer,
            id,
            status: MessageStatus::Queued,
            delivered_at: None,
        });
        assert_eq!(payload["status"], json!("queued"));
        assert_eq!(payload["deliveredAt"], json!(null));
    }

    #[test]
    fn a_notice_and_an_own_profile_are_flat_too() {
        let (name, payload) = emitted(&CoreEvent::Notice {
            level: NoticeLevel::Warning,
            message: "careful".to_owned(),
        });
        assert_eq!(name, "notice");
        assert_eq!(payload, json!({ "level": "warning", "message": "careful" }));

        let nickname = Nickname::parse("Nemo").expect("valid nickname");
        let seed = AvatarSeed::parse("seed").expect("valid seed");
        let (name, payload) = emitted(&CoreEvent::OwnProfile {
            nickname,
            avatar_seed: seed,
        });
        assert_eq!(name, "own_profile");
        assert_eq!(payload, json!({ "nickname": "Nemo", "avatarSeed": "seed" }));
    }

    #[test]
    fn the_message_payload_carries_the_row_and_its_peer() {
        let peer = DeviceId::generate();
        let message = ChatMessage {
            id: MessageId::generate(),
            peer,
            direction: Direction::Incoming,
            body: Some(MessageBody::parse("hallo").expect("valid body")),
            attachments: Vec::new(),
            sent_at: UnixMillis(1_790_000_000_000),
            received_at: UnixMillis(1_790_000_000_001),
            delivered_at: None,
            status: MessageStatus::Received,
            read: false,
        };

        let (name, payload) = emitted(&CoreEvent::Message {
            peer: peer_view(peer),
            message,
        });

        assert_eq!(name, "message");
        let message = &payload["message"];
        assert_eq!(message["id"].as_str().map(str::len), Some(36));
        assert_eq!(message["sentAt"], json!(1_790_000_000_000_i64));
        assert_eq!(message["receivedAt"], json!(1_790_000_000_001_i64));
        assert_eq!(message["deliveredAt"], json!(null));
        assert_eq!(message["direction"], json!("incoming"));
        assert_eq!(message["status"], json!("received"));
        assert_eq!(message["read"], json!(false));
        assert_eq!(message["body"], json!("hallo"));
        assert_eq!(payload["peer"]["deviceId"], json!(peer.to_string()));
        assert_eq!(payload["peer"]["notifyMuted"], json!(false));
    }

    #[test]
    fn a_unit_event_is_null_rather_than_a_name_wrapper() {
        let (name, payload) = emitted(&CoreEvent::Stopped);
        assert_eq!(name, "stopped");
        assert_eq!(payload, serde_json::Value::Null);
    }

    #[test]
    fn every_event_name_is_the_one_the_front_end_subscribes_to() {
        // `CoreEventName` in `src/ipc/types.ts` lists these; a rename on either side would
        // otherwise be a subscription that silently never fires.
        let names: Vec<&str> = [
            CoreEvent::Peers { peers: Vec::new() },
            CoreEvent::Attachment {
                peer: DeviceId::generate(),
                attachment: Attachment {
                    id: crate::domain::ids::AttachmentId::generate(),
                    message_id: MessageId::generate(),
                    peer: DeviceId::generate(),
                    direction: Direction::Outgoing,
                    name: crate::domain::attachment::FileName::sanitise("a.bin"),
                    size: 1,
                    kind: crate::domain::attachment::AttachmentKind::File,
                    state: crate::domain::attachment::AttachmentState::Queued,
                    transferred: 0,
                    sha256: None,
                    created_at: UnixMillis(1),
                    path: None,
                },
            },
            CoreEvent::Notice {
                level: NoticeLevel::Info,
                message: String::new(),
            },
            CoreEvent::Stopped,
        ]
        .iter()
        .map(CoreEvent::name)
        .collect();
        assert_eq!(names, vec!["peers", "attachment", "notice", "stopped"]);
    }
}
