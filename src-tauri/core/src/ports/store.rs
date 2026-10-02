//! Storage port.
//!
//! Two invariants implementations must hold, because the service layer relies on them:
//!
//! * `insert_message` is idempotent on the message identifier and reports whether the row was
//!   new; this is the deduplication mechanism for a retransmitted frame.
//! * `forget_peer` writes the row as forgotten rather than deleting it when history is kept,
//!   so the settings screen can still list the device.
//! * an outgoing row moves `queued` → `sending` → `delivered`; it is never dropped because a
//!   socket failed, so `next_outbox_message` and `requeue_pending_messages` are the only ways a
//!   waiting message changes state.

use std::path::Path;

use crate::domain::attachment::{Attachment, AttachmentState, Sha256};
use crate::domain::ids::{AttachmentId, AvatarSeed, DeviceId, MessageId};
use crate::domain::message::{ChatMessage, MessagePreview, MessageStatus};
use crate::domain::nickname::Nickname;
use crate::domain::peer::PeerProfile;
use crate::error::StorageError;

/// Position in a conversation's history, newest first.
///
/// Cursor paging rather than `OFFSET`: the chat list stays correct while new messages arrive,
/// which an offset cannot promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryCursor {
    pub sent_at_ms: i64,
    pub id: MessageId,
}

/// Key holding the persistent device identifier.
pub const META_DEVICE_ID: &str = "device_id";
/// Key holding this device's nickname.
pub const META_NICKNAME: &str = "nickname";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPeer {
    pub profile: PeerProfile,
    pub last_address: Option<String>,
    pub last_seen_ms: Option<i64>,
    pub first_seen_ms: i64,
    pub unread: u32,
    pub notify_muted: bool,
    pub forgotten: bool,
    pub last_activity_ms: Option<i64>,
    pub last_message: Option<MessagePreview>,
}

impl StoredPeer {
    #[must_use]
    pub const fn is_listed(&self) -> bool {
        !self.forgotten
    }
}

/// A device known to this installation, for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownDevice {
    pub device_id: DeviceId,
    pub nickname: Nickname,
    pub avatar_seed: AvatarSeed,
    pub forgotten: bool,
    pub first_seen_ms: i64,
    pub last_seen_ms: Option<i64>,
    pub message_count: u64,
}

pub trait Store: Send + Sync + 'static {
    fn meta_get(
        &self,
        key: &str,
    ) -> impl Future<Output = Result<Option<String>, StorageError>> + Send;

    fn meta_set(
        &self,
        key: &str,
        value: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Records the address and clears the forgotten flag: a device that appears again is a
    /// known device again.
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

    fn peer(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<Option<StoredPeer>, StorageError>> + Send;

    fn peers(&self) -> impl Future<Output = Result<Vec<StoredPeer>, StorageError>> + Send;

    fn set_peer_muted(
        &self,
        device_id: DeviceId,
        muted: bool,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    fn touch_peer_seen(
        &self,
        device_id: DeviceId,
        seen_at_ms: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Returns how many messages changed, so the UI can skip a redraw when nothing did.
    fn mark_peer_read(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<u32, StorageError>> + Send;

    /// With `delete_history` the conversation is removed; without it the row is kept and
    /// flagged, so the device still shows in the settings list.
    fn forget_peer(
        &self,
        device_id: DeviceId,
        delete_history: bool,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    fn known_devices(&self) -> impl Future<Output = Result<Vec<KnownDevice>, StorageError>> + Send;

    /// Returns `true` when the row was inserted and `false` when a message with the same
    /// identifier already existed. A duplicate is not an error: it is how a retransmission is
    /// absorbed.
    ///
    /// The message's attachments are written in the same transaction, and each is inserted with
    /// `INSERT OR IGNORE` as well, so a retransmitted `chat` frame leaves the state of a
    /// half-received file exactly as it was.
    fn insert_message(
        &self,
        message: &ChatMessage,
    ) -> impl Future<Output = Result<bool, StorageError>> + Send;

    /// Where attachment files live on this machine. One directory per attachment, named after its
    /// identifier, so a hostile file name never becomes part of a path.
    fn files_root(&self) -> &Path;

    fn attachment(
        &self,
        id: AttachmentId,
    ) -> impl Future<Output = Result<Option<Attachment>, StorageError>> + Send;

    /// Outgoing messages that still have an attachment to deliver, oldest first, each carrying
    /// every one of its attachments.
    ///
    /// This is what a reconnected peer is offered: the metadata may need to be repeated (the
    /// recipient may have been reinstalled) and the stream has to resume wherever the recipient
    /// actually is.
    fn messages_with_unfinished_attachments(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<Vec<ChatMessage>, StorageError>> + Send;

    /// How many bytes are known to have arrived. Written on every acknowledgement, so a crash
    /// loses at most one window of progress.
    fn set_attachment_progress(
        &self,
        id: AttachmentId,
        transferred: u64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    fn set_attachment_state(
        &self,
        id: AttachmentId,
        state: AttachmentState,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Records the whole-file digest before the first chunk leaves, so a resumed transfer does
    /// not have to read the source again.
    fn set_attachment_digest(
        &self,
        id: AttachmentId,
        sha256: Sha256,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    fn set_message_status(
        &self,
        id: MessageId,
        status: MessageStatus,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// Marks an outgoing message delivered and records when, in one write: the interface prints
    /// this as the second date of a message that waited.
    fn mark_message_delivered(
        &self,
        id: MessageId,
        delivered_at_ms: i64,
    ) -> impl Future<Output = Result<(), StorageError>> + Send;

    /// The oldest outgoing message still waiting for its peer, in send order. `None` means the
    /// outbox for that conversation is empty.
    fn next_outbox_message(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<Option<ChatMessage>, StorageError>> + Send;

    /// Returns the messages that were written to a socket but never acknowledged to the outbox,
    /// reporting their identifiers so the interface can redraw them. Called when a peer goes
    /// offline: a message waiting for a socket that no longer exists must keep waiting rather
    /// than be lost or claim to be on its way.
    fn requeue_pending_messages(
        &self,
        device_id: DeviceId,
    ) -> impl Future<Output = Result<Vec<MessageId>, StorageError>> + Send;

    /// The same for every conversation, at startup: a process that died mid-send leaves rows in
    /// `sending`, and they must be retried rather than stuck.
    fn requeue_all_pending(&self) -> impl Future<Output = Result<u32, StorageError>> + Send;

    /// `before` is the cursor: only messages strictly older than it are returned.
    fn history_page(
        &self,
        device_id: DeviceId,
        before: Option<HistoryCursor>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<ChatMessage>, StorageError>> + Send;

    fn clear_history(&self) -> impl Future<Output = Result<u64, StorageError>> + Send;
}
