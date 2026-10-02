# Contributing to LocalMe

Thanks for helping. This document is the human workflow. [AGENTS.md](AGENTS.md) is the project map
for both people and coding agents — read it before changing module boundaries. Claude Code uses
[CLAUDE.md](CLAUDE.md); the full design document is [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Prerequisites

- Node.js 20.19+ (CI uses the version in [.nvmrc](.nvmrc))
- Rust 1.85+ with the Tauri 2 desktop toolchain for your OS
- On Debian/Ubuntu: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
libssl-dev libxdo-dev`

See the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for WebView2 (Windows),
WebKit (Linux) and Xcode CLT (macOS).

## Setup

```bash
git clone <your fork>
cd LocalMe
npm install
npm run tauri dev
```

Use `npm run tauri dev` for almost all work. `npm run dev` is the Vite UI without native commands,
so discovery, chat, the tray and persistence will not behave as in the packaged app.

## Everyday commands

| Command                                                 | Purpose                                              |
| ------------------------------------------------------- | ---------------------------------------------------- |
| `npm run tauri dev`                                     | Run the desktop app                                  |
| `npm run lint`                                          | ESLint, no warnings tolerated                        |
| `npm run format` / `format:check`                       | Prettier (100 width)                                 |
| `npm run typecheck`                                     | `vue-tsc`, strict                                    |
| `npm run test`                                          | Front-end unit tests (Vitest)                        |
| `npm run test:watch`                                    | Front-end unit tests, rerun on change                |
| `npm run test:coverage`                                 | Front-end tests + v8 coverage, gated by a threshold  |
| `npm run test:scripts`                                  | Version-tooling tests (`node --test`)                |
| `npm run build`                                         | `vue-tsc` + Vite production bundle                   |
| `npm run check:versions`                                | Confirm SemVer files and the changelog section match |
| `npm run version:patch` / `minor` / `major`             | Bump every version file and insert a changelog stub  |
| `npm run changelog`                                     | Print release notes for the current version          |
| `npm run release`                                       | Tag `vX.Y.Z` and push it to origin                   |
| `cargo test --workspace`                                | Rust unit tests and the two-instance loopback suite  |
| `cargo clippy --workspace --all-targets -- -D warnings` | Native lint                                          |
| `npm run tauri build`                                   | Packaged installer / binary                          |

Run `npm run build`, `npm run check:versions` and `cargo test --workspace` before you open a pull
request. GitHub Actions CI must stay green.

## How to make a change

1. Branch from `main` with a short, descriptive name.
2. Keep the change focused. Do not mix a feature with unrelated formatting.
3. Follow the import direction: `features/` → `ipc/` | `stores/` | `ui/` | `theme/` | `i18n/`.
   Features never import each other; `ui/` never imports anything above it.
4. Route every Tauri `invoke`/`listen` through `src/ipc/`. Components must not call the host
   directly.
5. Add or update both `en` and `ru` strings in `src/i18n/messages/` for any user-facing text.
6. Register new Rust commands in `src-tauri/src/lib.rs`, implement them in `commands.rs`, and extend
   `src/ipc/types.ts`.
7. Bump the app version with `npm run version:patch` (or `minor` / `major`) and replace the
   changelog stub with user-facing notes. `npm run check:versions` must pass.
8. Open a pull request describing what changed, why, and how you tested it.

## Code conventions

- **TypeScript/Vue:** PascalCase components, camelCase functions. No `any` in new code. Use the
  `@/` alias. Components use `<script setup lang="ts">`.
- **Rust:** `snake_case` functions and Tauri command names. Return a typed error converted at the
  boundary; do not panic on user-controlled input. `localme-core` must not depend on `tauri`.
- **Features:** kebab-case directories under `src/features/`.
- **Tests:** next to the module (`*.spec.ts`) or under a matching `tests/` directory.
- **Prettier:** 100 print width, single quotes, trailing commas. Rust uses default `rustfmt`.

The interface is custom Material Design 3 components over CSS tokens in `src/theme/`. Do not add a
component library or another state library without an explicit design change first.

## Security rules

- Treat every inbound frame as hostile: size, JSON shape, nickname, body and rate are checked before
  the domain sees anything.
- Never render message content as HTML — `{{ }}` only. `v-html` is banned by lint.
- Keep the Tauri capabilities in `src-tauri/capabilities/default.json` at least privilege.
- Do not log secrets or message contents at a level higher than trace.

## What to test

Exercise the path you touched, then the shared flows when the change is close to networking or
persistence:

- Discovery, dial, reconnect and the simultaneous-connect tie-break
- Send, the outbox (queued → sending → delivered), deduplication and the requeue on disconnect
- Presence transitions and the offline render
- Persistence across restart (peers, history, settings, window bounds)
- Tray close, native notification, single-instance focus
- English and Russian copy for new strings
- Light and dark theme if you changed chrome or tokens

A front-end spec sits beside the module as `<module>.spec.ts` and runs in the `node` environment
unless its first line declares `// @vitest-environment happy-dom`. Mount a component through
`mountView` from `@/test/mount`, build host DTOs with `@/test/factories`, and steer the media
queries with `@/test/matchMedia` — the harness in `src/test/` exists so a spec states only what it
checks. Mock `@/ipc` in any store or component spec; the boundary itself is tested against mocked
Tauri modules. `npm run test:coverage` enforces the thresholds in `vite.config.ts`, so a new module
without a test fails the build. On the Rust side, `cargo llvm-cov --workspace` measures the same
thing; the Tauri mock runtime is not usable in this project (see `docs/ARCHITECTURE.md` §9.2), so
host-side logic is covered where it does not need an `AppHandle`. Coverage of the whole
front end is described in `docs/ARCHITECTURE.md` §10.7.

The two-instance check on one machine:

```bash
LOCALME_DATA_DIR=/tmp/localme-a src-tauri/target/debug/localme
LOCALME_DATA_DIR=/tmp/localme-b src-tauri/target/debug/localme
```

## Pull requests

- Prefer small, reviewable diffs.
- Describe user-visible behaviour, not only file lists.
- Do not commit `dist/`, `src-tauri/target/`, `src-tauri/gen/`, credentials or OS temp files.
- Do not commit secrets.

Commit messages should say **why** the change exists in one or two sentences, using
`add` / `update` / `fix` language that matches the actual intent.

## Release

1. On the change itself, bump with `npm run version:patch` (or `minor` / `major`) and rewrite the
   changelog stub. `npm run check:versions` must pass.
2. Merge the change to `main`.
3. From a clean `main`:

```bash
npm run release -- --dry-run
npm run release
```

That tags `v` + the `package.json` version and pushes it. The tag message and the GitHub Release
body are the latest `[x.y.z]` section from [CHANGELOG.md](CHANGELOG.md). The Release workflow runs CI
first, then builds Windows, macOS and Linux installers and opens a **draft** GitHub Release. Review
the artifacts, then publish the draft.

Useful flags: `--dry-run`, `--no-push`, `--allow-branch`, `--allow-empty-notes`.

To build installers without a tag, run **Actions → Release → Run workflow**. Leave "Create a draft
GitHub Release" unchecked to upload artifacts only.
