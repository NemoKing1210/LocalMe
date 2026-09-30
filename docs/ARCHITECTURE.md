# LocalMe — Architecture

> Local-network desktop messenger. Rust core (Tauri 2) + Vue 3 front end.
> Zero configuration: instances find each other over mDNS/DNS-SD.

---

## 1. Goals and non-goals

**In scope.** Peer discovery on a LAN without configuration, 1:1 text chat, local history,
presence, tray integration, native notifications, settings, three desktop platforms
(Windows, macOS, Linux).

**Out of scope.** File transfer, group chats, mobile, offline message delivery.
The protocol and storage layers are designed so these can be added additively
(see §7.5 and §8.4), but no code for them exists.

**Priorities, in order.** Correctness and reliability → maintainability → resource usage →
visual polish. When two of these conflict, the earlier one wins and the trade-off is
recorded here.

---

## 2. Workspace layout

```
LocalMe/
├─ docs/ARCHITECTURE.md          # this file
├─ AGENTS.md  CLAUDE.md          # agent/contributor map and Claude Code brief
├─ CONTRIBUTING.md  CHANGELOG.md  LICENSE
├─ scripts/                      # version bump, changelog extract, release tag
├─ src/                          # Vue 3 front end (see §10)
├─ src-tauri/                    # Cargo workspace root
│  ├─ Cargo.toml                 # package `localme` (Tauri host) + workspace definition
│  ├─ tauri.conf.json            # Tauri 2 configuration
│  ├─ capabilities/default.json  # Tauri 2 capability grants (least privilege)
│  ├─ icons/                     # app + tray icons
│  ├─ src/                       # Tauri host: commands, events, tray, window policy
│  └─ core/                      # crate `localme-core` — no Tauri, no UI
└─ .github/
   ├─ workflows/ci.yml           # quality gate (front end, core matrix, versions)
   ├─ workflows/release.yml      # tagged installer builds → draft GitHub Release
   └─ actions/setup              # shared Node/Rust/Linux setup
```

`localme-core` deliberately does **not** depend on `tauri`. It holds the domain, protocol,
discovery, transport, storage and service layers, and it is the crate that unit and
integration tests exercise. The Tauri crate is a thin adapter: it owns the window, the tray,
the single-instance guard and the IPC surface, and contains no protocol logic.

---

## 3. Layer map (ports & adapters)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ localme (Tauri host)                                                     │
│  commands.rs   thin: parse → call service → map error → DTO              │
│  events.rs     CoreEvent → window.emit (suppressed while hidden in tray)  │
│  tray.rs  window.rs  single_instance.rs  notifications.rs  autostart.rs   │
└───────────────▲──────────────────────────────────────┬───────────────────┘
                │ CoreHandle (typed command senders)   │ CoreEvent (broadcast)
┌───────────────┴──────────────────────────────────────▼───────────────────┐
│ localme-core — services/                                                 │
│  Session    actor owning peers, presence FSM, message pipeline           │
│  Settings   typed settings document + persistence                        │
├──────────────────────────────────────────────────────────────────────────┤
│ domain/          pure: DeviceId Nickname MessageId Peer Presence PeerList │
│                  no I/O, no async, no tauri. Fully unit-testable.         │
│ protocol/        pure: framing codec, Envelope, validation, rate limiter   │
├──────────────────────────────────────────────────────────────────────────┤
│ Ports (traits)   Discovery · Store · Clock                                │
│ Adapters         discovery::{MdnsDiscovery, UdpBeacon, CompositeDiscovery}│
│                  storage::SqliteStore · transport::TcpTransport           │
└──────────────────────────────────────────────────────────────────────────┘
                │                    │                     │
          mDNS 5353 (UDP)      UDP beacon           TCP listen + dial
                               47821 multicast      47820 (configurable)
```

**The dependency rule.** `domain` imports nothing from the layers above it. `protocol`
imports `domain`. `services` import both and the port traits, never an adapter. Adapters
implement the traits and are wired only in `localme-core::runtime` and the Tauri host.
Swapping `Store` for an in-memory implementation in a test requires no change anywhere else.

### 3.1 Why traits with generics instead of `dyn`

Port traits are used as generic bounds (`Session<S: Store>`), not as `Box<dyn Trait>`.
Rust's async-fn-in-trait is not dyn-compatible, and the usual escape hatch
(`async_trait` + boxing) costs an allocation per call and a dependency. The core wires
concrete types at startup, so monomorphisation is free and there is exactly one instantiation
per port. Test doubles are plain structs implementing the same traits.

### 3.2 Concurrency model

There is **no global `Mutex` around application state**. State lives in one of three places:

| State | Owner | Access |
|---|---|---|
| Peer map, presence FSM, per-peer write queues | `Session` actor task | exclusive; commands arrive on a bounded `mpsc` |
| SQLite connection | `storage` writer thread | exclusive; commands arrive on a bounded `mpsc` |
| Settings document | `Settings` actor task | exclusive; reads are answered over `oneshot` |
| Read-only projections (window visibility, unread count) | `AtomicBool` / `AtomicU32` | lock-free reads from sync contexts |

Everything cross-task is a bounded channel. A slow consumer therefore applies back pressure
instead of growing an unbounded queue — the guarantee that keeps idle memory flat.

Actor commands that need an answer carry a `tokio::sync::oneshot` sender; commands that do
not are fire-and-forget. Actors resolve the answer *before* returning to the mailbox, so a
command's effect is observable when its await returns.

---

## 4. Domain model

```rust
pub struct DeviceId(Uuid);          // v4, generated once on first launch, persisted
pub struct Nickname(String);        // invariant: trimmed, 1..=32 chars, no control chars
pub struct MessageId(Uuid);         // v7 — time-ordered, so ORDER BY id is a time order
pub struct AvatarSeed(String);      // opaque, ≤64 chars; announced by its owner
pub struct UnixMillis(i64);
```

* **Identity is the device id.** Not the nickname (not unique), not the MAC address
  (randomisation, multiple adapters), not the IP (DHCP, VPN, multiple interfaces).
* **`Nickname` is a validated newtype.** `Nickname::parse(&str)` is the only constructor,
  so an invalid nickname cannot reach the protocol or the database.
* **`AvatarSeed` is owned by the announcing device.** The peer computes
  `"{device_id}:{nickname}"` and sends it in the handshake and in the nickname-change
  broadcast. Every other device renders from the received seed, which is what makes an
  avatar identical across machines, immune to locale, and stable when the nickname changes
  only on the peer that changed it.

### 4.1 Presence finite-state machine

Presence is a pure function of observed events, driven by `PresenceMachine` in
`domain/presence.rs`. It has no clock of its own: every method takes `now`, which makes
the whole machine deterministic under test.

```
                  discover(addrs)
        ┌────────────────────────────────┐
        │                                ▼
   ┌─────────┐  dial ok   ┌────────────┐  hello ok  ┌─────────┐
   │ Unknown │───────────▶│ Connecting │───────────▶│ Online  │
   └─────────┘            └────────────┘            └─────────┘
        ▲                       │ fail/                  │
        │                       │ timeout                │ heartbeat gap > T
        │   forget()            ▼                        ▼
        │                  ┌─────────┐  reconnect ok ┌─────────┐
        └──────────────────│ Offline │◀──────────────│ Stalled │
                           └─────────┘   disconnect  └─────────┘
