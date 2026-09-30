//! SQLite storage adapter.
//!
//! One dedicated writer thread owns the `rusqlite::Connection` (it is `!Sync`); callers
//! send a [`Request`] over a bounded channel and await a `oneshot` reply. That is the
//! storage concurrency model from `docs/ARCHITECTURE.md` §3.2: serialised writes, no lock
//! contention, no thread pool, and back pressure instead of an unbounded queue.
//!
//! On open the database is verified with `PRAGMA integrity_check`; an unusable file is
//! quarantined rather than deleted and a fresh database is created in its place, so the
//! application always starts (`docs/ARCHITECTURE.md` §7.3).

mod schema;

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::{mpsc, oneshot};

use crate::domain::clock::UnixMillis;
use crate::domain::ids::{AvatarSeed, DeviceId, MessageId};
use crate::domain::message::{ChatMessage, Direction, MessageBody, MessageStatus};
use crate::domain::nickname::Nickname;
use crate::domain::peer::PeerProfile;
use crate::error::StorageError;
use crate::ports::store::{HistoryCursor, KnownDevice, Store, StoredPeer};

use schema::migrate;

/// Capacity of the bounded request mailbox.
const REQUEST_QUEUE_CAPACITY: usize = 64;

/// Upper bound on how long `Drop` waits for the writer thread to finish.
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_secs(1);

/// A reply channel carrying one operation's outcome.
type Reply<T> = oneshot::Sender<Result<T, StorageError>>;

/// The SQLite-backed [`Store`].
///
/// Cloning is deliberately not offered: the store owns the single writer thread, and the
/// service layer shares it behind an `Arc` if it needs to.
pub struct SqliteStore {
    /// The mailbox to the writer thread; `None` once `Drop` has begun tearing it down.
    sender: Option<mpsc::Sender<Request>>,
    /// The writer thread; `None` in the closed-channel construction used by tests.
    writer: Option<thread::JoinHandle<()>>,
}

/// One unit of work for the writer thread.
enum Request {
    /// Reads one metadata value.
    MetaGet {
        /// Metadata key.
        key: String,
        /// Reply channel.
        reply: Reply<Option<String>>,
    },
    /// Writes one metadata value.
    MetaSet {
        /// Metadata key.
        key: String,
        /// Metadata value.
        value: String,
        /// Reply channel.
        reply: Reply<()>,
    },
    /// Inserts or updates a peer seen on the network.
    UpsertPeerSeen {
        /// The announced identity.
        profile: PeerProfile,
        /// Last successfully used address.
        address: Option<String>,
        /// Liveness evidence: when the peer was last confirmed online, if ever.
        seen_at_ms: Option<i64>,
        /// Reply channel.
        reply: Reply<StoredPeer>,
    },
    /// Reads one peer.
    Peer {
        /// Peer to read.
        device_id: DeviceId,
        /// Reply channel.
        reply: Reply<Option<StoredPeer>>,
    },
    /// Reads every peer.
    Peers(Reply<Vec<StoredPeer>>),
    /// Sets a peer's notification preference.
    SetPeerMuted {
        /// Peer to update.
        device_id: DeviceId,
        /// Whether notifications are suppressed.
        muted: bool,
        /// Reply channel.
        reply: Reply<()>,
    },
    /// Updates a peer's `last_seen_ms`.
    TouchPeerSeen {
        /// Peer to update.
        device_id: DeviceId,
        /// When the peer was seen, on the local clock.
        seen_at_ms: i64,
        /// Reply channel.
        reply: Reply<()>,
    },
    /// Marks a peer's incoming messages read.
    MarkPeerRead {
        /// Peer whose conversation is read.
        device_id: DeviceId,
        /// Reply channel.
        reply: Reply<u32>,
    },
    /// Forgets a peer, with or without its history.
    ForgetPeer {
        /// Peer to forget.
        device_id: DeviceId,
        /// Whether the stored conversation is deleted.
        delete_history: bool,
        /// Reply channel.
        reply: Reply<()>,
    },
    /// Lists every known device.
    KnownDevices(Reply<Vec<KnownDevice>>),
    /// Stores a message.
    InsertMessage {
        /// The message to store.
        message: ChatMessage,
        /// Reply channel carrying whether a row was inserted.
        reply: Reply<bool>,
    },
    /// Updates a message's delivery status.
    SetMessageStatus {
        /// Message to update.
        id: MessageId,
        /// New status.
        status: MessageStatus,
        /// Reply channel.
        reply: Reply<()>,
    },
    /// Fails every pending outgoing message for a peer.
    FailPendingMessages {
        /// Peer whose queue is being abandoned.
        device_id: DeviceId,
        /// Reply channel carrying the number of rows changed.
        reply: Reply<u32>,
    },
    /// Reads one page of a conversation.
    HistoryPage {
        /// Conversation to read.
        device_id: DeviceId,
        /// Exclusive cursor, if paging past the first page.
        before: Option<HistoryCursor>,
        /// Requested page size; clamped by the implementation.
        limit: u32,
        /// Reply channel.
        reply: Reply<Vec<ChatMessage>>,
    },
    /// Deletes every stored message.
    ClearHistory(Reply<u64>),
}

impl SqliteStore {
    /// Opens (or creates) the database at `path`, discarding the corruption report.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the database cannot be opened, migrated, or quarantined.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        Self::open_with_report(path).map(|(store, _report)| store)
    }

    /// Opens (or creates) the database at `path`.
    ///
    /// If the file is not a usable SQLite database, it is renamed to
    /// `<name>.corrupt-<unix-millis>` (together with its `-wal`/`-shm` siblings), a fresh
    /// database is created, and the preserved path is returned as the second element. This
    /// is not an error: the application must start. [`StorageError::Corrupted`] is returned
    /// only when the quarantine itself fails, because then the user's data cannot be saved.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the replacement database cannot be opened or migrated,
    /// or if a corrupt file cannot be moved aside.
    pub fn open_with_report(path: &Path) -> Result<(Self, Option<String>), StorageError> {
        let (mut conn, preserved) = match connect_and_check(path) {
            Ok(conn) => (conn, None),
            Err(error) => {
                tracing::warn!(%error, "database unusable, quarantining it");
                let preserved = quarantine(path)?;
                let conn = connect_and_check(path).map_err(sqlite_error)?;
                (conn, Some(preserved.to_string_lossy().into_owned()))
            }
        };

        migrate(&mut conn)?;

        let (sender, receiver) = mpsc::channel(REQUEST_QUEUE_CAPACITY);
        let writer = thread::Builder::new()
            .name("localme-storage".to_owned())
            .spawn(move || run(conn, receiver))
            .map_err(sqlite_error)?;

        Ok((
            Self {
                sender: Some(sender),
                writer: Some(writer),
            },
            preserved,
        ))
    }

    /// Sends one request and awaits its reply, mapping a closed mailbox to `Unavailable`.
    async fn call<R>(&self, build: impl FnOnce(Reply<R>) -> Request) -> Result<R, StorageError>
    where
        R: Send + 'static,
    {
        let (reply, receiver) = oneshot::channel();
        let sender = self.sender.as_ref().ok_or(StorageError::Unavailable)?;
        sender
            .send(build(reply))
            .await
            .map_err(|_| StorageError::Unavailable)?;
        receiver.await.map_err(|_| StorageError::Unavailable)?
    }
}

