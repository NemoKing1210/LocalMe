//! Protocol constants. Every bound that protects the process from a hostile peer lives here
//! so it can be reviewed in one place.

use std::time::Duration;

/// Peers that announce a different version complete the handshake and are then told to go away
/// with a clear `version_mismatch` error, rather than exchanging frames whose meaning we cannot
/// vouch for.
pub const PROTOCOL_VERSION: u16 = 1;

/// The single most important inbound bound: it caps the allocation a peer can provoke with a
/// four-byte header. Comfortably fits a maximum-length message ([`MAX_BODY_CHARS`] characters
/// of four-byte UTF-8 is 32 KiB plus envelope overhead).
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

pub const LENGTH_PREFIX_BYTES: usize = 4;

pub const MAX_NICKNAME_CHARS: usize = 32;

pub const MAX_AVATAR_SEED_CHARS: usize = 64;

pub const MAX_BODY_CHARS: usize = 8_000;

pub const MAX_ERROR_TEXT_CHARS: usize = 200;

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Three missed intervals, so a single dropped packet does not flap the presence state.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub const RATE_LIMIT_PER_SECOND: f64 = 20.0;

pub const RATE_LIMIT_BURST: f64 = 40.0;

/// How many outbox messages may leave in one burst when a peer comes back online.
///
/// The drain must stay under the *recipient's* inbound limiter (`RATE_LIMIT_BURST` /
/// `RATE_LIMIT_PER_SECOND`), which counts our `chat` frames and whose denial closes the
/// connection — a backlog that ignores it would never drain, because every attempt would trip
/// the limit, drop the link and start over. The budget is shared with the acknowledgements the
/// peer sends back for *its* backlog, so the drain is paced for two conversations' worth of
/// traffic: `2 × OUTBOX_RATE_PER_SECOND < RATE_LIMIT_PER_SECOND` and
/// `2 × OUTBOX_BURST < RATE_LIMIT_BURST`.
pub const OUTBOX_BURST: f64 = 12.0;

/// Sustained rate of the outbox drain; see [`OUTBOX_BURST`].
pub const OUTBOX_RATE_PER_SECOND: f64 = 6.0;

pub const MAX_PEERS: usize = 128;

/// Bounded on purpose: a peer that stops reading its socket must not grow our memory, so after
/// this many queued frames the send fails and the UI marks the message undeliverable.
pub const OUTBOUND_QUEUE_CAPACITY: usize = 64;

pub const EVENT_CHANNEL_CAPACITY: usize = 256;

pub const COMMAND_CHANNEL_CAPACITY: usize = 256;

pub const DIAL_RETRY_DELAY: Duration = Duration::from_secs(3);

/// If it is taken we fall back to an ephemeral port and advertise that through discovery, so a
/// conflict degrades instead of failing.
pub const DEFAULT_TCP_PORT: u16 = 47820;

pub const DEFAULT_BEACON_PORT: u16 = 47821;

pub const SERVICE_TYPE: &str = "_localme._tcp.local.";

pub const BEACON_MULTICAST_ADDR: [u8; 4] = [239, 255, 77, 77];

pub const BEACON_INTERVAL_IDLE: Duration = Duration::from_secs(3);

/// Once at least one peer is known: enough to survive a loss, cheap enough that an idle
/// instance is invisible on the wire.
pub const BEACON_INTERVAL_SETTLED: Duration = Duration::from_secs(20);

pub const DISCOVERY_TTL: Duration = Duration::from_secs(45);

pub const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

pub const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_secs(1);

pub const HISTORY_PAGE_SIZE: u32 = 50;

pub const PRESENCE_TICK_INTERVAL: Duration = Duration::from_secs(2);

// Two peers draining at once must stay under each side's inbound limiter (§5.4): the first denied
// frame closes the connection, so a pace above the limit is a backlog that can never be delivered.
// Asserted at compile time, so raising either rate past the budget fails the build.
const _: () = {
    assert!(2.0 * OUTBOX_RATE_PER_SECOND < RATE_LIMIT_PER_SECOND);
    assert!(2.0 * OUTBOX_BURST < RATE_LIMIT_BURST);
    assert!(OUTBOX_BURST >= 1.0);
};