```

Transitions and their inputs:

| Input | Effect |
|---|---|
| `discovered(addrs)` | records addresses; `Unknown`/`Offline` → `Connecting` (dial) |
| `connected()` | `Connecting` → `Online`, `last_heartbeat = now` |
| `heartbeat()` | any state → `Online`, refresh `last_heartbeat`; also writes `last_seen` in storage (receiver's own clock) |
| `disconnected()` / `goodbye()` | → `Offline`, record `last_seen = now` |
| `tick(now)` | `Online` with `now - last_heartbeat > HEARTBEAT_TIMEOUT` → `Stalled` (socket closed, `Stalled` is reported to the UI as offline) |
| `forget()` | → `Unknown`, drop addresses; a later `discovered` re-adds the peer as new |

`last_seen` is always written from the **receiver's** local clock. Remote clocks are never
trusted or compared; `sent_at` in a message is displayed as information, never used for
ordering or expiry.

Presence is *reported* as online only in `Online`. `Connecting` and `Stalled` report offline
with the peer still listed, which is what the offline UI (`не в сети`, `был(а) в сети N минут назад`)
renders. Because dialling starts the moment a peer is discovered, the `Connecting` window is
a LAN round trip — short enough that the UI does not visibly flicker.

### 4.2 Online/offline is decided by three signals

1. **Discovery** — a live mDNS/BEACON record says the peer exists somewhere and gives addresses.
2. **Connection** — an established, handshaken TCP session.
3. **Heartbeat** — a frame every `HEARTBEAT_INTERVAL` (5 s); the peer is `Stalled` after
   `HEARTBEAT_TIMEOUT` (15 s, i.e. three missed beats).

Any one signal alone is insufficient: discovery survives a half-open socket, and a socket
says nothing about a peer that moved to another network. Requiring all three is what makes
"unplug the cable" and "sleep the laptop" both converge to offline within 15 s.

---

## 5. Protocol

### 5.1 Transport and framing

TCP. Every frame is:

```
┌────────────────┬──────────────────────────────────┐
│ u32 BE length  │ payload (length bytes, UTF-8)    │
└────────────────┴──────────────────────────────────┘
```

* `MAX_FRAME = 64 KiB`. A length prefix outside `1..=MAX_FRAME` is a protocol violation:
  the connection is closed and logged. This bound is what prevents a hostile peer from
  making us allocate gigabytes with a 4-byte header, and it is checked *before* any
  allocation proportional to the claimed length.
* Payload is JSON (`serde_json`). Chosen over a hand-rolled binary format because the frame
  budget is already bounded by the 8 000-character message limit, JSON is self-describing
  while debugging a LAN protocol, and every field is behind a versioned envelope, so a
  future binary codec can be introduced as a new `protocol_version` at the framing layer
  without touching the domain. Cost: roughly 40 % more bytes on the wire for a typical
  short message — irrelevant at LAN latencies and text-sized payloads.
* Framing is implemented as an explicit state machine over a read buffer
  (`protocol/framing.rs`), not with a combinator library, so the size check, the incremental
  decode and the "invalid frame" error path are all directly unit-testable.

### 5.2 Envelope

```jsonc
{ "v": 1, "t": "<type>", ...fields }
```

`v` is the protocol version (currently `1`), `t` the variant tag. Two peers with different
versions complete a handshake and then refuse to exchange anything but `Goodbye`/`Error`,
so a mismatch surfaces as a clear message instead of undefined behaviour.

| `t` | Direction | Fields | Purpose |
|---|---|---|---|
| `hello` | dialer → acceptor | `device_id, nickname, avatar_seed, listen_port, protocol_version` | open a session |
| `welcome` | acceptor → dialer | same shape | accept, symmetric identity exchange |
| `heartbeat` | both | `seq` | liveness, one frame per 5 s; the `seq` is only useful in a log |
| `chat` | both | `id, body` | a chat message; sender/recipient are implied by the connection |
| `chat_ack` | both | `id` | "delivered": persisted by the recipient |
| `profile` | both | `nickname, avatar_seed` | nickname change, re-broadcast to all live sessions |
| `goodbye` | both | `reason` | graceful shutdown; recipient goes offline immediately |
| `error` | both | `code, message` | recoverable protocol error, then close |

`chat` carries no `sender`/`recipient` field: the frame arrives on an authenticated
(because handshaken) 1:1 connection, so a field naming the sender would be both redundant
and forgeable. This is the smallest wire format that cannot lie about who sent what.

### 5.3 Handshake

```
dialer                                   acceptor
  │ ── hello {device_id, nick, seed, listen_port, v} ──▶
  │                                          validate: size limits, version, nickname
  │                                          device_id != own id (else close: self-connect)
  │ ◀────────── welcome {device_id, nick, seed, listen_port, v} ──
  │  verify: id matches what discovery announced
  │ ── heartbeat ──▶ ◀── heartbeat ──   (both directions, 5 s)
