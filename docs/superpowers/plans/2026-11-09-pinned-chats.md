# Pinned Chats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user pin conversations, keep them above every unpinned one, and reorder the pinned block by dragging.

**Architecture:** Pin state and its manual order live in the core, in two new `peers` columns written by a schema migration; `PeerView::sort_for_display` puts the pinned block first and leaves the existing rule untouched below it. The session exposes pin and reorder commands through the thin Tauri host, and the virtualised people list gains a row-menu action, a pin glyph, pointer dragging and `Alt+↑/↓`.

**Tech Stack:** Rust 2024 (localme-core + localme host, SQLite via rusqlite), Vue 3.5 + TypeScript strict, Pinia, Vitest, Tauri 2.

**Spec:** `docs/superpowers/specs/2026-11-09-pinned-chats-design.md`

## Global Constraints

- Version this change as **0.13.0** (minor capability bump); run `npm run version:minor` and edit the generated CHANGELOG stub.
- Schema migrations are forward-only. The new migration is **5**; `SCHEMA_VERSION` becomes 5.
- Pinned rows sort above **all** unpinned rows, including unread and online ones. Inside the pinned block the only order is `pin_rank` ascending, ties broken by `device_id`. The old rule (`unread > online > activity > nickname`) is unchanged for the unpinned block, with `device_id` as the final tie-break.
- Pinning a row gives it `pin_rank = MIN(pin_rank over pinned, not forgotten) - 1`, so a new pin is on top.
- Forgetting a peer clears its pin.
- Every new user-facing string exists in all seven catalogues (`en`, `ru`, `es`, `de`, `fr`, `pt`, `zh`); `MessageKey` is derived from `en`, so a missing key is a typecheck error.
- No new dependencies, no `any`, no `invoke` outside `src/ipc/`, no `v-html`.
- Prettier: 100 columns, single quotes. Rust: default `rustfmt`.
- Commit messages carry no AI co-author or generated-by trailer in this repository.
- Before the change is finished: `npm run lint`, `npm run format:check`, `npm run typecheck`, `npm run test`, `npm run build`, `npm run check:versions`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

## Review Focus

- Two pinned rows with equal or missing `pin_rank` (hand-edited database, interrupted reorder) must still produce a stable, deterministic order.
- A reorder request that is not exactly the current pinned set — missing an id, carrying an extra one or a duplicate (stale UI, concurrent forget) — must be rejected without rewriting any rank.
- Pin, unpin and reorder while a search filter hides some pinned rows: pinning must still work, and a drag must never persist an order built from a filtered subset.
- A storage failure during pin or reorder must not leave the in-memory list claiming a state that was never persisted.
- Starting a drag must not swallow a normal click, and releasing the pointer outside the list must not leave a stuck drag.

---

### Task 1: Pin state in the store

**Files:**

- Modify: `src-tauri/core/src/storage/schema.rs`
- Modify: `src-tauri/core/src/ports/store.rs`
- Modify: `src-tauri/core/src/storage/mod.rs`
- Modify: `src-tauri/core/src/services/session.rs` (the `FakeStore` fixture only)

**Interfaces:**

- Consumes: nothing from earlier tasks.
- Produces:
  - `StoredPeer { …, pinned: bool, pin_rank: Option<i64> }`
  - `Store::set_peer_pinned(&self, device_id: DeviceId, pinned: bool) -> impl Future<Output = Result<Option<i64>, StorageError>> + Send`
  - `Store::reorder_pinned(&self, device_ids: Vec<DeviceId>) -> impl Future<Output = Result<(), StorageError>> + Send`
  - `set_peer_pinned` returns `Some(rank)` when the row ends up pinned and `None` when it ends up unpinned.

- [ ] **Step 1: Write the failing migration test**

In `src-tauri/core/src/storage/schema.rs`, add to `mod tests`:

```rust
#[test]
fn migration_five_adds_the_pin_columns() {
    let (_dir, mut conn) = connection();
    migrate(&mut conn).unwrap();

    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('peers')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert!(columns.contains(&"pinned".to_owned()));
    assert!(columns.contains(&"pin_rank".to_owned()));
    assert_eq!(SCHEMA_VERSION, 5, "the build under test");
}
```

Also change the existing assertion in `a_fresh_database_migrates_to_the_current_version` from `4` to `5`.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p localme-core migration_five_adds_the_pin_columns`
Expected: FAIL — the columns are absent and `SCHEMA_VERSION` is 4.

- [ ] **Step 3: Add migration 5**

In `schema.rs`, set `pub(super) const SCHEMA_VERSION: u32 = 5;` and add:

```rust
/// Pin state and the manual order of the pinned block.
///
/// `pin_rank` is `NULL` for an unpinned row; among pinned rows it is the position, ascending.
pub(super) const MIGRATION_5: &str = "
ALTER TABLE peers ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE peers ADD COLUMN pin_rank INTEGER;
CREATE INDEX peers_pinned ON peers(pinned, pin_rank) WHERE pinned = 1;
";
```

Add `5 => MIGRATION_5,` to `apply_migration`.

- [ ] **Step 4: Run the migration tests to verify they pass**

Run: `cargo test -p localme-core storage::schema`
Expected: PASS.

- [ ] **Step 5: Write the failing storage tests**

In `src-tauri/core/src/storage/mod.rs`, add to `mod tests`:

```rust
#[tokio::test]
async fn pinning_a_peer_places_it_on_top_and_reordering_rewrites_the_ranks() {
    let (_dir, store) = open_store();
    let ann = profile(61, "Ann");
    let ben = profile(62, "Ben");
    let cid = profile(63, "Cid");
    for peer in [&ann, &ben, &cid] {
        store.upsert_peer_seen(peer, None, Some(1)).await.unwrap();
    }

    assert_eq!(store.set_peer_pinned(ann.device_id, true).await.unwrap(), Some(0));
    assert_eq!(store.set_peer_pinned(ben.device_id, true).await.unwrap(), Some(-1));
    assert_eq!(store.set_peer_pinned(cid.device_id, true).await.unwrap(), Some(-2));

    store
        .reorder_pinned(vec![ann.device_id, cid.device_id, ben.device_id])
        .await
        .unwrap();

    let stored = store.peers().await.unwrap();
    let rank = |id| {
        stored
            .iter()
            .find(|peer| peer.profile.device_id == id)
            .unwrap()
            .pin_rank
    };
    assert_eq!(rank(ann.device_id), Some(0));
    assert_eq!(rank(cid.device_id), Some(1));
    assert_eq!(rank(ben.device_id), Some(2));

    assert_eq!(store.set_peer_pinned(ben.device_id, false).await.unwrap(), None);
    let ben_row = store.peer(ben.device_id).await.unwrap().unwrap();
    assert!(!ben_row.pinned);
    assert_eq!(ben_row.pin_rank, None);
}

