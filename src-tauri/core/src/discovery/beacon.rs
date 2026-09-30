//! UDP beacon discovery: a datagram announce for networks that filter mDNS.
//!
//! The protocol is deliberately minimal, because it exists to work where the smart mechanism
//! does not:
//!
//! * every instance sends an announce to the broadcast address and to the beacon multicast
//!   group, every [`BEACON_INTERVAL_IDLE`] while it has heard nobody and every
//!   [`BEACON_INTERVAL_SETTLED`] once it has;
//! * hearing an announce emits [`DiscoveryEvent::Found`] *and* answers the sender with a
//!   unicast announce (at most one per peer per [`REPLY_MIN_INTERVAL`]), which is what makes
//!   discovery work when multicast is blocked for the response direction;
//! * a peer that goes silent for [`DISCOVERY_TTL`] is reported lost, and a `bye` reports it
//!   lost immediately.
//!
//! Everything received is untrusted: size, encoding and schema are all checked, and a
//! malformed datagram is a debug log, never an error.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{self, MissedTickBehavior};

use crate::domain::ids::{AvatarSeed, DeviceId};
use crate::domain::nickname::Nickname;
use crate::error::DiscoveryError;
use crate::ports::discovery::{DiscoveredPeer, Discovery, DiscoveryEvent};
use crate::protocol::limits::{
    BEACON_INTERVAL_IDLE, BEACON_INTERVAL_SETTLED, BEACON_MULTICAST_ADDR, DISCOVERY_TTL,
};

use super::OwnAnnouncement;

/// Wire format version carried in every datagram. A datagram with any other value is
/// ignored, so a future format can be rolled out without either side logging errors.
const WIRE_VERSION: u8 = 1;

/// Largest datagram we accept, in bytes. Anything longer is dropped unparsed.
const MAX_DATAGRAM_BYTES: usize = 512;

/// Shortest interval between two unicast replies to the same peer.
const REPLY_MIN_INTERVAL: Duration = Duration::from_secs(5);

/// Age at which a reply-limiter entry is dropped.
const REPLY_PRUNE_AGE: Duration = Duration::from_secs(60);

/// How often the bookkeeping is swept for peers that went silent.
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Discovered peers over UDP broadcast and multicast.
///
/// # Socket options
///
/// The socket is bound to `0.0.0.0:<port>` with `SO_BROADCAST`, `SO_REUSEADDR` and
/// `SO_REUSEPORT` enabled and multicast joined where the platform allows it. The reuse options
/// matter for one specific case: the port being held by another process. Without them the
/// second binder fails outright and discovery loses its fallback; with them both sockets
/// receive the announcements, which is the behaviour a peer-to-peer discovery protocol wants.
pub struct UdpBeacon {
    /// What we announce about ourselves.
    own: OwnAnnouncement,
    /// The UDP port the beacon announces on and listens to.
    port: u16,
    /// The live socket and task, or `None` when stopped.
    running: Mutex<Option<Running>>,
}

/// A started beacon.
struct Running {
    /// Kept so that `stop` can say goodbye without waiting for the task to be scheduled.
    socket: Arc<UdpSocket>,
    /// The receive/announce/sweep loop.
    task: JoinHandle<()>,
}

impl UdpBeacon {
    /// Creates the beacon for the given announcement and port.
    ///
    /// Nothing is bound until [`Discovery::start`], so constructing one cannot fail; pass
    /// [`crate::protocol::limits::DEFAULT_BEACON_PORT`] unless a test wants otherwise.
    #[must_use]
    pub fn new(own: OwnAnnouncement, port: u16) -> Self {
        Self {
            own,
            port,
            running: Mutex::new(None),
        }
    }

    /// Sends a goodbye and stops the task. Safe to call more than once.
    fn shutdown(&self) {
        let running = super::lock(&self.running).take();
        let Some(running) = running else {
            return;
        };
        if let Some(goodbye) = encode(Tag::Bye, &self.own) {
            for target in announce_targets(self.port) {
                // Best effort by nature, and `stop` must not block: a full send queue is a
                // datagram nobody receives, not a reason to wait.
                if let Err(err) = running.socket.try_send_to(&goodbye, target) {
                    tracing::debug!(error = %err, %target, "could not send the beacon goodbye");
                }
            }
        }
        running.task.abort();
    }
}