```

Both sides learn the peer's identity from the handshake, not from the discovery record:
the discovery record is unauthenticated TXT data and is used only to decide *where* to dial.

**Simultaneous connect.** Both peers dial each other whenever both have just discovered the
other. The tie-break is deterministic and needs no extra frames:

> A connection is *preferred* when the connection was initiated by the peer with the
> lexicographically smaller `device_id`, i.e. `dialer_id < acceptor_id`.
> On receiving a `hello` while already connected to that device id, the peer keeps the
> preferred connection and closes the other, sending `goodbye{reason:"superseded"}`.

Both sides evaluate the same predicate on the same pair of ids, so exactly one connection
survives and the loser is closed by both ends. Ordering is total and stable, so the rule
never oscillates.

**Self-connection and duplicate instances.** A `hello` whose `device_id` equals our own is
rejected. Two instances on one machine are prevented outright by the single-instance plugin
(§9.4); two instances on *different* machines that happen to have the same id (a copied
profile) are rejected by the same check, and the collision is surfaced in the log.

### 5.4 Message delivery and deduplication

1. Sender assigns a `MessageId` (UUIDv7 → k-sortable, so storage order == time order).
2. Sender persists the row `status = 'sending'`, queues the frame, UI shows *отправляется*.
3. Recipient validates, deduplicates, persists, sends `chat_ack`, emits a UI event.
4. Sender flips the row to `status = 'delivered'`, UI shows *доставлено*.

**One clock.** A `chat` frame carries no timestamp. The time a message is stored under, and
displayed with, is the *local* clock: for an outgoing message the moment it was queued, for an
incoming one the moment it arrived. On a LAN those differ by a round trip, and using one clock
for both directions removes an entire class of bug — a peer whose clock is an hour fast cannot
reorder its own messages into the middle of the conversation, and "12:03" always means 12:03
on this machine. `received_at_ms` is written alongside as the tie-break for messages that share
a millisecond, and `sent_at_ms` is ordered with the `id` so the sort is total.

Deduplication happens in storage: `messages.id` is the primary key and the insert is
`INSERT OR IGNORE`, so a retransmitted frame is a no-op and the unread counter is incremented
only when the row was genuinely new. A duplicate still receives `chat_ack` — the sender must
never be left waiting because *it* retried.

Delivery is best-effort with no retransmission beyond the live connection: out-of-scope
offline delivery means a frame that could not be written to a live socket is marked
`status = 'failed'` and the UI says so. There is no queue that pretends otherwise.

### 5.5 Hostile-input defences

Every inbound frame passes through these checks before it reaches the domain:

| Check | Bound | Failure |
|---|---|---|
| frame length | `1..=64 KiB` | close connection |
| JSON structure | must match `Envelope` | `error{code:"malformed"}`, close |
| nickname | 1..=32 chars after trim, no control chars | `error{code:"bad_nickname"}`, close |
| message body | non-empty after trim, ≤ 8 000 chars, no control chars except `\n`/`\t` | `error{code:"bad_body"}`, keep connection |
| per-connection rate | token bucket, 20 msg/s, burst 40 | `error{code:"rate_limited"}`, close |
| concurrent peers | 128 | refuse the `hello` |
| concurrent dials | 8, 1 per peer | queue/drop the extra |
| handshake deadline | 10 s from accept | close |

The rate limiter is a pure struct over `(tokens, last_refill)` with a caller-supplied `now`,
so its behaviour is unit-tested without sleeping.

### 5.6 Encryption

The transport is split so that encryption is an insertion, not a rewrite:

```
TcpStream ──▶ [ StreamLayer ] ──▶ FrameCodec ──▶ Envelope ──▶ Session
                  ▲
                  └─ boxed AsyncRead + AsyncWrite