#[tokio::test]
async fn forgetting_a_pinned_peer_clears_the_pin() {
    let (_dir, store) = open_store();
    let peer = profile(64, "Dana");
    store.upsert_peer_seen(&peer, None, Some(1)).await.unwrap();
    store.set_peer_pinned(peer.device_id, true).await.unwrap();

    store.forget_peer(peer.device_id, false).await.unwrap();

    let row = store.peer(peer.device_id).await.unwrap().unwrap();
    assert!(row.forgotten);
    assert!(!row.pinned);
    assert_eq!(row.pin_rank, None);
}
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -p localme-core pinning_a_peer_places_it_on_top`
Expected: FAIL — `set_peer_pinned` does not exist on `Store`.

- [ ] **Step 7: Add the fields, the trait methods and the SQLite implementation**

1. In `ports/store.rs`, add to `StoredPeer`:

```rust
pub pinned: bool,
pub pin_rank: Option<i64>,
```

2. Add to `trait Store`:

```rust
fn set_peer_pinned(
    &self,
    device_id: DeviceId,
    pinned: bool,
) -> impl Future<Output = Result<Option<i64>, StorageError>> + Send;

/// Rewrites the positions of the pinned rows to the order given. The caller has already
/// checked that the list is exactly the pinned set. Unknown or unpinned ids are ignored.
fn reorder_pinned(
    &self,
    device_ids: Vec<DeviceId>,
) -> impl Future<Output = Result<(), StorageError>> + Send;
```

3. In `storage/mod.rs`:

- Append `p.pinned, p.pin_rank` to the end of `peer_columns()`.
- Add `pinned: i64` and `pin_rank: Option<i64>` to `PeerRow`; read them at indices `12` and `13`; set them in `into_stored` as `pinned: self.pinned != 0` and `pin_rank: self.pin_rank`.
- Add `Request::SetPeerPinned { device_id, pinned, reply: Reply<Option<i64>> }` and `Request::ReorderPinned { device_ids, reply: Reply<()> }`, plus their `dispatch` arms.
- Add the storage functions:

```rust
fn set_peer_pinned(
    conn: &Connection,
    device_id: DeviceId,
    pinned: bool,
) -> Result<Option<i64>, StorageError> {
    if !pinned {
        conn.execute(
            "UPDATE peers SET pinned = 0, pin_rank = NULL WHERE device_id = ?1",
            params![device_id.to_string()],
        )
        .map_err(sqlite_error)?;
        return Ok(None);
    }

    let min: Option<i64> = conn
        .query_row(
            "SELECT MIN(pin_rank) FROM peers WHERE pinned = 1 AND forgotten = 0",
            [],
            |row| row.get(0),
        )
        .map_err(sqlite_error)?;
    let rank = min.map_or(0, |value| value.saturating_sub(1));
    conn.execute(
        "UPDATE peers SET pinned = 1, pin_rank = ?2 WHERE device_id = ?1",
        params![device_id.to_string(), rank],
    )
    .map_err(sqlite_error)?;
    Ok(Some(rank))
}

fn reorder_pinned(conn: &mut Connection, device_ids: &[DeviceId]) -> Result<(), StorageError> {
    let tx = conn.transaction().map_err(sqlite_error)?;
    for (rank, device_id) in device_ids.iter().enumerate() {
        let rank = i64::try_from(rank).unwrap_or(i64::MAX);
        tx.execute(
            "UPDATE peers SET pin_rank = ?2 WHERE device_id = ?1 AND pinned = 1",
            params![device_id.to_string(), rank],
        )
        .map_err(sqlite_error)?;
    }
    tx.commit().map_err(sqlite_error)?;
    Ok(())
}
```

- In `forget_peer`, change the peer update to `UPDATE peers SET forgotten = 1, unread = 0, pinned = 0, pin_rank = NULL WHERE device_id = ?1`.
- In `impl Store for SqliteStore`, delegate both methods through `self.call(...)`.

4. In `services/session.rs`, update the `FakeStore`:

- Add `set_peer_pinned: bool` and `reorder_pinned: bool` to `Fail`.
- Initialise `pinned: false, pin_rank: None` in `upsert_locked` and `IntoStored::into_stored`.
- Add `pinned: false, pin_rank: None` to every other `StoredPeer { … }` literal in the test module (lines around 2631, 2704, 3508 and the trait helper).
- Implement the two trait methods: pinning computes the same minimum over `peers.values().filter(|p| p.pinned && !p.forgotten)`, unpinning clears both fields; reorder assigns `0..n` to the listed ids that are pinned.
- In `FakeStore::forget_peer`, also set `entry.pinned = false; entry.pin_rank = None;`.

- [ ] **Step 8: Run the core tests**

Run: `cargo test -p localme-core`
Expected: PASS. Any remaining compile error is a `StoredPeer` literal that still needs the two fields.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/core/src/storage/schema.rs src-tauri/core/src/ports/store.rs \
  src-tauri/core/src/storage/mod.rs src-tauri/core/src/services/session.rs
git commit -m "feat: store pinned conversations and their order"
```

