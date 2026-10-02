//! The session actor: the only place mutable network state lives.
//!
//! One task owns the peer table, presence machines, per-peer links and dial bookkeeping; all
//! changes arrive as messages on three channels and are applied sequentially, so there is no
//! lock anywhere in this file. Storage calls are awaited inline, which is what guarantees an
//! acknowledgement is sent only after the received row is committed.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::domain::attachment::{Attachment, AttachmentMeta, AttachmentState, FileName, Sha256};
use crate::domain::clock::{Clock, UnixMillis};
use crate::domain::ids::{AttachmentId, DeviceId, MessageId};
use crate::domain::message::{ChatMessage, Direction, MessageBody, MessagePreview, MessageStatus};
use crate::domain::nickname::Nickname;
use crate::domain::peer::{Handshake, PeerProfile, PeerView};
use crate::domain::presence::{PresenceChange, PresenceMachine};
use crate::error::CoreError;
use crate::ports::discovery::{DiscoveredPeer, DiscoveryEvent};
use crate::ports::store::{HistoryCursor, KnownDevice, META_NICKNAME, Store, StoredPeer};
use crate::protocol::limits::{
    COMMAND_CHANNEL_CAPACITY, DIAL_RETRY_DELAY, EVENT_CHANNEL_CAPACITY, FILE_ACK_EVERY_BYTES,
    FILE_CHUNK_BYTES, FILE_SEND_BURST_BYTES, FILE_SEND_RATE_PER_SECOND, HEARTBEAT_TIMEOUT,
    MAX_ATTACHMENT_BYTES, MAX_ATTACHMENTS_PER_MESSAGE, MAX_PEERS, OUTBOX_BURST,
    OUTBOX_RATE_PER_SECOND, PRESENCE_TICK_INTERVAL, PROTOCOL_VERSION, SHUTDOWN_DRAIN_TIMEOUT,
    TRANSFER_CHUNKS_PER_TURN, TRANSFER_POLL,
};
use crate::protocol::{FileAckState, FileCancelReason, Frame, GoodbyeReason, TokenBucket};
use crate::services::attachment::{self, IncomingTransfer, OutgoingTransfer};
use crate::transport::connection::{self, ConnectionContext};
use crate::transport::{DisconnectReason, PeerLink, Role, TransportEvent, is_preferred};

use super::events::{CoreEvent, NoticeLevel};

/// How many dials may be in flight at once. Without a cap, discovering a network full of
/// devices that do not answer would open a socket per peer at the same instant; eight connects
/// a room's worth of peers per round trip while keeping the burst bounded.
const MAX_CONCURRENT_DIALS: usize = 8;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub own: PeerProfile,
    /// Announced in the handshake so peers can dial back.
    pub listen_port: u16,
    pub max_peers: usize,
}

impl SessionConfig {
    #[must_use]
    pub fn new(own: PeerProfile, listen_port: u16) -> Self {
        Self {
            own,
            listen_port,
            max_peers: MAX_PEERS,
        }
    }

    #[must_use]
    pub fn handshake(&self) -> Handshake {
        Handshake::new(PROTOCOL_VERSION, &self.own, self.listen_port)
    }
}

#[derive(Debug)]
pub enum SessionCommand {
    ListPeers {
        reply: oneshot::Sender<Vec<PeerView>>,
    },
    GetOwnProfile {
        reply: oneshot::Sender<PeerProfile>,
    },
    SetNickname {
        nickname: Nickname,
        reply: oneshot::Sender<PeerProfile>,
    },
    SendMessage {
        peer: DeviceId,
        text: Option<MessageBody>,
        files: Vec<PathBuf>,
        reply: oneshot::Sender<Result<ChatMessage, CoreError>>,
    },
    /// One attachment row, for the host's open/reveal/save commands.
    Attachment {
        id: AttachmentId,
        reply: oneshot::Sender<Option<Attachment>>,
    },
    CancelAttachment {
        id: AttachmentId,
        reply: oneshot::Sender<()>,
    },
    RetryAttachment {
        id: AttachmentId,
        reply: oneshot::Sender<()>,
    },
    History {
        peer: DeviceId,
        before: Option<HistoryCursor>,
        limit: u32,
        reply: oneshot::Sender<Vec<ChatMessage>>,
    },
    MarkRead {
        peer: DeviceId,
        reply: oneshot::Sender<u32>,
    },
    Forget {
        peer: DeviceId,
        delete_history: bool,
        reply: oneshot::Sender<()>,
    },
    Restore {
        peer: DeviceId,
        reply: oneshot::Sender<()>,
    },
    SetMuted {
        peer: DeviceId,
        muted: bool,
        reply: oneshot::Sender<()>,
    },
    KnownDevices {
        reply: oneshot::Sender<Vec<KnownDevice>>,
    },
    ClearHistory {
        reply: oneshot::Sender<u64>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

/// Cheap to clone; every clone talks to the same actor, which is the only way to reach the
/// session.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    commands: mpsc::Sender<SessionCommand>,
    events: broadcast::Sender<CoreEvent>,
}

impl SessionHandle {
    /// A receiver that falls behind loses events rather than blocking the session. That is safe
    /// because every event describes state the host can also read back through this handle: a
    /// lost event costs a repaint, not correctness.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.commands.is_closed()
    }

    async fn request<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<T>) -> SessionCommand,
    ) -> Result<T, CoreError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(build(reply))
            .await
            .map_err(|_| CoreError::ShuttingDown)?;
        receiver.await.map_err(|_| CoreError::ShuttingDown)
    }

    pub async fn list_peers(&self) -> Result<Vec<PeerView>, CoreError> {
        self.request(|reply| SessionCommand::ListPeers { reply })
            .await
    }

    pub async fn own_profile(&self) -> Result<PeerProfile, CoreError> {
        self.request(|reply| SessionCommand::GetOwnProfile { reply })
            .await
    }

    pub async fn set_nickname(&self, nickname: Nickname) -> Result<PeerProfile, CoreError> {
        self.request(|reply| SessionCommand::SetNickname { nickname, reply })
            .await
    }

    /// Stores the message and hands it to the peer if it is reachable. A peer that is offline
    /// is not an error: the row stays in the outbox and is sent, in order, once the peer is back.
    ///
    /// `files` are paths on *this* machine; the core stats each one, so a caller cannot announce
    /// a size it has not checked.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownPeer`] if the device is not in the list, [`CoreError::Storage`] if the
    /// message could not be written down at all, and [`CoreError::Domain`] if the message carries
    /// neither text nor a usable file.
    pub async fn send_message(
        &self,
        peer: DeviceId,
        text: Option<MessageBody>,
        files: Vec<PathBuf>,
    ) -> Result<ChatMessage, CoreError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(SessionCommand::SendMessage {
                peer,
                text,
                files,
                reply,
            })
            .await
            .map_err(|_| CoreError::ShuttingDown)?;
        receiver.await.map_err(|_| CoreError::ShuttingDown)?
    }

    pub async fn attachment(&self, id: AttachmentId) -> Result<Option<Attachment>, CoreError> {
        self.request(|reply| SessionCommand::Attachment { id, reply })
            .await
    }

    pub async fn cancel_attachment(&self, id: AttachmentId) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::CancelAttachment { id, reply })
            .await
    }

    pub async fn retry_attachment(&self, id: AttachmentId) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::RetryAttachment { id, reply })
            .await
    }

    pub async fn history(
        &self,
        peer: DeviceId,
        before: Option<HistoryCursor>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>, CoreError> {
        self.request(|reply| SessionCommand::History {
            peer,
            before,
            limit,
            reply,
        })
        .await
    }

    pub async fn mark_read(&self, peer: DeviceId) -> Result<u32, CoreError> {
        self.request(|reply| SessionCommand::MarkRead { peer, reply })
            .await
    }

    pub async fn forget(&self, peer: DeviceId, delete_history: bool) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::Forget {
            peer,
            delete_history,
            reply,
        })
        .await
    }

    pub async fn restore(&self, peer: DeviceId) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::Restore { peer, reply })
            .await
    }

    pub async fn set_muted(&self, peer: DeviceId, muted: bool) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::SetMuted { peer, muted, reply })
            .await
    }

    pub async fn known_devices(&self) -> Result<Vec<KnownDevice>, CoreError> {
        self.request(|reply| SessionCommand::KnownDevices { reply })
            .await
    }

    pub async fn clear_history(&self) -> Result<u64, CoreError> {
        self.request(|reply| SessionCommand::ClearHistory { reply })
            .await
    }

    pub async fn shutdown(&self) -> Result<(), CoreError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(SessionCommand::Shutdown { reply })
            .await
            .map_err(|_| CoreError::ShuttingDown)?;
        receiver.await.map_err(|_| CoreError::ShuttingDown)
    }
}

#[derive(Debug)]
pub struct SessionRuntime {
    pub handle: SessionHandle,
    pub transport: mpsc::Sender<TransportEvent>,
    pub discovery: mpsc::Sender<DiscoveryEvent>,
    pub connection: Arc<ConnectionContext>,
    pub task: JoinHandle<()>,
}

