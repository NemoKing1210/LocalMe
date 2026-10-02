//! Forward-only migrations. The version is stored in `meta.schema_version`; each migration runs
//! in a transaction and bumps the version, so a crash leaves the database on the previous version
//! rather than half-upgraded. A fresh database (version 0) and an upgraded one take the same path.

use rusqlite::{Connection, OptionalExtension, params};

use super::sqlite_error;
use crate::error::StorageError;

pub(super) const SCHEMA_VERSION: u32 = 3;

pub(super) const MIGRATION_1: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE peers (
  device_id    TEXT PRIMARY KEY,
  nickname     TEXT NOT NULL,
  avatar_seed  TEXT NOT NULL,
  last_address TEXT,
  last_seen_ms INTEGER,
  first_seen_ms INTEGER NOT NULL,
  unread       INTEGER NOT NULL DEFAULT 0,
  notify_muted INTEGER NOT NULL DEFAULT 0,
  forgotten    INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE messages (
  id           TEXT PRIMARY KEY,
  peer_id      TEXT NOT NULL REFERENCES peers(device_id) ON DELETE CASCADE,
  outgoing     INTEGER NOT NULL,
  body         TEXT NOT NULL,
  sent_at_ms   INTEGER NOT NULL,
  received_at_ms INTEGER NOT NULL,
  status       TEXT NOT NULL,
  read         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX messages_peer_time ON messages(peer_id, sent_at_ms DESC, id DESC);
CREATE INDEX messages_unread    ON messages(peer_id) WHERE read = 0;
";

/// The outbox: messages that could not be handed to a socket yet keep their place in the
/// conversation and are retried, so the statuses that used to mean "this attempt is over"
/// (`sending`, `sent`, `failed`) become one durable `queued`. `delivered_at_ms` is the second
/// date the interface shows for a message that waited.
pub(super) const MIGRATION_2: &str = "
ALTER TABLE messages ADD COLUMN delivered_at_ms INTEGER;
UPDATE messages SET status = 'queued'
  WHERE outgoing = 1 AND status IN ('sending', 'sent', 'failed');
CREATE INDEX messages_outbox ON messages(peer_id, sent_at_ms, id)
  WHERE outgoing = 1 AND status = 'queued';
";

/// Attachments: the columns a file transfer needs, and the one column that had to change.
///
/// `messages.body` becomes nullable, because a message may be nothing but files — and SQLite
/// cannot drop a `NOT NULL`, so the table is rebuilt. The rebuild is the documented
/// copy-drop-rename: the old indexes travel with the renamed table and are recreated for the new
/// one. It is safe here because nothing references `messages` yet at this point in the schema's
/// life; `attachments` is created below, after the rename.
pub(super) const MIGRATION_3: &str = "
ALTER TABLE messages RENAME TO messages_v2;
CREATE TABLE messages (
  id           TEXT PRIMARY KEY,
  peer_id      TEXT NOT NULL REFERENCES peers(device_id) ON DELETE CASCADE,
  outgoing     INTEGER NOT NULL,
  body         TEXT,
  sent_at_ms   INTEGER NOT NULL,
  received_at_ms INTEGER NOT NULL,
  delivered_at_ms INTEGER,
  status       TEXT NOT NULL,
  read         INTEGER NOT NULL DEFAULT 0
);
INSERT INTO messages (id, peer_id, outgoing, body, sent_at_ms, received_at_ms, delivered_at_ms, status, read)
  SELECT id, peer_id, outgoing, body, sent_at_ms, received_at_ms, delivered_at_ms, status, read
  FROM messages_v2;
DROP TABLE messages_v2;
CREATE INDEX messages_peer_time ON messages(peer_id, sent_at_ms DESC, id DESC);
CREATE INDEX messages_unread    ON messages(peer_id) WHERE read = 0;
CREATE INDEX messages_outbox    ON messages(peer_id, sent_at_ms, id)
  WHERE outgoing = 1 AND status = 'queued';

CREATE TABLE attachments (
  id            TEXT PRIMARY KEY,
  message_id    TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  peer_id       TEXT NOT NULL,
  outgoing      INTEGER NOT NULL,
  name          TEXT NOT NULL,
  size          INTEGER NOT NULL,
  kind          TEXT NOT NULL,
  state         TEXT NOT NULL,
  transferred   INTEGER NOT NULL DEFAULT 0,
  sha256        TEXT,
  path          TEXT,
  created_at_ms INTEGER NOT NULL
);
CREATE INDEX attachments_message ON attachments(message_id);
CREATE INDEX attachments_outbox  ON attachments(peer_id, message_id)
  WHERE outgoing = 1 AND state IN ('queued', 'sending');
";

pub(super) fn migrate(conn: &mut Connection) -> Result<(), StorageError> {
    let mut version = current_version(conn)?;
    while version < SCHEMA_VERSION {
        let next = version
            .checked_add(1)
            .ok_or_else(|| StorageError::Migration {
                version,
                reason: "schema version overflow".to_owned(),
            })?;
        apply_migration(conn, next)?;
        version = next;
    }
    Ok(())
}

fn current_version(conn: &Connection) -> Result<u32, StorageError> {
    let has_meta: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
            [],
            |row| row.get(0),
        )
        .map_err(sqlite_error)?;
    if has_meta == 0 {
        return Ok(0);
    }

    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sqlite_error)?;
    match stored {
        None => Ok(0),
        Some(raw) => raw.parse::<u32>().map_err(|error| StorageError::Migration {
            version: 0,
            reason: format!("schema_version `{raw}` is not a number: {error}"),
        }),
    }
}

fn apply_migration(conn: &mut Connection, version: u32) -> Result<(), StorageError> {
    let sql = match version {
        1 => MIGRATION_1,
        2 => MIGRATION_2,
        3 => MIGRATION_3,
        other => {
            return Err(StorageError::Migration {
                version: other,
                reason: "no migration is defined for this version".to_owned(),
            });
        }
    };

    let tx = conn.transaction().map_err(sqlite_error)?;
    tx.execute_batch(sql)
        .map_err(|error| StorageError::Migration {
            version,
            reason: error.to_string(),
        })?;
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![version.to_string()],
    )
    .map_err(|error| StorageError::Migration {
        version,
        reason: error.to_string(),
    })?;
    tx.commit().map_err(|error| StorageError::Migration {
        version,
        reason: error.to_string(),
    })?;
    Ok(())
}
