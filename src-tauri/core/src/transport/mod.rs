//! TCP transport: the listening socket, outbound dials, and one task per connection.
//!
//! The transport owns sockets and frames; it owns no policy about *who* a peer is. It reports
//! what happened on a connection as a [`TransportEvent`] and accepts writes through a
//! [`PeerLink`] handle, which is what keeps the peer table, presence and message pipeline in
//! the session actor where they can be reasoned about sequentially.
//!
//! Framing, handshake and the connection loop are in [`codec`] and [`connection`]; the
//! listening socket is in [`listener`].

pub mod codec;
pub mod connection;
pub mod listener;

pub use codec::{FrameReader, FrameWriter};
pub use connection::{ConnectionContext, Handshaken, accept, dial, serve};
pub use listener::Listener;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::mpsc;

use crate::domain::ids::DeviceId;
use crate::domain::peer::Handshake;
use crate::error::TransportError;
use crate::protocol::{Frame, GoodbyeReason, limits::OUTBOUND_QUEUE_CAPACITY};

/// Which side initiated a connection.
///
/// Part of the deterministic tie-break for simultaneous connections: both peers evaluate
/// [`is_preferred`] on the same pair of identifiers and reach the same conclusion, so exactly
/// one connection survives without any additional exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// We opened this connection.
    Dialer,
    /// The peer opened it and we accepted.
    Acceptor,
}

impl Role {
    /// A short label for logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dialer => "dialer",
            Self::Acceptor => "acceptor",
        }
    }

    /// The identifiers of this connection, as `(dialer, acceptor)`.
    #[must_use]
    pub fn endpoints(self, own: DeviceId, peer: DeviceId) -> (DeviceId, DeviceId) {
        match self {
            Self::Dialer => (own, peer),
            Self::Acceptor => (peer, own),
        }
    }
}

/// Whether a connection is the one both peers keep when they dial each other at the same time.
///
/// The rule is "the connection initiated by the peer with the smaller identifier wins". It is
/// total and stable, and both ends can evaluate it from the same two values, so the losing
/// connection is closed by both sides and the winner is never dropped by one of them.
#[must_use]
pub fn is_preferred(dialer: DeviceId, acceptor: DeviceId) -> bool {
    dialer < acceptor
}

/// Commands the connection task accepts from the session.
#[derive(Debug)]
pub enum LinkCommand {
    /// Write a frame, failing if the peer stops reading for too long.
    Send(Frame),
    /// Say goodbye and close.
    Close(GoodbyeReason),
}

/// The session's handle on one connection.
///
/// Cloning is cheap and all clones address the same connection task. Dropping every clone
/// closes the connection with a `shutdown` goodbye, which is what makes an abandoned link
/// self-cleaning rather than a leaked socket.
#[derive(Debug, Clone)]
pub struct PeerLink {
    commands: mpsc::Sender<LinkCommand>,
}

impl PeerLink {
    /// Creates a link plus the receiver the connection task drains.
    #[must_use]
    pub fn channel() -> (Self, mpsc::Receiver<LinkCommand>) {
        let (commands, receiver) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
        (Self { commands }, receiver)
    }

    /// Queues a frame, waiting for room.
    ///
    /// # Errors
    ///
    /// [`TransportError::Closed`] when the connection task has stopped.
    pub async fn send(&self, frame: Frame) -> Result<(), TransportError> {
        self.commands
            .send(LinkCommand::Send(frame))
            .await
            .map_err(|_| TransportError::Closed)
    }

    /// Queues a frame if there is room right now.
    ///
    /// Used by paths that must not block — an acknowledgement sent from the session's own
    /// loop — so the session can never be stalled by a peer that stopped reading.
    ///
    /// # Errors
    ///
    /// [`TransportError::BackPressure`] when the queue is full,
    /// [`TransportError::Closed`] when the task has stopped.
    pub fn try_send(&self, frame: Frame) -> Result<(), TransportError> {
        match self.commands.try_send(LinkCommand::Send(frame)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(TransportError::BackPressure),
            Err(mpsc::error::TrySendError::Closed(_)) => Err(TransportError::Closed),
        }
    }