impl Discovery for UdpBeacon {
    fn start(&self, sink: mpsc::Sender<DiscoveryEvent>) -> Result<(), DiscoveryError> {
        let mut running = super::lock(&self.running);
        if running.is_some() {
            tracing::warn!("the udp beacon is already running");
            return Ok(());
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            DiscoveryError::Beacon(io::Error::other("the udp beacon needs a tokio runtime"))
        })?;

        // Bind and configure synchronously: `start` reports a missing port immediately, and
        // the socket is moved into the runtime afterwards.
        //
        // `SO_REUSEADDR`/`SO_REUSEPORT` are set so that the beacon port being held by another
        // process — a stale instance, a second copy of this application, anything else that
        // picked 47821 — degrades to "both sockets receive the announcements" instead of
        // "discovery is off". `socket2` is already in the dependency tree through `mdns-sd`;
        // `std` still cannot express these options.
        let socket = bind_reusable(SocketAddr::from((Ipv4Addr::UNSPECIFIED, self.port)))
            .map_err(DiscoveryError::Beacon)?;
        socket.set_broadcast(true).map_err(DiscoveryError::Beacon)?;
        if let Err(err) = socket.set_multicast_loop_v4(false) {
            tracing::warn!(error = %err, "could not disable the beacon's multicast loopback");
        }
        // Joining the group is best effort: the group is a second path to the same peers,
        // and broadcast covers the machines where the join is refused. `0.0.0.0` as the
        // interface lets the platform pick the multicast-capable one.
        let group = multicast_group();
        if let Err(err) = socket.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED) {
            tracing::warn!(
                error = %err,
                group = %group,
                "could not join the beacon multicast group, continuing with broadcast only"
            );
        }
        socket
            .set_nonblocking(true)
            .map_err(DiscoveryError::Beacon)?;
        let socket = Arc::new(UdpSocket::from_std(socket).map_err(DiscoveryError::Beacon)?);

        let task = runtime.spawn(run(Arc::clone(&socket), self.own.clone(), self.port, sink));
        *running = Some(Running { socket, task });
        Ok(())
    }

    fn stop(&self) {
        self.shutdown();
    }
}

impl Drop for UdpBeacon {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The IPv4 multicast group announcements are also sent to.
fn multicast_group() -> Ipv4Addr {
    Ipv4Addr::from(BEACON_MULTICAST_ADDR)
}

/// The two destinations every announce goes to: the local broadcast and the beacon group.
fn announce_targets(port: u16) -> [SocketAddr; 2] {
    [
        SocketAddr::from((Ipv4Addr::BROADCAST, port)),
        SocketAddr::from((multicast_group(), port)),
    ]
}

/// The `t` field: what the sender is telling us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Tag {
    /// The sender is (still) here.
    Announce,
    /// The sender is going away.
    Bye,
}

/// The JSON body of a beacon datagram.
///
/// The identity fields are flat strings so that they go through the same domain constructors
/// as any other untrusted input: a datagram carrying an invalid nickname is dropped at the
/// boundary, exactly like an invalid frame on the TCP path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Datagram {
    /// Wire format version, see [`WIRE_VERSION`].
    v: u8,
    /// Announce or goodbye.
    t: Tag,
    /// The sender's device id, as a full UUID string.
    id: String,
    /// The sender's nickname.
    nick: String,
    /// The sender's avatar seed.
    seed: String,
    /// The TCP port the sender listens on.
    port: u16,
}

/// A beacon payload that passed validation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Announcement {
    /// The announcing device.
    device_id: DeviceId,
    /// The nickname it announced.
    nickname: Nickname,
    /// The avatar seed it announced.
    avatar_seed: AvatarSeed,
    /// The TCP port it listens on.
    port: u16,
}

impl Announcement {
    /// The peer to report, dialled at the address the datagram came from and the port it
    /// announced.
    fn discovered(self, from: SocketAddr) -> DiscoveredPeer {
        DiscoveredPeer {
            device_id: self.device_id,
            nickname: self.nickname,
            avatar_seed: self.avatar_seed,
            addresses: vec![SocketAddr::new(from.ip(), self.port)],
        }
    }
}

