//! mDNS/DNS-SD discovery: the primary mechanism, see `docs/ARCHITECTURE.md` §6.
//!
//! The daemon's `flume` channels are not async-friendly, so two plain threads drain them.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use mdns_sd::{
    DaemonEvent, Receiver, ResolvedService, ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo,
};
use tokio::sync::mpsc;

use crate::domain::ids::{AvatarSeed, DeviceId};
use crate::domain::nickname::Nickname;
use crate::error::DiscoveryError;
use crate::ports::discovery::{DiscoveredPeer, Discovery, DiscoveryEvent};
use crate::protocol::limits::{PROTOCOL_VERSION, SERVICE_TYPE};

use super::{OwnAnnouncement, ordered_addresses};

const BACKOFF_INITIAL_SECS: u64 = 3;

const BACKOFF_MAX_SECS: u64 = 60;

/// Discovered peers via mDNS.
///
/// Cheaper than it looks to start: creating the daemon spawns its thread, and registering and
/// browsing are two non-blocking commands. Stopping is equally cheap because [`ServiceDaemon`]
/// never blocks the caller.
pub struct MdnsDiscovery {
    /// What we announce about ourselves.
    own: OwnAnnouncement,
    /// The full instance name of our advertisement, needed to unregister it.
    service_fullname: String,
    /// The daemon while we are running; `None` before [`Discovery::start`] and after
    /// [`Discovery::stop`].
    daemon: std::sync::Mutex<Option<ServiceDaemon>>,
}

impl MdnsDiscovery {
    /// Creates the adapter for the given announcement. Touches nothing until
    /// [`Discovery::start`] is called.
    #[must_use]
    pub fn new(own: OwnAnnouncement) -> Self {
        let service_fullname = full_name(own.device_id);
        Self {
            own,
            service_fullname,
            daemon: std::sync::Mutex::new(None),
        }
    }

    /// The service record we advertise: identity in the TXT records, addresses filled in by
    /// the library from every interface it finds.
    fn service_info(&self) -> Result<ServiceInfo, DiscoveryError> {
        let device_id = self.own.device_id;
        let properties = [
            ("id", device_id.to_string()),
            ("nick", self.own.nickname.as_str().to_owned()),
            ("seed", self.own.avatar_seed.as_str().to_owned()),
            ("pv", PROTOCOL_VERSION.to_string()),
        ];
        // An empty address string plus `enable_addr_auto` means "advertise the addresses of
        // every interface, and keep them up to date as interfaces come and go".
        ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name(device_id),
            &hostname(device_id),
            "",
            self.own.port,
            properties.as_slice(),
        )
        .map(ServiceInfo::enable_addr_auto)
        .map_err(|err| DiscoveryError::Announce(err.to_string()))
    }

    /// Stops the daemon and releases the advertisement. Safe to call more than once.
    fn shutdown(&self) {
        let daemon = super::lock(&self.daemon).take();
        let Some(daemon) = daemon else {
            return;
        };
        if let Err(err) = daemon.stop_browse(SERVICE_TYPE) {
            tracing::warn!(error = %err, "failed to stop the mdns browse session");
        }
        if let Err(err) = daemon.unregister(&self.service_fullname) {
            tracing::warn!(error = %err, "failed to unregister the mdns service");
        }
        // The unregister status is deliberately not awaited: shutdown must not block the
        // caller on the network.
        shutdown_quietly(&daemon);
    }
}

impl Discovery for MdnsDiscovery {
    fn start(&self, sink: mpsc::Sender<DiscoveryEvent>) -> Result<(), DiscoveryError> {
        let mut daemon_slot = super::lock(&self.daemon);
        if daemon_slot.is_some() {
            tracing::warn!("mdns discovery is already running");
            return Ok(());
        }

        let daemon = ServiceDaemon::new().map_err(|err| DiscoveryError::Mdns(err.to_string()))?;
        // From here on every failure has to take the daemon thread down with it, or a failed
        // start would leave it running for the lifetime of the process.
        let (monitor, service_info) = match (daemon.monitor(), self.service_info()) {
            (Ok(monitor), Ok(service_info)) => (monitor, service_info),
            (Err(err), _) => {
                shutdown_quietly(&daemon);
                return Err(DiscoveryError::Mdns(err.to_string()));
            }
            (_, Err(err)) => {
                shutdown_quietly(&daemon);
                return Err(err);
            }
        };
        if let Err(err) = daemon.register(service_info) {
            shutdown_quietly(&daemon);
            return Err(DiscoveryError::Announce(err.to_string()));
        }

        let stop = Arc::new(AtomicBool::new(false));
        let restart = Arc::new(AtomicBool::new(false));
        let backoff = Arc::new(AtomicU64::new(BACKOFF_INITIAL_SECS));

        let monitored = daemon.clone();
        let monitor_stop = Arc::clone(&stop);
        let monitor_restart = Arc::clone(&restart);
        let monitor_backoff = Arc::clone(&backoff);
        if let Err(err) = spawn_thread("localme-mdns-monitor", move || {
            monitor_loop(
                monitored,
                monitor,
                monitor_stop,
                monitor_restart,
                monitor_backoff,
            );
        }) {
            shutdown_quietly(&daemon);
            return Err(err);
        }

        let browsing = daemon.clone();
        let own_device_id = self.own.device_id;
        if let Err(err) = spawn_thread("localme-mdns-browse", move || {
            browse_loop(browsing, own_device_id, sink, stop, restart, backoff);
        }) {
            shutdown_quietly(&daemon);
            return Err(err);
        }

        *daemon_slot = Some(daemon);
        Ok(())
    }

