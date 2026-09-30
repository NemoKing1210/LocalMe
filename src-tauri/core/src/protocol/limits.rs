//! Protocol constants. Every bound that protects the process from a hostile peer lives
//! here so it can be reviewed in one place — see `docs/ARCHITECTURE.md` §5.5.

use std::time::Duration;

/// Version of the wire protocol this build speaks.
///
/// Peers that announce a different version complete the handshake and are then told to go
/// away with a clear `version_mismatch` error, rather than exchanging frames whose meaning
/// we cannot vouch for.
pub const PROTOCOL_VERSION: u16 = 1;

/// Largest frame we will read or write, in bytes, excluding the four-byte length prefix.
///
/// This is the single most important inbound bound: it caps the allocation a peer can
/// provoke with a four-byte header. It comfortably fits a maximum-length message
/// ([`MAX_BODY_CHARS`] characters of four-byte UTF-8 is 32 KiB plus envelope overhead).
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Bytes used by the big-endian frame length prefix.
pub const LENGTH_PREFIX_BYTES: usize = 4;

/// Maximum number of characters in a nickname.
pub const MAX_NICKNAME_CHARS: usize = 32;

/// Maximum number of characters in an avatar seed.
pub const MAX_AVATAR_SEED_CHARS: usize = 64;

/// Maximum number of characters in a message body.
pub const MAX_BODY_CHARS: usize = 8_000;

/// Maximum length of a peer-supplied error description before it is truncated for logging.
pub const MAX_ERROR_TEXT_CHARS: usize = 200;

/// Interval at which an idle connection sends a heartbeat frame.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long a connection may go without a heartbeat before the peer is considered stalled.
/// Three missed intervals, so a single dropped packet does not flap the presence state.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Deadline for the hello/welcome exchange after a socket is accepted or opened.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Deadline for a TCP connect attempt.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum inbound frames per second sustained, per connection.
pub const RATE_LIMIT_PER_SECOND: f64 = 20.0;

/// Maximum inbound frame burst, per connection.
pub const RATE_LIMIT_BURST: f64 = 40.0;

/// Maximum number of simultaneously connected peers.
pub const MAX_PEERS: usize = 128;

/// Capacity of the per-peer outbound queue.
///
/// Bounded on purpose: a peer that stops reading its socket must not be able to grow our
/// memory, so after this many queued frames the send fails and the UI marks the message
/// undeliverable.
pub const OUTBOUND_QUEUE_CAPACITY: usize = 64;

/// Capacity of the broadcast channel carrying events to the host application.
pub const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Capacity of the session command mailbox.
pub const COMMAND_CHANNEL_CAPACITY: usize = 256;

/// Delay between dial attempts after a failure, per peer.
pub const DIAL_RETRY_DELAY: Duration = Duration::from_secs(3);

/// Default TCP port. If it is taken we fall back to an ephemeral port and advertise that
/// through discovery, so a conflict degrades instead of failing.
pub const DEFAULT_TCP_PORT: u16 = 47820;

/// UDP port carrying the broadcast/multicast discovery beacon.
pub const DEFAULT_BEACON_PORT: u16 = 47821;

/// DNS-SD service type advertised and browsed.
pub const SERVICE_TYPE: &str = "_localme._tcp.local.";

/// IPv4 multicast group for the discovery beacon.
pub const BEACON_MULTICAST_ADDR: [u8; 4] = [239, 255, 77, 77];

/// Announce period while no peer is known, so a fresh instance is found quickly.
pub const BEACON_INTERVAL_IDLE: Duration = Duration::from_secs(3);

/// Announce period once at least one peer is known: enough to survive a loss, cheap enough
/// that an idle instance is invisible on the wire.
pub const BEACON_INTERVAL_SETTLED: Duration = Duration::from_secs(20);

/// How long a peer may go unseen before discovery reports it lost.
pub const DISCOVERY_TTL: Duration = Duration::from_secs(45);

/// How long shutdown waits for `goodbye` frames to reach the sockets.
pub const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// How long shutdown waits for connection tasks to finish after the drain.
pub const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Page size used when loading chat history.
pub const HISTORY_PAGE_SIZE: u32 = 50;

/// Interval of the presence tick that detects stalled peers.
///
/// This is the only periodic timer in the core besides heartbeats and the discovery
/// beacon. All three are sub-millisecond wakeups and none of them touch the front end.
pub const PRESENCE_TICK_INTERVAL: Duration = Duration::from_secs(2);