/// Binds a UDP socket with the reuse options that let two processes share the port.
///
/// On Windows only `SO_REUSEADDR` exists; on Unix `SO_REUSEPORT` is what actually distributes
/// datagrams between the sockets. Setting whichever the platform has is the most that can be
/// done from here, and both outcomes are better than refusing to start.
fn bind_reusable(address: SocketAddr) -> io::Result<std::net::UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;

    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    socket.set_reuse_port(true)?;

    socket.bind(&address.into())?;
    Ok(socket.into())
}

/// Returns `None` if serialisation fails, which cannot happen for this shape of data but
/// must not be a panic either; every caller treats it as "nothing to send".
fn encode(tag: Tag, own: &OwnAnnouncement) -> Option<Vec<u8>> {
    serde_json::to_vec(&Datagram {
        v: WIRE_VERSION,
        t: tag,
        id: own.device_id.to_string(),
        nick: own.nickname.as_str().to_owned(),
        seed: own.avatar_seed.as_str().to_owned(),
        port: own.port,
    })
    .ok()
}

/// Decodes a datagram received from the network.
///
/// Returns `None` for an oversized datagram, one that is not UTF-8, one that does not match
/// the schema, one from another wire version, or one whose identity fields are invalid. All
/// of those are debug logs: the sender may be another program broadcasting on this port.
fn decode(bytes: &[u8]) -> Option<(Tag, Announcement)> {
    if bytes.len() > MAX_DATAGRAM_BYTES {
        tracing::debug!(len = bytes.len(), "ignoring an oversized beacon datagram");
        return None;
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        tracing::debug!(
            len = bytes.len(),
            "ignoring a beacon datagram that is not UTF-8"
        );
        return None;
    };
    let Ok(datagram) = serde_json::from_str::<Datagram>(text) else {
        tracing::debug!("ignoring a beacon datagram that is not a well-formed announcement");
        return None;
    };
    if datagram.v != WIRE_VERSION {
        tracing::debug!(
            version = datagram.v,
            "ignoring a beacon datagram from another wire version"
        );
        return None;
    }
    let Ok(device_id) = datagram.id.parse::<DeviceId>() else {
        tracing::debug!("ignoring a beacon datagram with an unparseable device id");
        return None;
    };
    let Ok(nickname) = Nickname::parse(&datagram.nick) else {
        tracing::debug!("ignoring a beacon datagram with an invalid nickname");
        return None;
    };
    let Ok(avatar_seed) = AvatarSeed::parse(&datagram.seed) else {
        tracing::debug!("ignoring a beacon datagram with an invalid avatar seed");
        return None;
    };
    Some((
        datagram.t,
        Announcement {
            device_id,
            nickname,
            avatar_seed,
            port: datagram.port,
        },
    ))
}

/// Limits unicast replies to one per peer per [`REPLY_MIN_INTERVAL`].
///
/// Replying is what makes the beacon work when multicast is filtered in the response
/// direction, so it has to happen often enough to be useful and rarely enough that two
/// instances cannot bounce announces off each other.
#[derive(Debug, Default)]
struct ReplyLimiter {
    /// When each peer was last answered.
    replied: HashMap<DeviceId, Instant>,
}

impl ReplyLimiter {
    /// Records an intent to reply, returning whether the reply may be sent now.
    fn allow(&mut self, peer: DeviceId, now: Instant) -> bool {
        match self.replied.get(&peer) {
            Some(last) if now.saturating_duration_since(*last) < REPLY_MIN_INTERVAL => false,
            _ => {
                self.replied.insert(peer, now);
                true
            }
        }
    }

    /// Drops entries that can no longer suppress anything, so the map stays bounded by the
    /// number of peers seen in the last [`REPLY_PRUNE_AGE`].
    fn prune(&mut self, now: Instant) {
        self.replied
            .retain(|_, last| now.saturating_duration_since(*last) < REPLY_PRUNE_AGE);
    }
}

/// Tracks when each peer was last heard, so a silent peer is reported lost exactly once.
#[derive(Debug, Default)]
struct Liveness {
    /// When each known peer last announced itself.
    last_heard: HashMap<DeviceId, Instant>,
}

impl Liveness {
    /// Records a sighting of a peer.
    fn heard(&mut self, peer: DeviceId, now: Instant) {
        self.last_heard.insert(peer, now);
    }