```

`FrameCodec` is generic over `AsyncRead + AsyncWrite + Unpin + Send`. A cipher layer only has
to produce such a stream. Nothing above `FrameCodec` changes, and the handshake/envelope
stay byte-identical.

**Decision: not enabled in this version.** The seam is in place and the design is
specified (Noise `XX` via `snow`, static keypair generated at first launch and stored beside
the device id, peer key learned on first contact and pinned by device id — TOFU — with a
mismatch surfaced as a security event and the connection refused). It is documented rather
than half-built because the framing above a Noise transport (record chunking, nonce
accounting, rekeying) is exactly the code where a subtle mistake silently degrades to
"encrypted-looking", and an unverified implementation would be worse than an honest
plaintext LAN protocol whose threat model is written down. The threat model is in §11.

---

## 6. Discovery

Two independent mechanisms feed the same `DiscoveryEvent { Found { device_id, nickname,
avatar_seed, addresses }, Lost { device_id } }` stream:

**Primary — mDNS/DNS-SD.** Service type `_localme._tcp.local.` via the `mdns-sd` crate
(0.21, Apache-2.0/MIT). Each instance publishes `ServiceInfo` with
`instance = "localme-<first 20 hex chars of device id>"` (28 bytes, inside the service-name
length cap) and TXT records `id`, `nick`, `seed`, `pv`. The authoritative device id lives in
TXT, not in the instance name, so the name only has to be *unique*, not parseable.
Addresses arrive from the SRV/A records, so multi-homed peers yield every address; the
dialer tries them in order and remembers the winner.

**Fallback — UDP beacon.** A datagram on the announce port (`47821`, configurable) sent to
`255.255.255.255` and to the IPv4 multicast group `239.255.77.77:47821`, carrying the same
identity tuple. It exists for networks where mDNS is filtered (some corporate, some
container/VPN setups). Receiving a beacon produces a `Found`; the announcement period is
adaptive — every 3 s while alone, every 20 s once a peer is known — so an idle instance does
not produce a steady drip of traffic. The socket is bound with `SO_REUSEADDR` (and
`SO_REUSEPORT` where the platform has it), so a port already held by another process degrades
to "both sockets receive" instead of "the fallback is gone".

Both adapters feed `CompositeDiscovery`, which de-duplicates by `(device_id, address)` and
emits `Lost` only when the last live source for a device disappears. A peer seen by both
mechanisms is not reported twice.

**Network changes.** `mdns-sd` re-announces automatically when the host's addresses change
(its internal IP check). VPN and virtual adapters are handled by mdns-sd's per-interface
send/recv, which advertises on, and browses, every interface it finds; this is the mechanism
that makes "several interfaces" work without us enumerating them. When a dial fails, the
address is dropped and the peer is re-resolved from the mDNS cache before the next attempt,
which is what makes an IP change recover; when the socket for a peer fails, the presence
machine returns to `Offline` and the next discovery event re-dials. The discovery supervisor
also restarts browse/register on `DaemonEvent::Error` with exponential backoff, capped at 60 s.

**Self-exclusion.** Our own device id is filtered at the `DiscoveryEvent` boundary, so our
own record never reaches the UI, regardless of which mechanism found it; `set_multicast_loop_v4(false)`
additionally stops the beacon from hearing itself.

### 6.1 Why not discovery over UDP only

mDNS already solves query/response, caching, name conflict resolution, and per-interface
behaviour, and every desktop OS ships a responder, so interoperating with the platform is
worth more than a bespoke protocol. The UDP beacon is the escape hatch for the networks
where the platform blocks it — it is deliberately dumb (no query, just periodic announce and
direct replies) because it must work precisely where the smarter mechanism does not.

---

## 7. Storage

SQLite via `rusqlite` (0.40) with the `bundled` feature: sqlite is compiled in, so no system
library is required and builds are identical on all three platforms. WAL journal mode,
`synchronous = NORMAL`, `foreign_keys = ON`.

The connection is owned by one dedicated writer thread (rusqlite's `Connection` is `!Sync`);
callers send `StoreCommand`s over a bounded `mpsc` and await a `oneshot`. This gives serialised
writes with no lock contention and no thread pool.

### 7.1 Schema

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) -- device_id, schema_version, keys…

CREATE TABLE peers (
  device_id    TEXT PRIMARY KEY,          -- UUID string
  nickname     TEXT NOT NULL,
  avatar_seed  TEXT NOT NULL,
  last_address TEXT,                      -- last successfully used "ip:port"
  last_seen_ms INTEGER,                   -- receiver's clock, NULL until first seen
  first_seen_ms INTEGER NOT NULL,
  unread       INTEGER NOT NULL DEFAULT 0,
  notify_muted INTEGER NOT NULL DEFAULT 0, -- 0/1
  forgotten    INTEGER NOT NULL DEFAULT 0  -- 0/1
);

CREATE TABLE messages (
  id           TEXT PRIMARY KEY,          -- UUIDv7
  peer_id      TEXT NOT NULL REFERENCES peers(device_id) ON DELETE CASCADE,
  outgoing     INTEGER NOT NULL,          -- 1 = we sent it
  body         TEXT NOT NULL,
  sent_at_ms   INTEGER NOT NULL,          -- sender's clock (display only)
  received_at_ms INTEGER NOT NULL,        -- our clock (ordering fallback)
  status       TEXT NOT NULL,             -- sending|sent|delivered|received|failed
  read         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX messages_peer_time ON messages(peer_id, sent_at_ms DESC, id DESC);
CREATE INDEX messages_unread    ON messages(peer_id) WHERE read = 0;
```

`peers` doubles as the *known/forgotten device* list required by the settings screen.
`forgotten = 1` rows are excluded from the user list but always kept, which is what lets the
settings screen show forgotten devices and offer to bring them back. `forgotten` is cleared
when the peer is seen again — "if their computer appears on the network again, it will be
added as a new person", which is what the confirmation dialog promises — and by an explicit
restore, for a device that is not on the network right now.

### 7.2 Migrations

`meta.schema_version` drives a forward-only migration list. Each migration runs inside a
transaction and bumps the version. Version 0 (no tables) → 1 (schema above) is the initial
migration, so a fresh database and an upgraded one take the same code path.

### 7.3 Corrupted database

On open, `PRAGMA integrity_check` runs (cheap on small files). If it fails, or if opening
fails, the file is renamed to `localme.db.corrupt-<timestamp>` and a fresh database is
created; the app reports this once through a UI event, naming the preserved file, so the user
keeps their data and knows what happened. Silently deleting a user's history is not
acceptable; crashing on startup is worse.

### 7.4 Paging

History is loaded newest-first in pages of 50 (`messages_peer_time` serves this directly).
The chat view starts with one page and requests the next when the scroll position approaches
the top. No offset-based deep paging: the cursor is `(sent_at_ms, id)`, which stays correct
while new messages arrive.

### 7.5 Forward compatibility

`messages.status` is a text column and the envelope is versioned, so file transfer and group
chats are added as new envelope variants plus new columns/tables in later migrations. Nothing
in the current schema presumes 1:1 forever beyond `peers.device_id` being the conversation
key, which the group feature would extend with a conversation table rather than replace.

### 7.6 Search

Peer search is by nickname, case-insensitive, over the in-memory peer list — the list is
bounded by the number of devices on a LAN, so no SQL is involved and results are instant.
Message search is not implemented (not requested) and therefore has no index.

---

## 8. Services

### 8.1 `Session`

The only mutable network state in the process. It is an actor: a single task owning

* `peers: HashMap<DeviceId, PeerEntry>` where `PeerEntry = { profile, addresses, presence: PresenceMachine, outbound: mpsc::Sender<Envelope> }`,
* the dial task registry (one in-flight dial per peer, `JoinHandle`),
* the deduplication and rate-limiting state.

Inputs are `SessionCommand` (from the Tauri layer) and `NetworkEvent` (from connections and
discovery), merged into one mailbox so every state change is sequential. Outputs are
`CoreEvent`s on a bounded `broadcast` channel, consumed by the Tauri layer, which forwards
them to the webview.

