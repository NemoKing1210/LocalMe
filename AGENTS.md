# LocalMe agent guide

This file is the project map for coding agents and contributors. Prefer it over assumptions from
similar Tauri or Vue apps. [CLAUDE.md](CLAUDE.md) is the concise session brief for Claude Code;
`docs/ARCHITECTURE.md` is the full design document, and this file links to it rather than
repeating it.

## What LocalMe is

LocalMe is a **Tauri 2 desktop app** that is a **local-network messenger**. Two computers on the
same LAN find each other with no server, no account and nothing to configure, and exchange text
messages. Messages are stored locally in SQLite on both ends. There is no server component.

Typical user loop:

1. First launch: pick a nickname and an avatar (onboarding).
2. The core discovers peers over mDNS/DNS-SD, with a UDP beacon as a fallback.
3. Pick a peer, type a message; the recipient persists it and acknowledges, and the sender shows
   _delivered_. Writing to a peer that is away queues the message and sends it, in order, when the
   peer is back.
4. History, presence, unread counts and settings persist across restarts.

UI languages: English, Russian, Spanish, German, French, Portuguese and Chinese. Identifier:
`dev.localme.desktop`. Version: `0.9.0`. Changelog: [CHANGELOG.md](CHANGELOG.md). Design notes:
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Stack (accurate)

| Piece     | Reality                                                                                              |
| --------- | ---------------------------------------------------------------------------------------------------- |
| Shell     | Tauri 2                                                                                              |
| Front end | Vue 3.5 + TypeScript 5.9 (strict) + Vite 8                                                           |
| UI kit    | **Own components** on Material Design 3 CSS tokens, not a component library. Primitives in `src/ui/` |
| Palette   | `@material/material-color-utilities` (HCT tonal palettes from the accent colour)                     |
| Avatars   | `blobatar` (static SVG data URIs, memoised)                                                          |
| Lists     | `@tanstack/vue-virtual` (headless)                                                                   |
| Pages     | `vue-router` with hash history; one route per page, the shell is the parent route                    |
| Motion    | `motion-v` (Motion for Vue), configured once through `MotionConfig`                                  |
| State     | Pinia (`src/stores/`)                                                                                |
| i18n      | A typed module (`src/i18n/`), `en` `ru` `es` `de` `fr` `pt` `zh`, missing key is a compile error     |
| Native    | Rust edition 2024, Tauri 2                                                                           |
| Storage   | SQLite via `rusqlite` (bundled) owned by one writer thread                                           |
| Discovery | `mdns-sd` + a UDP beacon (`socket2`)                                                                 |
| Logs      | `tracing` to `stderr` and to a daily file under `logs/`, pruned by the settings document             |
| Tests     | Vitest (front end, `src/**/*.spec.ts`) and `cargo test` (core, incl. a two-instance loopback suite)  |

Path alias `@/*` → `src/*` (`tsconfig.app.json`, `vite.config.ts`). Vite dev server is
**127.0.0.1:5173** (`strictPort: true`).

`npm run dev` is UI-only: `src/ipc/index.ts` calls Tauri commands that do not exist in a browser,
so the interface renders but has no data. Use `npm run tauri dev` for the real app.

## Repository map

```
src/
  main.ts, App.vue         Vue mount, theme init, the process-level gates
  ipc/                     the only frontend ↔ Tauri boundary (typed commands + events)
  theme/                   MD3 tokens, palettes, accent generation
  i18n/                    typed t(), seven catalogues, Intl formatting
  ui/                      design-system components (MdButton, MdTextField, MdDialog, …)
                           icons.ts (own marks), fileIcons.ts (file-type badges)
  features/
    onboarding/            first-run nickname + avatar
    users/                 user list, search, sorting, forget
    chat/                  message list, composer, attachments, history paging
    settings/              profile, appearance, language, notifications, system, data, logs
  stores/                  Pinia: peers, chat, settings, ui
  composables/             useNow, useMediaQuery, useEntrance
  app/                     router + routes, ShellView, connect (event bridge), errors, ready
src-tauri/
  Cargo.toml               package `localme` (host) + workspace definition
  tauri.conf.json          Tauri 2 configuration
  capabilities/default.json  least-privilege grants
  src/
    lib.rs                 bootstrap, plugin wiring, command registration
    main.rs                thin entry
    commands.rs            thin IPC commands: parse → call service → map error → DTO
    args.rs                argument parsing/validation (unit-tested)
    state.rs               the shared host state handed to commands
    events.rs              CoreEvent → window.emit (suppressed while hidden in the tray)
    files.rs               what may be attached, and opening/revealing what arrived
    tray.rs  window.rs  notifications.rs  autostart.rs  error.rs
    logging.rs              the subscriber: daily files, retention pruning, the level reload
  core/                    crate `localme-core` — no Tauri, no UI
    src/
      domain/              pure: DeviceId, MessageId, AttachmentId, FileName, Peer, PresenceMachine
      protocol/            pure: framing codec, Envelope, validation, rate limiter
      ports/               traits: Discovery, Store
      discovery/           mDNS, UDP beacon, composite adapter
      transport/           TCP listener, connection, codec
      storage/             SQLite store + schema/migrations
      services/            Session actor, file-transfer engine, Settings actor, events
      runtime.rs           wires concrete adapters into the actor
    tests/loopback.rs      two full instances discovering and messaging each other
.github/
  workflows/ci.yml         quality gate (front end, core matrix, versions)
  workflows/release.yml    tag/manual installer builds → draft GitHub Release
  actions/setup            shared Node/Rust/Linux setup
scripts/                   version bump, changelog extract, release tag
docs/ARCHITECTURE.md       full design document
```