/// Connects, applies the connection pragmas, and verifies the file is a database.
fn connect_and_check(path: &Path) -> Result<Connection, StorageError> {
    let conn = Connection::open(path).map_err(sqlite_error)?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(sqlite_error)?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(sqlite_error)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(sqlite_error)?;
    conn.pragma_update(None, "busy_timeout", 5_000_i64)
        .map_err(sqlite_error)?;

    if !integrity_ok(&conn).map_err(sqlite_error)? {
        return Err(sqlite_error("integrity_check did not report ok"));
    }
    Ok(conn)
}

/// Runs `PRAGMA integrity_check`, which returns exactly one `ok` row on a healthy database.
fn integrity_ok(conn: &Connection) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let mut rows = stmt.query([])?;
    match rows.next()? {
        Some(row) => {
            let verdict: String = row.get(0)?;
            Ok(verdict == "ok" && rows.next()?.is_none())
        }
        None => Ok(false),
    }
}

/// Moves an unusable database aside, along with its WAL siblings.
///
/// Returns the path the main file was preserved under. Sibling renames are best effort: a
/// stale `-wal` next to a fresh database is harmless, but the main file is the user's data,
/// and failing to preserve it is reported as [`StorageError::Corrupted`].
fn quarantine(path: &Path) -> Result<PathBuf, StorageError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or(0);
    let preserved = with_suffix(path, &format!(".corrupt-{millis}"));

    if path.exists() {
        fs::rename(path, &preserved).map_err(|_| StorageError::Corrupted {
            preserved_path: preserved.to_string_lossy().into_owned(),
        })?;
    }

    for suffix in ["-wal", "-shm"] {
        let sibling = with_suffix(path, suffix);
        if sibling.exists() {
            let _ = fs::rename(sibling, with_suffix(&preserved, suffix));
        }
    }
    Ok(preserved)
}

/// Appends a suffix to a path's file name, e.g. `localme.db` + `.corrupt-1`.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(suffix);
    PathBuf::from(os)
}

/// The writer thread: owns the connection and serves requests until the mailbox closes.
fn run(mut conn: Connection, mut receiver: mpsc::Receiver<Request>) {
    while let Some(request) = receiver.blocking_recv() {
        dispatch(&mut conn, request);
    }
    // `conn` drops here, closing the file after the WAL is checkpointed.
}

/// Executes one request and delivers its outcome.
fn dispatch(conn: &mut Connection, request: Request) {
    match request {
        Request::MetaGet { key, reply } => {
            let _ = reply.send(meta_get(conn, &key));
        }
        Request::MetaSet { key, value, reply } => {
            let _ = reply.send(meta_set(conn, &key, &value));
        }
        Request::UpsertPeerSeen {
            profile,
            address,
            seen_at_ms,
            reply,
        } => {
            let _ = reply.send(upsert_peer_seen(
                conn,
                &profile,
                address.as_deref(),
                seen_at_ms,
            ));
        }
        Request::Peer { device_id, reply } => {
            let _ = reply.send(read_peer(conn, device_id));
        }
        Request::Peers(reply) => {
            let _ = reply.send(read_peers(conn));
        }
        Request::SetPeerMuted {
            device_id,
            muted,
            reply,
        } => {
            let _ = reply.send(set_peer_muted(conn, device_id, muted));
        }
        Request::TouchPeerSeen {
            device_id,
            seen_at_ms,
            reply,
        } => {
            let _ = reply.send(touch_peer_seen(conn, device_id, seen_at_ms));
        }
        Request::MarkPeerRead { device_id, reply } => {
            let _ = reply.send(mark_peer_read(conn, device_id));
        }
        Request::ForgetPeer {
            device_id,
            delete_history,
            reply,
        } => {
            let _ = reply.send(forget_peer(conn, device_id, delete_history));
        }
        Request::KnownDevices(reply) => {
            let _ = reply.send(read_known_devices(conn));
        }
        Request::InsertMessage { message, reply } => {
            let _ = reply.send(insert_message(conn, &message));
        }
        Request::SetMessageStatus { id, status, reply } => {
            let _ = reply.send(set_message_status(conn, id, status));
        }
        Request::FailPendingMessages { device_id, reply } => {
            let _ = reply.send(fail_pending_messages(conn, device_id));
        }
        Request::HistoryPage {
            device_id,
            before,
            limit,
            reply,
        } => {
            let _ = reply.send(history_page(conn, device_id, before, limit));
        }
        Request::ClearHistory(reply) => {
            let _ = reply.send(clear_history(conn));
        }
    }
}

/// Wraps any displayable error as [`StorageError::Sqlite`].
fn sqlite_error(error: impl std::fmt::Display) -> StorageError {
    StorageError::Sqlite(error.to_string())
}

/// Builds an [`StorageError::InvalidRow`].
fn invalid_row(message: impl Into<String>) -> StorageError {
    StorageError::InvalidRow(message.into())
}

/// Parses a stored device identifier.
fn parse_device(raw: &str) -> Result<DeviceId, StorageError> {
    raw.parse::<DeviceId>()
        .map_err(|error| invalid_row(format!("device id `{raw}`: {error}")))
}

/// Parses a stored message identifier.
fn parse_message_id(raw: &str) -> Result<MessageId, StorageError> {
    raw.parse::<MessageId>()
        .map_err(|error| invalid_row(format!("message id `{raw}`: {error}")))
}