---

### Task 2: Sort the pinned block first

**Files:**

- Modify: `src-tauri/core/src/domain/peer.rs`
- Modify: `src-tauri/core/src/services/session.rs` (`PeerEntry::view`)
- Modify: `src-tauri/core/src/services/events.rs` (test helper only)

**Interfaces:**

- Consumes: `StoredPeer::pinned` and `StoredPeer::pin_rank` from Task 1.
- Produces:
  - `PeerView { …, pinned: bool, pin_rank: Option<i64> }`
  - `PeerView::sort_for_display` with the pinned block first.
  - The serialised peer carries `pinned` and `pinRank` (camelCase).

- [ ] **Step 1: Write the failing ordering tests**

In `src-tauri/core/src/domain/peer.rs`, extend the test helper to set the new fields:

```rust
fn peer(id: u128, nick: &str, online: bool, unread: u32, activity: Option<i64>) -> PeerView {
    // …
    PeerView {
        // …
        pinned: false,
        pin_rank: None,
    }
}

fn pinned(mut peer: PeerView, rank: i64) -> PeerView {
    peer.pinned = true;
    peer.pin_rank = Some(rank);
    peer
}
```

Add:

```rust
#[test]
fn pinned_rows_sort_above_unread_and_online_rows() {
    let mut peers = vec![
        peer(1, "unread-online", true, 4, Some(9_000)),
        pinned(peer(2, "pinned-offline", false, 0, Some(1_000)), 0),
    ];
    PeerView::sort_for_display(&mut peers);
    assert_eq!(peers.first().unwrap().nickname.as_str(), "pinned-offline");
}

#[test]
fn pinned_rows_keep_their_manual_rank_above_activity() {
    let mut peers = vec![
        pinned(peer(1, "second", true, 9, Some(9_000)), 1),
        pinned(peer(2, "first", false, 0, Some(1_000)), 0),
    ];
    PeerView::sort_for_display(&mut peers);
    let order: Vec<&str> = peers.iter().map(|p| p.nickname.as_str()).collect();
    assert_eq!(order, vec!["first", "second"]);
}

#[test]
fn equal_pin_ranks_fall_back_to_the_device_id() {
    let mut forward = vec![
        pinned(peer(7, "b", true, 0, None), 0),
        pinned(peer(8, "a", true, 0, None), 0),
    ];
    let mut backward = vec![
        pinned(peer(8, "a", true, 0, None), 0),
        pinned(peer(7, "b", true, 0, None), 0),
    ];
    PeerView::sort_for_display(&mut forward);
    PeerView::sort_for_display(&mut backward);
    assert_eq!(forward, backward);
    assert_eq!(
        forward.first().unwrap().device_id,
        DeviceId::from_uuid(uuid::Uuid::from_u128(7))
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p localme-core pinned_rows_sort_above`
Expected: FAIL — `PeerView` has no `pinned` field.

- [ ] **Step 3: Add the fields and the comparator**

Add to `PeerView`:

```rust
/// Whether this conversation stays in the block above the rest of the list.
pub pinned: bool,
/// Position inside the pinned block, ascending. `None` when the row is not pinned.
pub pin_rank: Option<i64>,
```

Replace `sort_for_display` with a comparator whose first decision is the pinned block:

```rust
pub fn sort_for_display(peers: &mut [Self]) {
    peers.sort_by(|a, b| match (a.pinned, b.pinned) {
        (true, true) => a
            .pin_rank
            .unwrap_or(i64::MAX)
            .cmp(&b.pin_rank.unwrap_or(i64::MAX))
            .then_with(|| a.device_id.cmp(&b.device_id)),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => {
            let a_unread = u8::from(a.unread > 0);
            let b_unread = u8::from(b.unread > 0);
            b_unread
                .cmp(&a_unread)
                .then_with(|| b.online.cmp(&a.online))
                .then_with(|| b.activity().cmp(&a.activity()))
                .then_with(|| a.nickname.as_str().cmp(b.nickname.as_str()))
                .then_with(|| a.device_id.cmp(&b.device_id))
        }
    });
}
```

The final `device_id` term also makes duplicate nicknames total; keep the existing comment truthful by mentioning it.

Update `PeerEntry::view` in `services/session.rs`:

```rust
pinned: self.stored.pinned,
pin_rank: self.stored.pin_rank,
```

Add the same two fields to the `PeerView` literal in `services/events.rs` (`pinned: false, pin_rank: None`).

- [ ] **Step 4: Run the core tests**

Run: `cargo test -p localme-core`
Expected: PASS. The old ordering tests still pass because their helper rows are unpinned.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/core/src/domain/peer.rs src-tauri/core/src/services/session.rs \
  src-tauri/core/src/services/events.rs
