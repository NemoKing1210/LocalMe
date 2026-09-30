//! Storage port.
//!
//! The session actor is generic over this trait, so the database is an implementation detail
//! it never sees. Every method is a complete operation rather than a query builder: the
//! store owns its schema, and the service layer states *what* it needs, not *how* to get it.
//!
//! Two invariants the implementations must hold, because the service layer relies on them:
//!
//! * `insert_message` is idempotent on the message identifier, and reports whether the row
//!   was new. This is the deduplication mechanism for a retransmitted frame.
//! * `forget_peer` writes the row as forgotten rather than deleting it when history is kept,
//!   so the settings screen can still list the device.

use crate::domain::ids::{AvatarSeed, DeviceId, MessageId};
use crate::domain::message::{ChatMessage, MessageStatus};
use crate::domain::nickname::Nickname;
use crate::domain::peer::PeerProfile;
use crate::error::StorageError;

/// Position in a conversation's history, newest first.
///
/// Cursor paging rather than `OFFSET`: the chat list stays correct while new messages
/// arrive, which an offset cannot promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryCursor {
    /// Timestamp of the last row already returned.
    pub sent_at_ms: i64,
    /// Identifier of the last row already returned, breaking timestamp ties.
    pub id: MessageId,
}

// Metadata keys, so the runtime and the session agree on where the durable identity lives.

/// Key holding the persistent device identifier.
pub const META_DEVICE_ID: &str = "device_id";
/// Key holding this device's nickname.
pub const META_NICKNAME: &str = "nickname";

/// A stored peer as defined by the schema, without any live state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPeer {
    /// Public identity.
    pub profile: PeerProfile,
    /// Last address that successfully carried a connection, if any.
    pub last_address: Option<String>,
    /// When this device last saw the peer online, on the local clock.
    pub last_seen_ms: Option<i64>,
    /// When this device first saw the peer, on the local clock.
    pub first_seen_ms: i64,
    /// Unread incoming messages.
    pub unread: u32,
    /// Whether notifications from this peer are suppressed.
    pub notify_muted: bool,
    /// Whether the user forgot this device. Forgotten peers stay out of the user list until
    /// they are seen again.
    pub forgotten: bool,
    /// Newest of `last_seen_ms` and the last message timestamp, computed by the query.
    pub last_activity_ms: Option<i64>,
}

impl StoredPeer {
    /// Whether the peer should appear in the user list.
    #[must_use]
    pub const fn is_listed(&self) -> bool {
        !self.forgotten
    }
}

/// A device known to this installation, for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDevice {
    /// The device identifier.
    pub device_id: DeviceId,
    /// Last known nickname.
    pub nickname: Nickname,
    /// Avatar seed as last announced.
    pub avatar_seed: AvatarSeed,
    /// Whether the user has forgotten this device.
    pub forgotten: bool,
    /// When it was first seen, on the local clock.
    pub first_seen_ms: i64,
    /// When it was last seen, on the local clock.
    pub last_seen_ms: Option<i64>,
    /// How many messages are stored for the conversation with it.
    pub message_count: u64,
}

/// Everything the application needs to persist.
pub trait Store: Send + Sync + 'static {
    /// Reads a metadata value.
    fn meta_get(
        &self,
        key: &str,
    ) -> impl Future<Output = Result<Option<String>, StorageError>> + Send;

    /// Writes a metadata value.
    fn meta_set(
        &self,
        key: &str,
        value: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Inserts or updates a peer's announced identity.
    ///
    /// Records the address and clears the forgotten flag: a device that appears again is a
    /// known device again, which is exactly what "if it shows up on the network it is
    /// discovered as new" means in practice.
    ///
    /// `seen_at_ms` is evidence of *liveness*, not of discovery, so it is optional:
    ///
    /// * `Some(ms)` sets `last_seen_ms` to `max(existing, ms)`, so a late observation never
    ///   moves it backwards;
    /// * `None` leaves `last_seen_ms` exactly as it is (NULL on a fresh row), because a peer
    ///   we can discover but cannot connect to has never been online.
    ///
    /// `first_seen_ms` is set once, on the first insert.
    fn upsert_peer_seen(
        &self,
        profile: &PeerProfile,
        address: Option<&str>,
        seen_at_ms: Option<i64>,
    ) -> impl Future<Output = Result<StoredPeer, StorageError>> + Send;

    /// Reads one peer.
    fn peer(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<Option<StoredPeer>, StorageError>> + Send;

    /// Reads every peer ever seen, forgotten ones included.
    fn peers(&self) -> impl Future<Output = Result<Vec<StoredPeer>, StorageError>> + Send;

    /// Sets whether this peer's notifications are suppressed.
    fn set_peer_muted(
        &self,
        device_id: DeviceId,
        muted: bool,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Records that the peer is online, updating `last_seen`.
    fn touch_peer_seen(
        &self,
        device_id: DeviceId,
        seen_at_ms: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Marks every incoming message from this peer as read and clears its unread count.
    ///
    /// Returns how many messages changed, so the UI can skip a redraw when nothing did.
    fn mark_peer_read(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<u32, StorageError>> + Send;

    /// Forgets a peer.
    ///
    /// With `delete_history` the conversation is removed; without it the row is kept and
    /// flagged, so the device still shows in the settings list.
    fn forget_peer(
        &self,
        device_id: DeviceId,
        delete_history: bool,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Lists every known device, newest activity first, for the settings screen.
    fn known_devices(&self) -> impl Future<Output = Result<Vec<KnownDevice>, StorageError>> + Send;

    /// Stores a message.
    ///
    /// Returns `true` when the row was inserted and `false` when a message with the same
    /// identifier already existed. A duplicate is not an error: it is how a retransmission
    /// is absorbed.
    fn insert_message(
        &self,
        message: &ChatMessage,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send;

    /// Updates the delivery status of a message we sent.
    fn set_message_status(
        &self,
        id: MessageId,
        status: MessageStatus,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Marks every outgoing message still in `sending`/`sent` as `failed`.
    ///
    /// Called when a peer goes offline: a message that was queued for a socket that no longer
    /// exists must not keep claiming it is on its way.
    fn fail_pending_messages(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<u32, StorageError>> + Send;

    /// Reads one page of a conversation, newest first.
    ///
    /// `before` is the cursor: only messages strictly older than it are returned.
    fn history_page(
        &self,
        device_id: DeviceId,
        before: Option<HistoryCursor>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<ChatMessage>, StorageError>> + Send;

    /// Deletes every message on this computer, keeping the peer list.
    fn clear_history(&self) -> impl Future<Output = Result<u64, StorageError>> + Send;
}