/// Parses a stored nickname.
fn parse_nickname(raw: &str) -> Result<Nickname, StorageError> {
    Nickname::parse(raw).map_err(|error| invalid_row(format!("nickname `{raw}`: {error}")))
}

/// Parses a stored avatar seed.
fn parse_avatar(raw: &str) -> Result<AvatarSeed, StorageError> {
    AvatarSeed::parse(raw).map_err(|error| invalid_row(format!("avatar seed `{raw}`: {error}")))
}

/// Narrows a stored count to `u32`.
fn to_u32(value: i64) -> Result<u32, StorageError> {
    u32::try_from(value)
        .map_err(|_| invalid_row(format!("expected a non-negative count, found {value}")))
}

/// The columns of `peers` plus the computed `last_activity_ms`, in row order.
const PEER_COLUMNS: &str = "\
  p.device_id, p.nickname, p.avatar_seed, p.last_address, p.last_seen_ms, \
  p.first_seen_ms, p.unread, p.notify_muted, p.forgotten, \
  CASE \
    WHEN p.last_seen_ms IS NULL \
      AND NOT EXISTS (SELECT 1 FROM messages m WHERE m.peer_id = p.device_id) \
    THEN NULL \
    ELSE MAX( \
      COALESCE(p.last_seen_ms, 0), \
      COALESCE((SELECT MAX(m.sent_at_ms) FROM messages m WHERE m.peer_id = p.device_id), 0) \
    ) \
  END";

/// One `peers` row as read from SQLite, before validation.
struct PeerRow {
    device_id: String,
    nickname: String,
    avatar_seed: String,
    last_address: Option<String>,
    last_seen_ms: Option<i64>,
    first_seen_ms: i64,
    unread: i64,
    notify_muted: i64,
    forgotten: i64,
    last_activity_ms: Option<i64>,
}

impl PeerRow {
    /// Reads a row from the column list in [`PEER_COLUMNS`].
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            device_id: row.get(0)?,
            nickname: row.get(1)?,
            avatar_seed: row.get(2)?,
            last_address: row.get(3)?,
            last_seen_ms: row.get(4)?,
            first_seen_ms: row.get(5)?,
            unread: row.get(6)?,
            notify_muted: row.get(7)?,
            forgotten: row.get(8)?,
            last_activity_ms: row.get(9)?,
        })
    }

    /// Validates the raw strings into a [`StoredPeer`].
    fn into_stored(self) -> Result<StoredPeer, StorageError> {
        Ok(StoredPeer {
            profile: PeerProfile {
                device_id: parse_device(&self.device_id)?,
                nickname: parse_nickname(&self.nickname)?,
                avatar_seed: parse_avatar(&self.avatar_seed)?,
            },
            last_address: self.last_address,
            last_seen_ms: self.last_seen_ms,
            first_seen_ms: self.first_seen_ms,
            unread: to_u32(self.unread)?,
            notify_muted: self.notify_muted != 0,
            forgotten: self.forgotten != 0,
            last_activity_ms: self.last_activity_ms,
        })
    }
}

/// One `messages` row as read from SQLite, before validation.
struct MessageRow {
    id: String,
    peer_id: String,
    outgoing: i64,
    body: String,
    sent_at_ms: i64,
    received_at_ms: i64,
    status: String,
    read: i64,
}

impl MessageRow {
    /// Reads a row from the standard `messages` column list.
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            peer_id: row.get(1)?,
            outgoing: row.get(2)?,
            body: row.get(3)?,
            sent_at_ms: row.get(4)?,
            received_at_ms: row.get(5)?,
            status: row.get(6)?,
            read: row.get(7)?,
        })
    }

    /// Validates the raw strings into a [`ChatMessage`].
    fn into_message(self) -> Result<ChatMessage, StorageError> {
        Ok(ChatMessage {
            id: parse_message_id(&self.id)?,
            peer: parse_device(&self.peer_id)?,
            direction: if self.outgoing != 0 {
                Direction::Outgoing
            } else {
                Direction::Incoming
            },
            body: MessageBody::from_stored(self.body)
                .map_err(|error| invalid_row(format!("message body: {error}")))?,
            sent_at: UnixMillis(self.sent_at_ms),
            received_at: UnixMillis(self.received_at_ms),
            status: MessageStatus::from_db(&self.status)
                .map_err(|error| invalid_row(format!("message status: {error}")))?,
            read: self.read != 0,
        })
    }
}

/// One `peers` row projected for the settings screen.
struct KnownDeviceRow {
    device_id: String,
    nickname: String,
    avatar_seed: String,
    forgotten: i64,
    first_seen_ms: i64,
    last_seen_ms: Option<i64>,
    message_count: i64,
}

impl KnownDeviceRow {
    /// Reads a row from the `known_devices` query.
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            device_id: row.get(0)?,
            nickname: row.get(1)?,
            avatar_seed: row.get(2)?,
            forgotten: row.get(3)?,
            first_seen_ms: row.get(4)?,
            last_seen_ms: row.get(5)?,
            message_count: row.get(6)?,
        })
    }

    /// Validates the raw strings into a [`KnownDevice`].
    fn into_device(self) -> Result<KnownDevice, StorageError> {
        Ok(KnownDevice {
            device_id: parse_device(&self.device_id)?,
            nickname: parse_nickname(&self.nickname)?,
            avatar_seed: parse_avatar(&self.avatar_seed)?,
            forgotten: self.forgotten != 0,
            first_seen_ms: self.first_seen_ms,
            last_seen_ms: self.last_seen_ms,
            message_count: u64::try_from(self.message_count).map_err(|_| {
                invalid_row(format!(
                    "expected a non-negative message count, found {}",
                    self.message_count
                ))
            })?,
        })
    }
}

/// Reads one metadata value.
fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(sqlite_error)
}

/// Upserts one metadata value.
fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(sqlite_error)?;
    Ok(())
}