Do not edit `dist/`, `src-tauri/target/` or `src-tauri/gen/` by hand.

## Import direction

```
features → ipc | stores | ui | theme | i18n | composables
stores   → ipc | i18n
ui       → theme (tokens) only; never ipc, stores or features
ipc      → nothing in src/ (it is the boundary)
```

Feature modules must not import other features; the route table (`src/app/router.ts`) and the
shell (`src/app/ShellView.vue`) compose them. Components never call the
host directly — every command and event goes through `src/ipc/`. When a feature grows, add
siblings in the same `src/features/<feature>/` directory.

On the Rust side the dependency rule is enforced by the crate split: `localme-core` must not
depend on `tauri`, and `domain`/`protocol` must not import adapters or services.

## Runtime architecture

### Two crates

- `localme-core` holds everything with behaviour: domain, protocol, discovery, transport, storage
  and the service actors. It has **no** Tauri dependency, which is what lets the integration suite
  drive two complete instances without a web view.
- `localme` (in `src-tauri/src/`) is the Tauri host: window, tray, single-instance guard and the
  IPC surface. It contains no protocol logic.

### IPC

`src/ipc/index.ts` declares one typed function per command and one typed subscription per event,
mirroring the Rust DTOs in `src/ipc/types.ts`. It is the only file that names a command as a
string. Register a new command in `src-tauri/src/lib.rs`, implement it in `commands.rs`, then wrap
it in `src/ipc/`.

| Command group        | Commands                                                                                                                          |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Bootstrap & peers    | `bootstrap`, `list_peers`, `known_devices`, `forget_peer`, `restore_peer`, `set_peer_muted`                                       |
| Chat                 | `history`, `send_message`, `mark_read`, `clear_history`                                                                           |
| Files                | `pick_files`, `inspect_files`, `open_attachment`, `reveal_attachment`, `save_attachment`, `cancel_attachment`, `retry_attachment` |
| Profile              | `own_profile`, `set_nickname`, `complete_onboarding`                                                                              |
| Settings & UI labels | `get_settings`, `update_settings`, `is_autostart_enabled`, `set_ui_labels`, `set_active_chat`                                     |
| Window & lifecycle   | `show_window`, `hide_window`, `quit`, `diagnostics`                                                                               |
| Logs                 | `logs_info`, `open_logs_folder`, `clear_logs`, `log_frontend`                                                                     |

Host events: `peers`, `message`, `message_status`, `attachment`, `settings_changed`,
`state_snapshot`, `open_chat`, `notice`, `stopped`. They are fanned into the stores by
`src/app/connect.ts` — the only place that subscribes to the host.

Commands return a typed `ApiError` on failure (never a raw panic); `CommandError` in `src/ipc/`
normalises it, and `isOffline` distinguishes "the peer is not reachable" from real errors.

### Message pipeline

1. The composer calls `sendMessage`; `commands.rs` validates and calls the session.
2. The session persists the row `status = 'queued'` — the send is durable before it is attempted —
   and emits a `message` event. It then drains that conversation's outbox, oldest first, moving each
   row to `sending` as its frame is handed to the socket.
3. The recipient validates, deduplicates (`INSERT OR IGNORE` on the UUIDv7 primary key),
   persists, sends `chat_ack`, and increments unread only if the row was new.
4. The sender flips the row to `delivered` and records `delivered_at` on `chat_ack`, and emits
   `message_status`.

A peer that is offline is not an error: the rows wait in SQLite and are written out, in order, when
it is back. The drain is paced (see `OUTBOX_BURST` / `OUTBOX_RATE_PER_SECOND`) so it stays under the
recipient's inbound rate limit, and a disconnect returns every in-flight row to `queued`
(`requeue_pending_messages`); startup does the same for the whole database. The interface shows a
second date — when the message was delivered — for a message that had to wait.

