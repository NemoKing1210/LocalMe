//! Events the core emits for the host application to forward to the interface.
//!
//! The set is deliberately small and coarse. A peer list on a local network is bounded by the
//! number of devices in the building — tens, not thousands — so emitting the arranged list
//! whenever it changes is simpler for the consumer than a stream of deltas, and it cannot
//! drift out of sync. Message traffic, which is unbounded, is emitted per message.

use crate::domain::ids::{AvatarSeed, DeviceId, MessageId};
use crate::domain::message::{ChatMessage, MessageStatus};
use crate::domain::nickname::Nickname;
use crate::domain::peer::PeerView;

/// Something the host application should know about.
///
/// Serialised with camelCase field names, because these payloads *are* the interface's API:
/// the TypeScript declarations in `src/ipc` are written to match this derivation, and a field
/// renamed here breaks the front-end build.
///
/// `untagged`, so the payload of an event is the variant's *content* — `{"peers":[…]}` for
/// [`CoreEvent::Peers`], `null` for [`CoreEvent::Stopped`] — rather than serde's default
/// externally-tagged `{"peers":{"peers":[…]}}`. The variant's identity is carried by
/// [`CoreEvent::name`], which is the Tauri event name; repeating it inside the payload gives
/// every subscription two shapes to reconcile, and the outer key does not even agree with the
/// event name for `MessageStatus` (camelCased variant, snake_cased event). The declarations in
/// `src/ipc/types.ts` are the contract, and this is the derivation that matches them.
///
/// `rename_all_fields` rather than `rename_all`: with untagged variants there are no variant
/// names left to rename, and it is the *fields* — `avatar_seed`, and every field added later —
/// that the interface reads by their camelCase names.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all_fields = "camelCase", untagged)]
pub enum CoreEvent {
    /// The user list changed: presence, unread counts, nicknames, or its membership.
    ///
    /// Already filtered, searched and ordered as the interface should show it.
    Peers {
        /// The list, ready to render.
        peers: Vec<PeerView>,
    },
    /// A message was stored, incoming or outgoing.
    ///
    /// Carries the peer's row as well as the message, because every consumer of this event
    /// needs both: the interface to label the notification and to move the conversation up the
    /// list, the notification layer to know the sender's name and whether they are muted.
    Message {
        /// The conversation's peer, as the list shows it.
        peer: PeerView,
        /// The stored row, including its final status.
        message: ChatMessage,
    },
    /// The delivery status of a message we sent changed.
    MessageStatus {
        /// The conversation it belongs to.
        peer: DeviceId,
        /// The message.
        id: MessageId,
        /// The new status.
        status: MessageStatus,
    },
    /// This device's own nickname changed.
    OwnProfile {
        /// The new nickname.
        nickname: Nickname,
        /// The avatar seed derived from it.
        avatar_seed: AvatarSeed,
    },
    /// Something the user should be told about, but which is not an error of theirs.
    Notice {
        /// How serious it is, for the interface to choose a presentation.
        level: NoticeLevel,
        /// What happened.
        message: String,
    },
    /// The core has stopped and nothing more will arrive.
    Stopped,
}

/// Severity of a [`CoreEvent::Notice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeLevel {
    /// Worth knowing, not a problem.
    Info,
    /// Something is degraded but the application works.
    Warning,
    /// Something failed.
    Error,
}

impl CoreEvent {
    /// A short name, used as the Tauri event name by the host.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Peers { .. } => "peers",
            Self::Message { .. } => "message",
            Self::MessageStatus { .. } => "message_status",
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

    /// A [`PeerView`] with nothing interesting in it, for the payloads that carry a peer row.
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

    /// The payload as the host emits it: the event's own name, and its serialised content.
    fn emitted(event: &CoreEvent) -> (String, serde_json::Value) {
        (
            event.name().to_owned(),
            serde_json::to_value(event).expect("serialises"),
        )
    }

    #[test]
    fn an_event_carries_its_fields_and_not_a_second_copy_of_its_name() {
        // This shape *is* the interface's API: `src/ipc/types.ts` declares the payload without
        // the variant wrapper, so an outside-tagged representation here would arrive at the
        // front end as `{ "peers": { "peers": [...] } }` and the subscriptions would read
        // `undefined` from every field.
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
        });

        assert_eq!(name, "message_status");
        assert_eq!(payload["peer"], json!(peer.to_string()));
        assert_eq!(payload["id"], json!(id.to_string()));
        assert_eq!(payload["status"], json!("delivered"));
        assert_eq!(payload.as_object().map(serde_json::Map::len), Some(3));
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
            body: MessageBody::parse("hallo").expect("valid body"),
            sent_at: UnixMillis(1_790_000_000_000),
            received_at: UnixMillis(1_790_000_000_001),
            status: MessageStatus::Received,
            read: false,
        };

        let (name, payload) = emitted(&CoreEvent::Message {
            peer: peer_view(peer),
            message,
        });

        assert_eq!(name, "message");
        // The keys the front end reads: `payload.peer` and `payload.message`, with the camelCase
        // fields `src/ipc/types.ts` declares for both rows.
        let message = &payload["message"];
        assert_eq!(message["id"].as_str().map(str::len), Some(36));
        assert_eq!(message["sentAt"], json!(1_790_000_000_000_i64));
        assert_eq!(message["receivedAt"], json!(1_790_000_000_001_i64));
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
            CoreEvent::Notice {
                level: NoticeLevel::Info,
                message: String::new(),
            },
            CoreEvent::Stopped,
        ]
        .iter()
        .map(CoreEvent::name)
        .collect();
        assert_eq!(names, vec!["peers", "notice", "stopped"]);
    }
}
