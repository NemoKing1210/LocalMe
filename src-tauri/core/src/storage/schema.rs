//! Forward-only migrations. The version is stored in `meta.schema_version`; each migration runs
//! in a transaction and bumps the version, so a crash leaves the database on the previous version
//! rather than half-upgraded. A fresh database (version 0) and an upgraded one take the same path.

use rusqlite::{Connection, OptionalExtension, params};

use super::sqlite_error;
use crate::error::StorageError;

pub(super) const SCHEMA_VERSION: u32 = 1;

const MIGRATION_1: &str = "
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