Files ride on the same message and then go their own way: the metadata is announced by the `chat`
frame (so the bubble appears immediately, with the file cards in it), and the bytes follow as
`file_chunk` frames paced by `FILE_SEND_RATE_PER_SECOND`. The recipient's part file is the
authority on how much arrived — a chunk that does not continue it is refused with the real offset —
so an interrupted transfer resumes from wherever it stopped instead of starting over, and the
digest in `file_done` is what says the file is the file. `attachment` events carry the whole row to
the interface. The design is §5.7 of `docs/ARCHITECTURE.md`.

### The hidden-window rule

`CoreEvent`s are forwarded to the webview only while the window is visible. While the window is
hidden in the tray the host keeps the state it needs (unread count, cached peers) and emits
nothing; on show it emits one `state_snapshot` instead of replaying a backlog. This is what keeps
a tray-resident instance at ~0 % CPU.

### Discovery and host policy

- Discovery is DNS-SD over mDNS (`_localme._tcp.local.`) plus a UDP beacon on port 47821 for
  networks that filter multicast; `CompositeDiscovery` de-duplicates by device id.
- Ports: **47820/TCP** (peer connections, ephemeral if taken), **47821/UDP** (beacon),
  **5353/UDP** (mDNS).
- Single instance: a second launch focuses the first window. `LOCALME_DATA_DIR` overrides the data
  directory _and_ lifts the guard, which is how the two-instance check is done on one machine.
- Close-to-tray, start-minimised and autostart are settings (`tauri-plugin-autostart`,
  `--minimized`).

## Domain rules (do not break)

1. **Identity is the device id**, a UUID v4 generated once and persisted. Never use the nickname,
   MAC address or IP as identity.
2. **`Nickname` is a validated newtype** — 1..=32 chars, trimmed, no control characters. Its only
   constructor is `Nickname::parse`.
3. **Message ids are UUID v7** so `ORDER BY id` is a time order and `INSERT OR IGNORE` deduplicates.
4. **One clock.** Every stored timestamp is an observation of the _local_ clock; remote clocks are
   display-only and never used for ordering, expiry or presence.
5. **Presence is a pure function of events** (`PresenceMachine`), always taking `now` as a
   parameter — no internal clock.
6. **Every inbound byte is hostile.** Frame size, JSON shape, nickname, body and rate are checked
   before the domain sees anything (see `docs/ARCHITECTURE.md` §5.5).
7. **No `v-html`, ever.** Message bodies and nicknames are rendered with `{{ }}`; an ESLint rule
   (`vue/no-v-html`) enforces it.
8. Keep the core's state serialisable and UI-independent. UI renders; services perform I/O.
9. Every user-facing string exists in **every** catalogue (`en`, `ru`, `es`, `de`, `fr`, `pt`, `zh`); a
   missing key is a compile error and `messages.spec.ts` checks the placeholders and the plural
   forms of each language.
10. Do not commit secrets, build output (`dist/`, `src-tauri/target/`, `src-tauri/gen/`), or
    OS-specific temp files.
11. **A file name from the network is never a path.** `FileName::sanitise` is the only way a name
    becomes a path component, and the file lands in a directory named after the attachment's
    identifier — `<data-dir>/files/<id>/<name>` — so a peer can choose what a file is called and
    never where it goes.

## Naming

| Kind                     | Convention                                         |
| ------------------------ | -------------------------------------------------- |
| Vue components           | PascalCase (`MdButton.vue`)                        |
| TS functions / variables | camelCase                                          |
| Rust fn + Tauri commands | snake_case                                         |
| Feature directories      | kebab-case                                         |
| Tests                    | next to the module (`*.spec.ts`) or under `tests/` |

Formatting: Prettier, **100** print width, single quotes (`/.prettierrc.json`). Rust: default
`rustfmt`.

## Comments

Write code that explains itself. Add a comment only when the reason for a line is not visible in
the line itself — a non-obvious constraint, a tradeoff, a workaround, a domain rule the code cannot
express. Do not comment every edit, do not restate the code in prose, and do not annotate an
obvious change. Keep existing comments true or delete them; never comment out code.

## How to add work safely

- **Screen / feature:** add under `src/features/<name>/`, register it as a route in
  `src/app/router.ts` (a page is an address, not a flag), and import only from `ipc/`, `stores/`,
  `ui/`, `theme/`, `i18n/`, `composables/`.
- **Native capability:** implement in the matching Rust module, register in `lib.rs`, wrap in
  `src/ipc/index.ts` and `types.ts`, then call it from a store or a feature.