struct PeerEntry {
    profile: PeerProfile,
    stored: StoredPeer,
    addresses: Vec<SocketAddr>,
    presence: PresenceMachine,
    link: Option<PeerLink>,
    dial: Option<JoinHandle<()>>,
    last_dial_attempt: Option<Instant>,
    /// Newest of `last_seen` and the last message, for the list ordering.
    last_activity_ms: Option<i64>,
    /// The outbox drain's own budget, so a backlog does not outrun the recipient's rate limit.
    send_budget: TokenBucket,
    /// Set while this conversation may still have messages waiting to be written out. It keeps
    /// the presence tick from querying the database for peers with nothing in their outbox.
    outbox_pending: bool,
    /// The file being sent to this peer right now. One at a time per peer: the disk is the
    /// bottleneck, and a second stream would only make both slower without changing anything a
    /// user can see.
    transfer: Option<OutgoingTransfer>,
    /// Files arriving from this peer, by identifier. A transfer that is interrupted is dropped
    /// here and picked up again from the part file on disk.
    incoming: HashMap<AttachmentId, IncomingTransfer>,
    /// The file budget: the pace is ours, the limit is the recipient's.
    file_budget: TokenBucket,
}

impl PeerEntry {
    fn new(stored: StoredPeer) -> Self {
        Self {
            profile: stored.profile.clone(),
            last_activity_ms: stored.last_activity_ms,
            stored,
            addresses: Vec::new(),
            presence: PresenceMachine::new(),
            link: None,
            dial: None,
            last_dial_attempt: None,
            send_budget: TokenBucket::new(OUTBOX_BURST, OUTBOX_RATE_PER_SECOND, Instant::now()),
            outbox_pending: false,
            transfer: None,
            incoming: HashMap::new(),
            file_budget: TokenBucket::new(
                FILE_SEND_BURST_BYTES,
                FILE_SEND_RATE_PER_SECOND,
                Instant::now(),
            ),
        }
    }

    fn view(&self) -> PeerView {
        PeerView {
            device_id: self.profile.device_id,
            nickname: self.profile.nickname.clone(),
            avatar_seed: self.profile.avatar_seed.clone(),
            online: self.presence.is_online(),
            last_seen_ms: self.stored.last_seen_ms,
            unread: self.stored.unread,
            notify_muted: self.stored.notify_muted,
            last_activity_ms: self.last_activity_ms,
            last_message: self.stored.last_message.clone(),
        }
    }

    fn note_activity(&mut self, at_ms: i64) {
        self.last_activity_ms = Some(match self.last_activity_ms {
            Some(current) => current.max(at_ms),
            None => at_ms,
        });
    }

    /// Opens, or adopts, the part file for an incoming attachment.
    ///
    /// Adoption is the whole point: a part file left by a disconnect or a crash is the most
    /// precise statement of how much arrived, and a second transfer for the same attachment must
    /// continue it rather than replace it. `Ok` also covers a transfer that is already open.
    fn ensure_incoming(&mut self, attachment: &Attachment, files_root: &Path) -> io::Result<()> {
        match self.incoming.entry(attachment.id) {
            Entry::Occupied(_) => Ok(()),
            Entry::Vacant(slot) => {
                slot.insert(IncomingTransfer::open(attachment, files_root)?);
                Ok(())
            }
        }
    }

    fn dial_order(&self) -> Vec<SocketAddr> {
        let Some(remembered) = self.stored.last_address.as_deref() else {
            return self.addresses.clone();
        };
        let mut ordered: Vec<SocketAddr> = self
            .addresses
            .iter()
            .filter(|address| address.to_string() == remembered)
            .copied()
            .collect();
        ordered.extend(
            self.addresses
                .iter()
                .filter(|address| address.to_string() != remembered)
                .copied(),
        );
        ordered
    }
}

/// Results of the two things a transfer cannot do on the actor: read a whole file to digest it,
/// and read a whole file to check it.
///
/// They arrive on their own channel so the actor keeps serving messages, heartbeats and the
/// store while a half-gigabyte is being read on a blocking task.
#[derive(Debug)]
enum Prep {
    /// The sender's digest is ready, so the transfer can start (or fail, if the source is gone).
    SenderDigest {
        peer: DeviceId,
        attachment: AttachmentId,
        digest: Result<Sha256, String>,
    },
    /// The recipient has assembled the whole file; the digest decides whether it is kept.
    ReceiverDigest {
        peer: DeviceId,
        attachment: AttachmentId,
        digest: Result<Sha256, String>,
    },
}