git commit -m "feat: sort pinned conversations above the rest"
```

---

### Task 3: Session commands and IPC

**Files:**

- Modify: `src-tauri/core/src/error.rs`
- Modify: `src-tauri/core/src/services/session.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/error.rs`
- Modify: `src/ipc/index.ts`

**Interfaces:**

- Consumes: `Store::set_peer_pinned`, `Store::reorder_pinned`, `PeerView::pinned`/`pin_rank`.
- Produces:
  - `SessionHandle::set_pinned(&self, peer: DeviceId, pinned: bool) -> Result<(), CoreError>`
  - `SessionHandle::reorder_pinned(&self, peer_ids: Vec<DeviceId>) -> Result<(), CoreError>`
  - `CoreError::InvalidPinnedOrder`
  - Tauri commands `set_peer_pinned(peer_id: String, pinned: bool)` and
    `reorder_pinned_peers(peer_ids: Vec<String>)`
  - `ipc.setPeerPinned(peerId, pinned): Promise<void>` and
    `ipc.reorderPinnedPeers(peerIds): Promise<void>`

- [ ] **Step 1: Write the failing session tests**

In `services/session.rs`, add to `mod tests` (the `Harness` has direct access to the private session):

```rust
#[tokio::test]
async fn command_set_pinned_puts_the_row_on_top_and_emits_peers() {
    let mut h = harness();
    h.add_peer(did(2), "bob");
    h.add_peer(did(3), "ann");
    let _ = h.events();

    let (tx, reply) = oneshot::channel();
    h.session
        .handle_command(SessionCommand::SetPinned {
            peer: did(2),
            pinned: true,
            reply: tx,
        })
        .await;
    reply.await.unwrap();

    let views = h.session.arranged_peers();
    assert_eq!(views.first().unwrap().device_id, did(2));
    assert!(views.first().unwrap().pinned);
    assert_eq!(views.first().unwrap().pin_rank, Some(0));
    assert!(
        h.events()
            .iter()
            .any(|event| matches!(event, CoreEvent::Peers { .. }))
    );
}

#[tokio::test]
async fn command_reorder_pinned_rewrites_the_order_and_rejects_a_non_permutation() {
    let mut h = harness();
    for id in [2, 3, 4] {
        h.add_peer(did(id), "peer");
        let (tx, reply) = oneshot::channel();
        h.session
            .handle_command(SessionCommand::SetPinned {
                peer: did(id),
                pinned: true,
                reply: tx,
            })
            .await;
        reply.await.unwrap();
    }

    let (tx, reply) = oneshot::channel();
    h.session
        .handle_command(SessionCommand::ReorderPinned {
            peer_ids: vec![did(4), did(2), did(3)],
            reply: tx,
        })
        .await;
    reply.await.unwrap().unwrap();
    let order: Vec<DeviceId> = h
        .session
        .arranged_peers()
        .into_iter()
        .take(3)
        .map(|peer| peer.device_id)
        .collect();
    assert_eq!(order, vec![did(4), did(2), did(3)]);

    let (tx, reply) = oneshot::channel();
    h.session
        .handle_command(SessionCommand::ReorderPinned {
            peer_ids: vec![did(2), did(3)],
            reply: tx,
        })
        .await;
    assert!(matches!(
        reply.await.unwrap(),
        Err(CoreError::InvalidPinnedOrder)
    ));
    assert_eq!(
        h.session.arranged_peers().first().unwrap().device_id,
        did(4),
        "a rejected reorder changes nothing"
    );
}