    /// Asks the connection to say goodbye and close.
    pub fn close(&self, reason: GoodbyeReason) {
        // Failing here means the task is already gone, which is the desired end state.
        if self.commands.try_send(LinkCommand::Close(reason)).is_err() {
            tracing::debug!("connection was already closed before the goodbye could be queued");
        }
    }

    /// Whether the connection task has stopped.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.commands.is_closed()
    }
}

/// Why a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    /// The peer said goodbye.
    Goodbye(GoodbyeReason),
    /// We said goodbye, or the session dropped the link.
    LocalGoodbye(GoodbyeReason),
    /// The peer closed the socket between frames.
    Closed,
    /// No frame arrived for longer than the heartbeat timeout.
    Stalled,
    /// The connection failed: protocol error, socket error, or a refused handshake.
    Failed(String),
}

impl DisconnectReason {
    /// Whether the peer may be dialled again immediately.
    ///
    /// A `superseded` goodbye means "we kept the other connection", so dialling again would
    /// undo the tie-break; anything else is worth retrying once discovery reports the peer.
    #[must_use]
    pub const fn allows_redial(&self) -> bool {
        !matches!(self, Self::Goodbye(GoodbyeReason::Superseded))
    }
}

/// What the transport reports to the session.
#[derive(Debug)]
pub enum TransportEvent {
    /// A connection completed its handshake.
    Connected {
        /// Which side initiated it.
        role: Role,
        /// The peer's announced identity.
        handshake: Handshake,
        /// The handle used to write to it.
        link: PeerLink,
        /// The address the connection actually runs over.
        remote: SocketAddr,
    },
    /// The peer sent a frame the session must act on.
    ///
    /// Liveness frames (`heartbeat`) and fatal ones (`error`) are handled inside the
    /// connection task and never forwarded, so everything here is content.
    Frame {
        /// The peer that sent it.
        peer: DeviceId,
        /// The frame.
        frame: Frame,
    },
    /// A connection ended.
    Disconnected {
        /// The peer that went away.
        peer: DeviceId,
        /// Why.
        reason: DisconnectReason,
    },
    /// Every address a peer advertised failed.
    ///
    /// Reported by the dial task so the session can return the presence machine to a state
    /// that will try again, rather than leaving the peer stuck in `Connecting` forever.
    DialFailed {
        /// The peer we could not reach.
        peer: DeviceId,
        /// Why, for the log.
        reason: String,
    },
}

impl TransportEvent {
    /// The device this event is about.
    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        match self {
            Self::Connected { handshake, .. } => handshake.device_id,
            Self::Frame { peer, .. }
            | Self::Disconnected { peer, .. }
            | Self::DialFailed { peer, .. } => *peer,
        }
    }
}

/// Counts live connections so the handshake can refuse a peer beyond the configured maximum
/// without a round trip through the session mailbox.
#[derive(Debug, Clone, Default)]
pub struct LiveConnections(Arc<AtomicUsize>);

impl LiveConnections {
    /// A counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The current number of live connections.
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    /// Reserves a slot, or fails when the limit is already reached.
    ///
    /// The returned guard releases the slot when it is dropped, including on the error paths
    /// and on a panic inside the connection task.
    #[must_use]
    pub fn try_reserve(&self, max: usize) -> Option<LiveConnectionGuard> {
        let mut current = self.0.load(Ordering::Relaxed);
        loop {
            if current >= max {
                return None;
            }
            match self.0.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return Some(LiveConnectionGuard {
                        counter: self.0.clone(),
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }
}

/// Releases a reserved connection slot on drop.
#[derive(Debug)]
pub struct LiveConnectionGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for LiveConnectionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::DeviceId;

    fn id(value: u128) -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(value))
    }