pub async fn spawn<C: Clock, S: Store>(
    config: SessionConfig,
    clock: C,
    store: S,
) -> Result<SessionRuntime, CoreError> {
    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (transport_tx, transport_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (discovery_tx, discovery_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
    let (prep_tx, prep_rx) = mpsc::channel(4);

    let connection = Arc::new(ConnectionContext::new(
        transport_tx.clone(),
        config.max_peers,
    ));

    let mut session = Session {
        handshake: config.handshake(),
        config,
        clock,
        store,
        events: events_tx.clone(),
        transport: transport_tx.clone(),
        connection: connection.clone(),
        prep: prep_tx,
        peers: HashMap::new(),
        dial_queue: Vec::new(),
        active_dials: 0,
    };
    session.load_peers().await?;
    session.requeue_in_flight().await;

    let handle = SessionHandle {
        commands: commands_tx,
        events: events_tx,
    };
    let task = tokio::spawn(async move {
        session
            .run(commands_rx, transport_rx, discovery_rx, prep_rx)
            .await;
    });

    Ok(SessionRuntime {
        handle,
        transport: transport_tx,
        discovery: discovery_tx,
        connection,
        task,
    })
}

struct Session<C: Clock, S: Store> {
    config: SessionConfig,
    handshake: Handshake,
    clock: C,
    store: S,
    events: broadcast::Sender<CoreEvent>,
    transport: mpsc::Sender<TransportEvent>,
    connection: Arc<ConnectionContext>,
    /// See [`Prep`].
    prep: mpsc::Sender<Prep>,
    peers: HashMap<DeviceId, PeerEntry>,
    dial_queue: Vec<DeviceId>,
    active_dials: usize,
}

impl<C: Clock, S: Store> Session<C, S> {
    async fn load_peers(&mut self) -> Result<(), CoreError> {
        let stored = self.store.peers().await?;
        for peer in stored {
            self.peers
                .insert(peer.profile.device_id, PeerEntry::new(peer));
        }
        tracing::debug!(peers = self.peers.len(), "loaded the stored peer list");
        self.emit_peers();
        Ok(())
    }

    async fn run(
        mut self,
        mut commands: mpsc::Receiver<SessionCommand>,
        mut transport: mpsc::Receiver<TransportEvent>,
        mut discovery: mpsc::Receiver<DiscoveryEvent>,
        mut prep: mpsc::Receiver<Prep>,
    ) {
        let mut ticker = tokio::time::interval(PRESENCE_TICK_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;

        loop {
            // Recomputed every turn: a transfer schedules its own next chunk, so the loop sleeps
            // exactly as long as the pacing asks and no longer. With nothing in flight the branch
            // below is disabled and the deadline is never used.
            let deadline = self.transfer_deadline();

            tokio::select! {
                command = commands.recv() => {
                    match command {
                        Some(SessionCommand::Shutdown { reply }) => {
                            self.shutdown().await;
                            let _ = reply.send(());
                            break;
                        }
                        Some(command) => self.handle_command(command).await,
                        None => {
                            self.shutdown().await;
                            break;
                        }
                    }
                }

                event = transport.recv() => {
                    match event {
                        Some(event) => self.handle_transport(event).await,
                        None => break,
                    }
                }

                event = discovery.recv() => {
                    match event {
                        Some(event) => self.handle_discovery(event).await,
                        None => break,
                    }
                }

                prepared = prep.recv() => {
                    match prepared {
                        Some(prepared) => self.on_prepared(prepared).await,
                        None => break,
                    }
                }

                _ = tokio::time::sleep_until(deadline.unwrap_or_else(far_future)), if deadline.is_some() => {
                    self.pump_transfers().await;
                }

                _ = ticker.tick() => self.tick().await,
            }
        }

        let _ = self.events.send(CoreEvent::Stopped);
    }

    async fn handle_command(&mut self, command: SessionCommand) {
        match command {
            SessionCommand::ListPeers { reply } => {
                let _ = reply.send(self.arranged_peers());
            }

            SessionCommand::GetOwnProfile { reply } => {
                let _ = reply.send(self.config.own.clone());
            }

            SessionCommand::SetNickname { nickname, reply } => {
                self.rename(nickname).await;
                let _ = reply.send(self.config.own.clone());
            }

            SessionCommand::SendMessage {
                peer,
                text,
                files,
                reply,
            } => {
                let _ = reply.send(self.send_message(peer, text, files).await);
            }

            SessionCommand::Attachment { id, reply } => {
                let attachment = match self.store.attachment(id).await {
                    Ok(attachment) => attachment,
                    Err(error) => {
                        tracing::warn!(%error, "failed to read an attachment");
                        None
                    }
                };
                let _ = reply.send(attachment);
            }

            SessionCommand::CancelAttachment { id, reply } => {
                self.cancel_attachment(id).await;
                let _ = reply.send(());
            }

            SessionCommand::RetryAttachment { id, reply } => {
                self.retry_attachment(id).await;
                let _ = reply.send(());
            }

            SessionCommand::History {
                peer,
                before,
                limit,
                reply,
            } => {
                let page = match self.store.history_page(peer, before, limit).await {
                    Ok(page) => page,
                    Err(error) => {
                        self.notice(
                            NoticeLevel::Error,
                            format!("could not read the conversation: {error}"),
                        );
                        Vec::new()
                    }
                };
                let _ = reply.send(page);
            }

            SessionCommand::MarkRead { peer, reply } => {
                let changed = match self.store.mark_peer_read(peer).await {
                    Ok(changed) => changed,
                    Err(error) => {
                        tracing::warn!(%error, "failed to mark the conversation as read");
                        0
                    }
                };
                if changed > 0
                    && let Some(entry) = self.peers.get_mut(&peer)
                {
                    entry.stored.unread = 0;
                    self.emit_peers();
                }
                let _ = reply.send(changed);
            }

            SessionCommand::Forget {
                peer,
                delete_history,
                reply,
            } => {
                if let Some(mut entry) = self.peers.remove(&peer) {
                    if let Some(link) = entry.link.take() {
                        link.close(GoodbyeReason::Shutdown);
                    }
                    if let Some(dial) = entry.dial.take() {
                        dial.abort();
                    }
                    // A transfer belongs to the conversation that is going away: the handles are
                    // dropped (which flushes what has been written) and the files stay where they
                    // are, because the rows that name them are only deleted when the history is.
                    entry.transfer = None;
                    entry.incoming.clear();
                    // The stored row stays, so the settings screen can still list and restore
                    // the device: that is what makes "forgotten" different from "never seen".
                    if let Err(error) = self.store.forget_peer(peer, delete_history).await {
                        tracing::warn!(%error, "failed to forget the peer");
                    }
                    self.emit_peers();
                }
                let _ = reply.send(());
            }

            SessionCommand::Restore { peer, reply } => {
                self.restore(peer).await;
                let _ = reply.send(());
            }

            SessionCommand::SetMuted { peer, muted, reply } => {
                if let Err(error) = self.store.set_peer_muted(peer, muted).await {
                    tracing::warn!(%error, "failed to store the notification preference");
                }
                if let Some(entry) = self.peers.get_mut(&peer) {
                    entry.stored.notify_muted = muted;
                    self.emit_peers();
                }
                let _ = reply.send(());
            }

            SessionCommand::KnownDevices { reply } => {
                let devices = match self.store.known_devices().await {
                    Ok(devices) => devices,
                    Err(error) => {
                        self.notice(
                            NoticeLevel::Error,
                            format!("could not read the device list: {error}"),
                        );
                        Vec::new()
                    }
                };
                let _ = reply.send(devices);
            }

            SessionCommand::ClearHistory { reply } => {
                let deleted = match self.store.clear_history().await {
                    Ok(deleted) => deleted,
                    Err(error) => {
                        tracing::warn!(%error, "failed to clear the history");
                        0
                    }
                };
                for entry in self.peers.values_mut() {
                    entry.stored.unread = 0;
                    entry.stored.last_message = None;
                    entry.transfer = None;
                    entry.incoming.clear();
                }
                self.emit_peers();
                let _ = reply.send(deleted);
            }

            SessionCommand::Shutdown { reply } => drop(reply),
        }
    }

    async fn rename(&mut self, nickname: Nickname) {
        self.config.own = PeerProfile::new(self.config.own.device_id, nickname);
        self.handshake = self.config.handshake();

        if let Err(error) = self
            .store
            .meta_set(META_NICKNAME, self.config.own.nickname.as_str())
            .await
        {
            tracing::warn!(%error, "failed to persist the new nickname");
        }

        let frame = Frame::Profile {
            nickname: self.config.own.nickname.clone(),
            avatar_seed: self.config.own.avatar_seed.clone(),
        };
        for entry in self.peers.values() {
            if let Some(link) = &entry.link
                && let Err(error) = link.try_send(frame.clone())
            {
                tracing::debug!(peer = %entry.profile.device_id, %error, "rename not delivered");
            }
        }

        let _ = self.events.send(CoreEvent::OwnProfile {
            nickname: self.config.own.nickname.clone(),
            avatar_seed: self.config.own.avatar_seed.clone(),
        });
    }

    /// Writes the message down first, then tries to hand it to the peer. The order matters: a
    /// message is never lost because the socket failed, and a peer that is offline is not an
    /// error at all — the row waits in the outbox and is retried in order when it comes back.
    ///
    /// Every attached path is stat'ed here. The interface cannot announce a size it has not
    /// checked, and a file that is a directory, missing or above the cap is refused before a row
    /// exists rather than failing halfway through a transfer.
    async fn send_message(
        &mut self,
        peer: DeviceId,
        text: Option<MessageBody>,
        files: Vec<PathBuf>,
    ) -> Result<ChatMessage, CoreError> {
        let Some(entry) = self.peers.get(&peer) else {
            return Err(CoreError::UnknownPeer(peer.to_string()));
        };
        if !entry.stored.is_listed() {
            return Err(CoreError::UnknownPeer(peer.to_string()));
        }

        let now = self.clock.wall();
        let id = MessageId::generate();
        let attachments = resolve_files(&peer, id, &files, now)?;
        if text.is_none() && attachments.is_empty() {
            return Err(CoreError::Domain(crate::error::DomainError::EmptyMessage));
        }

        let message = ChatMessage {
            id,
            peer,
            direction: Direction::Outgoing,
            body: text,
            attachments,
            sent_at: now,
            received_at: now,
            delivered_at: None,
            status: MessageStatus::Queued,
            read: true,
        };

        if let Err(error) = self.store.insert_message(&message).await {
            tracing::warn!(%error, "failed to store the outgoing message");
            return Err(CoreError::Storage(error));
        }

        // This send is now the newest row in the conversation, so it is what the list shows.
        if let Some(entry) = self.peers.get_mut(&peer) {
            entry.stored.last_message = Some(MessagePreview::for_message(&message));
            entry.outbox_pending = true;
            entry.note_activity(message.sent_at.as_i64());
        }

        // A connected peer is served right here; otherwise the row waits and the drain resumes
        // on the presence tick. The messages handed off now are reported back so the row the
        // interface receives matches the state the socket actually put it in.
        let handed_off = self.pump_outbox(peer).await;
        let message = if handed_off.contains(&message.id) {
            ChatMessage {
                status: MessageStatus::Sending,
                ..message
            }
        } else {
            message
        };

        self.emit_message(peer, &message);
        self.emit_peers();
        // The metadata is on the socket (or waiting with it); the bytes can follow.
        self.maybe_start_transfer(peer).await;
        Ok(message)
    }

    /// Hands the oldest waiting messages to the peer's socket, oldest first, and reports the
    /// identifiers it handed off.
    ///
    /// The loop is paced by a token bucket rather than by the socket, because the recipient
    /// rate-limits what it accepts and closes the connection when the budget is exceeded: an
    /// outbox written out as fast as the socket allows would be refused by the peer and could
    /// never drain. It also stops as soon as the socket's own queue is full, and the presence
    /// tick resumes it.
    async fn pump_outbox(&mut self, peer: DeviceId) -> Vec<MessageId> {
        let mut handed_off = Vec::new();
        loop {
            let (link, allowed) = {
                let Some(entry) = self.peers.get_mut(&peer) else {
                    return handed_off;
                };
                if !entry.outbox_pending {
                    return handed_off;
                }
                let Some(link) = entry.link.clone() else {
                    // Offline: the rows keep their place until the next connection.
                    return handed_off;
                };
                let allowed = entry.send_budget.try_acquire(Instant::now());
                (link, allowed)
            };
            if !allowed {
                return handed_off;
            }

            let message = match self.store.next_outbox_message(peer).await {
                Ok(Some(message)) => message,
                Ok(None) => {
                    if let Some(entry) = self.peers.get_mut(&peer) {
                        entry.outbox_pending = false;
                    }
                    return handed_off;
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to read the outbox");
                    return handed_off;
                }
            };

            // A full queue or a closed socket leaves the row queued; stopping here is what keeps
            // the conversation in order, because the next attempt starts from the same oldest
            // row.
            if let Err(error) = link.try_send(Frame::Chat {
                id: message.id,
                text: message.body.clone(),
                attachments: message.metas(),
            }) {
                tracing::debug!(%peer, %error, "an outbox message could not be queued");
                return handed_off;
            }

            // The row is only marked in flight once the frame is on its way; if that write
            // fails, the row still reads as queued and a retry would duplicate it, so this is
            // the one place where giving up is safer than repeating.
            if let Err(error) = self
                .store
                .set_message_status(message.id, MessageStatus::Sending)
                .await
            {
                tracing::warn!(%error, "failed to record a message as in flight");
                return handed_off;
            }
            self.emit_status(peer, message.id, MessageStatus::Sending, None);
            handed_off.push(message.id);
        }
    }

    /// A process that stopped between writing a frame and reading its acknowledgement leaves
    /// rows marked in flight; they have to be retried rather than stuck behind a socket that no
    /// longer exists.
    async fn requeue_in_flight(&mut self) {
        match self.store.requeue_all_pending().await {
            Ok(0) => {}
            Ok(count) => tracing::info!(count, "returned in-flight messages to the outbox"),
            Err(error) => tracing::warn!(%error, "failed to recover in-flight messages"),
        }
    }

    async fn restore(&mut self, peer: DeviceId) {
        let stored = match self.store.peer(peer).await {
            Ok(Some(stored)) => stored,
            Ok(None) => {
                tracing::debug!(%peer, "cannot restore a device that was never seen");
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to read the device to restore");
                return;
            }
        };

        // Marking it known again is a decision about the list, not evidence of reachability,
        // so `last_seen` is left untouched.
        if let Err(error) = self
            .store
            .upsert_peer_seen(&stored.profile, stored.last_address.as_deref(), None)
            .await
        {
            tracing::warn!(%error, "failed to restore the peer");
        }

        let entry = self
            .peers
            .entry(peer)
            .or_insert_with(|| PeerEntry::new(stored));
        entry.stored.forgotten = false;
        self.maybe_dial(peer);
        self.emit_peers();
    }

    async fn handle_transport(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::Connected {
                role,
                handshake,
                link,
                remote,
            } => self.on_connected(role, handshake, link, remote).await,
            TransportEvent::Frame { peer, frame } => self.on_frame(peer, frame).await,
            TransportEvent::Disconnected { peer, link, reason } => {
                self.on_disconnected(peer, link, reason).await;
            }
            TransportEvent::DialFailed { peer, reason } => {
                self.active_dials = self.active_dials.saturating_sub(1);
                tracing::debug!(%peer, %reason, "could not reach the peer");
                let Some(entry) = self.peers.get_mut(&peer) else {
                    return;
                };
                entry.dial = None;
                if entry.presence.dial_failed().is_visible() {
                    self.emit_peers();
                }
            }
        }
    }

    async fn on_connected(
        &mut self,
        role: Role,
        handshake: Handshake,
        link: PeerLink,
        remote: SocketAddr,
    ) {
        let peer = handshake.device_id;
        // Only a dial we started consumes a dial slot.
        if role == Role::Dialer {
            self.active_dials = self.active_dials.saturating_sub(1);
        }

        let now = self.clock.wall().as_i64();
        let profile = PeerProfile {
            device_id: handshake.device_id,
            nickname: handshake.nickname.clone(),
            avatar_seed: handshake.avatar_seed.clone(),
        };
        let address = remote.to_string();

        let stored = match self
            .store
            .upsert_peer_seen(&profile, Some(&address), Some(now))
            .await
        {
            Ok(stored) => stored,
            Err(error) => {
                tracing::warn!(%error, "failed to record the connection");
                return;
            }
        };

        let entry = self
            .peers
            .entry(peer)
            .or_insert_with(|| PeerEntry::new(stored.clone()));
        // The handshake is authoritative for the peer's identity, so it replaces whatever
        // discovery announced.
        entry.profile = profile;
        entry.stored = stored;
        entry.stored.forgotten = false;
        entry.note_activity(now);
        entry.dial = None;

        let (dialer, acceptor) = role.endpoints(self.config.own.device_id, peer);
        if !is_preferred(dialer, acceptor) && entry.link.is_some() {
            // We already hold the connection both sides agreed to keep.
            tracing::debug!(%peer, "closing a superseded connection");
            link.close(GoodbyeReason::Superseded);
            self.emit_peers();
            return;
        }

        if let Some(existing) = entry.link.replace(link) {
            // The peer dialled us while we were dialling it: the preferred connection won.
            existing.close(GoodbyeReason::Superseded);
        }
        if entry.presence.connected(Instant::now()).is_visible() {
            tracing::info!(%peer, role = role.as_str(), %address, "peer is online");
        }
        self.emit_peers();

        // The peer may have been away while messages piled up: mark the conversation for the
        // drain and hand over what fits in this burst.
        if let Some(entry) = self.peers.get_mut(&peer) {
            entry.outbox_pending = true;
        }
        let _ = self.pump_outbox(peer).await;
        // Files that were interrupted pick up where the recipient's own file ends.
        self.resume_attachments(peer).await;
    }

    async fn on_frame(&mut self, peer: DeviceId, frame: Frame) {
        match frame {
            Frame::Chat {
                id,
                text,
                attachments,
            } => self.on_chat(peer, id, text, attachments).await,

            Frame::ChatAck { id } => {
                // The sender's own clock, read now, is the second date it prints: the moment it
                // learned the message had been stored on the other side.
                let delivered_at = self.clock.wall();
                if let Err(error) = self
                    .store
                    .mark_message_delivered(id, delivered_at.as_i64())
                    .await
                {
                    tracing::warn!(%error, "failed to record a delivery");
                    return;
                }
                self.emit_status(peer, id, MessageStatus::Delivered, Some(delivered_at));
                // The recipient has the metadata now, so the bytes may follow.
                self.maybe_start_transfer(peer).await;
            }

            Frame::FileChunk {
                attachment,
                offset,
                data,
            } => self.on_file_chunk(peer, attachment, offset, data).await,

            Frame::FileDone { attachment, sha256 } => {
                self.on_file_done(peer, attachment, sha256).await;
            }

            Frame::FileAck {
                attachment,
                received,
                state,
            } => self.on_file_ack(peer, attachment, received, state).await,

            Frame::FileCancel { attachment, reason } => {
                self.on_file_cancel(peer, attachment, reason).await;
            }

            Frame::FileRequest { attachment } => self.on_file_request(peer, attachment).await,

            Frame::Profile {
                nickname,
                avatar_seed,
            } => {
                let Some(entry) = self.peers.get_mut(&peer) else {
                    return;
                };
                entry.profile.nickname = nickname.clone();
                entry.profile.avatar_seed = avatar_seed.clone();
                let profile = entry.profile.clone();
                if let Err(error) = self.store.upsert_peer_seen(&profile, None, None).await {
                    tracing::warn!(%error, "failed to store a peer rename");
                }
                tracing::debug!(%peer, nickname = %nickname, "peer renamed itself");
                self.emit_peers();
            }

            // Liveness. The presence machine measures silence, so a beat has to reach it; the
            // connection task keeps its own timer for the socket, but only the session can say
            // whether the *peer* is still there.
            Frame::Heartbeat { .. } => {
                if let Some(entry) = self.peers.get_mut(&peer)
                    && entry.presence.heartbeat(Instant::now()).is_visible()
                {
                    self.emit_peers();
                }
            }

            Frame::Hello(_) | Frame::Welcome(_) => {
                tracing::warn!(%peer, "ignoring a handshake frame on an established connection");
            }

            // Shutdown and protocol errors are the connection task's business: it closes on
            // them and never forwards them.
            Frame::Goodbye { .. } | Frame::Error { .. } => {}
        }
    }

    /// A `chat` frame: a message and the files it announces.
    ///
    /// The metadata is stored, never trusted: the size is the sender's claim until the file is
    /// on disk and its digest agrees. An offer above the cap is refused by name, which keeps the
    /// message visible (the user can see what someone tried to send) and the connection up.
    async fn on_chat(
        &mut self,
        peer: DeviceId,
        id: MessageId,
        text: Option<MessageBody>,
        metas: Vec<AttachmentMeta>,
    ) {
        let Some(entry) = self.peers.get_mut(&peer) else {
            // A connection always creates the entry first, so this means the peer was
            // forgotten while the conversation was open.
            tracing::debug!(%peer, "dropping a message from an unknown peer");
            return;
        };
        if entry.stored.forgotten {
            return;
        }

        let now = self.clock.wall();
        let mut refused: Vec<AttachmentId> = Vec::new();
        let mut attachments = Vec::with_capacity(metas.len());
        for meta in metas {
            let state = if meta.size > MAX_ATTACHMENT_BYTES {
                refused.push(meta.id);
                AttachmentState::Cancelled
            } else {
                AttachmentState::Receiving
            };
            attachments.push(Attachment {
                id: meta.id,
                message_id: id,
                peer,
                direction: Direction::Incoming,
                name: meta.name,
                size: meta.size,
                kind: meta.kind,
                state,
                transferred: 0,
                sha256: None,
                created_at: now,
                path: None,
            });
        }

        let message = ChatMessage {
            id,
            peer,
            direction: Direction::Incoming,
            body: text,
            attachments,
            // The local clock is the only time this application trusts: using it for both
            // directions keeps a conversation ordered even when a peer's clock is wrong.
            sent_at: now,
            received_at: now,
            delivered_at: None,
            status: MessageStatus::Received,
            read: false,
        };

        match self.store.insert_message(&message).await {
            // A retransmission: acknowledge it again so the sender stops caring, but do not
            // store or count it twice. The attachment rows keep the state they already had,
            // which is what lets a half-received file continue after a reconnect.
            Ok(false) => tracing::debug!(%peer, %id, "ignoring a duplicate message"),
            Ok(true) => {
                entry.stored.unread = entry.stored.unread.saturating_add(1);
                entry.stored.last_message = Some(MessagePreview::for_message(&message));
                entry.note_activity(now.as_i64());
                let peer_view = entry.view();
                let _ = self.events.send(CoreEvent::Message {
                    peer: peer_view,
                    message: message.clone(),
                });
            }
            Err(error) => {
                tracing::warn!(%error, "failed to store a received message");
                return;
            }
        }

        // The acknowledgement goes out only after the row is committed, which is what makes
        // "delivered" on the sender mean "durably stored here".
        if let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) {
            if let Err(error) = link.try_send(Frame::ChatAck { id }) {
                tracing::debug!(%peer, %error, "failed to acknowledge a message");
            }
            for attachment in refused {
                tracing::info!(%peer, %attachment, "refusing an attachment above the size limit");
                if let Err(error) = link.try_send(Frame::FileCancel {
                    attachment,
                    reason: FileCancelReason::TooLarge,
                }) {
                    tracing::debug!(%peer, %error, "failed to refuse an oversized attachment");
                }
            }
        }

        self.emit_peers();
    }

    /// One chunk of a file from `peer`.
    async fn on_file_chunk(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        offset: u64,
        data: Vec<u8>,
    ) {
        let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) else {
            return;
        };
        let files_root = self.store.files_root().to_path_buf();

        let row = match self.store.attachment(id).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                self.refuse(peer, id, FileCancelReason::Unknown);
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to read an attachment");
                return;
            }
        };
        if row.peer != peer || row.direction != Direction::Incoming {
            self.refuse(peer, id, FileCancelReason::Unknown);
            return;
        }
        if row.state == AttachmentState::Complete {
            // A retransmission after the recipient already had the file: say so and stop.
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received: row.size,
                state: FileAckState::Complete,
            });
            return;
        }
        if row.size > MAX_ATTACHMENT_BYTES {
            self.set_attachment_state(peer, id, AttachmentState::Cancelled)
                .await;
            self.refuse(peer, id, FileCancelReason::TooLarge);
            return;
        }

        // A row whose transfer had ended is revived: the sender is trying again, and the fastest
        // way to agree on where to continue is to let it send and correct it.
        let mut attachment = row;
        if attachment.state.is_finished() {
            if let Some(entry) = self.peers.get_mut(&peer) {
                entry.incoming.remove(&id);
            }
            attachment.state = AttachmentState::Receiving;
            attachment.transferred = 0;
            if self
                .store
                .set_attachment_state(id, AttachmentState::Receiving)
                .await
                .is_err()
            {
                return;
            }
            let _ = self.store.set_attachment_progress(id, 0).await;
            self.emit_attachment(peer, attachment.clone());
        }

        let opened = self
            .peers
            .get_mut(&peer)
            .map(|entry| entry.ensure_incoming(&attachment, &files_root));
        match opened {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                tracing::warn!(%error, %id, "could not open the destination file");
                self.fail_incoming(peer, id, FileCancelReason::Failed).await;
                return;
            }
            None => return,
        }

        let entry = match self.peers.get_mut(&peer) {
            Some(entry) => entry,
            None => return,
        };
        let Some(transfer) = entry.incoming.get_mut(&id) else {
            return;
        };
        let too_much = offset.saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX))
            > transfer.attachment.size;
        if too_much {
            let received = transfer.received;
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received,
                state: FileAckState::Receiving,
            });
            return;
        }

        match transfer.append(offset, &data) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {
                // Not the next chunk: report where the file really ends and let the sender
                // rewind. This is the mechanism that makes a resumed transfer converge.
                let received = transfer.received;
                tracing::debug!(%peer, %id, received, offset, "the sender is out of step");
                let _ = link.try_send(Frame::FileAck {
                    attachment: id,
                    received,
                    state: FileAckState::Receiving,
                });
                return;
            }
            Err(error) => {
                tracing::warn!(%error, %id, "writing a received chunk failed");
                self.fail_incoming(peer, id, FileCancelReason::Failed).await;
                return;
            }
        }

        let received = transfer.received;
        let report = received.saturating_sub(transfer.acked) >= FILE_ACK_EVERY_BYTES;
        if report {
            transfer.acked = received;
        }
        let mut progress = transfer.attachment.clone();
        progress.transferred = received;
        progress.state = AttachmentState::Receiving;

        if report {
            let _ = self.store.set_attachment_progress(id, received).await;
            self.emit_attachment(peer, progress);
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received,
                state: FileAckState::Receiving,
            });
        }
    }

    /// The sender says its last chunk is out and gives the digest of the whole file.
    async fn on_file_done(&mut self, peer: DeviceId, id: AttachmentId, digest: Sha256) {
        let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) else {
            return;
        };
        let files_root = self.store.files_root().to_path_buf();

        let row = match self.store.attachment(id).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                self.refuse(peer, id, FileCancelReason::Unknown);
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to read an attachment");
                return;
            }
        };
        if row.peer != peer || row.direction != Direction::Incoming {
            self.refuse(peer, id, FileCancelReason::Unknown);
            return;
        }
        if row.state == AttachmentState::Complete {
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received: row.size,
                state: FileAckState::Complete,
            });
            return;
        }

        let mut attachment = row;
        if attachment.state.is_finished() {
            if let Some(entry) = self.peers.get_mut(&peer) {
                entry.incoming.remove(&id);
            }
            attachment.state = AttachmentState::Receiving;
        }
        let opened = self
            .peers
            .get_mut(&peer)
            .map(|entry| entry.ensure_incoming(&attachment, &files_root));
        match opened {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                tracing::warn!(%error, %id, "could not open the destination file");
                self.fail_incoming(peer, id, FileCancelReason::Failed).await;
                return;
            }
            None => return,
        }

        let entry = match self.peers.get_mut(&peer) {
            Some(entry) => entry,
            None => return,
        };
        let Some(transfer) = entry.incoming.get_mut(&id) else {
            return;
        };
        if transfer.expected.is_some() {
            // Already being checked; a second announcement is a retransmission of the first.
            return;
        }
        if transfer.received != transfer.attachment.size {
            // Not everything arrived. Saying so is cheaper than hashing a file we know is short.
            let received = transfer.received;
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received,
                state: FileAckState::Receiving,
            });
            return;
        }

        // The whole file is on disk: read it back and check it. That is a whole-file read on a
        // blocking task, so the actor keeps answering while it happens.
        transfer.expected = Some(digest);
        transfer.release();
        let part = transfer.part.clone();
        let prep = self.prep.clone();
        tokio::task::spawn_blocking(move || {
            let digest = attachment::hash_file(&part).map_err(|error| error.to_string());
            let _ = prep.blocking_send(Prep::ReceiverDigest {
                peer,
                attachment: id,
                digest,
            });
        });
    }

    /// What the recipient is doing, in bytes.
    async fn on_file_ack(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        received: u64,
        state: FileAckState,
    ) {
        let outgoing = self
            .peers
            .get(&peer)
            .and_then(|entry| entry.transfer.as_ref())
            .is_some_and(|transfer| transfer.id() == id);

        if state == FileAckState::Complete {
            if outgoing && let Some(entry) = self.peers.get_mut(&peer) {
                entry.transfer = None;
            }
            let size = self
                .store
                .attachment(id)
                .await
                .ok()
                .flatten()
                .map(|row| row.size);
            if let Some(size) = size {
                let _ = self.store.set_attachment_progress(id, size).await;
                let _ = self
                    .store
                    .set_attachment_state(id, AttachmentState::Complete)
                    .await;
                self.emit_attachment_row(peer, id).await;
            }
            self.maybe_start_transfer(peer).await;
            return;
        }

        let Some(entry) = self.peers.get_mut(&peer) else {
            return;
        };
        let Some(transfer) = entry.transfer.as_mut() else {
            return;
        };
        if transfer.id() != id {
            return;
        }
        if received != transfer.sent {
            tracing::debug!(%peer, %id, received, sent = transfer.sent, "the recipient corrected the offset");
            transfer.rewind(received);
        } else {
            transfer.acked = transfer.acked.max(received);
        }
        let mut progress = transfer.attachment.clone();
        progress.transferred = transfer.sent;
        progress.state = AttachmentState::Sending;
        let sent = transfer.sent;
        let _ = self.store.set_attachment_progress(id, sent).await;
        self.emit_attachment(peer, progress);
    }

    /// The other end has given up on this transfer, or refuses to start it.
    async fn on_file_cancel(&mut self, peer: DeviceId, id: AttachmentId, reason: FileCancelReason) {
        let state = match reason {
            FileCancelReason::Cancelled => AttachmentState::Cancelled,
            _ => AttachmentState::Failed,
        };
        tracing::info!(%peer, %id, ?reason, "a transfer was cancelled");
        if let Some(entry) = self.peers.get_mut(&peer) {
            if entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
            {
                entry.transfer = None;
            }
            if let Some(mut incoming) = entry.incoming.remove(&id) {
                incoming.discard();
            }
        }
        let _ = self.store.set_attachment_state(id, state).await;
        let _ = self.store.set_attachment_progress(id, 0).await;
        self.emit_attachment_row(peer, id).await;
        self.maybe_start_transfer(peer).await;
    }

    /// "Send me this file": a recipient that has the metadata but not the bytes.
    async fn on_file_request(&mut self, peer: DeviceId, id: AttachmentId) {
        let Ok(Some(row)) = self.store.attachment(id).await else {
            return;
        };
        if row.peer != peer || row.direction != Direction::Outgoing {
            return;
        }
        if row.state.is_finished() {
            return;
        }
        if let Some(entry) = self.peers.get_mut(&peer)
            && entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
        {
            entry.transfer = None;
        }
        let _ = self
            .store
            .set_attachment_state(id, AttachmentState::Queued)
            .await;
        self.emit_attachment_row(peer, id).await;
        self.maybe_start_transfer(peer).await;
    }

    /// Stops a transfer, from whichever side the user is looking at it.
    async fn cancel_attachment(&mut self, id: AttachmentId) {
        let Ok(Some(row)) = self.store.attachment(id).await else {
            return;
        };
        if row.state.is_finished() {
            return;
        }
        let peer = row.peer;
        if let Some(entry) = self.peers.get_mut(&peer) {
            if entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
            {
                entry.transfer = None;
            }
            if let Some(mut incoming) = entry.incoming.remove(&id) {
                incoming.discard();
            }
        }
        let _ = self
            .store
            .set_attachment_state(id, AttachmentState::Cancelled)
            .await;
        let _ = self.store.set_attachment_progress(id, 0).await;
        self.emit_attachment_row(peer, id).await;
        self.refuse(peer, id, FileCancelReason::Cancelled);
        self.maybe_start_transfer(peer).await;
    }

    /// Starts a transfer again, from the side the user is on.
    async fn retry_attachment(&mut self, id: AttachmentId) {
        let Ok(Some(row)) = self.store.attachment(id).await else {
            return;
        };
        if row.state == AttachmentState::Complete {
            return;
        }
        let peer = row.peer;

        if row.direction == Direction::Incoming {
            // Ask the sender to start again. Whatever was on disk is dropped first, because the
            // offset the sender resumes from is measured against a file that is now empty.
            if let Some(entry) = self.peers.get_mut(&peer)
                && let Some(mut incoming) = entry.incoming.remove(&id)
            {
                incoming.discard();
            }
            let _ = self
                .store
                .set_attachment_state(id, AttachmentState::Receiving)
                .await;
            let _ = self.store.set_attachment_progress(id, 0).await;
            self.emit_attachment_row(peer, id).await;
            if let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone())
                && let Err(error) = link.try_send(Frame::FileRequest { attachment: id })
            {
                tracing::debug!(%peer, %id, %error, "failed to ask for the file again");
            }
            return;
        }

        if let Some(entry) = self.peers.get_mut(&peer)
            && entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
        {
            entry.transfer = None;
        }
        let _ = self
            .store
            .set_attachment_state(id, AttachmentState::Queued)
            .await;
        self.emit_attachment_row(peer, id).await;

        // Announce it again: the recipient's row may be a terminal one, and only a fresh
        // `chat` frame tells it that this transfer is starting over.
        let messages = self
            .store
            .messages_with_unfinished_attachments(peer)
            .await
            .unwrap_or_default();
        if let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) {
            for message in messages
                .iter()
                .filter(|message| message.attachments.iter().any(|file| file.id == id))
            {
                if let Err(error) = link.try_send(Frame::Chat {
                    id: message.id,
                    text: message.body.clone(),
                    attachments: message.metas(),
                }) {
                    tracing::debug!(%peer, %error, "failed to re-announce a message with files");
                }
            }
        }
        self.maybe_start_transfer(peer).await;
    }

    /// The result of a whole-file read that could not be done on the actor.
    async fn on_prepared(&mut self, prepared: Prep) {
        match prepared {
            Prep::SenderDigest {
                peer,
                attachment,
                digest,
            } => {
                let digest = match digest {
                    Ok(digest) => digest,
                    Err(reason) => {
                        self.fail_outgoing(peer, attachment, &reason).await;
                        self.maybe_start_transfer(peer).await;
                        return;
                    }
                };
                if self
                    .store
                    .set_attachment_digest(attachment, digest)
                    .await
                    .is_err()
                {
                    return;
                }
                let Some(entry) = self.peers.get_mut(&peer) else {
                    return;
                };
                let Some(transfer) = entry.transfer.as_mut() else {
                    return;
                };
                if transfer.id() != attachment {
                    return;
                }
                transfer.attachment.sha256 = Some(digest);
                transfer.attachment.state = AttachmentState::Sending;
                let progress = transfer.attachment.clone();
                let _ = self
                    .store
                    .set_attachment_state(attachment, AttachmentState::Sending)
                    .await;
                self.emit_attachment(peer, progress);
            }
            Prep::ReceiverDigest {
                peer,
                attachment,
                digest,
            } => self.finish_incoming(peer, attachment, digest).await,
        }
    }

    /// The digest of what was received: keep the file or throw it away.
    async fn finish_incoming(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        digest: Result<Sha256, String>,
    ) {
        let expected = self
            .peers
            .get_mut(&peer)
            .and_then(|entry| entry.incoming.get_mut(&id))
            .and_then(|transfer| transfer.expected.take());
        let Some(expected) = expected else {
            return;
        };

        let agreed = matches!(&digest, Ok(actual) if *actual == expected);
        if !agreed {
            let why = match digest {
                Ok(_) => "the digest does not match".to_owned(),
                Err(reason) => reason,
            };
            tracing::warn!(%peer, %id, why, "the received file did not verify");
            self.fail_incoming(peer, id, FileCancelReason::Checksum)
                .await;
            return;
        }

        let Some(entry) = self.peers.get_mut(&peer) else {
            return;
        };
        let Some(mut transfer) = entry.incoming.remove(&id) else {
            return;
        };
        let size = transfer.attachment.size;
        if let Err(error) = transfer.finish() {
            tracing::warn!(%error, %id, "could not move the received file into place");
            self.store_fail_incoming(peer, id).await;
            return;
        }
        let mut done = transfer.attachment.clone();
        done.state = AttachmentState::Complete;
        done.transferred = size;
        done.path = Some(transfer.final_path.to_string_lossy().into_owned());
        let _ = self.store.set_attachment_progress(id, size).await;
        let _ = self
            .store
            .set_attachment_state(id, AttachmentState::Complete)
            .await;
        self.emit_attachment(peer, done);
        if let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) {
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received: size,
                state: FileAckState::Complete,
            });
        }
    }

    /// Marks an incoming attachment finished-but-failed and tells the sender.
    async fn fail_incoming(&mut self, peer: DeviceId, id: AttachmentId, reason: FileCancelReason) {
        if let Some(mut incoming) = self
            .peers
            .get_mut(&peer)
            .and_then(|entry| entry.incoming.remove(&id))
        {
            incoming.discard();
        }
        self.store_fail_incoming(peer, id).await;
        self.refuse(peer, id, reason);
    }

    async fn store_fail_incoming(&mut self, peer: DeviceId, id: AttachmentId) {
        let _ = self
            .store
            .set_attachment_state(id, AttachmentState::Failed)
            .await;
        let _ = self.store.set_attachment_progress(id, 0).await;
        self.emit_attachment_row(peer, id).await;
    }

    /// Gives up on an outgoing transfer: a missing source, an unreadable one, a failed write.
    ///
    /// Deliberately does not look for the next file — the caller decides whether to carry on, so
    /// the call graph stays finite.
    async fn fail_outgoing(&mut self, peer: DeviceId, id: AttachmentId, reason: &str) {
        tracing::warn!(%peer, %id, reason, "the outgoing transfer failed");
        if let Some(entry) = self.peers.get_mut(&peer)
            && entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
        {
            entry.transfer = None;
        }
        let _ = self
            .store
            .set_attachment_state(id, AttachmentState::Failed)
            .await;
        self.emit_attachment_row(peer, id).await;
        self.refuse(peer, id, FileCancelReason::Failed);
    }

    async fn set_attachment_state(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        state: AttachmentState,
    ) {
        let _ = self.store.set_attachment_state(id, state).await;
        self.emit_attachment_row(peer, id).await;
    }

    /// Tells the peer a transfer is over. Best effort: the socket may already be gone.
    fn refuse(&self, peer: DeviceId, id: AttachmentId, reason: FileCancelReason) {
        let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) else {
            return;
        };
        if let Err(error) = link.try_send(Frame::FileCancel {
            attachment: id,
            reason,
        }) {
            tracing::debug!(%peer, %id, %error, "failed to refuse an attachment");
        }
    }

    /// Starts sending this peer's oldest unfinished attachment, if it is not already busy.
    ///
    /// Only a message that has been announced can be streamed: a `chat` frame still waiting in
    /// the outbox means the recipient has no idea what these chunks belong to.
    async fn maybe_start_transfer(&mut self, peer: DeviceId) {
        // A loop rather than a call to itself: a file that cannot be opened is marked failed and
        // the next one is tried, and the transfer of each candidate is one iteration.
        loop {
            let Some(entry) = self.peers.get(&peer) else {
                return;
            };
            if entry.transfer.is_some() || entry.link.is_none() {
                return;
            }

            let messages = match self.store.messages_with_unfinished_attachments(peer).await {
                Ok(messages) => messages,
                Err(error) => {
                    tracing::warn!(%error, "failed to read the unfinished attachments");
                    return;
                }
            };
            let Some(attachment) = messages
                .iter()
                .flat_map(|message| message.attachments.iter())
                .find(|attachment| {
                    attachment.direction == Direction::Outgoing && attachment.state.is_in_flight()
                })
                .cloned()
            else {
                return;
            };

            let Some(path) = attachment.path.clone().map(PathBuf::from) else {
                self.fail_outgoing(peer, attachment.id, "the source of the file is not known")
                    .await;
                continue;
            };

            let needs_digest = attachment.sha256.is_none();
            let mut transfer = OutgoingTransfer::new(attachment.clone(), path.clone());
            if !needs_digest {
                transfer.attachment.state = AttachmentState::Sending;
            }
            let Some(entry) = self.peers.get_mut(&peer) else {
                return;
            };
            if entry.transfer.is_some() {
                return;
            }
            entry.transfer = Some(transfer);

            if !needs_digest {
                let _ = self
                    .store
                    .set_attachment_state(attachment.id, AttachmentState::Sending)
                    .await;
                self.emit_attachment(peer, attachment);
                return;
            }

            // The digest has to exist before the first chunk, because the recipient checks the
            // file it assembled against it; reading a large file is a blocking task of its own.
            let prep = self.prep.clone();
            let id = attachment.id;
            tokio::task::spawn_blocking(move || {
                let digest = attachment::hash_file(&path).map_err(|error| error.to_string());
                let _ = prep.blocking_send(Prep::SenderDigest {
                    peer,
                    attachment: id,
                    digest,
                });
            });
            return;
        }
    }

    /// Re-offers every unfinished attachment of a peer that has just come back.
    ///
    /// The metadata is repeated because the recipient's copy of it may be gone (a cleared
    /// history, a reinstalled application) while ours says the message was delivered; the
    /// chunks then resume from wherever the recipient's file ends, which the first
    /// acknowledgement corrects.
    async fn resume_attachments(&mut self, peer: DeviceId) {
        let messages = match self.store.messages_with_unfinished_attachments(peer).await {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(%error, "failed to read the unfinished attachments");
                return;
            }
        };
        let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) else {
            return;
        };
        for message in &messages {
            // A message still in the outbox is announced by the drain itself, in order.
            if message.status == MessageStatus::Queued {
                continue;
            }
            if let Err(error) = link.try_send(Frame::Chat {
                id: message.id,
                text: message.body.clone(),
                attachments: message.metas(),
            }) {
                tracing::debug!(%peer, %error, "failed to re-announce a message with files");
                break;
            }
        }
        self.maybe_start_transfer(peer).await;
    }

    /// When the next chunk may be written, or `None` when nothing is waiting.
    fn transfer_deadline(&self) -> Option<tokio::time::Instant> {
        let waiting = self.peers.values().any(|entry| {
            entry.link.is_some()
                && entry
                    .transfer
                    .as_ref()
                    .is_some_and(OutgoingTransfer::wants_chunk)
        });
        waiting.then(|| tokio::time::Instant::from_std(Instant::now() + TRANSFER_POLL))
    }

    /// Writes what the pacing and the send queue allow, for every peer at once.
    ///
    /// Bounded per turn: a transfer is a background job, and a message arriving on another
    /// connection must not wait for a whole file to be written out.
    async fn pump_transfers(&mut self) {
        let busy: Vec<DeviceId> = self
            .peers
            .iter()
            .filter(|(_, entry)| {
                entry.link.is_some()
                    && entry
                        .transfer
                        .as_ref()
                        .is_some_and(OutgoingTransfer::wants_chunk)
            })
            .map(|(device_id, _)| *device_id)
            .collect();

        for peer in busy {
            let mut failure: Option<String> = None;
            for _ in 0..TRANSFER_CHUNKS_PER_TURN {
                let now = Instant::now();
                let Some(entry) = self.peers.get_mut(&peer) else {
                    break;
                };
                let Some(link) = entry.link.clone() else {
                    break;
                };
                let Some(transfer) = entry.transfer.as_mut() else {
                    break;
                };
                if !transfer.wants_chunk() {
                    break;
                }
                // Charged at the full chunk size: the read that follows is at most that big, and
                // a budget that under-charges is a budget that gets refused by the recipient.
                if !entry
                    .file_budget
                    .try_acquire_n(FILE_CHUNK_BYTES as f64, now)
                {
                    break;
                }

                if transfer.is_complete() {
                    let Some(sha256) = transfer.attachment.sha256 else {
                        break;
                    };
                    transfer.finished = true;
                    if let Err(error) = link.try_send(Frame::FileDone {
                        attachment: transfer.id(),
                        sha256,
                    }) {
                        tracing::debug!(%peer, %error, "could not announce a finished file");
                    }
                    break;
                }

                match transfer.read_chunk() {
                    Ok(Some(data)) => {
                        let offset = transfer.sent - u64::try_from(data.len()).unwrap_or(0);
                        if let Err(error) = link.try_send(Frame::FileChunk {
                            attachment: transfer.id(),
                            offset,
                            data,
                        }) {
                            tracing::debug!(%peer, %error, "could not queue a file chunk");
                            break;
                        }
                    }
                    Ok(None) => continue,
                    Err(error) => {
                        failure = Some(error.to_string());
                        break;
                    }
                }
            }

            if let Some(reason) = failure {
                let id = self
                    .peers
                    .get(&peer)
                    .and_then(|entry| entry.transfer.as_ref())
                    .map(OutgoingTransfer::id);
                if let Some(id) = id {
                    self.fail_outgoing(peer, id, &reason).await;
                    self.maybe_start_transfer(peer).await;
                }
            }
        }
    }

    async fn on_disconnected(&mut self, peer: DeviceId, link: u64, reason: DisconnectReason) {
        let now = self.clock.wall().as_i64();
        let Some(entry) = self.peers.get_mut(&peer) else {
            return;
        };

        // When both peers dial at once, one of the two connections is abandoned. Its task reports
        // its end afterwards, and that report must not be mistaken for the surviving connection's:
        // clearing the live link here would flap the peer offline and reconnect the two of them
        // for ever.
        if entry
            .link
            .as_ref()
            .is_some_and(|current| current.id() != link)
        {
            tracing::debug!(%peer, ?reason, "the superseded connection ended");
            return;
        }

        entry.link = None;
        entry.dial = None;
        // A transfer cannot outlive its socket. What has already been written stays on disk and
        // is picked up again from the part file on the next connection; the handles are released
        // so the last bytes reach the filesystem before the process can go away.
        let outgoing = entry.transfer.take().map(|transfer| {
            let id = transfer.id();
            (id, transfer.acked)
        });
        let incoming: Vec<(AttachmentId, u64)> = entry
            .incoming
            .drain()
            .map(|(id, mut transfer)| {
                transfer.release();
                (id, transfer.received)
            })
            .collect();
        if entry.presence.disconnected().is_visible() {
            tracing::info!(%peer, ?reason, "peer went offline");
        }
        entry.stored.last_seen_ms = Some(now);
        entry.note_activity(now);

        if let Err(error) = self.store.touch_peer_seen(peer, now).await {
            tracing::warn!(%error, "failed to record the last seen time");
        }
        // A message written to the socket but never acknowledged goes back to the outbox: it
        // keeps its position in the conversation and is retried on the next connection, which is
        // the whole point of sending to a peer that is not there.
        match self.store.requeue_pending_messages(peer).await {
            Ok(ids) => {
                if !ids.is_empty() {
                    tracing::debug!(%peer, count = ids.len(), "messages are waiting to be retried");
                    if let Some(entry) = self.peers.get_mut(&peer) {
                        entry.outbox_pending = true;
                    }
                    for id in ids {
                        self.emit_status(peer, id, MessageStatus::Queued, None);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to return in-flight messages to the outbox")
            }
        }

        self.emit_peers();

        // Record how far each transfer had come, and put the outgoing one back in the queue: the
        // interface should read "waiting" rather than "sending" while there is no socket.
        if let Some((id, transferred)) = outgoing {
            let _ = self.store.set_attachment_progress(id, transferred).await;
            let _ = self
                .store
                .set_attachment_state(id, AttachmentState::Queued)
                .await;
            self.emit_attachment_row(peer, id).await;
        }
        for (id, received) in incoming {
            let _ = self.store.set_attachment_progress(id, received).await;
            self.emit_attachment_row(peer, id).await;
        }

        if reason.allows_redial() {
            self.maybe_dial(peer);
        }
    }

    async fn handle_discovery(&mut self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::Found(peer) => self.on_discovered(peer).await,
            DiscoveryEvent::Lost { device_id } => {
                let Some(entry) = self.peers.get_mut(&device_id) else {
                    return;
                };
                if entry.link.is_some() {
                    // Losing sight of a peer whose connection is alive is normal when an
                    // announcement is dropped; the connection is the stronger signal.
                    return;
                }
                entry.addresses.clear();
                if entry.presence.disconnected().is_visible() {
                    self.emit_peers();
                }
            }
        }
    }

    async fn on_discovered(&mut self, peer: DiscoveredPeer) {
        // A peer we have never seen — including one the user forgot and which has come back.
        // `last_seen` is deliberately not stamped here: being announced is not being reachable.
        if !self.peers.contains_key(&peer.device_id) {
            let profile = profile_of(&peer);
            match self.store.upsert_peer_seen(&profile, None, None).await {
                Ok(stored) => {
                    self.peers.insert(peer.device_id, PeerEntry::new(stored));
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to store a discovered peer");
                    return;
                }
            }
        }

        let Some(entry) = self.peers.get_mut(&peer.device_id) else {
            return;
        };

        // The announced nickname and seed are shown immediately and replaced by the peer's
        // own copy as soon as a connection exists.
        let announced = profile_of(&peer);
        let renamed = entry.profile.nickname != announced.nickname
            || entry.profile.avatar_seed != announced.avatar_seed;
        let was_forgotten = entry.stored.forgotten;
        entry.addresses = peer.addresses;
        if renamed {
            entry.profile.nickname = announced.nickname.clone();
            entry.profile.avatar_seed = announced.avatar_seed.clone();
        }
        let device_id = peer.device_id;
        let now = self.clock.wall().as_i64();
        entry.note_activity(now);

        // A forgotten device that has appeared again is discovered as new, which is what the
        // forgetting dialog promises. Its row is reused, so the settings screen keeps a single
        // entry for it.
        if renamed || was_forgotten {
            if was_forgotten {
                tracing::info!(%device_id, "a forgotten device has reappeared; adding it again");
            }
            if let Err(error) = self.store.upsert_peer_seen(&announced, None, None).await {
                tracing::warn!(%error, "failed to store an updated peer profile");
            }
            if let Some(entry) = self.peers.get_mut(&device_id) {
                entry.stored.forgotten = false;
            }
        }

        self.maybe_dial(device_id);
        self.emit_peers();
    }

    async fn tick(&mut self) {
        let now_instant = Instant::now();
        let now_ms = self.clock.wall().as_i64();

        let mut stalled: Vec<DeviceId> = Vec::new();
        for (device_id, entry) in &mut self.peers {
            if entry.presence.tick(now_instant) == PresenceChange::WentOffline {
                stalled.push(*device_id);
            }
        }

        for peer in stalled {
            tracing::info!(
                %peer,
                timeout_secs = HEARTBEAT_TIMEOUT.as_secs(),
                "peer stopped responding"
            );
            // Dropping the link makes the connection task send a goodbye and close, which is
            // also how the socket is released for a peer that is genuinely gone.
            let link = self
                .peers
                .get_mut(&peer)
                .and_then(|entry| entry.link.take());
            if let Some(link) = link {
                link.close(GoodbyeReason::Shutdown);
            }
            if let Some(entry) = self.peers.get_mut(&peer) {
                entry.stored.last_seen_ms = Some(now_ms);
                entry.note_activity(now_ms);
            }
            if let Err(error) = self.store.touch_peer_seen(peer, now_ms).await {
                tracing::warn!(%error, "failed to record the last seen time");
            }
            self.emit_peers();
        }

        // A peer discovered while the dial slots were full is picked up here.
        while self.active_dials < MAX_CONCURRENT_DIALS && !self.dial_queue.is_empty() {
            if let Some(peer) = self.dial_queue.pop() {
                self.maybe_dial(peer);
            }
        }

        // …and so is a peer that was announced inside the retry window of a dial that has just
        // failed. Without this the only trigger left is the next discovery event, which on a
        // quiet network can be many seconds away — and a peer that has just restarted (a new
        // port, the same identity) is exactly the case where the user is waiting.
        let retry: Vec<DeviceId> = self
            .peers
            .iter()
            .filter(|(_, entry)| {
                entry.link.is_none() && entry.dial.is_none() && !entry.addresses.is_empty()
            })
            .map(|(device_id, _)| *device_id)
            .collect();
        for peer in retry {
            self.maybe_dial(peer);
        }

        // The outbox is drained a little per tick, so a peer that has just come back receives a
        // backlog in order and without tripping the recipient's rate limit.
        let waiting: Vec<DeviceId> = self
            .peers
            .iter()
            .filter(|(_, entry)| entry.outbox_pending && entry.link.is_some())
            .map(|(device_id, _)| *device_id)
            .collect();
        for peer in waiting {
            let _ = self.pump_outbox(peer).await;
        }
    }

    fn maybe_dial(&mut self, peer: DeviceId) {
        if self.active_dials >= MAX_CONCURRENT_DIALS {
            if !self.dial_queue.contains(&peer) {
                self.dial_queue.push(peer);
            }
            return;
        }

        let Some(entry) = self.peers.get_mut(&peer) else {
            return;
        };
        if entry.link.is_some() || entry.addresses.is_empty() {
            return;
        }
        if entry
            .last_dial_attempt
            .is_some_and(|at| at.elapsed() < DIAL_RETRY_DELAY)
        {
            return;
        }
        if !entry.presence.discovered() {
            return;
        }

        entry.last_dial_attempt = Some(Instant::now());
        let addresses = entry.dial_order();
        self.active_dials += 1;

        let handshake = self.handshake.clone();
        let context = self.connection.clone();
        let events = self.transport.clone();

        let task = tokio::spawn(async move {
            let mut last_error = None;
            for address in addresses {
                match connection::dial(address, &handshake, &context, Some(peer)).await {
                    Ok(handshaken) => {
                        connection::serve(handshaken, context).await;
                        return;
                    }
                    Err(error) => {
                        tracing::debug!(%peer, %address, %error, "dial attempt failed");
                        last_error = Some(error);
                    }
                }
            }
            let reason = match last_error {
                Some(error) => error.to_string(),
                None => "the peer advertised no addresses".to_owned(),
            };
            let _ = events
                .send(TransportEvent::DialFailed { peer, reason })
                .await;
        });

        if let Some(entry) = self.peers.get_mut(&peer) {
            if let Some(previous) = entry.dial.replace(task) {
                previous.abort();
            }
        }
    }

    /// `None` when the device is not in the list, which can happen when a peer is forgotten in
    /// the same turn that its message was stored.
    fn peer_view(&self, peer: DeviceId) -> Option<PeerView> {
        self.peers.get(&peer).map(PeerEntry::view)
    }

    fn arranged_peers(&self) -> Vec<PeerView> {
        PeerView::arrange(
            self.peers
                .values()
                .filter(|entry| entry.stored.is_listed())
                .map(PeerEntry::view)
                .collect::<Vec<_>>(),
            "",
        )
    }

    fn emit_message(&self, peer: DeviceId, message: &ChatMessage) {
        let Some(view) = self.peer_view(peer) else {
            return;
        };
        let _ = self.events.send(CoreEvent::Message {
            peer: view,
            message: message.clone(),
        });
    }

    fn emit_status(
        &self,
        peer: DeviceId,
        id: MessageId,
        status: MessageStatus,
        delivered_at: Option<UnixMillis>,
    ) {
        let _ = self.events.send(CoreEvent::MessageStatus {
            peer,
            id,
            status,
            delivered_at,
        });
    }

    fn emit_attachment(&self, peer: DeviceId, attachment: Attachment) {
        let _ = self.events.send(CoreEvent::Attachment { peer, attachment });
    }

    /// Reads a row back and emits it, for the paths that only know the identifier.
    async fn emit_attachment_row(&mut self, peer: DeviceId, id: AttachmentId) {
        match self.store.attachment(id).await {
            Ok(Some(attachment)) => self.emit_attachment(peer, attachment),
            Ok(None) => tracing::debug!(%peer, %id, "the attachment row is gone"),
            Err(error) => tracing::warn!(%error, "failed to read an attachment"),
        }
    }

    fn emit_peers(&self) {
        let _ = self.events.send(CoreEvent::Peers {
            peers: self.arranged_peers(),
        });
    }

    fn notice(&self, level: NoticeLevel, message: String) {
        let _ = self.events.send(CoreEvent::Notice { level, message });
    }

    async fn shutdown(&mut self) {
        tracing::info!(peers = self.peers.len(), "shutting down the session");

        for entry in self.peers.values_mut() {
            if let Some(dial) = entry.dial.take() {
                dial.abort();
            }
            if let Some(link) = entry.link.take() {
                link.close(GoodbyeReason::Shutdown);
            }
            // Drop the transfer handles so a half-received file is flushed to disk before the
            // process goes away: the length of that file is what the next start resumes from.
            entry.transfer = None;
            entry.incoming.clear();
            entry.presence.disconnected();
        }

        // Give the connection tasks a moment to write their goodbye frames and flush. The
        // wait is bounded: a peer that has stopped reading must not be able to delay exit.
        tokio::time::sleep(SHUTDOWN_DRAIN_TIMEOUT).await;
        self.emit_peers();
    }
}