#[tokio::test]
async fn a_store_failure_leaves_the_row_unpinned() {
    let mut h = harness();
    h.add_peer(did(2), "bob");
    h.store.fail(|fail| fail.set_peer_pinned = true);

    let (tx, reply) = oneshot::channel();
    h.session
        .handle_command(SessionCommand::SetPinned {
            peer: did(2),
            pinned: true,
            reply: tx,
        })
        .await;
    reply.await.unwrap();

    assert!(!h.session.peers[&did(2)].stored.pinned);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p localme-core command_set_pinned`
Expected: FAIL — the `SessionCommand::SetPinned` variant does not exist.

- [ ] **Step 3: Implement the session side**

- Add `#[error("the pinned order is not a permutation of the pinned conversations")] InvalidPinnedOrder,` to `CoreError`.
- Add to `SessionCommand`:

```rust
SetPinned {
    peer: DeviceId,
    pinned: bool,
    reply: oneshot::Sender<()>,
},
ReorderPinned {
    peer_ids: Vec<DeviceId>,
    reply: oneshot::Sender<Result<(), CoreError>>,
},
```

- Add `SessionHandle` methods. `set_pinned` uses the existing `request` helper; `reorder_pinned` sends the command and awaits the inner `Result`.
- Add handler arms:

```rust
SessionCommand::SetPinned { peer, pinned, reply } => {
    match self.store.set_peer_pinned(peer, pinned).await {
        Ok(rank) => {
            if let Some(entry) = self.peers.get_mut(&peer) {
                entry.stored.pinned = pinned;
                entry.stored.pin_rank = rank;
                self.emit_peers();
            }
        }
        Err(error) => tracing::warn!(%error, "failed to store the pin preference"),
    }
    let _ = reply.send(());
}

SessionCommand::ReorderPinned { peer_ids, reply } => {
    let _ = reply.send(self.reorder_pinned(peer_ids).await);
}
```

- Add the private helper:

```rust
async fn reorder_pinned(&mut self, peer_ids: Vec<DeviceId>) -> Result<(), CoreError> {
    let pinned: Vec<DeviceId> = self
        .peers
        .values()
        .filter(|entry| entry.stored.is_listed() && entry.stored.pinned)
        .map(|entry| entry.profile.device_id)
        .collect();
    let requested: std::collections::HashSet<DeviceId> = peer_ids.iter().copied().collect();

    if requested.len() != peer_ids.len()
        || requested.len() != pinned.len()
        || !pinned.iter().all(|id| requested.contains(id))
    {
        return Err(CoreError::InvalidPinnedOrder);
    }

    self.store.reorder_pinned(peer_ids.clone()).await?;
    for (rank, id) in peer_ids.iter().enumerate() {
        if let Some(entry) = self.peers.get_mut(id) {
            entry.stored.pin_rank = Some(i64::try_from(rank).unwrap_or(i64::MAX));
        }
    }
    self.emit_peers();
    Ok(())
}
```

- [ ] **Step 4: Run the session tests**

Run: `cargo test -p localme-core command_set_pinned command_reorder_pinned a_store_failure_leaves_the_row_unpinned`
Expected: PASS.

- [ ] **Step 5: Add the host commands and the error mapping**

In `src-tauri/src/commands.rs`:

```rust
/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn set_peer_pinned(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    pinned: bool,
) -> Result<(), ApiError> {
    let peer = args::device_id(&peer_id)?;
    Ok(state.session.set_pinned(peer, pinned).await?)
}

/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier, or when the list is not the pinned set.
#[tauri::command]
pub async fn reorder_pinned_peers(
    state: State<'_, Arc<AppState>>,
    peer_ids: Vec<String>,
) -> Result<(), ApiError> {
    let ids = peer_ids
        .iter()
        .map(|raw| {
            raw.parse::<localme_core::domain::ids::DeviceId>()
                .map_err(|error| ApiError::invalid_input("peerIds", error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(state.session.reorder_pinned(ids).await?)
}
```

Register both in the `ipc_handler!` macro list in `src-tauri/src/lib.rs`, beside `set_peer_muted`.

In `src-tauri/src/error.rs`, map the new variant:

```rust
CoreError::InvalidPinnedOrder => Self::InvalidInput {
    field: "peerIds".to_owned(),
    message: "the pinned order is not a permutation of the pinned conversations".to_owned(),
},
```

Add a test:

```rust
#[test]
fn an_invalid_pinned_order_is_an_input_error() {
    let error: ApiError = CoreError::InvalidPinnedOrder.into();
    assert!(matches!(
        error,
        ApiError::InvalidInput { ref field, .. } if field == "peerIds"
    ));
}
```

- [ ] **Step 6: Add the IPC wrappers**

In `src/ipc/index.ts`, beside `setPeerMuted`:

```ts
/** Pins a conversation above the rest of the list, or releases it back into the sorted block. */
export function setPeerPinned(peerId: DeviceId, pinned: boolean): Promise<void> {
  return call('set_peer_pinned', { peerId, pinned });
}

/** Persists the manual order of the pinned block. The list must contain every pinned id once. */
export function reorderPinnedPeers(peerIds: readonly DeviceId[]): Promise<void> {
  return call('reorder_pinned_peers', { peerIds: [...peerIds] });
}
```

- [ ] **Step 7: Run the host checks**

Run: `cargo test --workspace` then `npm run typecheck`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/core/src/error.rs src-tauri/core/src/services/session.rs \
  src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/error.rs src/ipc/index.ts
git commit -m "feat: expose pin and reorder over IPC"
```

---

### Task 4: Pin and unpin from the row menu

**Files:**

- Modify: `src/ipc/types.ts`
- Modify: `src/test/factories.ts`
- Modify: `src/ui/icons.ts`
- Modify: `src/i18n/messages/en.ts`, `ru.ts`, `es.ts`, `de.ts`, `fr.ts`, `pt.ts`, `zh.ts`
- Modify: `src/features/users/UserListItem.vue`
- Modify: `src/features/users/UserListView.vue`
- Test: `src/features/users/UserListItem.spec.ts`, `src/features/users/UserListView.spec.ts`

**Interfaces:**

- Consumes: `ipc.setPeerPinned`, the `pinned`/`pinRank` wire fields.
- Produces:
  - `Peer { …, pinned: boolean, pinRank: number | null }`
  - `ICONS.pin`
  - `UserListItem` emits `pin: []`
  - `UserListView` calls `ipc.setPeerPinned(peer.deviceId, !peer.pinned)`.

- [ ] **Step 1: Write the failing component tests**

In `UserListItem.spec.ts`, update the existing menu expectation and add cases:

```ts
it('offers pin first, then mute and forget', async () => {
  const wrapper = await mountItem(makePeer());
  await wrapper.get('.md-menu__trigger').trigger('click');
  await flushPromises();

  expect(wrapper.findAll('.md-menu__item').map((item) => item.text())).toEqual([
    'Pin',
    'Mute notifications',
    'Forget…',
  ]);

  await wrapper.findAll('.md-menu__item')[0]!.trigger('click');
  expect(wrapper.emitted('pin')).toEqual([[]]);
});

it('flips the pin label and shows a glyph once the row is pinned', async () => {
  const wrapper = await mountItem(makePeer({ pinned: true, pinRank: 0 }));
  expect(wrapper.find('.user-item__pin').exists()).toBe(true);

  await wrapper.get('.md-menu__trigger').trigger('click');
  await flushPromises();
  expect(wrapper.findAll('.md-menu__item')[0]!.text()).toBe('Unpin');
});
```

In `UserListView.spec.ts`, add `setPeerPinned: vi.fn(), reorderPinnedPeers: vi.fn()` to the `@/ipc` mock, then:

```ts
it('toggles the pin through the peer id when the row asks for it', async () => {
  const { wrapper } = await mountList([makePeer({ deviceId: 'device-a', pinned: false })]);

  wrapper.findComponent(UserListItem).vm.$emit('pin');
  await flushPromises();

  expect(ipc.setPeerPinned).toHaveBeenCalledWith('device-a', true);
});

it('pins a row while a search is active', async () => {
  const { wrapper } = await mountList([makePeer({ deviceId: 'device-b', nickname: 'Bob' })], {
    query: 'bo',
  });

  wrapper.findComponent(UserListItem).vm.$emit('pin');
  await flushPromises();

  expect(ipc.setPeerPinned).toHaveBeenCalledWith('device-b', true);
});

it('reports a failed pin instead of leaving it silent', async () => {
  vi.mocked(ipc.setPeerPinned).mockRejectedValueOnce(new Error('offline'));
  const { wrapper, pinia } = await mountList([makePeer({ deviceId: 'device-a' })]);

  wrapper.findComponent(UserListItem).vm.$emit('pin');
  await flushPromises();

  expect(useUiStore(pinia).notice?.key).toBe('error.internal');
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npm run test -- UserListItem UserListView`
Expected: FAIL — `pinned` is not a field of `Peer`, and no pin item exists.

- [ ] **Step 3: Add the data type, factory and icon**

`src/ipc/types.ts`, in `Peer`:

```ts
/** Whether the conversation stays in the block above the rest of the list. */
readonly pinned: boolean;
/** Position inside the pinned block, ascending; `null` when the row is not pinned. */
readonly pinRank: number | null;
```

`src/test/factories.ts`, in `makePeer` defaults:

```ts
pinned: false,
pinRank: null,
```

`src/ui/icons.ts`, in `ICONS`:

```ts
pin: [
  { kind: 'path', d: 'M9 4h6' },
  { kind: 'path', d: 'M10 4v5.2L7.6 13v1.6h8.8V13L14 9.2V4' },
  { kind: 'line', x1: 12, y1: 14.6, x2: 12, y2: 20.5 },
],
```

- [ ] **Step 4: Add the strings**

Add to `en.ts`:

```ts
'users.pin': 'Pin',
'users.unpin': 'Unpin',
'users.pinned': 'Pinned',
```

Add the same keys to every other catalogue with these values:

| Locale | pin | unpin | pinned |
| --- | --- | --- | --- |
| ru | `Закрепить` | `Открепить` | `Закреплён` |
| es | `Fijar` | `Quitar fijación` | `Fijado` |
| de | `Anheften` | `Lösen` | `Angeheftet` |
| fr | `Épingler` | `Détacher` | `Épinglé` |
| pt | `Fixar` | `Desafixar` | `Fixado` |
| zh | `置顶` | `取消置顶` | `已置顶` |

- [ ] **Step 5: Render the action and the glyph**

In `UserListItem.vue`:

- Add `pin: []` to the emits type.
- Add `MdIcon` import from `@/ui/MdIcon.vue`.
- Put the pin item first in `menuItems`:

```ts
{
  id: 'pin',
  label: i18n.t(props.peer.pinned ? 'users.unpin' : 'users.pin'),
  icon: 'pin',
},
```

- Handle it in `onMenuSelect`: `if (id === 'pin') emit('pin');`
- In the trailing slot, before `MdBadge`:

```html
<span v-if="peer.pinned" class="user-item__pin">
  <MdIcon name="pin" :size="16" />
  <span class="md-visually-hidden">{{ i18n.t('users.pinned') }}</span>
</span>
```

- Add scoped CSS:

```css
.user-item__pin {
  display: flex;
  align-items: center;
  color: var(--md-sys-color-outline);
}
```

In `UserListView.vue`, add:

```ts
async function onPin(peer: Peer): Promise<void> {
  await ui.attempt(
    () => ipc.setPeerPinned(peer.deviceId, !peer.pinned),
    'the conversation could not be pinned',
  );
}
```

and bind `@pin="onPin(row.peer)"` on `UserListItem`.

- [ ] **Step 6: Run the tests**

Run: `npm run test -- UserListItem UserListView` and `npm run typecheck`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/ipc/types.ts src/test/factories.ts src/ui/icons.ts \
  src/i18n/messages src/features/users
git commit -m "feat: pin a conversation from its row menu"
```

---

### Task 5: Reorder the pinned block

**Files:**

- Create: `src/features/users/pin-order.ts`
- Test: `src/features/users/pin-order.spec.ts`
- Modify: `src/features/users/UserListView.vue`
- Test: `src/features/users/UserListView.spec.ts`
- Modify: `src/i18n/messages/*.ts` if a drag-specific string is added (none is planned)

**Interfaces:**

- Consumes: `Peer.pinned`, `Peer.pinRank`, `ipc.reorderPinnedPeers`, `pinnedIds` from the store list.
- Produces:
  - `pinnedIds(peers: readonly Peer[]): DeviceId[]`
  - `movePinned(ids: readonly DeviceId[], id: DeviceId, to: number): DeviceId[]`
  - `pinnedDropIndex(clientY: number, listTop: number, scrollTop: number, count: number, rowHeight: number): number`

- [ ] **Step 1: Write the failing pure-function tests**

Create `src/features/users/pin-order.spec.ts`:

```ts
import { describe, expect, it } from 'vitest';

import { makePeer } from '@/test/factories';

import { movePinned, pinnedDropIndex, pinnedIds } from './pin-order';

describe('pin-order', () => {
  it('collects only pinned ids in the order it is given them', () => {
    expect(
      pinnedIds([
        makePeer({ deviceId: 'a', pinned: true }),
        makePeer({ deviceId: 'b', pinned: false }),
        makePeer({ deviceId: 'c', pinned: true }),
      ]),
    ).toEqual(['a', 'c']);
  });

  it('moves an id to a position and leaves the other ids in order', () => {
    expect(movePinned(['a', 'b', 'c'], 'a', 2)).toEqual(['b', 'c', 'a']);
    expect(movePinned(['a', 'b', 'c'], 'c', 0)).toEqual(['c', 'a', 'b']);
  });

  it('clamps a target outside the list and ignores an unknown id', () => {
    expect(movePinned(['a', 'b'], 'a', 9)).toEqual(['b', 'a']);
    expect(movePinned(['a', 'b'], 'missing', 0)).toEqual(['a', 'b']);
  });

  it('turns a pointer position into a clamped row index', () => {
    expect(pinnedDropIndex(80, 0, 0, 3, 72)).toBe(1);
    expect(pinnedDropIndex(-40, 0, 0, 3, 72)).toBe(0);
    expect(pinnedDropIndex(1_000, 0, 0, 3, 72)).toBe(2);
    expect(pinnedDropIndex(0, 100, 172, 3, 72)).toBe(1);
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `npm run test -- pin-order`
Expected: FAIL — the module does not exist.

- [ ] **Step 3: Implement the helpers**

Create `src/features/users/pin-order.ts` with the three exported functions. `movePinned` copies the array, removes the id and inserts it at the clamped target; an unknown id returns a copy unchanged. `pinnedDropIndex` computes `floor((clientY - listTop + scrollTop) / rowHeight)` and clamps to `0..count - 1`, returning `0` when `count` is zero.

- [ ] **Step 4: Run the pure tests**

Run: `npm run test -- pin-order`
Expected: PASS.

- [ ] **Step 5: Write the failing gesture tests**

In `UserListView.spec.ts`, add:

```ts
function pinnedRow(deviceId: string, nickname: string, pinRank: number): Peer {
  return makePeer({ deviceId, nickname, pinned: true, pinRank, unread: 0, online: false });
}

it('moves a pinned row with Alt+ArrowDown and persists the new order', async () => {
  const { wrapper } = await mountList([
    pinnedRow('device-a', 'Ann', 0),
    pinnedRow('device-b', 'Ben', 1),
    pinnedRow('device-c', 'Cid', 2),
  ]);

  await wrapper.findAll('.users__row')[0]!.get('.md-list-item__row').trigger('keydown', {
    key: 'ArrowDown',
    altKey: true,
  });
  await flushPromises();

  expect(ipc.reorderPinnedPeers).toHaveBeenCalledWith(['device-b', 'device-a', 'device-c']);
});

it('does not move the first pinned row upwards or an unpinned row at all', async () => {
  const { wrapper } = await mountList([
    pinnedRow('device-a', 'Ann', 0),
    makePeer({ deviceId: 'device-b', pinned: false }),
  ]);

  await wrapper.findAll('.users__row')[0]!.get('.md-list-item__row').trigger('keydown', {
    key: 'ArrowUp',
    altKey: true,
  });
  await wrapper.findAll('.users__row')[1]!.get('.md-list-item__row').trigger('keydown', {
    key: 'ArrowDown',
    altKey: true,
  });
  await flushPromises();

  expect(ipc.reorderPinnedPeers).not.toHaveBeenCalled();
});

it('reorders by dragging a pinned row across another', async () => {
  const { wrapper } = await mountList([
    pinnedRow('device-a', 'Ann', 0),
    pinnedRow('device-b', 'Ben', 1),
  ]);
  const first = wrapper.findAll('.users__row')[0]!.element;

  first.dispatchEvent(
    new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1, clientY: 10 }),
  );
  window.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, clientY: 90 }));
  window.dispatchEvent(new PointerEvent('pointerup', { pointerId: 1, clientY: 90 }));
  await flushPromises();

  expect(ipc.reorderPinnedPeers).toHaveBeenCalledWith(['device-b', 'device-a']);
});