    /// Removes and returns every peer not heard within [`DISCOVERY_TTL`].
    ///
    /// Removing them here is what makes the report happen once: a peer that is returned is
    /// forgotten, so only a fresh announce can bring it back.
    fn expire(&mut self, now: Instant) -> Vec<DeviceId> {
        let mut expired = Vec::new();
        self.last_heard.retain(|peer, last| {
            let silent = now.saturating_duration_since(*last) >= DISCOVERY_TTL;
            if silent {
                expired.push(*peer);
            }
            !silent
        });
        expired
    }

    /// Forgets a peer immediately, returning whether it was known.
    fn forget(&mut self, peer: DeviceId) -> bool {
        self.last_heard.remove(&peer).is_some()
    }
}

/// Sends one datagram, logging rather than failing.
///
/// A beacon that cannot reach one destination — a machine with no multicast route, for
/// instance — still has to keep the other direction working.
async fn send_to(socket: &UdpSocket, payload: &[u8], target: SocketAddr) {
    if let Err(err) = socket.send_to(payload, target).await {
        tracing::debug!(error = %err, %target, "could not send a beacon datagram");
    }
}

/// The beacon's single task: announce, listen, and sweep the bookkeeping.
async fn run(
    socket: Arc<UdpSocket>,
    own: OwnAnnouncement,
    port: u16,
    sink: mpsc::Sender<DiscoveryEvent>,
) {
    let announce = encode(Tag::Announce, &own);
    let targets = announce_targets(port);
    let mut limiter = ReplyLimiter::default();
    let mut liveness = Liveness::default();
    // Once a peer has been heard the period stays settled even if that peer is lost: flapping
    // between the two periods would only make the traffic less predictable.
    let mut settled = false;

    // The first tick of a fresh interval completes immediately, which is what sends the
    // opening announce.
    let mut announce_timer = time::interval(BEACON_INTERVAL_IDLE);
    announce_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut sweep = time::interval(SWEEP_INTERVAL);
    sweep.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // One byte more than the limit, so a datagram that hits the limit is recognisably not
    // within it instead of being silently truncated to something parseable.
    let mut buffer = vec![0_u8; MAX_DATAGRAM_BYTES + 1];

    loop {
        tokio::select! {
            _ = announce_timer.tick() => {
                let Some(payload) = announce.as_deref() else {
                    continue;
                };
                for target in targets {
                    send_to(&socket, payload, target).await;
                }
            }
            _ = sweep.tick() => {
                let now = Instant::now();
                limiter.prune(now);
                for device_id in liveness.expire(now) {
                    if sink.send(DiscoveryEvent::Lost { device_id }).await.is_err() {
                        return;
                    }
                }
            }
            received = socket.recv_from(&mut buffer) => {
                let Ok((len, from)) = received else {
                    tracing::debug!("the beacon socket reported a receive error");
                    continue;
                };
                let Some(bytes) = buffer.get(..len) else {
                    continue;
                };
                let Some((tag, announcement)) = decode(bytes) else {
                    continue;
                };
                // Our own datagram, looped back by the network stack.
                if announcement.device_id == own.device_id {
                    continue;
                }
                let now = Instant::now();
                match tag {
                    Tag::Announce => {
                        if !settled {
                            settled = true;
                            // The period cannot be changed in place, so the single interval is
                            // replaced here; there is still only ever one announce timer.
                            announce_timer = time::interval_at(
                                time::Instant::now() + BEACON_INTERVAL_SETTLED,
                                BEACON_INTERVAL_SETTLED,
                            );
                            announce_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
                        }
                        let device_id = announcement.device_id;
                        liveness.heard(device_id, now);
                        let peer = announcement.discovered(from);
                        if sink.send(DiscoveryEvent::Found(peer)).await.is_err() {
                            return;
                        }
                        if limiter.allow(device_id, now) {
                            if let Some(payload) = announce.as_deref() {
                                send_to(&socket, payload, from).await;
                            }
                        }
                    }
                    Tag::Bye => {
                        if liveness.forget(announcement.device_id) {
                            let lost = DiscoveryEvent::Lost {
                                device_id: announcement.device_id,
                            };
                            if sink.send(lost).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two sockets on one port is the whole point of the reuse options, and it is what makes a
    /// second instance on the same machine degrade to "both receive" instead of "no discovery".
    #[test]
    fn two_sockets_can_share_the_beacon_port() {
        let first = bind_reusable(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).expect("first bind");
        let port = first.local_addr().expect("address").port();

        let second = bind_reusable(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
        assert!(
            second.is_ok(),
            "a second socket on the same port must be allowed: {:?}",
            second.err()
        );

        let sender =
            std::net::UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).expect("sender");
        sender
            .send_to(b"announce", SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .expect("send");

        // Whichever socket the platform delivers to, the datagram must arrive somewhere: that
        // is the difference between this and a failed bind.
        let second = second.expect("second socket");
        let mut buffer = [0_u8; 32];
        let received = first
            .set_nonblocking(false)
            .and_then(|()| first.recv(&mut buffer))
            .or_else(|_| {
                second.set_nonblocking(false)?;
                second.recv(&mut buffer)
            });
        assert!(
            received.is_ok(),
            "the datagram was delivered to neither socket"
        );
    }

    /// A stand-in for a peer's announcement.
    fn own_named(nickname: &str) -> OwnAnnouncement {
        let nickname = Nickname::parse(nickname).expect("valid nickname");
        let device_id = DeviceId::generate();
        OwnAnnouncement {
            device_id,
            avatar_seed: AvatarSeed::derive(device_id, &nickname),
            nickname,
            port: 47820,
        }
    }

    fn decode_one(bytes: &[u8]) -> Option<(Tag, Announcement)> {
        decode(bytes)
    }

    #[test]
    fn an_announce_round_trips_through_the_wire_format() {
        let own = own_named("alice");
        let encoded = encode(Tag::Announce, &own).expect("serialisable announcement");

        let (tag, announcement) = decode_one(&encoded).expect("our own datagram decodes");

        assert_eq!(tag, Tag::Announce);
        assert_eq!(announcement.device_id, own.device_id);
        assert_eq!(announcement.nickname, own.nickname);
        assert_eq!(announcement.avatar_seed, own.avatar_seed);
        assert_eq!(announcement.port, own.port);
    }

    #[test]
    fn a_goodbye_round_trips_through_the_wire_format() {
        let own = own_named("bob");
        let encoded = encode(Tag::Bye, &own).expect("serialisable goodbye");

        let (tag, announcement) = decode_one(&encoded).expect("our own datagram decodes");

        assert_eq!(tag, Tag::Bye);
        assert_eq!(announcement.device_id, own.device_id);
    }

    #[test]
    fn a_resolved_announcement_is_dialled_at_its_source_address() {
        let own = own_named("carol");
        let from: SocketAddr = "192.168.1.44:47821".parse().expect("valid socket address");
        let (_, announcement) =
            decode_one(&encode(Tag::Announce, &own).expect("encoded")).expect("decodes");

        let peer = announcement.discovered(from);

        assert_eq!(
            peer.addresses,
            vec![SocketAddr::from((
                "192.168.1.44"
                    .parse::<std::net::IpAddr>()
                    .expect("valid ip"),
                47820
            ))],
            "the peer is dialled at the address the datagram came from and the port it announced"
        );
    }

    #[test]
    fn an_oversized_datagram_is_rejected() {
        let own = own_named("dave");
        let mut encoded = encode(Tag::Announce, &own).expect("encoded");
        assert!(encoded.len() <= MAX_DATAGRAM_BYTES);

        // Still a valid announcement, merely padded past the limit.
        encoded.resize(MAX_DATAGRAM_BYTES + 1, b' ');
        assert!(decode_one(&encoded).is_none());
    }

    #[test]
    fn an_unknown_wire_version_is_rejected() {
        let body = format!(
            r#"{{"v":2,"t":"announce","id":"{}","nick":"eve","seed":"s","port":47820}}"#,
            DeviceId::generate()
        );

        assert!(decode_one(body.as_bytes()).is_none());
    }

    #[test]
    fn an_unknown_tag_is_rejected() {
        let body = format!(
            r#"{{"v":1,"t":"shout","id":"{}","nick":"eve","seed":"s","port":47820}}"#,
            DeviceId::generate()
        );

        assert!(decode_one(body.as_bytes()).is_none());
    }

    #[test]
    fn a_datagram_that_is_not_utf8_or_not_json_is_rejected() {
        assert!(decode_one(&[0xff, 0xfe, 0xfd, 0x00]).is_none());
        assert!(decode_one(b"").is_none());
        assert!(decode_one(b"not json at all").is_none());
        assert!(
            decode_one(
                br#"{"v":1,"t":"announce","id":"not-a-uuid","nick":"e","seed":"s","port":1}"#
            )
            .is_none()
        );
        assert!(decode_one(br#"{"v":1,"t":"announce","id":"6a1f9e2c-1f8c-4b1c-9b3e-2e6bd0f6f1a1","nick":"","seed":"s","port":1}"#).is_none());
        assert!(decode_one(br#"{"v":1,"t":"announce","id":"6a1f9e2c-1f8c-4b1c-9b3e-2e6bd0f6f1a1","nick":"ok","seed":"","port":1}"#).is_none());
    }

    #[test]
    fn replies_are_rate_limited_per_peer() {
        let mut limiter = ReplyLimiter::default();
        let peer = DeviceId::generate();
        let other = DeviceId::generate();
        let start = Instant::now();

        assert!(limiter.allow(peer, start), "the first reply goes out");
        assert!(
            !limiter.allow(peer, start + REPLY_MIN_INTERVAL - Duration::from_millis(1)),
            "a second reply inside the interval must be suppressed"
        );
        assert!(
            limiter.allow(peer, start + REPLY_MIN_INTERVAL),
            "the interval has passed, so the peer may be answered again"
        );
        assert!(
            limiter.allow(other, start),
            "the limit is per peer, not global"
        );
    }

    #[test]
    fn stale_reply_entries_are_pruned() {
        let mut limiter = ReplyLimiter::default();
        let stale = DeviceId::generate();
        let fresh = DeviceId::generate();
        let start = Instant::now();

        assert!(limiter.allow(stale, start));
        assert!(limiter.allow(fresh, start + REPLY_PRUNE_AGE));
        limiter.prune(start + REPLY_PRUNE_AGE);

        assert_eq!(limiter.replied.len(), 1, "only the fresh entry survives");
        assert!(limiter.replied.contains_key(&fresh));
    }

    #[test]
    fn a_peer_that_stays_silent_is_reported_lost_exactly_once() {
        let mut liveness = Liveness::default();
        let peer = DeviceId::generate();
        let start = Instant::now();

        liveness.heard(peer, start);

        assert!(
            liveness
                .expire(start + DISCOVERY_TTL - Duration::from_secs(1))
                .is_empty(),
            "nothing expires before the TTL"
        );
        assert_eq!(
            liveness.expire(start + DISCOVERY_TTL),
            vec![peer],
            "the silent peer expires at the TTL"
        );
        assert!(
            liveness
                .expire(start + DISCOVERY_TTL + Duration::from_secs(120))
                .is_empty(),
            "the loss is reported once: an expired peer is forgotten"
        );
    }

    #[test]
    fn a_fresh_announce_keeps_a_peer_alive() {
        let mut liveness = Liveness::default();
        let peer = DeviceId::generate();
        let start = Instant::now();

        liveness.heard(peer, start);
        liveness.heard(peer, start + DISCOVERY_TTL - Duration::from_secs(1));

        assert!(
            liveness.expire(start + DISCOVERY_TTL).is_empty(),
            "the TTL runs from the last sighting, not the first"
        );
    }

    #[test]
    fn a_goodbye_forgets_a_known_peer_only() {
        let mut liveness = Liveness::default();
        let peer = DeviceId::generate();
        let start = Instant::now();
        liveness.heard(peer, start);

        assert!(liveness.forget(peer), "a known peer is forgotten");
        assert!(
            !liveness.forget(peer),
            "a peer that already left is not reported twice"
        );
        assert!(
            !liveness.forget(DeviceId::generate()),
            "an unknown peer produces no goodbye of its own"
        );
        assert!(liveness.expire(start + DISCOVERY_TTL).is_empty());
    }

    #[test]
    fn an_announce_is_sent_to_both_the_broadcast_and_the_group() {
        let targets = announce_targets(47821);

        assert!(targets.contains(&SocketAddr::from((Ipv4Addr::BROADCAST, 47821))));
        assert!(targets.contains(&SocketAddr::from((
            Ipv4Addr::from(BEACON_MULTICAST_ADDR),
            47821
        ))));
    }
}