fn profile_of(peer: &DiscoveredPeer) -> PeerProfile {
    PeerProfile {
        device_id: peer.device_id,
        nickname: peer.nickname.clone(),
        avatar_seed: peer.avatar_seed.clone(),
    }
}

/// A deadline far enough away that a branch guarded by `is_some()` never fires on it.
fn far_future() -> tokio::time::Instant {
    tokio::time::Instant::from_std(Instant::now() + Duration::from_secs(3600))
}

/// Turns the paths the interface chose into attachments, by looking at the files themselves.
///
/// The size and the name come from the filesystem rather than from the caller, and a path that
/// is missing, is a directory, or is above the cap is refused before a message row exists — so a
/// transfer can fail, but never because it was lied to about what it was sending.
fn resolve_files(
    peer: &DeviceId,
    message_id: MessageId,
    files: &[PathBuf],
    now: UnixMillis,
) -> Result<Vec<Attachment>, CoreError> {
    use crate::error::DomainError;

    if files.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        return Err(CoreError::Domain(DomainError::BadAttachment {
            reason: format!("{} files in one message", files.len()),
        }));
    }

    let mut attachments = Vec::with_capacity(files.len());
    for path in files {
        let reply = |reason: String| {
            CoreError::Domain(DomainError::BadAttachment {
                reason: format!("{}: {reason}", path.display()),
            })
        };
        let metadata = std::fs::metadata(path).map_err(|error| reply(error.to_string()))?;
        if !metadata.is_file() {
            return Err(reply("not a file".to_owned()));
        }
        let size = metadata.len();
        if size > MAX_ATTACHMENT_BYTES {
            return Err(CoreError::Domain(DomainError::AttachmentTooLarge {
                size,
                max: MAX_ATTACHMENT_BYTES,
            }));
        }
        let raw = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = FileName::sanitise(&raw);
        attachments.push(Attachment {
            id: AttachmentId::generate(),
            message_id,
            peer: *peer,
            direction: Direction::Outgoing,
            kind: name.kind(),
            name,
            size,
            state: AttachmentState::Queued,
            transferred: 0,
            sha256: None,
            created_at: now,
            path: Some(path.to_string_lossy().into_owned()),
        });
    }
    Ok(attachments)
}