/// Inserts or updates a peer's announced identity and returns the stored row.
///
/// `seen_at_ms` carries liveness evidence only. On update the stored `last_seen_ms` moves
/// forward monotonically and is left untouched when there is no evidence; on insert it
/// starts NULL. `first_seen_ms` is stamped from `seen_at_ms` when present, or from the
/// local clock otherwise, because a row only exists once the peer has at least been seen.
fn upsert_peer_seen(
    conn: &Connection,
    profile: &PeerProfile,
    address: Option<&str>,
    seen_at_ms: Option<i64>,
) -> Result<StoredPeer, StorageError> {
    let first_seen_ms = seen_at_ms.unwrap_or_else(|| UnixMillis::now().as_i64());
    conn.execute(
        "INSERT INTO peers \
           (device_id, nickname, avatar_seed, last_address, last_seen_ms, first_seen_ms, \
            unread, notify_muted, forgotten) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, 0) \
         ON CONFLICT(device_id) DO UPDATE SET \
           nickname = excluded.nickname, \
           avatar_seed = excluded.avatar_seed, \
           last_address = excluded.last_address, \
           last_seen_ms = CASE \
             WHEN excluded.last_seen_ms IS NULL THEN peers.last_seen_ms \
             ELSE MAX(COALESCE(peers.last_seen_ms, excluded.last_seen_ms), excluded.last_seen_ms) \
           END, \
           forgotten = 0",
        params![
            profile.device_id.to_string(),
            profile.nickname.as_str(),
            profile.avatar_seed.as_str(),
            address,
            seen_at_ms,
            first_seen_ms
        ],
    )
    .map_err(sqlite_error)?;

    read_peer(conn, profile.device_id)?.ok_or_else(|| {
        invalid_row(format!(
            "peer {} vanished immediately after upsert",
            profile.device_id
        ))
    })
}

/// Reads one peer, computing `last_activity_ms`.
fn read_peer(conn: &Connection, device_id: DeviceId) -> Result<Option<StoredPeer>, StorageError> {
    let sql = format!(
        "SELECT {} FROM peers p WHERE p.device_id = ?1",
        PEER_COLUMNS
    );
    let raw = conn
        .query_row(&sql, params![device_id.to_string()], PeerRow::read)
        .optional()
        .map_err(sqlite_error)?;
    raw.map(PeerRow::into_stored).transpose()
}

/// Reads every peer, forgotten ones included.
fn read_peers(conn: &Connection) -> Result<Vec<StoredPeer>, StorageError> {
    let sql = format!(
        "SELECT {} FROM peers p ORDER BY p.nickname ASC, p.device_id ASC",
        PEER_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).map_err(sqlite_error)?;
    let rows = stmt.query_map([], PeerRow::read).map_err(sqlite_error)?;
    let raw = rows
        .collect::<rusqlite::Result<Vec<PeerRow>>>()
        .map_err(sqlite_error)?;
    raw.into_iter().map(PeerRow::into_stored).collect()
}

/// Sets a peer's notification preference.
fn set_peer_muted(conn: &Connection, device_id: DeviceId, muted: bool) -> Result<(), StorageError> {
    conn.execute(
        "UPDATE peers SET notify_muted = ?2 WHERE device_id = ?1",
        params![device_id.to_string(), muted],
    )
    .map_err(sqlite_error)?;
    Ok(())
}

/// Updates a peer's `last_seen_ms`.
fn touch_peer_seen(
    conn: &Connection,
    device_id: DeviceId,
    seen_at_ms: i64,
) -> Result<(), StorageError> {
    conn.execute(
        "UPDATE peers SET last_seen_ms = ?2 WHERE device_id = ?1",
        params![device_id.to_string(), seen_at_ms],
    )
    .map_err(sqlite_error)?;
    Ok(())
}

/// Marks a peer's incoming messages read and clears its unread count.
fn mark_peer_read(conn: &mut Connection, device_id: DeviceId) -> Result<u32, StorageError> {
    let tx = conn.transaction().map_err(sqlite_error)?;
    let changed = tx
        .execute(
            "UPDATE messages SET read = 1 \
             WHERE peer_id = ?1 AND outgoing = 0 AND read = 0",
            params![device_id.to_string()],
        )
        .map_err(sqlite_error)?;
    tx.execute(
        "UPDATE peers SET unread = 0 WHERE device_id = ?1",
        params![device_id.to_string()],
    )
    .map_err(sqlite_error)?;
    tx.commit().map_err(sqlite_error)?;
    Ok(u32::try_from(changed).unwrap_or(u32::MAX))
}

/// Forgets a peer, optionally deleting its conversation.
fn forget_peer(
    conn: &mut Connection,
    device_id: DeviceId,
    delete_history: bool,
) -> Result<(), StorageError> {
    let tx = conn.transaction().map_err(sqlite_error)?;
    if delete_history {
        tx.execute(
            "DELETE FROM messages WHERE peer_id = ?1",
            params![device_id.to_string()],
        )
        .map_err(sqlite_error)?;
    }
    // The row is always flagged, and never deleted: the settings screen lists forgotten
    // devices so the user can see what they have forgotten and restore it. The history
    // choice only decides whether the conversation goes with it.
    tx.execute(
        "UPDATE peers SET forgotten = 1, unread = 0 WHERE device_id = ?1",
        params![device_id.to_string()],
    )
    .map_err(sqlite_error)?;
    tx.commit().map_err(sqlite_error)
}

/// Lists every known device, newest activity first.
fn read_known_devices(conn: &Connection) -> Result<Vec<KnownDevice>, StorageError> {
    let sql = "\
        SELECT p.device_id, p.nickname, p.avatar_seed, p.forgotten, p.first_seen_ms, \
               p.last_seen_ms, \
               (SELECT COUNT(*) FROM messages m WHERE m.peer_id = p.device_id) \
        FROM peers p \
        ORDER BY \
          CASE \
            WHEN p.last_seen_ms IS NULL \
              AND NOT EXISTS (SELECT 1 FROM messages m WHERE m.peer_id = p.device_id) \
            THEN NULL \
            ELSE MAX( \
              COALESCE(p.last_seen_ms, 0), \
              COALESCE((SELECT MAX(m.sent_at_ms) FROM messages m WHERE m.peer_id = p.device_id), 0) \
            ) \
          END DESC, \
          p.nickname ASC, \
          p.device_id ASC";
    let mut stmt = conn.prepare(sql).map_err(sqlite_error)?;
    let rows = stmt
        .query_map([], KnownDeviceRow::read)
        .map_err(sqlite_error)?;
    let raw = rows
        .collect::<rusqlite::Result<Vec<KnownDeviceRow>>>()
        .map_err(sqlite_error)?;
    raw.into_iter().map(KnownDeviceRow::into_device).collect()
}

