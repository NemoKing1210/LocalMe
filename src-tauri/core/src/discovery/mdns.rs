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
    use super::*;

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
}
