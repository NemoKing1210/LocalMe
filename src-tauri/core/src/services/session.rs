//! The session actor: the only place mutable network state lives.
//!
//! One task owns the peer table, the presence machines, the per-peer links and the dial
//! bookkeeping. Everything that can change that state arrives as a message on one of three
//! channels — commands from the host application, transport events from connection tasks, and
//! discovery events — and is applied sequentially. There is no lock anywhere in this file,
//! which is what makes the invariants ("a peer with a live link is online", "a forgotten peer
//! is never dialled") properties of the code rather than of a locking discipline.
//!
//! Storage calls are awaited inline. A local SQLite write is sub-millisecond, and doing it
//! inline is what guarantees that the acknowledgement of a received message is sent only
//! after its row is committed — that is, that "delivered" on the sender means "durably stored
//! here" rather than "written to a socket buffer".

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::domain::clock::Clock;
use crate::domain::ids::{DeviceId, MessageId};
use crate::domain::message::{ChatMessage, Direction, MessageBody, MessageStatus};
use crate::domain::nickname::Nickname;
use crate::domain::peer::{Handshake, PeerProfile, PeerView};
use crate::domain::presence::{PresenceChange, PresenceMachine};
use crate::error::CoreError;
use crate::ports::discovery::{DiscoveredPeer, DiscoveryEvent};
use crate::ports::store::{HistoryCursor, KnownDevice, META_NICKNAME, Store, StoredPeer};
use crate::protocol::limits::{
    COMMAND_CHANNEL_CAPACITY, DIAL_RETRY_DELAY, EVENT_CHANNEL_CAPACITY, HEARTBEAT_TIMEOUT,
    MAX_PEERS, PRESENCE_TICK_INTERVAL, PROTOCOL_VERSION, SHUTDOWN_DRAIN_TIMEOUT,
};
use crate::protocol::{Frame, GoodbyeReason};
use crate::transport::connection::{self, ConnectionContext};
use crate::transport::{DisconnectReason, PeerLink, Role, TransportEvent, is_preferred};

use super::events::{CoreEvent, NoticeLevel};

/// How many dials may be in flight at once.
///
/// Without a cap, discovering a network full of devices that do not answer would open a socket
/// per peer at the same instant. Eight connects a room's worth of peers in one round trip
/// while keeping the burst bounded.
const MAX_CONCURRENT_DIALS: usize = 8;

/// Configuration the session cannot change while it runs.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Our own identity.
    pub own: PeerProfile,
    /// The port we listen on, announced in the handshake so peers can dial back.
    pub listen_port: u16,
    /// Maximum simultaneous connections.
    pub max_peers: usize,
}

impl SessionConfig {
    /// Builds the configuration with the default peer limit.
    #[must_use]
    pub fn new(own: PeerProfile, listen_port: u16) -> Self {
        Self {
            own,
            listen_port,
            max_peers: MAX_PEERS,
        }
    }

    /// The handshake advertising this device.
    #[must_use]
    pub fn handshake(&self) -> Handshake {
        Handshake::new(PROTOCOL_VERSION, &self.own, self.listen_port)
    }
}