it('leaves a press without movement as a normal click', async () => {
  const { wrapper, router } = await mountList([pinnedRow('device-a', 'Ann', 0)]);

  await wrapper.get('.md-list-item__row').trigger('click');
  await flushPromises();

  expect(ipc.reorderPinnedPeers).not.toHaveBeenCalled();
  expect(router.currentRoute.value.name).toBe('chat');
});

it('does not reorder while a search filter is active', async () => {
  const { wrapper } = await mountList(
    [pinnedRow('device-a', 'Ann', 0), pinnedRow('device-b', 'Ben', 1)],
    { query: 'a' },
  );

  await wrapper.findAll('.users__row')[0]!.get('.md-list-item__row').trigger('keydown', {
    key: 'ArrowDown',
    altKey: true,
  });
  await flushPromises();

  expect(ipc.reorderPinnedPeers).not.toHaveBeenCalled();
});
```

- [ ] **Step 6: Run them to verify they fail**

Run: `npm run test -- UserListView`
Expected: FAIL — no gesture handlers exist.

- [ ] **Step 7: Implement the reorder interaction**

In `UserListView.vue`:

1. Import `pinnedIds`, `movePinned`, `pinnedDropIndex` and `DeviceId`.
2. Replace the virtualiser’s `peers.visible` source with an `ordered` computed:

```ts
const pinnedOrder = ref<readonly DeviceId[] | null>(null);

