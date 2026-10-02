//! Composite discovery: one event stream out of several sources.
//! A peer is reported once per distinct address set and lost only when the last source that had it says so.

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::sync::Mutex;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::domain::ids::DeviceId;
use crate::error::DiscoveryError;
use crate::ports::discovery::{DiscoveredPeer, Discovery, DiscoveryEvent};

/// Small on purpose: back pressure on a source beats unbounded memory.
const FEED_CAPACITY: usize = 64;

/// A source that fails to start is skipped rather than disabling the composite.
pub struct CompositeDiscovery {
    sources: Vec<Box<dyn Discovery>>,
    /// Own device id, filtered here as a last line of defence before the session.
    own_device_id: DeviceId,
    running: Mutex<Option<Running>>,
}

struct Running {
    tasks: Vec<JoinHandle<()>>,
}

impl CompositeDiscovery {
    #[must_use]
    pub fn new(sources: Vec<Box<dyn Discovery>>, own_device_id: DeviceId) -> Self {
        Self {
            sources,
            own_device_id,
            running: Mutex::new(None),
        }
    }

    /// Safe to call more than once.
    fn shutdown(&self) {
        for source in &self.sources {
            source.stop();
        }
        let running = super::lock(&self.running).take();
        if let Some(running) = running {
            for task in &running.tasks {
                task.abort();
            }
        }
    }
}

impl Discovery for CompositeDiscovery {
    fn start(&self, sink: mpsc::Sender<DiscoveryEvent>) -> Result<(), DiscoveryError> {
        let mut running = super::lock(&self.running);
        if running.is_some() {
            tracing::warn!("composite discovery is already running");
            return Ok(());
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            DiscoveryError::Beacon(io::Error::other(
                "composite discovery needs a tokio runtime",
            ))
        })?;

        // Each source needs its own channel so events can be attributed to their source for de-duplication.
        let (tagged, mut merged) = mpsc::channel::<(usize, DiscoveryEvent)>(FEED_CAPACITY);
        let mut tasks = Vec::with_capacity(self.sources.len() + 1);

        for (index, source) in self.sources.iter().enumerate() {
            let (feed, mut events) = mpsc::channel::<DiscoveryEvent>(FEED_CAPACITY);
            let forward = tagged.clone();
            tasks.push(runtime.spawn(async move {
                while let Some(event) = events.recv().await {
                    if forward.send((index, event)).await.is_err() {
                        return;
                    }
                }
            }));
            if let Err(err) = source.start(feed) {
                tracing::warn!(
                    source = index,
                    error = %err,
                    "a discovery source could not be started, continuing without it"
                );
            }
        }
        // Dropping our own sender lets the aggregator finish when every source stops.
        drop(tagged);

        let own_device_id = self.own_device_id;
        tasks.push(runtime.spawn(async move {
            let mut aggregator = Aggregator::default();
            while let Some((source, event)) = merged.recv().await {
                if event.device_id() == own_device_id {
                    continue;
                }
                if let Some(event) = aggregator.apply(source, event) {
                    if sink.send(event).await.is_err() {
                        return;
                    }
                }
            }
        }));

        *running = Some(Running { tasks });
        Ok(())
    }

    fn stop(&self) {
        self.shutdown();
    }
}

impl Drop for CompositeDiscovery {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug)]
struct Tracked {
    peer: DiscoveredPeer,
    sources: BTreeSet<usize>,
}

/// Merges events from several sources into one de-duplicated stream.
#[derive(Debug, Default)]
struct Aggregator {
    live: HashMap<DeviceId, Tracked>,
}

