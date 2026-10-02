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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    use crate::domain::ids::{DeviceId, MessageId};

    fn connection() -> (TempDir, Connection) {
        let dir = tempfile::tempdir().expect("temp dir");
        let conn = Connection::open(dir.path().join("schema.db")).expect("open");
        (dir, conn)
    }

    fn set_version(conn: &Connection, version: u32) {
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![version.to_string()],
        )
        .expect("set version");
    }

    fn insert_peer(conn: &Connection, device_id: &str, nickname: &str) {
        conn.execute(
            "INSERT INTO peers (device_id, nickname, avatar_seed, first_seen_ms) \
             VALUES (?1, ?2, 'seed', 11)",
            params![device_id, nickname],
        )
        .expect("insert peer");
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get::<_, i64>(0),
        )
        .expect("sqlite_master")
            > 0
    }

    /// A database created by the first shipped build: peer, one outgoing text message and one
    /// incoming one, at version 1.
    fn build_version_one(conn: &Connection, device_id: &str) -> (String, String) {
        conn.execute_batch(MIGRATION_1).expect("migration 1");
        set_version(conn, 1);
        insert_peer(conn, device_id, "Ann");
        let outgoing = MessageId::generate().to_string();
        let incoming = MessageId::generate().to_string();
        conn.execute(
            "INSERT INTO messages \
               (id, peer_id, outgoing, body, sent_at_ms, received_at_ms, status, read) \
             VALUES (?1, ?2, 1, 'out of the outbox', 42, 42, 'sent', 1)",
            params![outgoing, device_id],
        )
        .expect("outgoing row");
        conn.execute(
            "INSERT INTO messages \
               (id, peer_id, outgoing, body, sent_at_ms, received_at_ms, status, read) \
             VALUES (?1, ?2, 0, 'hello there', 43, 43, 'received', 0)",
            params![incoming, device_id],
        )
        .expect("incoming row");
        (outgoing, incoming)
    }

    fn message_status(conn: &Connection, id: &str) -> String {
        conn.query_row(
            "SELECT status FROM messages WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .expect("status")
    }

    #[test]
    fn a_fresh_database_migrates_to_the_current_version() {
        let (_dir, mut conn) = connection();
        assert_eq!(current_version(&conn).unwrap(), 0);
        migrate(&mut conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
        assert_eq!(SCHEMA_VERSION, 3, "the build under test");
        assert!(table_exists(&conn, "attachments"));
    }

    #[test]
    fn a_missing_version_row_reads_as_zero() {
        let (_dir, conn) = connection();
        // A older build may have created `meta` before recording a version.
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             INSERT INTO meta (key, value) VALUES ('nickname', 'Ann');",
        )
        .unwrap();
        assert_eq!(current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let (_dir, mut conn) = connection();
        let device = DeviceId::generate().to_string();
        build_version_one(&conn, &device);

        migrate(&mut conn).unwrap();
        let after_first: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after_first, 2);

        migrate(&mut conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
        let after_second: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after_second, 2, "a second pass must not duplicate rows");
        assert_eq!(
            conn.query_row(
                "SELECT nickname FROM peers WHERE device_id = ?1",
                params![device],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "Ann",
            "peers are untouched by a repeated migration"
        );
    }

    #[test]
    fn a_version_one_database_upgrades_through_every_step_preserving_rows() {
        let (_dir, mut conn) = connection();
        let device = DeviceId::generate().to_string();
        let (outgoing, incoming) = build_version_one(&conn, &device);

        migrate(&mut conn).unwrap();

        assert_eq!(current_version(&conn).unwrap(), SCHEMA_VERSION);
        // Migration 2 returns every unfinished outgoing row to the outbox.
        assert_eq!(message_status(&conn, &outgoing), "queued");
        assert_eq!(message_status(&conn, &incoming), "received");
        // Text and dates survive the table rebuild of migration 3.
        let (body, sent_at, read): (String, i64, i64) = conn
            .query_row(
                "SELECT body, sent_at_ms, read FROM messages WHERE id = ?1",
                params![incoming],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(body, "hello there");
        assert_eq!(sent_at, 43);
        assert_eq!(read, 0, "the unread flag is preserved verbatim");
        assert!(
            conn.query_row("SELECT COUNT(*) FROM attachments", [], |row| row
                .get::<_, i64>(0),)
                .unwrap()
                == 0
        );
    }

    #[test]
    fn migration_two_normalises_legacy_statuses() {
        let (_dir, mut conn) = connection();
        conn.execute_batch(MIGRATION_1).unwrap();
        set_version(&conn, 1);
        let device = DeviceId::generate().to_string();
        insert_peer(&conn, &device, "Bob");

        let legacy = [
            ("sending", "queued"),
            ("sent", "queued"),
            ("failed", "queued"),
            ("delivered", "delivered"),
            ("received", "received"),
        ];
        let ids: Vec<(String, &str)> = legacy
            .iter()
            .enumerate()
            .map(|(index, (before, _))| {
                let id = MessageId::generate().to_string();
                conn.execute(
                    "INSERT INTO messages \
                       (id, peer_id, outgoing, body, sent_at_ms, received_at_ms, status, read) \
                     VALUES (?1, ?2, 1, 'in flight', ?3, ?3, ?4, 1)",
                    params![id, device, index as i64, before],
                )
                .unwrap();
                (id, *before)
            })
            .collect();

        apply_migration(&mut conn, 2).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 2);
        // The new column exists and is NULL for every old row.
        for (id, before) in &ids {
            let expected = legacy
                .iter()
                .find(|(name, _)| name == before)
                .map(|(_, after)| *after)
                .unwrap();
            assert_eq!(
                message_status(&conn, id),
                expected,
                "`{before}` must become `{expected}`"
            );
            let delivered_at: Option<i64> = conn
                .query_row(
                    "SELECT delivered_at_ms FROM messages WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(delivered_at, None);
        }
    }

    #[test]
    fn a_newer_schema_version_is_left_untouched() {
        let (_dir, mut conn) = connection();
        let device = DeviceId::generate().to_string();
        build_version_one(&conn, &device);
        set_version(&conn, 99);

        // Observed behaviour: `migrate` only walks forward (`while version < SCHEMA_VERSION`),
        // so a database from a future build is accepted as-is rather than downgraded or
        // rejected — the version and every row stay exactly where they were.
        migrate(&mut conn).unwrap();

        assert_eq!(current_version(&conn).unwrap(), 99);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row
                .get::<_, i64>(0),)
                .unwrap(),
            2,
            "rows from a newer schema are not touched"
        );
    }

    #[test]
    fn a_failed_migration_rolls_back_to_the_previous_version() {
        let (_dir, mut conn) = connection();
        conn.execute_batch(MIGRATION_1).unwrap();
        conn.execute_batch(MIGRATION_2).unwrap();
        set_version(&conn, 2);
        let device = DeviceId::generate().to_string();
        insert_peer(&conn, &device, "Carol");
        conn.execute(
            "INSERT INTO messages \
               (id, peer_id, outgoing, body, sent_at_ms, received_at_ms, status, read) \
             VALUES ('m', ?1, 1, 'still here', 5, 5, 'queued', 1)",
            params![device],
        )
        .unwrap();
        // Collides with the `attachments` table migration 3 is about to create, so the step
        // fails *after* it has renamed and rebuilt `messages`.
        conn.execute_batch("CREATE TABLE attachments (id TEXT PRIMARY KEY)")
            .unwrap();

        let error = migrate(&mut conn).unwrap_err();
        assert!(
            matches!(error, StorageError::Migration { version: 3, .. }),
            "the failing step must be reported"
        );
        assert_eq!(
            current_version(&conn).unwrap(),
            2,
            "the version bump is part of the same transaction and is rolled back"
        );
        assert!(
            table_exists(&conn, "messages"),
            "the rename inside the failed step must be undone"
        );
        assert!(!table_exists(&conn, "messages_v2"));
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row
                .get::<_, i64>(0),)
                .unwrap(),
            1,
            "the rows survive the aborted migration"
        );
    }

    #[test]
    fn a_non_numeric_stored_version_is_a_migration_error() {
        let (_dir, conn) = connection();
        conn.execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', 'soon')",
            [],
        )
        .unwrap();

        match current_version(&conn) {
            Err(StorageError::Migration { version, reason }) => {
                assert_eq!(version, 0);
                assert!(reason.contains("soon"), "the raw value is named: {reason}");
            }
            other => panic!("expected a migration error, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_migration_version_is_rejected() {
        let (_dir, mut conn) = connection();
        let error = apply_migration(&mut conn, 9).unwrap_err();
        assert!(matches!(error, StorageError::Migration { version: 9, .. }));
        assert_eq!(
            current_version(&conn).unwrap(),
            0,
            "a rejected step leaves no version behind"
        );
    }
}