Commands: `SendMessage`, `LoadHistory`, `MarkRead`, `ForgetPeer`, `ListPeers`, `GetProfile`,
`SetNickname`, `SetNotifyMuted`, `Shutdown`.

### 8.2 Event flow for an inbound message

```
connection task: frame → validate → NetworkEvent::Frame{peer, chat}
   → Session: dedup (rate limit, then storage INSERT OR IGNORE)
   → Session → Store: InsertMessage (await: dedup verdict + unread increment)
   → Session → connection: chat_ack
   → Session → broadcast: MessageReceived{peer, message, unread}
   → Tauri: window hidden? → tray badge + native notification (unless muted/chat focused)
             window visible? → emit to webview → Pinia store → list/chat update
```

The acknowledgement is sent only after the row is committed, so "доставлено" on the sender
means "durably stored on the recipient", not "written to a socket buffer".

### 8.3 Settings

An actor over a typed `Settings` struct (serde), persisted as one JSON document in the
app-data directory and written atomically (temp file, then rename) by the actor itself —
`tauri-plugin-store` was the alternative and was rejected: a second writer for the same data
would be a second answer to "what is stored", and the host needs the document before the web
view exists. The schema is versioned (`SETTINGS_VERSION`, currently **2**), the file is
migrated (or reset, with the reason reported) on load, unknown fields are ignored rather
than kept, and every group carries `#[serde(default)]`, so a group added in a later version
is readable by an older build and vice versa. In-memory reads are answered by the actor, so
the front end never blocks on disk.

Settings groups map 1:1 to the settings screen: `appearance`, `locale`, `notifications`,
`system`, `logging`. Values that are individually valid JSON but unusable are repaired by
`Settings::normalise` on every load and update — a bad accent colour falls back to the
default, and a retention outside 1..=365 is clamped, because a hand-edited `0` would
otherwise delete today's log on the next start.

The logging group is the one group the host applies rather than merely stores: a change to
the level or the retention is pushed into the live subscriber by `events::apply_settings`
(§9.7), so the settings screen can change verbosity without a restart.

### 8.4 Graceful shutdown

On `Shutdown` (tray "Quit", window close with quit, or OS session end) the session actor:

1. stops the session loop from accepting new commands,
2. sends `goodbye{reason:"shutdown"}` on every live connection and waits up to 500 ms for the
   writes to drain,
3. unregisters the mDNS service and shuts the daemon down,
4. stops the UDP beacon,
5. closes every connection task and awaits their handles with a 1 s budget,
6. flushes and closes storage.

Steps 2, 3 and 5 have bounded waits so a wedged peer cannot prevent exit; the remaining
work is abandoned rather than hanging the process.

---

## 9. Tauri host

### 9.1 Commands are thin

A command validates its arguments, calls one service method, maps the error into an `ApiError`
and returns a DTO. No protocol, storage or presence logic exists in `src-tauri/src/commands.rs`.
Every argument type is a validated newtype where possible (`Nickname`, `DeviceId`), so
deserialisation is the validation boundary.

### 9.2 Typed IPC without a code generator

The front end has a single module, `src/ipc/`, exporting

```ts
invokeCmd('send_message', { peerId, body })   // typed by a Commands interface
onCoreEvent('message_received', handler)      // typed by an Events interface
```

with the `Commands`/`Events` interfaces declared once in TypeScript and mirrored by the Rust
DTOs. `tauri-specta` was considered and rejected: at the time of writing it is at `2.0.0-rc`,
and pulling a release candidate into the build to generate a few dozen type pairs trades a
stable, reviewable 150-line module for a supply-chain and churn risk. The wrapper keeps
runtime validation at the boundary (`assertNever` on unknown event names, exhaustiveness on
command results) and is checked by `vue-tsc` in strict mode against the DTO shapes, which is
the property that actually matters: a changed Rust field breaks the front-end build.

**How the layer is tested.** A command is three statements — parse, call one service method,
let the error convert — and the parsing is the only part with logic of its own, so it lives in
`src-tauri/src/args.rs` with unit tests covering every rejection path and the `field` name each
rejection reports (the interface uses that name to mark the offending input). The commands are
then small enough to read, and the behaviour behind them is covered by the core's integration
suite. Driving them through Tauri's mock runtime would be stronger, and the attempt is recorded
here rather than silently dropped: on the development machine `tauri::test` produces a binary
the Windows loader refuses to start (`STATUS_ENTRYPOINT_NOT_FOUND`), before any test runs. That
harness was removed rather than shipped untested.

### 9.3 Events and the hidden-window rule

`CoreEvent`s are forwarded to the webview only when the window is visible. While the window is
hidden in the tray, the Tauri layer keeps the state it needs (unread count for the tray
badge, a cached peer list) and emits nothing; on show it emits one `state_snapshot` event
instead of replaying a backlog. This is what keeps a tray-resident instance at ~0 % CPU and
prevents the "wake the webview for every heartbeat" class of bug.

The payload of an event is the **content of its variant**, not the variant wrapped in its own
name: `CoreEvent::Peers { peers }` arrives as `{"peers":[…]}` and `CoreEvent::Stopped` as
`null`, which is what `src/ipc/types.ts` declares and what `src/app/connect.ts` reads. Serde's
default (externally tagged) representation would nest it — `{"peers":{"peers":[…]}}` — and
that is not a cosmetic difference: a subscription reading `payload.peers` would get an object
where it expects an array, and the user list would silently empty itself the first time any
peer changed. `untagged` plus `rename_all_fields = "camelCase"` is the derivation that matches
the interface, and the unit tests in `core/src/services/events.rs` pin the shape of every
variant the front end subscribes to.

### 9.4 Process and window policy

* **Single instance** — `tauri-plugin-single-instance`: a second launch focuses the running
  window and exits, which also makes "two instances on one machine" impossible. Setting
  `LOCALME_DATA_DIR` lifts the guard and points the process at its own data directory, which is
  the only way to reproduce the two-device scenario on one computer — that is how the README's
  two-instance check is performed, and it is deliberately an environment variable rather than a
  setting, so it cannot be turned on by accident in a normal installation.