/// Stores a message, incrementing unread for a new incoming unread one.
fn insert_message(conn: &mut Connection, message: &ChatMessage) -> Result<bool, StorageError> {
    let tx = conn.transaction().map_err(sqlite_error)?;
    tx.execute(
        "INSERT OR IGNORE INTO messages \
           (id, peer_id, outgoing, body, sent_at_ms, received_at_ms, status, read) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            message.id.to_string(),
            message.peer.to_string(),
            i64::from(message.direction.is_outgoing()),
            message.body.as_str(),
            message.sent_at.as_i64(),
            message.received_at.as_i64(),
            message.status.as_str(),
            i64::from(message.read)
        ],
    )
    .map_err(sqlite_error)?;
    let inserted = tx.changes() == 1;

    if inserted && message.is_unread_incoming() {
        tx.execute(
            "UPDATE peers SET unread = unread + 1 WHERE device_id = ?1",
            params![message.peer.to_string()],
        )
        .map_err(sqlite_error)?;
    }
    tx.commit().map_err(sqlite_error)?;
    Ok(inserted)
}

/// Updates a message's delivery status.
fn set_message_status(
    conn: &Connection,
    id: MessageId,
    status: MessageStatus,
) -> Result<(), StorageError> {
    conn.execute(
        "UPDATE messages SET status = ?2 WHERE id = ?1",
        params![id.to_string(), status.as_str()],
    )
    .map_err(sqlite_error)?;
    Ok(())
}

/// Fails every pending outgoing message for a peer.
fn fail_pending_messages(conn: &Connection, device_id: DeviceId) -> Result<u32, StorageError> {
    let changed = conn
        .execute(
            "UPDATE messages SET status = 'failed' \
             WHERE peer_id = ?1 AND outgoing = 1 AND status IN ('sending', 'sent')",
            params![device_id.to_string()],
        )
        .map_err(sqlite_error)?;
    Ok(u32::try_from(changed).unwrap_or(u32::MAX))
}

/// Reads one page of a conversation, newest first, strictly below the cursor.
fn history_page(
    conn: &Connection,
    device_id: DeviceId,
    before: Option<HistoryCursor>,
    limit: u32,
) -> Result<Vec<ChatMessage>, StorageError> {
    let limit = limit.clamp(1, 200);
    let cursor_sent = before.map(|cursor| cursor.sent_at_ms);
    let cursor_id = before.map(|cursor| cursor.id.to_string());

    let mut stmt = conn
        .prepare(
            "SELECT id, peer_id, outgoing, body, sent_at_ms, received_at_ms, status, read \
             FROM messages \
             WHERE peer_id = ?1 \
               AND (?2 IS NULL OR sent_at_ms < ?2 OR (sent_at_ms = ?2 AND id < ?3)) \
             ORDER BY sent_at_ms DESC, id DESC \
             LIMIT ?4",
        )
        .map_err(sqlite_error)?;
    let rows = stmt
        .query_map(
            params![device_id.to_string(), cursor_sent, cursor_id, limit],
            MessageRow::read,
        )
        .map_err(sqlite_error)?;
    let raw = rows
        .collect::<rusqlite::Result<Vec<MessageRow>>>()
        .map_err(sqlite_error)?;
    raw.into_iter().map(MessageRow::into_message).collect()
}

/// Deletes every stored message, keeping the peer list.
fn clear_history(conn: &mut Connection) -> Result<u64, StorageError> {
    let tx = conn.transaction().map_err(sqlite_error)?;
    let deleted = tx
        .execute("DELETE FROM messages", [])
        .map_err(sqlite_error)?;
    tx.execute("UPDATE peers SET unread = 0", [])
        .map_err(sqlite_error)?;
    tx.commit().map_err(sqlite_error)?;
    Ok(u64::try_from(deleted).unwrap_or(u64::MAX))
}

impl Store for SqliteStore {
    async fn meta_get(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.call(|reply| Request::MetaGet {
            key: key.to_owned(),
            reply,
        })
        .await
    }

    async fn meta_set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.call(|reply| Request::MetaSet {
            key: key.to_owned(),
            value: value.to_owned(),
            reply,
        })
        .await
    }

    async fn upsert_peer_seen(
        &self,
        profile: &PeerProfile,
        address: Option<&str>,
        seen_at_ms: Option<i64>,
    ) -> Result<StoredPeer, StorageError> {
        self.call(|reply| Request::UpsertPeerSeen {
            profile: profile.clone(),
            address: address.map(str::to_owned),
            seen_at_ms,
            reply,
        })
        .await
    }

    async fn peer(&self, device_id: DeviceId) -> Result<Option<StoredPeer>, StorageError> {
        self.call(|reply| Request::Peer { device_id, reply }).await
    }

    async fn peers(&self) -> Result<Vec<StoredPeer>, StorageError> {
        self.call(Request::Peers).await
    }

    async fn set_peer_muted(&self, device_id: DeviceId, muted: bool) -> Result<(), StorageError> {
        self.call(|reply| Request::SetPeerMuted {
            device_id,
            muted,
            reply,
        })
        .await
    }

    async fn touch_peer_seen(
        &self,
        device_id: DeviceId,
        seen_at_ms: i64,
    ) -> Result<(), StorageError> {
        self.call(|reply| Request::TouchPeerSeen {
            device_id,
            seen_at_ms,
            reply,
        })
        .await
    }

    async fn mark_peer_read(&self, device_id: DeviceId) -> Result<u32, StorageError> {
        self.call(|reply| Request::MarkPeerRead { device_id, reply })
            .await
    }

    async fn forget_peer(
        &self,
        device_id: DeviceId,
        delete_history: bool,
    ) -> Result<(), StorageError> {
        self.call(|reply| Request::ForgetPeer {
            device_id,
            delete_history,
            reply,
        })
        .await
    }

    async fn known_devices(&self) -> Result<Vec<KnownDevice>, StorageError> {
        self.call(Request::KnownDevices).await
    }

    async fn insert_message(&self, message: &ChatMessage) -> Result<bool, StorageError> {
        self.call(|reply| Request::InsertMessage {
            message: message.clone(),
            reply,
        })
        .await
    }

    async fn set_message_status(
        &self,
        id: MessageId,
        status: MessageStatus,
    ) -> Result<(), StorageError> {
        self.call(|reply| Request::SetMessageStatus { id, status, reply })
            .await
    }

    async fn fail_pending_messages(&self, device_id: DeviceId) -> Result<u32, StorageError> {
        self.call(|reply| Request::FailPendingMessages { device_id, reply })
            .await
    }

    async fn history_page(
        &self,
        device_id: DeviceId,
        before: Option<HistoryCursor>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>, StorageError> {
        self.call(|reply| Request::HistoryPage {
            device_id,
            before,
            limit,
            reply,
        })
        .await
    }

    async fn clear_history(&self) -> Result<u64, StorageError> {
        self.call(Request::ClearHistory).await
    }
}