const ordered = computed<readonly Peer[]>(() => {
  if (pinnedOrder.value === null) return peers.visible;
  const byId = new Map(peers.visible.map((peer) => [peer.deviceId, peer]));
  const pinned = pinnedOrder.value.flatMap((id) => {
    const peer = byId.get(id);
    return peer === undefined ? [] : [peer];
  });
  return [...pinned, ...peers.visible.filter((peer) => !peer.pinned)];
});

const canReorder = computed<boolean>(() => peers.query.trim() === '');
```

Use `ordered.length` for the virtualiser `count` and `ordered[item.index]` for the row. While `pinnedOrder` is `null` this is exactly `peers.visible`, so existing behaviour is unchanged.

3. Add drag state:

```ts
interface DragState {
  readonly deviceId: DeviceId;
  readonly pointerId: number;
  readonly startY: number;
  readonly listTop: number;
  active: boolean;
  lastY: number;
}

const DRAG_THRESHOLD = 6;
const drag = ref<DragState | null>(null);
let suppressClick = false;
```

4. `onRowPointerDown(peer, event)`: ignore unless the primary button, the peer is pinned, `canReorder` is true, and the target is not inside `.md-list-item__trailing`. Record `startY`, `listTop` (the scroll element’s `getBoundingClientRect().top`), `lastY`, `active: false`; reset `suppressClick`; add `pointermove`, `pointerup` and `pointercancel` listeners on `window`.

5. `onPointerMove(event)`: for the matching `pointerId`, return while `|clientY - startY| < DRAG_THRESHOLD`. On the first crossing, set `active = true` and `pinnedOrder.value = pinnedIds(ordered.value)`. Then set `lastY = clientY`, compute `to = pinnedDropIndex(clientY, listTop, scrollEl.scrollTop, pinnedOrder.value.length, ROW_HEIGHT)` and, when `to` differs from the current index, set `pinnedOrder.value = movePinned(pinnedOrder.value, deviceId, to)`. Call `event.preventDefault()` once active.

6. `onPointerUp()`: remove the window listeners; if the drag was active and `pinnedOrder.value !== null`, capture the order, set `suppressClick = true`, clear `pinnedOrder.value`, and persist with `ui.attempt(() => ipc.reorderPinnedPeers(order), 'the chat order could not be saved')`; otherwise leave the preview `null`. Clear `drag.value`.

   Register the same cleanup with `onBeforeUnmount` (remove the window listeners, clear `drag` and `pinnedOrder`, reset `suppressClick`), so unmounting mid-drag cannot leave a listener pointing at a dead component.

7. `onClickCapture(event)`: if `suppressClick` is set, `preventDefault()` and `stopPropagation()` and clear the flag, so the click that ends a drag does not open the chat.

8. `onMove(peer, delta)`: return unless the peer is pinned and `canReorder`. Take `pinnedIds(ordered.value)`; if the target index is outside the list, return; otherwise persist `movePinned(ids, peer.deviceId, index + delta)` through the same `ui.attempt(...)` call.

9. In the template:

```html
<motion.li
  v-for="(row, position) in rows"
  :key="row.key"
  class="users__row"
  :class="{ 'users__row--dragging': drag?.active === true && drag.deviceId === row.peer.deviceId }"
  :style="{
    top: `${row.start}px`,
    transform: dragOffset(row.peer),
  }"
  @pointerdown="onRowPointerDown(row.peer, $event)"
  @click.capture="onClickCapture"
  @keydown.alt.up.prevent="onMove(row.peer, -1)"
  @keydown.alt.down.prevent="onMove(row.peer, 1)"
  …