impl Aggregator {
    fn apply(&mut self, source: usize, event: DiscoveryEvent) -> Option<DiscoveryEvent> {
        match event {
            DiscoveryEvent::Found(peer) => match self.live.get_mut(&peer.device_id) {
                Some(tracked) => {
                    tracked.sources.insert(source);
                    if tracked.peer.addresses == peer.addresses {
                        None
                    } else {
                        tracked.peer = peer.clone();
                        Some(DiscoveryEvent::Found(peer))
                    }
                }
                None => {
                    self.live.insert(
                        peer.device_id,
                        Tracked {
                            sources: BTreeSet::from([source]),
                            peer: peer.clone(),
                        },
                    );
                    Some(DiscoveryEvent::Found(peer))
                }
            },
            DiscoveryEvent::Lost { device_id } => {
                let tracked = self.live.get_mut(&device_id)?;
                tracked.sources.remove(&source);
                if !tracked.sources.is_empty() {
                    // Another mechanism still has the peer: it is not lost yet.
                    return None;
                }
                self.live.remove(&device_id);
                Some(DiscoveryEvent::Lost { device_id })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use crate::domain::ids::AvatarSeed;
    use crate::domain::nickname::Nickname;

    use super::*;

    /// A peer with the given addresses, as a source would report it.
    fn announced(device_id: DeviceId, addresses: &[&str]) -> DiscoveredPeer {
        let nickname = Nickname::parse("alice").expect("valid nickname");
        DiscoveredPeer {
            device_id,
            avatar_seed: AvatarSeed::derive(device_id, &nickname),
            nickname,
            addresses: addresses
                .iter()
                .map(|address| address.parse::<SocketAddr>().expect("valid address"))
                .collect(),
        }
    }

    #[test]
    fn a_peer_seen_by_two_sources_is_reported_once() {
        let mut aggregator = Aggregator::default();
        let device_id = DeviceId::generate();
        let peer = announced(device_id, &["192.168.1.20:47820"]);

        assert_eq!(
            aggregator.apply(0, DiscoveryEvent::Found(peer.clone())),
            Some(DiscoveryEvent::Found(peer.clone())),
            "the first source reports the peer"
        );
        assert_eq!(
            aggregator.apply(1, DiscoveryEvent::Found(peer.clone())),
            None,
            "the second source sees the same peer the same way"
        );
        assert_eq!(
            aggregator.apply(0, DiscoveryEvent::Found(peer)),
            None,
            "a repeat from a known source is not news"
        );
    }

    #[test]
    fn a_peer_is_lost_only_when_the_last_source_drops_it() {
        let mut aggregator = Aggregator::default();
        let device_id = DeviceId::generate();
        let peer = announced(device_id, &["192.168.1.20:47820"]);
        aggregator.apply(0, DiscoveryEvent::Found(peer.clone()));
        aggregator.apply(1, DiscoveryEvent::Found(peer));

        assert_eq!(
            aggregator.apply(0, DiscoveryEvent::Lost { device_id }),
            None,
            "the peer is still visible to the other source"
        );
        assert_eq!(
            aggregator.apply(1, DiscoveryEvent::Lost { device_id }),
            Some(DiscoveryEvent::Lost { device_id }),
            "the last source dropping the peer is the loss"
        );
        assert_eq!(
            aggregator.apply(0, DiscoveryEvent::Lost { device_id }),
            None,
            "a loss for a peer nobody has is not repeated"
        );
    }

    #[test]
    fn a_source_finding_the_peer_again_after_a_loss_reports_it_again() {
        let mut aggregator = Aggregator::default();
        let device_id = DeviceId::generate();
        let peer = announced(device_id, &["192.168.1.20:47820"]);

        aggregator.apply(0, DiscoveryEvent::Found(peer.clone()));
        assert!(
            aggregator
                .apply(0, DiscoveryEvent::Lost { device_id })
                .is_some()
        );

        assert_eq!(
            aggregator.apply(0, DiscoveryEvent::Found(peer.clone())),
            Some(DiscoveryEvent::Found(peer)),
            "a peer that comes back is reported again"
        );
    }

    #[test]
    fn changed_addresses_are_reported_again() {
        let mut aggregator = Aggregator::default();
        let device_id = DeviceId::generate();
        let peer = announced(device_id, &["192.168.1.20:47820"]);
        let moved = announced(device_id, &["192.168.1.21:47820"]);

        aggregator.apply(0, DiscoveryEvent::Found(peer));
        assert_eq!(
            aggregator.apply(0, DiscoveryEvent::Found(moved.clone())),
            Some(DiscoveryEvent::Found(moved)),
            "a new address is how a wake from sleep or a new lease becomes visible"
        );
    }

    #[test]
    fn a_loss_from_a_source_that_never_reported_the_peer_is_ignored() {
        let mut aggregator = Aggregator::default();
        let device_id = DeviceId::generate();
        let peer = announced(device_id, &["192.168.1.20:47820"]);
        aggregator.apply(3, DiscoveryEvent::Found(peer));

        assert_eq!(
            aggregator.apply(7, DiscoveryEvent::Lost { device_id }),
            None,
            "an unrelated source cannot make a peer disappear"
        );
    }
}