- **Motion:** use `motion-v`. `MotionConfig reduced-motion="user"` in `App.vue` is the one place the
  system preference is applied, so do not re-check it per component. In a virtualised list, tie the
  animation to the item _arriving_ (`useEntranceWindow`, `chat.consumeEntrance`) rather than to the
  row mounting, and leave no transform at rest on a list row — see the note in `UserListView.vue`.
- **String:** add the key to `src/i18n/messages/en.ts` (the source of truth for `MessageKey`) and to
  **every** other catalogue; `npm run test` fails when one is missing. A new language is a catalogue
  plus an entry in `src/i18n/locales.ts`, `src/ipc/types.ts` and `Locale` in `localme-core`.
- **UI control:** add an `Md*` component in `src/ui/` using the tokens from `src/theme/tokens.css`;
  do not import a component library.
- **File-type icon:** add the extension to `BY_EXTENSION` in `src/ui/fileIcons.ts` and import the
  badge from `~icons/vscode-icons/<name>`; check the icon exists in the installed set before you
  name it. Keep it cheap — an extension whose badge costs 15 KB is not worth an icon — and map to
  the nearest family rather than leaving it unhandled when the set has no badge of its own.
- **Setting:** extend the typed `Settings` document (and its schema version), its default, the
  settings screen, and every catalogue. A logging level or retention change is applied by the host
  without a restart: `events::apply_settings` calls `logging::apply`.
- **Protocol change:** bump the envelope protocol version and update the frame table in
  `docs/ARCHITECTURE.md` §5.2.

## Commands agents should run

```bash
npm run lint
npm run format:check
npm run typecheck          # vue-tsc, strict
npm run test               # Vitest (src/**/*.spec.ts)
npm run test:scripts       # node --test scripts/lib
npm run build              # vue-tsc + Vite
npm run check:versions     # SemVer files + changelog section agree

cd src-tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The opt-in real-multicast test is `/loopback -- --ignored`; it is advisory because it depends on the
machine's network interfaces.

## Versioning and the changelog

Every shipped change bumps the version and lands a dated `CHANGELOG.md` section **in the same
change** — do not defer either.

- **Patch** (`0.1.0` → `0.1.1`): bugfix, copy, docs, tweak, translation, hardening.
- **Minor** (`0.1.1` → `0.2.0`): a new user-visible capability that stays compatible.
- **Major** (`0.2.0` → `1.0.0`): a breaking change for users or persisted data.

Default to patch when unsure. Keep `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
`src-tauri/core/Cargo.toml`, `src-tauri/Cargo.lock` (`localme` and `localme-core`),
`src-tauri/tauri.conf.json`, and the `Version:` line in this file in sync — `npm run
version:patch` (or `minor` / `major`) does all of it and inserts a changelog stub. `npm run
check:versions` must pass.

## Change checklist

- Discovery, dial and reconnect paths
- Message send, the outbox (queued → sending → delivered), delivery acknowledgement, deduplication
  and the requeue on disconnect
- File offer, transfer, pause/resume, cancellation, retry and digest verification
- Presence transitions, including the simultaneous-connect tie-break
- Persistence across restart (peers, history, settings, window bounds)
- Tray close, native notification, single-instance focus
- Copy in every catalogue for each new string, with the plural forms the language actually uses
- Pages: a new screen is a route, and the back behaviour follows from it
- The log file: a start still writes one line, and a level change takes effect without a restart
- Light and dark theme if you touched chrome or tokens
- `CHANGELOG.md` updated and SemVer bumped across every version file
- `docs/ARCHITECTURE.md` updated when a decision or a bound changes

## Out of scope / traps

- Do not add a second component library, a second state library or a second i18n system.
- Do not add a second _icon library_. `unplugin-icons` with the `vscode-icons` collection is the
  one exception the project allows, and only for file-type badges keyed by extension
  (`src/ui/fileIcons.ts`); interface chrome uses `src/ui/icons.ts` and `MdIcon`. The exception
  exists because a PDF badge is someone else's format, not part of this application's visual
  language — it is not a precedent for pulling a general-purpose icon set into the chrome.
- Do not add a second animation library, and do not hand-write the same `motion` animation again in a
  component that a shared pattern already covers: the user list, the message log and the unread badge
  are the three that exist.
- Do not navigate with anything but the router: a store field that decides which page is on screen
  is the bug this architecture replaced.
- Do not call Tauri `invoke`/`listen` outside `src/ipc/`.
- Do not put protocol, discovery or storage logic in the Tauri host crate.
- Do not trust remote clocks, and do not add a clock to the presence machine.
- Do not rely on `npm run dev` (browser) when touching native behaviour — use `npm run tauri dev`.
- mDNS and the beacon are link-local: they do not cross a router or a VPN tunnel.