/// Commands the host application sends to the session.
#[derive(Debug)]
pub enum SessionCommand {
    /// The arranged user list.
    ListPeers {
        /// Reply channel.
        reply: oneshot::Sender<Vec<PeerView>>,
    },
    /// This device's own identity.
    GetOwnProfile {
        /// Reply channel.
        reply: oneshot::Sender<PeerProfile>,
    },
    /// Renames this device and tells every connected peer.
    SetNickname {
        /// The new nickname.
        nickname: Nickname,
        /// Reply channel.
        reply: oneshot::Sender<PeerProfile>,
    },
    /// Sends a message.
    SendMessage {
        /// The conversation.
        peer: DeviceId,
        /// The validated body.
        body: MessageBody,
        /// Reply channel carrying the stored row, or why it could not be attempted.
        reply: oneshot::Sender<Result<ChatMessage, CoreError>>,
    },
    /// Reads one page of a conversation.
    History {
        /// The conversation.
        peer: DeviceId,
        /// Where to continue from, newest-first.
        before: Option<HistoryCursor>,
        /// Maximum rows.
        limit: u32,
        /// Reply channel.
        reply: oneshot::Sender<Vec<ChatMessage>>,
    },
    /// Marks a conversation as read.
    MarkRead {
        /// The conversation.
        peer: DeviceId,
        /// Reply channel carrying the number of messages that changed.
        reply: oneshot::Sender<u32>,
    },
    /// Forgets a device, optionally deleting its history.
    Forget {
        /// The device.
        peer: DeviceId,
        /// Whether to delete the stored conversation as well.
        delete_history: bool,
        /// Reply channel.
        reply: oneshot::Sender<()>,
    },
    /// Lets a forgotten device back into the list.
    Restore {
        /// The device.
        peer: DeviceId,
        /// Reply channel.
        reply: oneshot::Sender<()>,
    },
    /// Sets whether a device's notifications are suppressed.
    SetMuted {
        /// The device.
        peer: DeviceId,
        /// Whether notifications are suppressed.
        muted: bool,
        /// Reply channel.
        reply: oneshot::Sender<()>,
    },
    /// Every device this installation has seen, forgotten ones included.
    KnownDevices {
        /// Reply channel.
        reply: oneshot::Sender<Vec<KnownDevice>>,
    },
    /// Deletes every stored message.
    ClearHistory {
        /// Reply channel carrying the number of deleted rows.
        reply: oneshot::Sender<u64>,
    },
    /// Stops the session, saying goodbye to every peer first.
    Shutdown {
        /// Reply channel signalled when the shutdown has finished.
        reply: oneshot::Sender<()>,
    },
}

/// The host application's handle on the session.
///
/// Cheap to clone; every clone talks to the same actor. The handle is the only way to reach
/// the session, so the command set above is the complete public behaviour of the core.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    commands: mpsc::Sender<SessionCommand>,
    events: broadcast::Sender<CoreEvent>,
}

impl SessionHandle {
    /// Subscribes to core events.
    ///
    /// A receiver that falls behind loses events rather than blocking the session. That is
    /// safe because every event is a statement about state the host can also read back through
    /// this handle: a lost event costs a repaint, not correctness.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    /// Whether the session is still accepting commands.
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

    /// The arranged user list.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn list_peers(&self) -> Result<Vec<PeerView>, CoreError> {
        self.request(|reply| SessionCommand::ListPeers { reply })
            .await
    }

    /// This device's own identity.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn own_profile(&self) -> Result<PeerProfile, CoreError> {
        self.request(|reply| SessionCommand::GetOwnProfile { reply })
            .await
    }

    /// Renames this device and broadcasts the change to every live connection.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn set_nickname(&self, nickname: Nickname) -> Result<PeerProfile, CoreError> {
        self.request(|reply| SessionCommand::SetNickname { nickname, reply })
            .await
    }

    /// Sends a message to a peer.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownPeer`] if the device is not in the list, [`CoreError::PeerOffline`]
    /// if it is not reachable. A message that was stored but could not be queued on the socket
    /// is *not* an error: it comes back with [`MessageStatus::Failed`] so the interface can
    /// show the row and offer to retry.
    pub async fn send_message(
        &self,
        peer: DeviceId,
        body: MessageBody,
    ) -> Result<ChatMessage, CoreError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(SessionCommand::SendMessage { peer, body, reply })
            .await
            .map_err(|_| CoreError::ShuttingDown)?;
        receiver.await.map_err(|_| CoreError::ShuttingDown)?
    }

    /// Reads one page of a conversation, newest first.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
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

    /// Marks a conversation as read.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn mark_read(&self, peer: DeviceId) -> Result<u32, CoreError> {
        self.request(|reply| SessionCommand::MarkRead { peer, reply })
            .await
    }

    /// Forgets a device.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn forget(&self, peer: DeviceId, delete_history: bool) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::Forget {
            peer,
            delete_history,
            reply,
        })
        .await
    }

    /// Lets a forgotten device back into the list so it can be reached again.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn restore(&self, peer: DeviceId) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::Restore { peer, reply })
            .await
    }

    /// Suppresses or restores notifications for one device.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn set_muted(&self, peer: DeviceId, muted: bool) -> Result<(), CoreError> {
        self.request(|reply| SessionCommand::SetMuted { peer, muted, reply })
            .await
    }

    /// Every device this installation has seen.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn known_devices(&self) -> Result<Vec<KnownDevice>, CoreError> {
        self.request(|reply| SessionCommand::KnownDevices { reply })
            .await
    }

    /// Deletes every stored message.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session has stopped.
    pub async fn clear_history(&self) -> Result<u64, CoreError> {
        self.request(|reply| SessionCommand::ClearHistory { reply })
            .await
    }

    /// Stops the session: every peer is told goodbye before the sockets close.
    ///
    /// # Errors
    ///
    /// [`CoreError::ShuttingDown`] if the session had already stopped.
    pub async fn shutdown(&self) -> Result<(), CoreError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(SessionCommand::Shutdown { reply })
            .await
            .map_err(|_| CoreError::ShuttingDown)?;
        receiver.await.map_err(|_| CoreError::ShuttingDown)
    }
}