impl Drop for SqliteStore {
    fn drop(&mut self) {
        // Dropping the sender closes the mailbox, which makes the writer thread's
        // `blocking_recv` return and the loop exit.
        self.sender.take();
        let Some(writer) = self.writer.take() else {
            return;
        };
        let deadline = Instant::now() + SHUTDOWN_JOIN_TIMEOUT;
        while !writer.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        // Only join when it has finished: joining a wedged thread would hang shutdown,
        // and abandoning it is the documented trade-off (`docs/ARCHITECTURE.md` §8.4).
        if writer.is_finished() {
            let _ = writer.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    use tempfile::TempDir;
    use tokio::sync::mpsc;

    use super::*;
    use crate::domain::clock::UnixMillis;
    use crate::domain::ids::{DeviceId, MessageId};
    use crate::domain::message::{ChatMessage, Direction, MessageBody, MessageStatus};
    use crate::domain::nickname::Nickname;
    use crate::domain::peer::PeerProfile;
    use crate::error::StorageError;

    /// A store in a fresh temporary directory, which is kept alive with it.
    fn open_store() -> (TempDir, SqliteStore) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SqliteStore::open(&dir.path().join("localme.db")).expect("open store");
        (dir, store)
    }

    /// A deterministic device id.
    fn device(seed: u128) -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(seed))
    }

    /// A profile for a device id derived from `seed`.
    fn profile(seed: u128, nickname: &str) -> PeerProfile {
        PeerProfile::new(device(seed), Nickname::parse(nickname).expect("nickname"))
    }

    /// An incoming message with a fresh id.
    fn incoming(peer: DeviceId, sent_at_ms: i64, read: bool) -> ChatMessage {
        ChatMessage {
            id: MessageId::generate(),
            peer,
            direction: Direction::Incoming,
            body: MessageBody::parse("incoming").expect("body"),
            sent_at: UnixMillis(sent_at_ms),
            received_at: UnixMillis(sent_at_ms),
            status: MessageStatus::Received,
            read,
        }
    }

    /// An outgoing message with a fresh id.
    fn outgoing(peer: DeviceId, sent_at_ms: i64, status: MessageStatus) -> ChatMessage {
        ChatMessage {
            id: MessageId::generate(),
            peer,
            direction: Direction::Outgoing,
            body: MessageBody::parse("outgoing").expect("body"),
            sent_at: UnixMillis(sent_at_ms),
            received_at: UnixMillis(sent_at_ms),
            status,
            read: true,
        }
    }

    #[tokio::test]
    async fn fresh_store_reports_version_one_and_has_no_peers() {
        let (_dir, store) = open_store();
        assert_eq!(
            store.meta_get("schema_version").await.unwrap().as_deref(),
            Some("1")
        );
        assert!(store.peers().await.unwrap().is_empty());
        assert!(store.known_devices().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn metadata_round_trips_and_overwrites() {
        let (_dir, store) = open_store();
        assert_eq!(store.meta_get("greeting").await.unwrap(), None);
        store.meta_set("greeting", "hi").await.unwrap();
        assert_eq!(
            store.meta_get("greeting").await.unwrap().as_deref(),
            Some("hi")
        );
        store.meta_set("greeting", "hello").await.unwrap();
        assert_eq!(
            store.meta_get("greeting").await.unwrap().as_deref(),
            Some("hello")
        );
    }

    #[tokio::test]
    async fn upsert_peer_seen_keeps_first_seen_and_updates_identity() {
        let (_dir, store) = open_store();
        let first = profile(1, "Alice");
        let stored = store
            .upsert_peer_seen(&first, Some("10.0.0.1:1"), Some(1_000))
            .await
            .unwrap();
        assert_eq!(stored.first_seen_ms, 1_000);
        assert_eq!(stored.last_seen_ms, Some(1_000));
        assert_eq!(stored.profile.nickname.as_str(), "Alice");

        // Forget it, then see it again: the forgotten flag must clear.
        store.forget_peer(first.device_id, false).await.unwrap();
        assert!(
            store
                .peer(first.device_id)
                .await
                .unwrap()
                .unwrap()
                .forgotten
        );

        let renamed = profile(1, "Alice Renamed");
        let stored = store
            .upsert_peer_seen(&renamed, Some("10.0.0.2:2"), Some(2_000))
            .await
            .unwrap();
        assert_eq!(stored.first_seen_ms, 1_000, "first_seen must survive");
        assert_eq!(stored.last_seen_ms, Some(2_000));
        assert_eq!(stored.profile.nickname.as_str(), "Alice Renamed");
        assert_eq!(stored.profile.avatar_seed, renamed.avatar_seed);
        assert_eq!(stored.last_address.as_deref(), Some("10.0.0.2:2"));
        assert!(!stored.forgotten);
    }

    #[tokio::test]
    async fn insert_message_is_idempotent_and_counts_unread_once() {
        let (_dir, store) = open_store();
        let peer = profile(2, "Bob");
        store.upsert_peer_seen(&peer, None, Some(1)).await.unwrap();

        let message = incoming(peer.device_id, 10, false);
        assert!(
            store.insert_message(&message).await.unwrap(),
            "first insert"
        );
        assert!(
            !store.insert_message(&message).await.unwrap(),
            "duplicate is a no-op"
        );
        assert_eq!(
            store.peer(peer.device_id).await.unwrap().unwrap().unread,
            1,
            "the retransmission must not double-count"
        );
    }

    #[tokio::test]
    async fn unread_counts_only_incoming_unread_and_mark_read_clears_it() {
        let (_dir, store) = open_store();
        let peer = profile(3, "Carol");
        store.upsert_peer_seen(&peer, None, Some(1)).await.unwrap();

        store
            .insert_message(&incoming(peer.device_id, 1, true))
            .await
            .unwrap();
        store
            .insert_message(&outgoing(peer.device_id, 2, MessageStatus::Sending))
            .await
            .unwrap();
        store
            .insert_message(&incoming(peer.device_id, 3, false))
            .await
            .unwrap();
        store
            .insert_message(&incoming(peer.device_id, 4, false))
            .await
            .unwrap();
        assert_eq!(store.peer(peer.device_id).await.unwrap().unwrap().unread, 2);

        assert_eq!(store.mark_peer_read(peer.device_id).await.unwrap(), 2);
        assert_eq!(store.peer(peer.device_id).await.unwrap().unwrap().unread, 0);
        assert_eq!(store.mark_peer_read(peer.device_id).await.unwrap(), 0);

        let messages = store.history_page(peer.device_id, None, 50).await.unwrap();
        assert!(
            messages
                .iter()
                .filter(|message| matches!(message.direction, Direction::Incoming))
                .all(|message| message.read)
        );
    }

    #[tokio::test]
    async fn history_pages_walk_the_conversation_without_gaps() {
        let (_dir, store) = open_store();
        let peer = profile(4, "Dave");
        store.upsert_peer_seen(&peer, None, Some(1)).await.unwrap();

        // 120 messages, two sharing each timestamp, so the id tie-break is exercised.
        for i in 0..120_i64 {
            store
                .insert_message(&incoming(peer.device_id, i / 2, true))
                .await
                .unwrap();
        }

        let first = store.history_page(peer.device_id, None, 50).await.unwrap();
        assert_eq!(first.len(), 50);
        assert!(
            first.first().unwrap().sent_at.as_i64() >= first.last().unwrap().sent_at.as_i64(),
            "newest first"
        );

        let mut all = Vec::new();
        let mut cursor = None;
        let mut pages = 0;
        loop {
            let page = store
                .history_page(peer.device_id, cursor, 50)
                .await
                .unwrap();
            pages += 1;
            if page.is_empty() {
                break;
            }
            let last = page.last().unwrap();
            cursor = Some(HistoryCursor {
                sent_at_ms: last.sent_at.as_i64(),
                id: last.id,
            });
            let len = page.len();
            all.extend(page);
            if len < 50 {
                break;
            }
        }
        assert_eq!(pages, 3);
        assert_eq!(all.len(), 120);
        let unique: HashSet<MessageId> = all.iter().map(|message| message.id).collect();
        assert_eq!(unique.len(), 120, "no duplicates");

        for pair in all.windows(2) {
            let (newer, older) = (&pair[0], &pair[1]);
            assert!(
                (newer.sent_at.as_i64(), newer.id) > (older.sent_at.as_i64(), older.id),
                "strictly descending (sent_at, id)"
            );
        }
    }

    #[tokio::test]
    async fn history_limit_is_clamped() {
        let (_dir, store) = open_store();
        let peer = profile(5, "Erin");
        store.upsert_peer_seen(&peer, None, Some(1)).await.unwrap();
        for i in 0..205_i64 {
            store
                .insert_message(&incoming(peer.device_id, i, true))
                .await
                .unwrap();
        }

        assert_eq!(
            store
                .history_page(peer.device_id, None, 1_000)
                .await
                .unwrap()
                .len(),
            200,
            "an absurd limit clamps to 200"
        );
        assert_eq!(
            store
                .history_page(peer.device_id, None, 0)
                .await
                .unwrap()
                .len(),
            1,
            "zero clamps to one"
        );
    }

    #[tokio::test]
    async fn message_status_updates_and_pending_failures() {
        let (_dir, store) = open_store();
        let peer = profile(6, "Frank");
        store.upsert_peer_seen(&peer, None, Some(1)).await.unwrap();

        let settled = outgoing(peer.device_id, 1, MessageStatus::Sending);
        store.insert_message(&settled).await.unwrap();
        store
            .set_message_status(settled.id, MessageStatus::Delivered)
            .await
            .unwrap();

        let pending_a = outgoing(peer.device_id, 2, MessageStatus::Sending);
        let pending_b = outgoing(peer.device_id, 3, MessageStatus::Sent);
        let delivered = outgoing(peer.device_id, 4, MessageStatus::Delivered);
        let received = incoming(peer.device_id, 5, true);
        store.insert_message(&pending_a).await.unwrap();
        store.insert_message(&pending_b).await.unwrap();
        store.insert_message(&delivered).await.unwrap();
        store.insert_message(&received).await.unwrap();

        assert_eq!(
            store.fail_pending_messages(peer.device_id).await.unwrap(),
            2
        );

        let messages = store.history_page(peer.device_id, None, 50).await.unwrap();
        let status = |id: MessageId| {
            messages
                .iter()
                .find(|message| message.id == id)
                .unwrap()
                .status
        };
        assert_eq!(status(settled.id), MessageStatus::Delivered);
        assert_eq!(status(pending_a.id), MessageStatus::Failed);
        assert_eq!(status(pending_b.id), MessageStatus::Failed);
        assert_eq!(status(delivered.id), MessageStatus::Delivered);
        assert_eq!(status(received.id), MessageStatus::Received);
    }

    #[tokio::test]
    async fn forget_peer_honours_the_history_choice() {
        let (_dir, store) = open_store();

        let wiped = profile(7, "Grace");
        store.upsert_peer_seen(&wiped, None, Some(1)).await.unwrap();
        store
            .insert_message(&incoming(wiped.device_id, 1, false))
            .await
            .unwrap();
        store
            .insert_message(&incoming(wiped.device_id, 2, false))
            .await
            .unwrap();
        store.forget_peer(wiped.device_id, true).await.unwrap();
        assert!(
            store
                .history_page(wiped.device_id, None, 50)
                .await
                .unwrap()
                .is_empty()
        );
        let stored = store
            .peer(wiped.device_id)
            .await
            .unwrap()
            .expect("the row must survive");
        assert_eq!(stored.unread, 0);
        assert!(
            stored.forgotten,
            "forgetting marks the device whether or not its history was deleted"
        );

        let kept = profile(8, "Heidi");
        store.upsert_peer_seen(&kept, None, Some(1)).await.unwrap();
        store
            .insert_message(&incoming(kept.device_id, 3, false))
            .await
            .unwrap();
        store.forget_peer(kept.device_id, false).await.unwrap();
        let stored = store.peer(kept.device_id).await.unwrap().unwrap();
        assert!(stored.forgotten);
        assert_eq!(stored.unread, 0);
        assert_eq!(
            store
                .history_page(kept.device_id, None, 50)
                .await
                .unwrap()
                .len(),
            1,
            "history must survive"
        );
    }

    #[tokio::test]
    async fn peers_and_known_devices_compute_activity_and_counts() {
        let (_dir, store) = open_store();

        let alice = profile(9, "Alice");
        let seen = store
            .upsert_peer_seen(&alice, Some("1.1.1.1:1"), Some(1_000))
            .await
            .unwrap();
        assert_eq!(seen.last_activity_ms, Some(1_000));

        store
            .insert_message(&incoming(alice.device_id, 5_000, true))
            .await
            .unwrap();
        store
            .insert_message(&outgoing(alice.device_id, 5_001, MessageStatus::Sent))
            .await
            .unwrap();
        assert_eq!(
            store
                .peer(alice.device_id)
                .await
                .unwrap()
                .unwrap()
                .last_activity_ms,
            Some(5_001),
            "the newest message beats last_seen"
        );

        let bob = profile(10, "Bob");
        let stored = store
            .upsert_peer_seen(&bob, None, Some(3_000))
            .await
            .unwrap();
        assert_eq!(stored.last_activity_ms, Some(3_000));

        let devices = store.known_devices().await.unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].device_id, alice.device_id);
        assert_eq!(devices[0].message_count, 2);
        assert_eq!(devices[1].device_id, bob.device_id);
        assert_eq!(devices[1].message_count, 0);
        assert_eq!(store.peers().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_peer_without_seen_or_messages_reports_no_activity() {
        let (_dir, store) = open_store();
        let ghost = profile(11, "Ghost");
        store
            .upsert_peer_seen(&ghost, Some("3.3.3.3:3"), None)
            .await
            .unwrap();

        let stored = store.peer(ghost.device_id).await.unwrap().expect("ghost");
        assert_eq!(stored.last_seen_ms, None);
        assert_eq!(stored.last_activity_ms, None);
        let devices = store.known_devices().await.unwrap();
        let ghost_device = devices
            .iter()
            .find(|known| known.device_id == ghost.device_id)
            .expect("ghost is known");
        assert_eq!(ghost_device.message_count, 0);
    }

    #[tokio::test]
    async fn upsert_peer_seen_stamps_last_seen_only_on_liveness_evidence() {
        let (_dir, store) = open_store();
        let peer = profile(21, "Nina");

        // A device we can discover but have never connected to has no liveness evidence.
        let created = store
            .upsert_peer_seen(&peer, Some("4.4.4.4:4"), None)
            .await
            .unwrap();
        assert_eq!(created.last_seen_ms, None);
        assert_eq!(created.last_activity_ms, None);
        assert!(created.first_seen_ms > 0);

        let seen = store
            .upsert_peer_seen(&peer, Some("4.4.4.4:4"), Some(10_000))
            .await
            .unwrap();
        assert_eq!(seen.last_seen_ms, Some(10_000));

        // A late, older observation must not move it backwards.
        let older = store
            .upsert_peer_seen(&peer, Some("4.4.4.4:4"), Some(9_000))
            .await
            .unwrap();
        assert_eq!(older.last_seen_ms, Some(10_000));

        // `None` leaves `last_seen` untouched, but still updates identity and clears
        // `forgotten`.
        store.forget_peer(peer.device_id, false).await.unwrap();
        let renamed = profile(21, "Nina Renamed");
        let updated = store
            .upsert_peer_seen(&renamed, Some("4.4.4.4:4"), None)
            .await
            .unwrap();
        assert_eq!(updated.last_seen_ms, Some(10_000));
        assert_eq!(updated.profile.nickname.as_str(), "Nina Renamed");
        assert!(!updated.forgotten);
    }

    #[tokio::test]
    async fn data_survives_closing_and_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("localme.db");
        let peer = profile(12, "Ivan");

        {
            let store = SqliteStore::open(&path).unwrap();
            store.meta_set("device_id", "ivan-device").await.unwrap();
            store
                .upsert_peer_seen(&peer, Some("2.2.2.2:2"), Some(42))
                .await
                .unwrap();
            store
                .insert_message(&incoming(peer.device_id, 43, false))
                .await
                .unwrap();
        }

        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(
            store.meta_get("device_id").await.unwrap().as_deref(),
            Some("ivan-device")
        );
        let stored = store.peer(peer.device_id).await.unwrap().unwrap();
        assert_eq!(stored.profile.nickname.as_str(), "Ivan");
        assert_eq!(stored.unread, 1);
        assert_eq!(
            store
                .history_page(peer.device_id, None, 50)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_corrupt_file_is_quarantined_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("localme.db");
        std::fs::write(&path, vec![b'X'; 4 * 1024]).unwrap();

        let (store, report) = SqliteStore::open_with_report(&path).expect("open must succeed");
        let preserved = report.expect("a preserved path");
        let preserved_path = Path::new(&preserved);
        assert!(preserved_path.exists(), "the corrupt file must be kept");
        assert_ne!(preserved_path, path.as_path());
        assert!(path.exists(), "a fresh database must be created");

        assert_eq!(
            store.meta_get("schema_version").await.unwrap().as_deref(),
            Some("1")
        );
        store.meta_set("k", "v").await.unwrap();
        assert_eq!(store.meta_get("k").await.unwrap().as_deref(), Some("v"));
    }

    #[tokio::test]
    async fn clear_history_keeps_the_peer_list() {
        let (_dir, store) = open_store();
        let peer = profile(13, "Judy");
        store.upsert_peer_seen(&peer, None, Some(1)).await.unwrap();
        store
            .insert_message(&incoming(peer.device_id, 1, false))
            .await
            .unwrap();
        store
            .insert_message(&incoming(peer.device_id, 2, false))
            .await
            .unwrap();
        store
            .insert_message(&outgoing(peer.device_id, 3, MessageStatus::Sent))
            .await
            .unwrap();

        assert_eq!(store.clear_history().await.unwrap(), 3);
        assert!(
            store
                .history_page(peer.device_id, None, 50)
                .await
                .unwrap()
                .is_empty()
        );
        let stored = store.peer(peer.device_id).await.unwrap().unwrap();
        assert_eq!(stored.unread, 0);
    }

    #[tokio::test]
    async fn a_stopped_writer_reports_unavailable() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        let store = SqliteStore {
            sender: Some(sender),
            writer: None,
        };

        assert!(matches!(
            store.meta_get("k").await,
            Err(StorageError::Unavailable)
        ));
        assert!(matches!(
            store.peers().await,
            Err(StorageError::Unavailable)
        ));
        assert!(matches!(
            store.insert_message(&incoming(device(14), 1, false)).await,
            Err(StorageError::Unavailable)
        ));
    }
}
