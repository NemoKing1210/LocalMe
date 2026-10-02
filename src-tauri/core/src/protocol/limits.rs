//! Protocol constants. Every bound that protects the process from a hostile peer lives here
//! so it can be reviewed in one place.

use std::time::Duration;

/// Peers that announce a different version complete the handshake and are then told to go away
/// with a clear `version_mismatch` error, rather than exchanging frames whose meaning we cannot
/// vouch for.
///
/// Version 2 adds the attachment frames (`file_chunk`, `file_done`, `file_ack`, `file_cancel`,
/// `file_request`) and the attachment list on `chat`. A version 1 peer has neither, so the two
/// must not be allowed to pretend they can talk.
pub const PROTOCOL_VERSION: u16 = 2;

/// The single most important inbound bound: it caps the allocation a peer can provoke with a
/// four-byte header. Comfortably fits a maximum-length message ([`MAX_BODY_CHARS`] characters
/// of four-byte UTF-8 is 32 KiB plus envelope overhead).
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

pub const LENGTH_PREFIX_BYTES: usize = 4;

pub const MAX_NICKNAME_CHARS: usize = 32;

pub const MAX_AVATAR_SEED_CHARS: usize = 64;

pub const MAX_BODY_CHARS: usize = 8_000;

pub const MAX_ERROR_TEXT_CHARS: usize = 200;

/// Largest file one attachment may carry.
///
/// Enforced on both ends for different reasons: the sender refuses to offer more, because an
/// unbounded offer is a promise it would have to keep for hours, and the recipient cancels
/// anything above the cap without writing a byte, because the size in a `chat` frame is a claim
/// from a peer and not a fact.
pub const MAX_ATTACHMENT_BYTES: u64 = 512 * 1024 * 1024;

/// Attachments in one message. A frame already bounds the count; this bounds the *work* a single
/// message can ask for.
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;

/// Characters kept of a file name after it has been sanitised for the local filesystem.
pub const MAX_FILE_NAME_CHARS: usize = 128;

/// Payload bytes carried by one `file_chunk`.
///
/// The frame is JSON, so the bytes travel base64-encoded: 4/3 of 32 KiB is 43 692 characters,
/// which leaves the whole envelope around 44 KiB — comfortably inside [`MAX_FRAME_BYTES`] with
/// room for the identifier and the offset.
///
/// It divides [`FILE_ACK_EVERY_BYTES`] exactly, so a whole number of chunks always falls inside
/// one acknowledgement, and the sender's whole window of [`FILE_WINDOW_BYTES`] fits the
/// connection's outbound queue.
pub const FILE_CHUNK_BYTES: usize = 32 * 1024;

/// How many bytes the sender may have in flight before it waits for an acknowledgement.
///
/// Without a window the sender would write the whole file into the connection's outbound queue
/// and have no way back: a recipient that has lost its partial file would have to reject
/// hundreds of chunks before the sender learned the offset.
pub const FILE_WINDOW_BYTES: u64 = 1024 * 1024;

/// Progress the recipient acknowledges. It is also the receiver's reset point after a crash, so
/// the value bounds how much a resume can repeat.
pub const FILE_ACK_EVERY_BYTES: u64 = 512 * 1024;

/// Inbound budget for `file_chunk` payload bytes, per connection.
///
/// File chunks are exempt from the per-frame bucket — a 40 MiB file is a thousand frames and
/// would trip it immediately — and are bounded in bytes instead, which is the resource that
/// actually matters. The frame-count bucket for chunks stays alongside it so that the byte
/// budget cannot be spent on millions of one-byte frames.
pub const DATA_BURST_BYTES: f64 = 8.0 * 1024.0 * 1024.0;

/// Sustained inbound file bytes per second; see [`DATA_BURST_BYTES`].
pub const DATA_RATE_PER_SECOND: f64 = 8.0 * 1024.0 * 1024.0;

pub const FILE_CHUNK_BURST: f64 = 1024.0;

pub const FILE_CHUNK_RATE_PER_SECOND: f64 = 512.0;

/// The sender's own pace.
///
/// Deliberately below the recipient's inbound budget: a drain that outruns it is refused, and a
/// refused frame closes the connection, so the transfer could never finish. The burst is small
/// enough that it cannot overshoot the recipient's burst either.
pub const FILE_SEND_BURST_BYTES: f64 = 2.0 * 1024.0 * 1024.0;

/// Sustained outbound file bytes per second; see [`FILE_SEND_BURST_BYTES`].
pub const FILE_SEND_RATE_PER_SECOND: f64 = 4.0 * 1024.0 * 1024.0;

/// How often the session looks at a transfer that is ready for its next chunk. The pacing is the
/// token bucket's, so this only has to be small enough to keep the socket busy.
pub const TRANSFER_POLL: Duration = Duration::from_millis(5);

/// Chunks one session turn writes at most, so a transfer cannot hold the actor while a message
/// is being delivered on another connection.
pub const TRANSFER_CHUNKS_PER_TURN: usize = 8;

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

    // A transfer that outruns the recipient's inbound budget is closed by the recipient, so the
    // sender's pace must stay under it — burst included, because a burst that is refused on the
    // first frame is the same as no transfer at all.
    assert!(FILE_SEND_RATE_PER_SECOND < DATA_RATE_PER_SECOND);
    assert!(FILE_SEND_BURST_BYTES <= DATA_BURST_BYTES);

    // The window has to be worth a round trip and has to fit the connection's send queue: one
    // chunk per queue slot, or the drain would stall on its own back pressure rather than on the
    // acknowledgement it is waiting for.
    assert!(FILE_WINDOW_BYTES >= FILE_ACK_EVERY_BYTES);
    assert!(FILE_WINDOW_BYTES % FILE_ACK_EVERY_BYTES == 0);
    assert!(FILE_ACK_EVERY_BYTES % FILE_CHUNK_BYTES as u64 == 0);

    // The base64 of a chunk plus the envelope has to fit one frame.
    const CHUNK_BASE64: usize = FILE_CHUNK_BYTES.div_ceil(3) * 4;
    assert!(CHUNK_BASE64 + 256 < MAX_FRAME_BYTES);
};