* **Close to tray** — `WindowEvent::CloseRequested` is intercepted when the setting is on;
  the window is hidden and the app keeps running. When the setting is off, close quits.
* **Autostart** — `tauri-plugin-autostart`, launched with `--minimized` when "start minimised
  in tray" is enabled; the flag is parsed at startup to decide whether to show the window.
* **Tray** — `TrayIconBuilder` with a menu (Open, Disable notifications, Quit) and a tooltip
  carrying the unread count. The unread indication is the tooltip plus the window title
  (`LocalMe (3)`), because per-platform tray *badges* (macOS `NSApplication.dockTile`, Windows
  overlay icons) are not exposed by Tauri 2 in a way that works identically on Linux. Left click
  raises the window; the menu is on the right button.
* **Notifications** — `tauri-plugin-notification`, suppressed when the window has focus *and*
  the active chat is the sender's. The plugin documents that on desktop it uses only the title,
  body, icon and sound of a notification and **ignores the action-related fields**, so a click on
  a notification is not reported back to the application on any platform. The substitute is
  implemented rather than promised: the host remembers the peer whose message produced the most
  recent notification, and when the window is raised — the documented path being the tray icon —
  it opens that conversation. The sound toggle is applied as far as the platform allows
  (a named sound when on; unset when off, which on Windows still plays the default, because the
  plugin exposes no way to force silence).
* **Menu labels** — the tray and a notification are drawn outside the web view, so their text
  cannot come from the front end's catalogue at the moment they are shown. The front end pushes
  the strings once, and again on a language change, through `set_ui_labels`. That keeps one
  translation catalogue instead of a second one in Rust, at the cost of the tray being in English
  for the few milliseconds between process start and the first render.
* **Web view policy** — WebView2 ships Microsoft Edge's *general* autofill switched on, and that
  is why focusing an ordinary text field could offer to fill in a saved name or address inside a
  local-network messenger. It is a browser feature, not a form feature: no attribute, header or
  CSP directive switches it off, so the host does it once in `setup` through the WebView2 settings
  object (`src-tauri/src/webview.rs`, Windows only, `IsGeneralAutofillEnabled = false`). Password
  and payment autofill are already off by default in WebView2, and no other engine used here has a
  saved-info autofill to disable. The same reasoning as `vue/no-v-html`: the surface that is not
  part of the application does not get to appear inside it.

### 9.5 Capabilities

`capabilities/default.json` grants the minimum: `core:default` (minus unused event/window
commands), `notification:default`, `autostart:allow-enable`/`allow-disable`/`allow-is-enabled`,
`store:default`, `os:allow-hostname`, `process:allow-restart` for the restart path. No
`shell`, no `fs`, no HTTP scope, no global Tauri API (`withGlobalTauri: false`).