    fn stop(&self) {
        self.shutdown();
    }
}

impl Drop for MdnsDiscovery {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The DNS-SD instance name: `localme-` plus the first 20 hex characters of the device id.
///
/// The authoritative identity is the `id` TXT record, not this name; the name only has to be
/// unique and short enough for the service-name length cap (28 bytes here).
fn instance_name(device_id: DeviceId) -> String {
    format!("localme-{}", device_id.short())
}

/// The advertised host name of our service.
fn hostname(device_id: DeviceId) -> String {
    format!("{}.local.", device_id.short())
}

/// The full instance name, as it appears in a PTR record and in `unregister`.
fn full_name(device_id: DeviceId) -> String {
    format!("{}.{}", instance_name(device_id), SERVICE_TYPE)
}

/// Shuts the daemon down, logging rather than propagating: this only runs on failure paths
/// and during shutdown, where the caller cannot do anything with the error.
fn shutdown_quietly(daemon: &ServiceDaemon) {
    if let Err(err) = daemon.shutdown() {
        tracing::warn!(error = %err, "failed to shut the mdns daemon down");
    }
}

/// Spawns a named blocking thread, detaching it.
///
/// Spawn failure is propagated: the browse thread is what drains the daemon's event channel,
/// and a daemon whose channel fills up blocks inside the library.
fn spawn_thread(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), DiscoveryError> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(drop)
        .map_err(|err| DiscoveryError::Mdns(err.to_string()))
}

/// Browses our service type and forwards results, restarting the browse after daemon errors.
///
/// The thread ends when the daemon closes the channel and no restart is pending, which is
/// what [`MdnsDiscovery::shutdown`] arranges.
fn browse_loop(
    daemon: ServiceDaemon,
    own_device_id: DeviceId,
    sink: mpsc::Sender<DiscoveryEvent>,
    stop: Arc<AtomicBool>,
    restart: Arc<AtomicBool>,
    backoff: Arc<AtomicU64>,
) {
    // A `ServiceRemoved` event names the service, not the device, so the device id seen when
    // the service was resolved is remembered here. Only this thread touches the map.
    let mut known_services: HashMap<String, DeviceId> = HashMap::new();

    loop {
        let events = match daemon.browse(SERVICE_TYPE) {
            Ok(events) => events,
            Err(err) => {
                tracing::warn!(error = %err, "could not start an mdns browse");
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                sleep_backoff(&backoff);
                continue;
            }
        };

        while let Ok(event) = events.recv() {
            forward(event, own_device_id, &sink, &mut known_services);
        }

        // The channel closed: either the daemon is gone, or the monitor ended the browse so
        // that an error could be retried.
        if stop.load(Ordering::Relaxed) || !restart.swap(false, Ordering::SeqCst) {
            return;
        }
        sleep_backoff(&backoff);
        if stop.load(Ordering::Relaxed) {
            return;
        }
    }
}

