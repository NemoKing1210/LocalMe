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
#[derive(Debug, Clone)]
pub enum CoreEvent {
    /// The user list changed: presence, unread counts, nicknames, or its membership.
    ///
    /// Already filtered, searched and ordered as the interface should show it.
    Peers {
        /// The list, ready to render.
        peers: Vec<PeerView>,
    },
    /// A message was stored, incoming or outgoing.
    Message {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