>
```

`dragOffset(peer)` returns `translateY(${lastY - (listTop + index * ROW_HEIGHT + ROW_HEIGHT / 2)}px)` for the active dragged row (index taken from `pinnedOrder`), and `undefined` for every other row.

10. Add scoped CSS:

```css
.users__row {
  transition: top 120ms cubic-bezier(0.2, 0, 0, 1);
}

.users__row--dragging {
  z-index: 1;
  transition: none;
  cursor: grabbing;
}
```

Keep the existing `top`, opacity and stacking-context comments accurate; the dragged row may use a translate because it is already its own stacking context through `z-index`.

- [ ] **Step 8: Run the tests**

Run: `npm run test -- UserListView pin-order`
Expected: PASS. If `PointerEvent` is not constructible in happy-dom, construct the three events through `new window.PointerEvent(...)`; do not weaken the assertions.

- [ ] **Step 9: Commit**

```bash
git add src/features/users
git commit -m "feat: reorder pinned conversations by dragging"
```

---

### Task 6: Version, changelog and the full gate

**Files:**

- Modify: `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/core/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`, `AGENTS.md`, `CHANGELOG.md`

**Interfaces:**

- Consumes: the finished feature.
- Produces: version `0.13.0` everywhere, a dated changelog section, and a green full gate.

- [ ] **Step 1: Bump the version**

Run: `npm run version:minor`
Expected: `0.12.0 → 0.13.0`, with a new `## [0.13.0]` stub in `CHANGELOG.md`.

- [ ] **Step 2: Write the changelog entry**

Replace the stub body with:

```markdown
### Added

- **Pinned conversations.** A conversation can be pinned from its row menu, and every pinned
  conversation stays above the rest of the list — above unread and online ones — with a pin
  glyph beside its name. Pinned conversations can be moved among themselves, by dragging a row
  or with `Alt+↑` / `Alt+↓` while it has focus, and a newly pinned one arrives at the top of
  the block. The order is kept across restarts; unpinning returns a conversation to the usual
  unread-first ordering.
```

- [ ] **Step 3: Run the full gate**

Run each, in this order:

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
npm run build
npm run check:versions
cd src-tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: every command exits 0. Fix anything that fails rather than weakening a check.

- [ ] **Step 4: Commit**

```bash
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/core/Cargo.toml \
  src-tauri/Cargo.lock src-tauri/tauri.conf.json AGENTS.md CHANGELOG.md
git commit -m "chore: release 0.13.0"
```

---

## Self-Review

- **Spec coverage:** storage columns and migration → Task 1; pinned-first ordering and manual rank → Task 2; pin/unpin and reorder operations, validation and IPC → Task 3; row-menu action, glyph and all seven catalogues → Task 4; drag, keyboard, threshold and click preservation → Task 5; version, changelog and gates → Task 6. Non-goals (sync, chat-view pinning, section header, folders) have no tasks.
- **Step scan:** every test step names its assertions and exact expected values; every implementation step names the file and signature; no "handle edge cases" or TBD lines.
- **Type consistency:** `pinned`/`pin_rank` in Rust, `pinned`/`pinRank` on the wire; `set_peer_pinned`/`reorder_pinned` in core, `set_peer_pinned`/`reorder_pinned_peers` at the IPC boundary, `setPeerPinned`/`reorderPinnedPeers` in TypeScript; helper names match across tasks.
- **Review Focus tests:** equal ranks → Task 2; non-permutation reorder → Task 3; pin while searching → Task 4, drag while searching → Task 5; store failure → Task 3; click after a press and a drag that crosses a row → Task 5.