/// Watches the daemon itself: logs its events, and asks for a re-browse after an error.
fn monitor_loop(
    daemon: ServiceDaemon,
    events: Receiver<DaemonEvent>,
    stop: Arc<AtomicBool>,
    restart: Arc<AtomicBool>,
    backoff: Arc<AtomicU64>,
) {
    while let Ok(event) = events.recv() {
        match event {
            DaemonEvent::Error(err) => {
                tracing::warn!(error = %err, "mdns daemon reported an error, re-browsing shortly");
                restart.store(true, Ordering::SeqCst);
                // The browse thread is parked inside `recv()` and cannot watch the flag, so
                // ending the browse session is what wakes it up.
                if let Err(err) = daemon.stop_browse(SERVICE_TYPE) {
                    tracing::warn!(error = %err, "failed to end the mdns browse session");
                }
            }
            other => {
                // Any other daemon event means the daemon is working again, so the next
                // error starts the backoff from the beginning.
                tracing::debug!(event = ?other, "mdns daemon event");
                backoff.store(BACKOFF_INITIAL_SECS, Ordering::Relaxed);
            }
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
    }
}

/// Sleeps for the current backoff, then doubles it up to [`BACKOFF_MAX_SECS`].
fn sleep_backoff(backoff: &AtomicU64) {
    let seconds = backoff
        .load(Ordering::Relaxed)
        .clamp(BACKOFF_INITIAL_SECS, BACKOFF_MAX_SECS);
    thread::sleep(Duration::from_secs(seconds));
    backoff.store(
        seconds.saturating_mul(2).min(BACKOFF_MAX_SECS),
        Ordering::Relaxed,
    );
}

/// Turns one daemon event into at most one discovery event.
fn forward(
    event: ServiceEvent,
    own_device_id: DeviceId,
    sink: &mpsc::Sender<DiscoveryEvent>,
    known_services: &mut HashMap<String, DeviceId>,
) {
    match event {
        ServiceEvent::ServiceResolved(resolved) => {
            let Some(peer) = peer_from_resolved(&resolved, own_device_id) else {
                return;
            };
            known_services.insert(resolved.fullname.clone(), peer.device_id);
            emit(sink, DiscoveryEvent::Found(peer));
        }
        ServiceEvent::ServiceRemoved(_service_type, fullname) => {
            // A service we never resolved (ours, or one whose records were incomplete) has
            // no device to report as lost.
            if let Some(device_id) = known_services.remove(&fullname) {
                emit(sink, DiscoveryEvent::Lost { device_id });
            }
        }
        other => tracing::trace!(event = ?other, "mdns service event"),
    }
}

/// Reads a resolved service into a peer, or `None` if it is ours or unusable.
///
/// The record is unauthenticated network input: every field is re-validated with the same
/// constructors the rest of the crate uses, and anything malformed is dropped quietly.
fn peer_from_resolved(
    resolved: &ResolvedService,
    own_device_id: DeviceId,
) -> Option<DiscoveredPeer> {
    let (Some(raw_id), Some(raw_nickname), Some(raw_seed)) = (
        resolved.txt_properties.get_property_val_str("id"),
        resolved.txt_properties.get_property_val_str("nick"),
        resolved.txt_properties.get_property_val_str("seed"),
    ) else {
        tracing::debug!(
            service = %resolved.fullname,
            "ignoring an mdns service without the required TXT records"
        );
        return None;
    };

    let Ok(device_id) = raw_id.parse::<DeviceId>() else {
        tracing::debug!(
            service = %resolved.fullname,
            "ignoring an mdns service with an unparseable device id"
        );
        return None;
    };
    if device_id == own_device_id {
        return None;
    }
    let Ok(nickname) = Nickname::parse(raw_nickname) else {
        tracing::debug!(
            service = %resolved.fullname,
            "ignoring an mdns service with an invalid nickname"
        );
        return None;
    };
    let Ok(avatar_seed) = AvatarSeed::parse(raw_seed) else {
        tracing::debug!(
            service = %resolved.fullname,
            "ignoring an mdns service with an invalid avatar seed"
        );
        return None;
    };

    let addresses = ordered_addresses(
        resolved.addresses.iter().map(ScopedIp::to_ip_addr),
        resolved.port,
    );
    if addresses.is_empty() {
        tracing::debug!(
            service = %resolved.fullname,
            "ignoring an mdns service that advertises no address"
        );
        return None;
    }

    Some(DiscoveredPeer {
        device_id,
        nickname,
        avatar_seed,
        addresses,
    })
}

/// Hands an event to the session, dropping it if the session is busy or has gone away.
///
/// This runs on a thread the daemon can be waiting on, so it must never wait for capacity
/// itself: a full sink costs one announcement, never a stalled mDNS daemon.
fn emit(sink: &mpsc::Sender<DiscoveryEvent>, event: DiscoveryEvent) {
    if let Err(err) = sink.try_send(event) {
        tracing::debug!(error = %err, "dropped an mdns discovery event");
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;

    const TEST_PORT: u16 = 47820;

    fn own_announcement() -> OwnAnnouncement {
        let nickname = Nickname::parse("alice").expect("valid nickname");
        let device_id = DeviceId::generate();
        OwnAnnouncement {
            device_id,
            avatar_seed: AvatarSeed::derive(device_id, &nickname),
            nickname,
            port: TEST_PORT,
        }
    }

    /// The TXT records an announcement carries, as raw strings so invalid values can be fed in.
    fn props(id: &str, nick: &str, seed: &str) -> Vec<(&'static str, String)> {
        vec![
            ("id", id.to_owned()),
            ("nick", nick.to_owned()),
            ("seed", seed.to_owned()),
        ]
    }

    /// A resolved service built through the crate's public constructor: `ResolvedService` is
    /// `#[non_exhaustive]`, so a struct literal is not available from outside `mdns-sd`.
    fn resolved(properties: &[(&str, String)], addresses: &[&str]) -> ResolvedService {
        ServiceInfo::new(
            SERVICE_TYPE,
            "peer",
            "peer.local.",
            addresses,
            TEST_PORT,
            properties,
        )
        .expect("the test service record is valid")
        .as_resolved_service()
    }

    fn resolved_peer(peer: &OwnAnnouncement, addresses: &[&str]) -> ResolvedService {
        resolved(
            &props(
                &peer.device_id.to_string(),
                peer.nickname.as_str(),
                peer.avatar_seed.as_str(),
            ),
            addresses,
        )
    }

    #[test]
    fn instance_and_host_names_fit_the_service_name_cap() {
        let device_id = DeviceId::generate();
        let instance = instance_name(device_id);

        assert_eq!(
            instance.len(),
            "localme-".len() + 20,
            "the instance name is the prefix plus the short id"
        );
        assert!(instance.len() <= 30);
        assert_eq!(instance, format!("localme-{}", device_id.short()));
        assert_eq!(hostname(device_id), format!("{}.local.", device_id.short()));
        assert_eq!(full_name(device_id), format!("{instance}.{SERVICE_TYPE}"));
    }

    #[test]
    fn the_advertised_service_carries_identity_in_its_txt_records() {
        let own = own_announcement();
        let discovery = MdnsDiscovery::new(own.clone());

        let info = discovery.service_info().expect("the announcement is valid");

        assert_eq!(info.get_type(), SERVICE_TYPE);
        assert_eq!(info.get_fullname(), full_name(own.device_id));
        assert_eq!(info.get_hostname(), hostname(own.device_id));
        assert_eq!(info.get_port(), own.port);
        let id = own.device_id.to_string();
        let version = PROTOCOL_VERSION.to_string();
        assert_eq!(info.get_property_val_str("id"), Some(id.as_str()));
        assert_eq!(info.get_property_val_str("nick"), Some("alice"));
        assert_eq!(
            info.get_property_val_str("seed"),
            Some(own.avatar_seed.as_str())
        );
        assert_eq!(info.get_property_val_str("pv"), Some(version.as_str()));
    }

    #[test]
    fn a_resolved_service_becomes_a_peer_with_ipv4_first() {
        let peer = own_announcement();
        let record = resolved_peer(&peer, &["fe80::1", "192.168.1.20"]);

        let discovered = peer_from_resolved(&record, DeviceId::generate()).expect("a usable peer");

        assert_eq!(discovered.device_id, peer.device_id);
        assert_eq!(discovered.nickname, peer.nickname);
        assert_eq!(discovered.avatar_seed, peer.avatar_seed);
        assert_eq!(
            discovered.addresses,
            vec![
                "192.168.1.20:47820"
                    .parse::<SocketAddr>()
                    .expect("valid address"),
                "[fe80::1]:47820"
                    .parse::<SocketAddr>()
                    .expect("valid address"),
            ],
            "the advertised port is applied to every address, IPv4 first"
        );
    }

    #[test]
    fn our_own_service_is_never_reported_as_a_peer() {
        let own = own_announcement();
        let record = resolved_peer(&own, &["192.168.1.20"]);

        assert!(peer_from_resolved(&record, own.device_id).is_none());
    }

    #[test]
    fn a_service_missing_required_txt_records_is_ignored() {
        let remote = DeviceId::generate();
        let seed = "seed";

        let cases = [
            props("irrelevant", "bob", seed),                            // no id
            vec![("id", remote.to_string()), ("seed", seed.to_owned())], // no nick
            vec![("id", remote.to_string()), ("nick", "bob".to_owned())], // no seed
        ];

        for properties in cases {
            let record = resolved(&properties, &["192.168.1.20"]);
            assert!(
                peer_from_resolved(&record, DeviceId::generate()).is_none(),
                "a service without id, nick and seed is unusable"
            );
        }
    }

    #[test]
    fn malformed_txt_values_are_ignored() {
        let remote = DeviceId::generate();
        let seed = "seed";

        let bad_id = resolved(&props("not-a-device-id", "bob", seed), &["192.168.1.20"]);
        assert!(peer_from_resolved(&bad_id, DeviceId::generate()).is_none());

        let bad_nick = resolved(&props(&remote.to_string(), "  ", seed), &["192.168.1.20"]);
        assert!(peer_from_resolved(&bad_nick, DeviceId::generate()).is_none());

        let bad_seed = resolved(&props(&remote.to_string(), "bob", ""), &["192.168.1.20"]);
        assert!(peer_from_resolved(&bad_seed, DeviceId::generate()).is_none());
    }

    #[test]
    fn a_service_that_advertises_no_address_is_ignored() {
        let peer = own_announcement();
        let record = resolved_peer(&peer, &[]);

        assert!(peer_from_resolved(&record, DeviceId::generate()).is_none());
    }

    #[test]
    fn a_resolved_service_is_forwarded_until_the_removal_forgets_it() {
        let own = own_announcement();
        let remote = own_announcement();
        let record = resolved_peer(&remote, &["192.168.1.30"]);
        let fullname = record.fullname.clone();
        let (sink, mut events) = mpsc::channel(4);
        let mut known = HashMap::new();

        forward(
            ServiceEvent::ServiceResolved(Box::new(record)),
            own.device_id,
            &sink,
            &mut known,
        );

        let expected_address: SocketAddr = "192.168.1.30:47820".parse().expect("valid address");
        assert_eq!(
            events.try_recv().expect("the peer is reported"),
            DiscoveryEvent::Found(DiscoveredPeer {
                device_id: remote.device_id,
                nickname: remote.nickname.clone(),
                avatar_seed: remote.avatar_seed.clone(),
                addresses: vec![expected_address],
            })
        );
        assert_eq!(known.get(&fullname), Some(&remote.device_id));

        forward(
            ServiceEvent::ServiceRemoved(SERVICE_TYPE.to_owned(), fullname),
            own.device_id,
            &sink,
            &mut known,
        );
        assert_eq!(
            events.try_recv().expect("the removal is reported"),
            DiscoveryEvent::Lost {
                device_id: remote.device_id
            }
        );
        assert!(known.is_empty(), "the service is forgotten after its loss");

        // A removal for a service that was never resolved has no device to report.
        forward(
            ServiceEvent::ServiceRemoved(
                SERVICE_TYPE.to_owned(),
                "ghost._localme._tcp.local.".to_owned(),
            ),
            own.device_id,
            &sink,
            &mut known,
        );
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn our_own_resolved_service_is_not_forwarded() {
        let own = own_announcement();
        let record = resolved_peer(&own, &["192.168.1.30"]);
        let (sink, mut events) = mpsc::channel(4);
        let mut known = HashMap::new();

        forward(
            ServiceEvent::ServiceResolved(Box::new(record)),
            own.device_id,
            &sink,
            &mut known,
        );

        assert!(known.is_empty());
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn unrelated_service_events_are_ignored() {
        let own = DeviceId::generate();
        let (sink, mut events) = mpsc::channel(4);
        let mut known = HashMap::new();

        for event in [
            ServiceEvent::SearchStarted(SERVICE_TYPE.to_owned()),
            ServiceEvent::ServiceFound(
                SERVICE_TYPE.to_owned(),
                "peer._localme._tcp.local.".to_owned(),
            ),
            ServiceEvent::SearchStopped(SERVICE_TYPE.to_owned()),
        ] {
            forward(event, own, &sink, &mut known);
        }

        assert!(known.is_empty());
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn emit_drops_events_instead_of_blocking_or_panicking() {
        let peer = own_announcement();
        let event = DiscoveryEvent::Found(DiscoveredPeer {
            device_id: peer.device_id,
            nickname: peer.nickname.clone(),
            avatar_seed: peer.avatar_seed.clone(),
            addresses: vec!["127.0.0.1:47820".parse().expect("valid address")],
        });

        let (sink, mut events) = mpsc::channel(1);
        emit(&sink, event.clone());
        emit(&sink, event.clone()); // the sink is full: dropped, never awaited
        assert_eq!(events.try_recv().expect("the first event"), event);

        drop(events);
        emit(&sink, event); // the session is gone: dropped too
    }

    #[test]
    fn stopping_a_never_started_discovery_is_a_no_op() {
        let discovery = MdnsDiscovery::new(own_announcement());

        discovery.shutdown();
        discovery.stop();
    }
}