    #[test]
    fn exactly_one_direction_of_a_pair_is_preferred() {
        let low = id(1);
        let high = id(2);

        // The connection opened by the smaller identifier wins, and both peers agree.
        assert!(is_preferred(low, high), "low dialling high is preferred");
        assert!(
            !is_preferred(high, low),
            "high dialling low is not, so it is closed by both ends"
        );
    }

    #[test]
    fn the_preference_is_stable_for_every_pairing() {
        // Whatever order the two dials happen in, exactly one survives.
        for a in 0..8_u128 {
            for b in (a + 1)..8_u128 {
                let preferred =
                    u8::from(is_preferred(id(a), id(b))) + u8::from(is_preferred(id(b), id(a)));
                assert_eq!(
                    preferred, 1,
                    "pair ({a}, {b}) has {preferred} preferred sides"
                );
            }
        }
    }

    #[test]
    fn endpoints_are_reported_from_both_perspectives() {
        let own = id(1);
        let peer = id(2);
        assert_eq!(Role::Dialer.endpoints(own, peer), (own, peer));
        assert_eq!(Role::Acceptor.endpoints(own, peer), (peer, own));
    }

    #[test]
    fn a_superseded_goodbye_suppresses_the_redial() {
        assert!(!DisconnectReason::Goodbye(GoodbyeReason::Superseded).allows_redial());
        assert!(DisconnectReason::Goodbye(GoodbyeReason::Shutdown).allows_redial());
        assert!(DisconnectReason::Closed.allows_redial());
        assert!(DisconnectReason::Stalled.allows_redial());
        assert!(DisconnectReason::Failed("nope".to_owned()).allows_redial());
    }

    #[test]
    fn the_live_connection_counter_enforces_its_limit() {
        let live = LiveConnections::new();
        assert_eq!(live.count(), 0);

        let first = live.try_reserve(2).expect("first slot");
        let second = live.try_reserve(2).expect("second slot");
        assert_eq!(live.count(), 2);
        assert!(live.try_reserve(2).is_none(), "the limit is enforced");

        drop(first);
        assert_eq!(live.count(), 1);
        let third = live.try_reserve(2).expect("a released slot is reusable");
        drop(second);
        drop(third);
        assert_eq!(live.count(), 0);
    }

    #[test]
    fn a_limit_of_zero_refuses_everything() {
        let live = LiveConnections::new();
        assert!(live.try_reserve(0).is_none());
        assert_eq!(live.count(), 0);
    }

    #[tokio::test]
    async fn a_dropped_link_closes_its_commands() {
        let (link, mut commands) = PeerLink::channel();
        assert!(!link.is_closed());
        drop(link);

        // The connection task sees the channel close and can shut down.
        assert!(commands.recv().await.is_none());
    }

    #[tokio::test]
    async fn a_full_queue_reports_back_pressure_instead_of_blocking() {
        let (link, _commands) = PeerLink::channel();
        for seq in 0..OUTBOUND_QUEUE_CAPACITY as u64 {
            link.try_send(Frame::Heartbeat { seq })
                .expect("the first frames fit in the queue");
        }
        assert!(matches!(
            link.try_send(Frame::Heartbeat { seq: 9_999 }),
            Err(TransportError::BackPressure)
        ));
    }

    #[tokio::test]
    async fn a_send_after_the_task_stops_is_reported_as_closed() {
        let (link, commands) = PeerLink::channel();
        drop(commands);
        assert!(matches!(
            link.send(Frame::Heartbeat { seq: 1 }).await,
            Err(TransportError::Closed)
        ));
        assert!(link.is_closed());
    }

    #[tokio::test]
    async fn commands_reach_the_connection_task_in_order() {
        let (link, mut commands) = PeerLink::channel();
        link.send(Frame::Heartbeat { seq: 1 }).await.expect("send");
        link.close(GoodbyeReason::Shutdown);

        assert!(matches!(
            commands.recv().await,
            Some(LinkCommand::Send(Frame::Heartbeat { seq: 1 }))
        ));
        assert!(matches!(
            commands.recv().await,
            Some(LinkCommand::Close(GoodbyeReason::Shutdown))
        ));
    }
}
