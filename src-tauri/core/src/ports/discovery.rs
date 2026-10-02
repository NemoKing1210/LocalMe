//! Discovery port: how the service layer learns that another instance exists.

use std::net::SocketAddr;

use tokio::sync::mpsc;

use crate::domain::ids::{AvatarSeed, DeviceId};
use crate::domain::nickname::Nickname;
use crate::error::DiscoveryError;

/// A peer observed on the network.
///
/// The nickname and avatar seed come from the announcement's payload, which is
/// unauthenticated: good enough to show in a list and to decide *where* to dial, and replaced
/// by the handshake's values as soon as a connection is established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    pub device_id: DeviceId,
    pub nickname: Nickname,
    pub avatar_seed: AvatarSeed,
    pub addresses: Vec<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    /// A peer is (still) present, with its current addresses.
    ///
    /// Repeats are expected and useful: they are how an address change or a wake from sleep
    /// becomes visible, so the session treats every `Found` as a reason to check the connection
    /// rather than a one-off notification.
    Found(DiscoveredPeer),
    /// A peer announced a graceful goodbye, or discovery concluded it is gone.
    Lost { device_id: DeviceId },
}

impl DiscoveryEvent {
    #[must_use]
    pub fn device_id(&self) -> DeviceId {
        match self {
            Self::Found(peer) => peer.device_id,
            Self::Lost { device_id } => *device_id,
        }
    }
}

/// A source of [`DiscoveryEvent`]s.
///
/// The sink is bounded, so a source that floods the session applies back pressure instead of
/// growing memory; a source must never block forever on it, which is why `start` is called
/// with a clone of a channel the session drains continuously.
pub trait Discovery: Send + Sync + 'static {
    /// # Errors
    ///
    /// Returns [`DiscoveryError`] if the underlying mechanism cannot be initialised. A failure
    /// here is not fatal: the session keeps running with whatever other sources started.
    fn start(&self, sink: mpsc::Sender<DiscoveryEvent>) -> Result<(), DiscoveryError>;

    fn stop(&self);
}
