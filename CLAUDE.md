# CLAUDE.md

Instructions for [Claude Code](https://code.claude.com/docs/en/claude-md) working in this repo.
Full project map: [AGENTS.md](AGENTS.md). Design document: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
Human workflow: [CONTRIBUTING.md](CONTRIBUTING.md).

## Project

LocalMe is a **Tauri 2** desktop messenger for the **local network**: instances discover each other
over mDNS/DNS-SD (with a UDP beacon fallback) and exchange text messages over TCP. No server, no
account, no configuration. Messages are stored locally in SQLite on both ends.

Stack: Vue 3.5 + TypeScript (strict) + Vite 8 + Pinia + own Material Design 3 components +
`@tanstack/vue-virtual` + `blobatar`. Pages: `vue-router`. Motion: `motion-v`. i18n: `en`, `ru`,
`es`, `de`, `fr`, `pt`, `zh`. Native: Rust edition 2024, Tauri 2, a
Tauri-free `localme-core` crate plus a thin `localme` host crate. Identifier: `dev.localme.desktop`.
Alias `@/*` → `src/*`. Vite dev server **127.0.0.1:5173**.

`npm run dev` is UI-only. Use `npm run tauri dev` for discovery, chat, tray, notifications and
persistence.

## Commands

```bash
npm install
npm run tauri dev          # real desktop app
npm run lint
npm run format:check       # Prettier, 100 print width
npm run typecheck          # vue-tsc, strict
npm run test               # Vitest
npm run test:scripts       # node --test scripts/lib
npm run build              # vue-tsc + Vite
npm run check:versions     # SemVer files + changelog section agree
npm run version:patch      # bump every version file + changelog stub
npm run release            # tag vX.Y.Z and push

cd src-tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Before finishing a change: `npm run build`, `npm run check:versions`, and
`cargo test --workspace`. Prettier: 100 width, single quotes. Rust: default `rustfmt`.

## Layout

```
src/main.ts, App.vue       Vue mount, theme init, request routing
src/ipc/index.ts           the only frontend ↔ Tauri boundary (typed commands + events)
src/theme/                 MD3 tokens, palettes, accent generation
src/i18n/                  typed t(), seven catalogues
src/ui/                    design-system components (Md*)
src/features/<name>/       one page; kebab-case dirs (onboarding, users, chat, settings)
src/stores/                Pinia: peers, chat, settings, ui
src/app/router.ts          the page table; ShellView.vue is the two-pane shell
src/app/connect.ts         the only place that subscribes to host events
src-tauri/src/lib.rs       bootstrap + command registration
src-tauri/src/commands.rs  thin IPC commands; args.rs validates arguments
src-tauri/core/            localme-core: domain, protocol, discovery, transport, storage, services
```

Do not edit `dist/`, `src-tauri/target/` or `src-tauri/gen/`.

## Import direction

```
features → ipc | stores | ui | theme | i18n | composables
stores   → ipc | i18n
ui       → theme only
ipc      → nothing in src/
```

Features must not import other features. On the Rust side, `localme-core` must not depend on
`tauri`, and `domain`/`protocol` must not import adapters or services.

## Architecture (short)

- `localme-core` holds all behaviour and is fully testable with `cargo test`; the `localme` crate is
  a thin Tauri host (window, tray, single-instance, IPC).
- New Tauri commands: implement in `commands.rs` → register in `lib.rs` → wrap in
  `src/ipc/index.ts` + `types.ts`. Components never `invoke`.
- Message flow: persist `sending` → send frame → recipient deduplicates (`INSERT OR IGNORE`) and
  acknowledges → sender marks `delivered`.
- Presence combines discovery + connection + heartbeat; the FSM takes `now` as a parameter and has
  no clock of its own.
- Events reach the webview only while the window is visible; on show the host sends one
  `state_snapshot`.
- `LOCALME_DATA_DIR` overrides the data directory and lifts the single-instance guard (two-instance
  testing only).

## Versioning

SemVer starts at **0.1.0**. Every shipped change must bump the version and add a dated section to
[CHANGELOG.md](CHANGELOG.md). Do not defer this.

- **Patch**: bugfix, copy, docs, tweak, translation, hardening.
- **Minor**: a new user-visible capability, still compatible.
- **Major**: a breaking change for users or persisted data.

Default to patch when unsure. Keep `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
`src-tauri/core/Cargo.toml`, `src-tauri/Cargo.lock` (`localme`, `localme-core`),
`src-tauri/tauri.conf.json` and the `Version:` line in [AGENTS.md](AGENTS.md) in sync.

## Rules

1. Identity is the device id (UUID v4); never the nickname, MAC or IP.
2. Validate everything at the boundary: `Nickname::parse`, argument parsing, frame validation.
3. Every inbound byte is hostile — size, shape, length and rate are checked before the domain.
4. One clock: every stored timestamp is the local clock; remote clocks are display-only.
5. No `v-html` anywhere (lint-enforced). Render message bodies as text.
6. Every user-facing string: every catalogue, and a missing key must not compile.
7. No `any` in new TypeScript. Small functions, explicit types.
8. Do not commit secrets, `dist/`, `src-tauri/target/`, `src-tauri/gen/`, or OS temp files.
9. Log every change in `CHANGELOG.md` and bump SemVer (patch for small changes).

## Naming

Vue components PascalCase. TS functions camelCase. Rust fns and Tauri commands `snake_case`.
Feature dirs kebab-case. Tests next to the module (`*.spec.ts`) or under `tests/`.

## Do not

- Add a component library, a second state library, a second i18n system or a second animation
  library.
- Put the current page in a store: the router owns it.
- Call Tauri `invoke`/`listen` outside `src/ipc/`.
- Put protocol, discovery or storage logic in the Tauri host crate.
- Trust remote clocks.
- Treat `npm run dev` (browser) as a substitute for `npm run tauri dev` when touching native
  behaviour.