### 9.6 Content Security Policy

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';   /* MD3 tokens and dynamic accent colour are inline CSS vars */
img-src 'self' data:;               /* blobatar data: URIs */
connect-src ipc: http://ipc.localhost;
font-src 'self';
object-src 'none'; base-uri 'none'; frame-ancestors 'none';
```

`'unsafe-inline'` for styles is required by the design-token strategy (the accent colour is
a computed CSS custom property) and is the minimum relaxation that works; scripts remain
strictly non-inline and no remote origin is reachable. Message bodies are rendered with
`{{ }}` interpolation only — no `v-html` anywhere in the codebase, and an ESLint rule
(`vue/no-v-html`) keeps it that way.

### 9.7 Logging

The log is the only artefact a user can send about a failure on a machine we will never see, so
the application writes one rather than relying on whatever `stderr` is attached to. `tracing`
with one subscriber, four decisions:

* **One file per day**, `logs/localme.YYYY-MM-DD.log` in the data directory (beside the
  database, so a report's log and its data agree on where "the data directory" is). The day is
  **UTC**: `std` has no local-time API, and a dependency that can fail to resolve a time zone at
  the moment a log record must be written is a worse trade than a name that is a few hours off
  the user's calendar at the boundary.
* **Append per record.** The current file is opened, written and closed for each record instead
  of being held open for the process's lifetime. That costs an `open` per record — nothing at
  this application's volume — and buys the two operations that matter: clearing the directory and
  pruning it work *while the application runs*, on Windows too, where deleting an open file
  fails.
* **Bounded retention**, from the settings document (default 14 days, 1..=365). Pruning runs at
  startup and on the first record after the date rolls over, so nothing has to keep a timer
  alive to keep the directory bounded.
* **Never fatal.** Every filesystem step is best-effort: a log that could not be written must
  not take the messenger down while it is reporting a problem.

`stderr` receives the same records, so `cargo tauri dev` and a launch from a terminal behave as
they did before files existed; ANSI colour is on only when `stderr` is a terminal, which also
keeps escape codes out of the files.

Verbosity comes from the settings document and is applied to the running subscriber through
`tracing_subscriber::reload`, so changing it in the settings screen takes effect immediately;
`RUST_LOG` wins over the stored level, because the case where debugging matters most is the one
where the interface itself is broken. A panic hook writes the payload and location before the
default hook runs, which with `panic = "abort"` is the last chance to record why the process
went away.

The web view has no filesystem access and its console is invisible in a packaged build, so
`src/app/errors.ts` forwards every component, window and unhandled-rejection error to
`log_frontend`, with the first stack frames attached: an error thrown by a component would
otherwise exist only on a screen the user has already closed. The settings screen exposes the
directory (`logs_info`), opens it in the platform's file manager (`open_logs_folder`, via
`explorer` / `open` / `xdg-open`) and empties it (`clear_logs`, which reports the bytes freed).

---

## 10. Front end

```
src/
├─ main.ts, App.vue          # mount, theme, the process-level gates (loading, first run)
├─ app/                      # router, route names, the two-pane shell, event bridge, errors
├─ ipc/                      # typed command + event wrapper (§9.2)
├─ theme/                    # MD3 tokens, palettes, accent generation
├─ i18n/                     # seven typed catalogues, tiny typed t(), Intl formatting
├─ ui/                       # design-system components (Button, TextField, Dialog, …)
├─ features/
│  ├─ onboarding/            # first-run nickname + avatar preview
│  ├─ users/                 # user list, search, sorting, forget
│  ├─ chat/                  # message list, composer, history paging
│  └─ settings/              # grouped settings screen, including the log directory
├─ stores/                   # Pinia: peers, chat, settings, ui
└─ composables/              # useNow, useMediaQuery, useEntranceWindow
```

*Stores hold state, composables hold behaviour, components hold markup.* Components never call
the host directly: every command and event goes through `ipc/`, and the only file that subscribes
to host events is `app/connect.ts`, so what the interface does when a message arrives is readable
in one place.

### 10.1 Material Design 3

**Decision: own components on MD3 tokens, plus the official `@material/material-color-utilities`
for palette generation. Material Web and Vuetify were both rejected.**

* `@material/web` (2.5.0, Apache-2.0) is genuinely MD3, but it ships Lit-based web components
  whose full component set is far larger than the ~12 controls this app needs, requires
  `isCustomElement` plumbing in Vue, and covers no Expressive elements. Its bundle cost lands
  directly against the "small front-end bundle" requirement.
* Vuetify is MD3-flavoured but brings a large runtime, its own theming layer competing with
  the token strategy, and an opinionated layout system that would have to be fought for a
  two-pane messenger.
* Custom components over tokens give exact control over the tokens, state layers, adaptive
  layout and reduced motion, cost only the CSS actually used, and keep the DOM predictable for
  virtualisation.

`material-color-utilities` is the same HCT algorithm Material's own tooling uses, has zero
dependencies, and is used only to derive tonal palettes from the accent colour. All tokens
are CSS custom properties on `:root`:

```
--md-sys-color-primary / on-primary / primary-container / …
--md-sys-color-surface / surface-container-low|high|highest / outline / …
--md-sys-typescale-{display|headline|title|body|label}-{large|medium|small}-{size|weight|line-height|tracking}
--md-sys-shape-corner-{extra-small|small|medium|large|full}
--md-sys-state-{hover|focus|pressed}-opacity
--md-sys-motion-duration-{short|medium|long} / --md-sys-motion-easing-{standard|emphasized}
```

Theme modes: `system | light | dark`, resolved from `matchMedia('(prefers-color-scheme: dark)')`
(watched, not polled). Reduced motion is honoured globally by zeroing the motion durations
under `@media (prefers-reduced-motion: reduce)`, which also disables the blobatar idle
animation.

### 10.2 Avatars

`blobatar` (2.7.0, MIT) — deterministic geometric avatars from a string, ~4.4 KB gzipped,
zero dependencies. Rendering uses the string API:

```ts
const svg = blobatarUri(seed, { background: 'squircle', size: 96 });   // data: URI
```

one `<img>` per avatar. The Vue adapter was not used: the adapter renders inline animated
SVG, and an idle animation per row in a virtualised list is exactly the kind of
always-running work this project is trying not to do. Static `<img src="data:…">` is
composited by the browser with no per-frame cost. Rendered URIs are memoised in an LRU keyed
by `(seed, size)` in `composables/useAvatar.ts`, so a re-render or a list scroll never
re-generates the same avatar.

The **seed is received from the peer**, never derived from IP or nickname locally: two devices
must agree pixel-for-pixel, and only the peer's own announcement is authoritative.

### 10.3 Virtualisation

`@tanstack/vue-virtual` (headless, MIT) for both lists: fixed-size rows for the user list,
dynamic, measured rows for the message list, which also handles the reverse-anchored
"stick to bottom" behaviour of a chat log. Written from scratch this is a measurement-
invalidation and scroll-anchoring problem that is hard to get right; the library is
framework-thin and adds no styling, so the token strategy is unaffected.

While a conversation's first page is being read, the log draws bubble-shaped placeholders —
`MessageSkeleton` arranging `MdSkeleton` blocks, one per shape, on the side the message will
belong to — rather than a spinner in the middle of an empty pane. The placeholder carries the
shape and the width of what is coming, so the log does not jump when the real rows replace it.

### 10.4 i18n

A typed module rather than a framework: `MessageKey` is derived from the English catalogue, so a
missing key is a **compile error** and a typo in `t('…')` is a compile error at the call site;
the other catalogues are typed `Record<MessageKey, string>` plus the plural specialisations a
language actually needs. Seven languages ship: English, Russian, Spanish, German, French,
Portuguese and Chinese. `LOCALES` is the list, `src/ipc/types.ts` mirrors it for the settings
document, and `Locale` in `localme-core` is what the file stores; adding a language is a
catalogue plus those three entries.

Number, date and relative-time formatting goes through `Intl`, and so do the units: file sizes
and day counts are formatted with `Intl.NumberFormat({ style: 'unit' })`, so «14 дней», "14 Tg."
and "14 days" are the runtime's translations rather than seven hand-written forms. Plural forms
go through `Intl.PluralRules` with per-language specialisations (`users.unread.few` and friends),
and `messages.spec.ts` fails when a language is missing a key, renames a placeholder, declares a
plural form its `Intl` never selects, or ships an untranslated copy of English.

### 10.5 Pages and routing

**Decision: `vue-router` with hash history; a page is an address.**

The window is a two-pane shell, and the two panes are two levels of one route tree:

```
/                     → shell: the people list beside a RouterView
├─ /chat              → the placeholder ("pick someone to talk to")
├─ /chat/:deviceId    → that conversation, in the detail pane
└─ /settings          → the settings page, in the detail pane
```

The alternative — a `screen` field in a store, which is what this replaced — made settings an
overlay drawn on top of the conversation and left "which page is on screen" in two places: the
store and the (nonexistent) address. With routes, the back gesture, the back button and the
system's window-history all do what the address says; the peer list's selection is derived from
the route rather than stored beside it; and the tray's "open the conversation that notified"
request is a `router.replace` like every other navigation. Hash history, not HTML5 history,
because a packaged Tauri build serves the front end from a custom protocol where a path is a
file name.

The pages themselves are lazy route components, so the settings screen is not in the bundle of a
user who never opens it.

### 10.6 Motion

`motion-v` (Motion for Vue, MIT) — the Vue port of Framer Motion's API, chosen over
`@vueuse/motion` because it is the same API the rest of the ecosystem documents and because it
supports exit animations, which the page transitions need. `MotionConfig reduced-motion="user"`
in `App.vue` is the single place the operating system's preference is applied to every animation,
including those inside components that never mention motion; the CSS-transition animations
(snackbar, dialog, state layers) are switched off by the token sheet zeroing the motion durations
under `prefers-reduced-motion`.

Where motion is used, and why:

* **Page changes** — an `AnimatePresence` around the detail pane's `RouterView`, so a page leaves
  before the next arrives. The key includes the conversation's peer, because moving between two
  conversations *is* a page change: the header, the log and the composer belong to the peer in the
  address and travel together. Within a conversation the key is stable, so an arriving message
  never remounts the log. The page slides in horizontally, and the detail pane clips it: without
  the clip the 16 px that is still outside the pane would paint over the list column beside it.
* **The two lists** — entrance animations tied to the *list appearing*, not to a row mounting:
  `useEntranceWindow()` for the user list, and `chat.consumeEntrance(id)` for the message log.
  Both lists are virtualised, so a row mounts and unmounts as the reader scrolls; an animation
  bound to mounting would replay on every flick of the wheel. The user list's rows animate
  opacity only and never a transform, because a transform would leave every row in its own
  stacking context and the row's overflow menu could then never paint above the rows below it.
* **Delivery status and unread counts** — the status glyph cross-fades and the badge pops, keyed
  by the value, so a change is visibly a change.
* **The theme change** — the View Transitions API (`document.startViewTransition`) cross-fades the
  whole window when the mode or the accent changes, which is what it exists for; it is skipped on
  the first paint and on an engine without it.
* **Presence** — the avatar's presence dot transitions its colour, and the dialog animates in.

Motion is the largest single contributor to the main bundle, which now sits just above the
deliberately low `chunkSizeWarningLimit` in `vite.config.ts`; the build reports it rather than
hiding it, and the two pages are lazy chunks that a user who never opens them never downloads.

---

## 11. Security and threat model

**Trust boundary:** the LAN. Every inbound byte is treated as hostile; every outbound byte
is treated as public. Plaintext Transport with no authentication beyond the handshake means:

* **In scope:** malformed frames, oversized claims, resource exhaustion, flooding, spoofed
  discovery records, stored-XSS through message bodies, injection into the UI.
* **Out of scope (documented, not defended):** an on-path attacker reading message contents,
  an attacker impersonating a peer's device id, and traffic analysis. These require the
  encryption layer of §5.6.

Concrete measures: the frame size cap and validation table of §5.5; the strict CSP of §9.6;
text-only rendering (`v-html` banned by lint); least-privilege capabilities of §9.5; the
storage writer thread as the only SQL site (all statements are prepared with bound
parameters, so message bodies cannot become SQL); nickname/seed/body length caps enforced on
*both* the wire and in the domain newtypes, so a value that bypassed one check still cannot
be stored; and a maximum peer count so a hostile LAN cannot make us open sockets without
bound.

---

## 12. Platform notes

| Concern | Windows | macOS | Linux |
|---|---|---|---|
| Firewall prompt | on first `bind`; documented in README (ports 47820/TCP, 47821/UDP, 5353/UDP) | app-level prompt on first inbound | depends on distro firewall |
| Tray | notification area; hidden by default in Win11 (documented) | menu bar icon | requires a StatusNotifier/AppIndicator host; on GNOME, an extension |
| Notification click → open chat | supported by `tauri-plugin-notification` actions | supported | support depends on the notification daemon; documented fallback: unread badge + tray |
| Autostart | `HKCU\…\Run` via plugin | LaunchAgent via plugin | `.desktop` in `~/.config/autostart` via plugin |

Notification-click behaviour could not be verified on all three platforms from one machine;
the README states which behaviour is verified and which is platform-dependent by design,
and the UI never depends on the click action (the unread badge and the user list are always
the source of truth).

---

## 13. Assumptions

1. Peers are on the same broadcast domain. Across subnets, mDNS reflection or the UDP beacon
   must be forwarded; this is not handled and is documented in the README.
2. The user's clock may be wrong; it is used only for display. Every stored timestamp is an
   observation of the *local* clock — including the time shown for an incoming message, which
   is when it arrived (`§5.4`). Ordering inside a conversation uses `(sent_at_ms, id)` with
   `received_at_ms` as a tie-break, and `last_seen` is always the local clock. No remote clock
   is ever used for a decision.
3. Instances must share a protocol version to talk; mismatches are refused with a clear error
   rather than degraded.
4. Nicknames are not unique and are never used as identity; the UI shows a device-id-derived
   disambiguator when two peers share a nickname.
5. A user has one instance per machine; the single-instance plugin enforces it, and the
   requirement "two instances on one machine are forbidden" is therefore satisfied by
   construction rather than by protocol.
6. No encryption in this version, per §5.6.
7. Ports 47820 (TCP), 47821 (UDP) and 5353 (UDP, mDNS) must be permitted; if 47820 is taken,
   an ephemeral port is used and advertised through discovery, so a conflict degrades to
   "still works" rather than "fails to start".
8. **The web view stays alive while the window is hidden in the tray.** Measured: ~169 MB of
   private memory for the WebView2 process tree against 6.4 MB for the Rust host and core
   together (README, Performance). Destroying the web view on hide and recreating it on show is
   possible without any architectural change — the host owns every piece of state the interface
   renders, and `state_snapshot` exists precisely to resynchronise a freshly loaded front end —
   and it would be the largest resource saving available to this design. It is not done because
   the reopen path is the one path that cannot be exercised without a visible desktop in the
   environment this was built in, and shipping an unverifiable critical path to save memory on
   machines that have it is the wrong trade. The cost is measured and in the README so the
   decision can be revisited with evidence.
