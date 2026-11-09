# Pinned chats

## Intent

A conversation the user cares about should stay at the top of the people list, and the user
should be able to choose the order of the pinned ones by moving them.

## Goals

- Pin and unpin a conversation from its row menu.
- Pinned conversations always appear above every unpinned one, including unread ones.
- Among pinned conversations the order is manual only: a new message, unread state or presence
  never moves a pinned row.
- Pinned conversations can be dragged to reorder them, and moved with the keyboard while the row
  has focus.
- A newly pinned conversation goes to the top of the pinned block.
- Pin state and order survive restarts.

## Non-goals

- Syncing pin state to the other device. Pins are local to this installation, like mute.
- Pinning from inside the conversation view.
- A separate "Pinned" section header or collapsible block.
- Folders, groups or tags.

## Constraints

- The list order is produced by the core (`PeerView::sort_for_display`), and the interface
  receives it already sorted in the `peers` event. Pin state therefore has to live in the core
  and the sort, otherwise the next event would undo any front-end ordering.
- The people list is virtualised with a fixed 72 px row and absolute positioning, so reordering
  uses pointer events rather than native HTML5 drag and drop.
- Schema migrations are forward-only; a new migration is version 5.
- Every user-facing string exists in all seven catalogues; typecheck must fail on a missing key.
- The change is a new user-visible capability, so the version bumps to 0.13.0.

## Approach

One approach fits: persist the pin in the existing `peers` table and teach the existing sort
about it. Keeping the pin only in Pinia would be overwritten by the next `peers` event; keeping
it in the settings blob would separate it from the peer row it describes and still leave the
core sorting by the old rule.

### Data

Migration 5 adds to `peers`:

```sql
ALTER TABLE peers ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE peers ADD COLUMN pin_rank INTEGER;
CREATE INDEX peers_pinned ON peers(pinned, pin_rank) WHERE pinned = 1;
```

`pin_rank` is `NULL` for an unpinned row. Among pinned rows it is the position, ascending.
Ties (only possible from a hand-edited database) break on `device_id`.

`StoredPeer` and `PeerView` gain `pinned: bool` and `pin_rank: Option<i64>`.

`PeerView::sort_for_display` gains one leading term: pinned before unpinned; within the pinned
block `pin_rank` ascending, then `device_id`; the existing unread/online/activity/nickname rule
is unchanged for the unpinned block.

### Operations

- `set_peer_pinned(device_id, pinned)`: pinning sets `pinned = 1` and
  `pin_rank = MIN(pin_rank) - 1` over the pinned rows (so the new row is on top); unpinning sets
  `pinned = 0, pin_rank = NULL`. The session updates the in-memory entry and emits `peers`.
- `reorder_pinned(device_ids)`: accepts the full pinned order, rejects a list that does not
  contain exactly every currently pinned device (no duplicates, no strangers), and rewrites
  `pin_rank = 0..n-1` in one transaction. The session emits `peers`.
- `forget_peer` clears the pin with the row.

### Interface

- The row menu gains "Pin"/"Unpin" with a pin icon, above mute.
- A pinned row shows a small pin glyph beside the nickname, so its state is visible without
  opening the menu.
- Reordering: pressing a row and moving more than a 6 px threshold starts a drag; the row follows
  the pointer, the other pinned rows shift with a short transition, and releasing persists the
  new order. A press without movement still activates the row, so opening a chat is unchanged.
- Keyboard: `Alt+↑` / `Alt+↓` on a focused pinned row moves it one place and persists.
- Dragging is limited to the pinned block; an unpinned row does not move.

## Left to the builder

- Exact drag physics and the drop indicator.
- Whether the pin glyph sits in the leading avatar slot or beside the nickname.
- The precise wording and translations of the two new strings.
- Tests, which follow the existing Rust unit-test style and Vitest component tests.
