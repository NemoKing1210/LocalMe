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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

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

    #[test]
    fn the_most_recent_address_set_wins_across_sources() {
        let mut aggregator = Aggregator::default();
        let device_id = DeviceId::generate();
        let stale = announced(device_id, &["192.168.1.20:47820"]);
        let fresh = announced(device_id, &["192.168.1.21:47820"]);

        assert_eq!(
            aggregator.apply(0, DiscoveryEvent::Found(stale.clone())),
            Some(DiscoveryEvent::Found(stale))
        );
        assert_eq!(
            aggregator.apply(1, DiscoveryEvent::Found(fresh.clone())),
            Some(DiscoveryEvent::Found(fresh)),
            "a newer address set replaces the tracked one and is reported again"
        );
    }

    /// A discovery source whose start can be made to fail. It records the sink it is given so
    /// the test can feed events through it.
    #[derive(Default)]
    struct MockSource {
        starts: AtomicUsize,
        stops: AtomicUsize,
        fail: AtomicBool,
        sink: Mutex<Option<mpsc::Sender<DiscoveryEvent>>>,
    }

    impl MockSource {
        fn new(fail: bool) -> Self {
            Self {
                fail: AtomicBool::new(fail),
                ..Self::default()
            }
        }

        /// The sink the composite handed this source, so the test can push events through it.
        fn feed(&self) -> mpsc::Sender<DiscoveryEvent> {
            self.sink
                .lock()
                .expect("the mock sink lock")
                .clone()
                .expect("the source was started")
        }
    }

    impl Discovery for Arc<MockSource> {
        fn start(&self, sink: mpsc::Sender<DiscoveryEvent>) -> Result<(), DiscoveryError> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(DiscoveryError::Beacon(io::Error::other(
                    "this source refuses to start",
                )));
            }
            *self.sink.lock().expect("the mock sink lock") = Some(sink);
            Ok(())
        }

        fn stop(&self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn sources(first: &Arc<MockSource>, second: &Arc<MockSource>) -> Vec<Box<dyn Discovery>> {
        vec![Box::new(Arc::clone(first)), Box::new(Arc::clone(second))]
    }

    async fn next_event(events: &mut mpsc::Receiver<DiscoveryEvent>) -> DiscoveryEvent {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("an event arrived before the timeout")
            .expect("the composite sink stayed open")
    }

    #[test]
    fn the_composite_needs_a_tokio_runtime_to_start() {
        let composite = CompositeDiscovery::new(Vec::new(), DeviceId::generate());
        let (sink, _events) = mpsc::channel(1);

        let error = Discovery::start(&composite, sink).expect_err("there is no runtime here");

        assert!(matches!(error, DiscoveryError::Beacon(_)));
    }

    #[test]
    fn shutdown_fans_out_to_every_source() {
        let first = Arc::new(MockSource::new(false));
        let second = Arc::new(MockSource::new(false));
        let composite = CompositeDiscovery::new(sources(&first, &second), DeviceId::generate());

        Discovery::stop(&composite);
        assert_eq!(first.stops.load(Ordering::SeqCst), 1);
        assert_eq!(second.stops.load(Ordering::SeqCst), 1);

        // Dropping stops them again, and repeated shutdown stays safe.
        drop(composite);
        assert_eq!(first.stops.load(Ordering::SeqCst), 2);
        assert_eq!(second.stops.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_source_that_fails_to_start_is_skipped_without_disabling_the_others() {
        let bad = Arc::new(MockSource::new(true));
        let good = Arc::new(MockSource::new(false));
        let composite = CompositeDiscovery::new(sources(&bad, &good), DeviceId::generate());
        let (sink, mut events) = mpsc::channel(4);

        Discovery::start(&composite, sink).expect("the composite starts regardless of one source");
        // A second start while running is a no-op, not an error.
        Discovery::start(&composite, mpsc::channel(1).0).expect("a second start is a no-op");

        assert_eq!(bad.starts.load(Ordering::SeqCst), 1);
        assert_eq!(good.starts.load(Ordering::SeqCst), 1);
        assert!(
            bad.sink.lock().expect("the mock sink lock").is_none(),
            "the source that failed was never handed a sink"
        );

        let peer = announced(DeviceId::generate(), &["192.168.1.40:47820"]);
        good.feed()
            .send(DiscoveryEvent::Found(peer.clone()))
            .await
            .expect("feed the surviving source");
        assert_eq!(next_event(&mut events).await, DiscoveryEvent::Found(peer));

        Discovery::stop(&composite);
        assert_eq!(bad.stops.load(Ordering::SeqCst), 1);
        assert_eq!(good.stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn events_are_deduplicated_across_sources_and_our_own_id_is_filtered() {
        let own = DeviceId::generate();
        let first = Arc::new(MockSource::new(false));
        let second = Arc::new(MockSource::new(false));
        let composite = CompositeDiscovery::new(sources(&first, &second), own);
        let (sink, mut events) = mpsc::channel(4);
        Discovery::start(&composite, sink).expect("the composite starts");

        let peer = announced(DeviceId::generate(), &["192.168.1.20:47820"]);
        first
            .feed()
            .send(DiscoveryEvent::Found(peer.clone()))
            .await
            .expect("feed the first source");
        assert_eq!(
            next_event(&mut events).await,
            DiscoveryEvent::Found(peer.clone())
        );

        // The second source seeing the same peer the same way is not news; the sentinel that
        // follows it proves the repeat was processed and discarded.
        let sentinel = announced(DeviceId::generate(), &["192.168.1.30:47820"]);
        second
            .feed()
            .send(DiscoveryEvent::Found(peer.clone()))
            .await
            .expect("feed the second source");
        second
            .feed()
            .send(DiscoveryEvent::Found(sentinel.clone()))
            .await
            .expect("feed the sentinel");
        assert_eq!(
            next_event(&mut events).await,
            DiscoveryEvent::Found(sentinel)
        );

        // Our own announcement is dropped at the composite boundary, never forwarded.
        let own_peer = announced(own, &["192.168.1.99:47820"]);
        let sentinel2 = announced(DeviceId::generate(), &["192.168.1.31:47820"]);
        first
            .feed()
            .send(DiscoveryEvent::Found(own_peer))
            .await
            .expect("feed our own announcement");
        first
            .feed()
            .send(DiscoveryEvent::Found(sentinel2.clone()))
            .await
            .expect("feed the sentinel");
        assert_eq!(
            next_event(&mut events).await,
            DiscoveryEvent::Found(sentinel2),
            "our own device id must never reach the session"
        );

        // One source dropping the peer is not the loss: the other still sees it.
        let sentinel3 = announced(DeviceId::generate(), &["192.168.1.32:47820"]);
        first
            .feed()
            .send(DiscoveryEvent::Lost {
                device_id: peer.device_id,
            })
            .await
            .expect("feed a loss from the first source");
        first
            .feed()
            .send(DiscoveryEvent::Found(sentinel3.clone()))
            .await
            .expect("feed the sentinel");
        assert_eq!(
            next_event(&mut events).await,
            DiscoveryEvent::Found(sentinel3)
        );

        // The last source dropping it is the loss.
        second
            .feed()
            .send(DiscoveryEvent::Lost {
                device_id: peer.device_id,
            })
            .await
            .expect("feed a loss from the second source");
        assert_eq!(
            next_event(&mut events).await,
            DiscoveryEvent::Lost {
                device_id: peer.device_id
            }
        );

        Discovery::stop(&composite);
        assert_eq!(first.stops.load(Ordering::SeqCst), 1);
        assert_eq!(second.stops.load(Ordering::SeqCst), 1);
    }
}