/// Everything the runtime needs to talk to a running session.
#[derive(Debug)]
pub struct SessionRuntime {
    /// The host application's handle.
    pub handle: SessionHandle,
    /// Where connection tasks report.
    pub transport: mpsc::Sender<TransportEvent>,
    /// Where discovery sources report.
    pub discovery: mpsc::Sender<DiscoveryEvent>,
    /// Shared connection context, used by the accept loop.
    pub connection: Arc<ConnectionContext>,
    /// The actor task.
    pub task: JoinHandle<()>,
}

/// One peer as the session sees it.
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
        }
    }

    fn note_activity(&mut self, at_ms: i64) {
        self.last_activity_ms = Some(match self.last_activity_ms {
            Some(current) => current.max(at_ms),
            None => at_ms,
        });
    }

    /// Reorders the dial list so the address that worked last time is tried first.
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

/// Starts the session actor.
///
/// # Errors
///
/// [`CoreError::Storage`] if the stored peer list cannot be read, which means the database is
/// unusable and no conversation can be shown.
pub async fn spawn<C: Clock, S: Store>(
    config: SessionConfig,
    clock: C,
    store: S,
) -> Result<SessionRuntime, CoreError> {
    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (transport_tx, transport_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (discovery_tx, discovery_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

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
        peers: HashMap::new(),
        dial_queue: Vec::new(),
        active_dials: 0,
    };
    session.load_peers().await?;

    let handle = SessionHandle {
        commands: commands_tx,
        events: events_tx,
    };
    let task = tokio::spawn(async move {
        session.run(commands_rx, transport_rx, discovery_rx).await;
    });

    Ok(SessionRuntime {
        handle,
        transport: transport_tx,
        discovery: discovery_tx,
        connection,
        task,
    })
}

/// The session actor's state.
struct Session<C: Clock, S: Store> {
    config: SessionConfig,
    handshake: Handshake,
    clock: C,
    store: S,
    events: broadcast::Sender<CoreEvent>,
    transport: mpsc::Sender<TransportEvent>,
    connection: Arc<ConnectionContext>,
    peers: HashMap<DeviceId, PeerEntry>,
    /// Peers waiting for a dial slot.
    dial_queue: Vec<DeviceId>,
    /// Dials currently in flight.
    active_dials: usize,
}

impl<C: Clock, S: Store> Session<C, S> {
    /// Reads the persisted peer list so the user list is populated before discovery finds
    /// anything: a peer seen yesterday must still be listed when the application starts.
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
    ) {
        let mut ticker = tokio::time::interval(PRESENCE_TICK_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;

        loop {
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
                            // The host application dropped its handle: stop as well.
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

                _ = ticker.tick() => self.tick().await,
            }
        }

        let _ = self.events.send(CoreEvent::Stopped);
    }

    // ---- Commands -------------------------------------------------------------

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

            SessionCommand::SendMessage { peer, body, reply } => {
                let _ = reply.send(self.send_message(peer, body).await);
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
                    // The stored row stays, so the settings screen can still list the device
                    // and offer to restore it: that is what makes "forgotten" different from
                    // "never seen".
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
                }
                self.emit_peers();
                let _ = reply.send(deleted);
            }

            // Handled by the run loop, which needs to break out of it.
            SessionCommand::Shutdown { reply } => drop(reply),
        }
    }

    /// Applies a nickname change locally and tells every connected peer.
    ///
    /// The avatar seed is derived from the device id and the new nickname, so a rename is a
    /// visible change for everyone — which is the point: the seed is part of the profile the
    /// peers store, not something they recompute.
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

    async fn send_message(
        &mut self,
        peer: DeviceId,
        body: MessageBody,
    ) -> Result<ChatMessage, CoreError> {
        let Some(entry) = self.peers.get(&peer) else {
            return Err(CoreError::UnknownPeer(peer.to_string()));
        };
        if !entry.stored.is_listed() {
            return Err(CoreError::UnknownPeer(peer.to_string()));
        }
        let Some(link) = entry.link.clone() else {
            return Err(CoreError::PeerOffline(peer.to_string()));
        };

        let now = self.clock.wall();
        let mut message = ChatMessage {
            id: MessageId::generate(),
            peer,
            direction: Direction::Outgoing,
            body,
            sent_at: now,
            received_at: now,
            status: MessageStatus::Sending,
            read: true,
        };

        if let Err(error) = self.store.insert_message(&message).await {
            tracing::warn!(%error, "failed to store the outgoing message");
            message.status = MessageStatus::Failed;
            let _ = self.events.send(CoreEvent::Message {
                message: message.clone(),
            });
            return Ok(message);
        }

        // A full queue means the peer has stopped reading. Failing the send is better than
        // waiting, because waiting would block every other peer behind it.
        let queued = link.try_send(Frame::Chat {
            id: message.id,
            body: message.body.clone(),
        });
        let status = match queued {
            Ok(()) => MessageStatus::Sent,
            Err(error) => {
                tracing::debug!(%peer, %error, "message could not be queued");
                MessageStatus::Failed
            }
        };
        message.status = status;
        if let Err(error) = self.store.set_message_status(message.id, status).await {
            tracing::warn!(%error, "failed to record the message status");
        }

        if status == MessageStatus::Sent {
            self.note_activity(peer, message.sent_at.as_i64());
        }
        let _ = self.events.send(CoreEvent::Message {
            message: message.clone(),
        });
        self.emit_peers();
        Ok(message)
    }

    /// Lets a forgotten device back into the list without waiting for it to reappear.
    ///
    /// A forgotten device that announces itself is discovered as new on its own (that is what
    /// "forget" means for the user), so this exists for the other case: a device that has
    /// been forgotten and is *not* on the network right now, which the settings screen can
    /// still offer to bring back.
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

    // ---- Transport input ------------------------------------------------------

    async fn handle_transport(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::Connected {
                role,
                handshake,
                link,
                remote,
            } => self.on_connected(role, handshake, link, remote).await,
            TransportEvent::Frame { peer, frame } => self.on_frame(peer, frame).await,
            TransportEvent::Disconnected { peer, reason } => {
                self.on_disconnected(peer, reason).await;
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
    }

    async fn on_frame(&mut self, peer: DeviceId, frame: Frame) {
        match frame {
            Frame::Chat { id, body } => self.on_chat(peer, id, body).await,

            Frame::ChatAck { id } => {
                if let Err(error) = self
                    .store
                    .set_message_status(id, MessageStatus::Delivered)
                    .await
                {
                    tracing::warn!(%error, "failed to mark a message delivered");
                    return;
                }
                let _ = self.events.send(CoreEvent::MessageStatus {
                    peer,
                    id,
                    status: MessageStatus::Delivered,
                });
            }

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

            // Liveness, shutdown and protocol errors are the connection task's business: it
            // never forwards them.
            Frame::Heartbeat { .. } | Frame::Goodbye { .. } | Frame::Error { .. } => {}

            Frame::Hello(_) | Frame::Welcome(_) => {
                tracing::warn!(%peer, "ignoring a handshake frame on an established connection");
            }
        }
    }

    async fn on_chat(&mut self, peer: DeviceId, id: MessageId, body: MessageBody) {
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
        let message = ChatMessage {
            id,
            peer,
            direction: Direction::Incoming,
            body,
            // The local clock is the only time this application trusts. On a local network
            // the difference from the sender's clock is a LAN round trip, and using one clock
            // for both directions is what keeps a conversation ordered when a peer's clock is
            // wrong — see `docs/ARCHITECTURE.md` §13.2.
            sent_at: now,
            received_at: now,
            status: MessageStatus::Received,
            read: false,
        };

        match self.store.insert_message(&message).await {
            // A retransmission: acknowledge it again so the sender stops caring, but do not
            // store it twice and do not count it twice.
            Ok(false) => tracing::debug!(%peer, %id, "ignoring a duplicate message"),
            Ok(true) => {
                entry.stored.unread = entry.stored.unread.saturating_add(1);
                entry.note_activity(now.as_i64());
                let _ = self.events.send(CoreEvent::Message {
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
        if let Some(link) = &entry.link
            && let Err(error) = link.try_send(Frame::ChatAck { id })
        {
            tracing::debug!(%peer, %error, "failed to acknowledge a message");
        }

        self.emit_peers();
    }

    async fn on_disconnected(&mut self, peer: DeviceId, reason: DisconnectReason) {
        let now = self.clock.wall().as_i64();
        let Some(entry) = self.peers.get_mut(&peer) else {
            return;
        };

        entry.link = None;
        entry.dial = None;
        if entry.presence.disconnected().is_visible() {
            tracing::info!(%peer, ?reason, "peer went offline");
        }
        entry.stored.last_seen_ms = Some(now);
        entry.note_activity(now);

        if let Err(error) = self.store.touch_peer_seen(peer, now).await {
            tracing::warn!(%error, "failed to record the last seen time");
        }
        // A message that never reached a socket must stop claiming it is on its way.
        match self.store.fail_pending_messages(peer).await {
            Ok(failed) if failed > 0 => {
                tracing::debug!(%peer, failed, "marked undeliverable messages as failed");
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "failed to mark pending messages"),
        }

        self.emit_peers();

        if reason.allows_redial() {
            self.maybe_dial(peer);
        }
    }

    // ---- Discovery input ------------------------------------------------------

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
        // `last_seen` is deliberately not stamped here: being announced is not being
        // reachable, and "last seen just now" would be a lie for a peer whose inbound port is
        // blocked.
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

        // A device the user forgot and which has appeared again is discovered as new, which
        // is exactly what the forgetting dialog promises. Its row is reused, so the settings
        // screen keeps showing a single entry for it rather than a duplicate.
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

    // ---- Presence tick --------------------------------------------------------

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
    }

    // ---- Dialling -------------------------------------------------------------

    /// Starts a dial when this peer should be dialled right now.
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

    // ---- Output ---------------------------------------------------------------

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

    fn emit_peers(&self) {
        let _ = self.events.send(CoreEvent::Peers {
            peers: self.arranged_peers(),
        });
    }

    fn notice(&self, level: NoticeLevel, message: String) {
        let _ = self.events.send(CoreEvent::Notice { level, message });
    }

    fn note_activity(&mut self, peer: DeviceId, at_ms: i64) {
        if let Some(entry) = self.peers.get_mut(&peer) {
            entry.note_activity(at_ms);
        }
    }

    // ---- Shutdown -------------------------------------------------------------

    /// Says goodbye to every peer, then lets everything drain.
    async fn shutdown(&mut self) {
        tracing::info!(peers = self.peers.len(), "shutting down the session");

        for entry in self.peers.values_mut() {
            if let Some(dial) = entry.dial.take() {
                dial.abort();
            }
            if let Some(link) = entry.link.take() {
                link.close(GoodbyeReason::Shutdown);
            }
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
