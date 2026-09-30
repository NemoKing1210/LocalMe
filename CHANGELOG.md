# Changelog

All notable changes to LocalMe are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.8] - 2026-10-01

### Removed

- The profile card in settings no longer prints the device identifier or the paragraph explaining
  it; the value is still shown in the diagnostics block, where copying it for a bug report is the
  point

## [0.1.7] - 2026-10-01

### Added

- The search field in the people list clears itself with a button that appears once there is
  something to clear, and returns the caret to the field

## [0.1.6] - 2026-10-01

### Fixed

- Focusing a text field no longer opens the web view's saved-info autofill dropdown; the WebView2
  feature is turned off at startup instead of being left to Microsoft Edge's defaults

## [0.1.5] - 2026-10-01

### Fixed

- An empty text field's label is centred in the field again instead of sitting on the line the
  typed text uses; the floating label keeps its clear gap above the text

## [0.1.4] - 2026-10-01

### Changed

- Text fields keep a clear gap between the floating label and the typed text
- The focus indicator on a text field is a 2 px ring instead of a hairline that disappeared into
  the border, and the whole 56 px field — not just the text line — is a click and focus target

## [0.1.3] - 2026-10-01

### Changed

- The button that opens settings now uses a sliders glyph instead of the cog, which blurred into
  a ring at 24 px; the button itself, its label and its position are unchanged

## [0.1.2] - 2026-10-01

### Changed

- Right-clicking no longer opens the webview's own context menu (Reload, Save image, Inspect);
  clipboard and devtools keyboard shortcuts are unaffected

## [0.1.1] - 2026-10-01

### Added

- Project guide for contributors and coding agents (`AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`)
  and a `CHANGELOG.md`
- Version tooling: `check:versions`, `changelog`, `version:patch` / `minor` / `major` and
  `release` keep every version file and the changelog section in sync
- Tagged releases build Windows, macOS and Linux installers and open a draft GitHub Release
- Reusable GitHub Actions setup action, Dependabot configuration and a version-sync CI job

### Changed

- CI is a quality gate of front end, core (Windows, macOS, Linux) and version checks behind a
  single aggregate status; installer builds moved to the release workflow
- README carries a logo, badges and navigation links; MIT license added

## [0.1.0] - 2026-10-01

Initial release.

### Added

- Zero-configuration peer discovery on a local network: DNS-SD over mDNS with a UDP beacon
  fallback for networks that filter multicast
- A length-prefixed JSON protocol over TCP with heartbeats, a deterministic simultaneous-connect
  tie-break, delivery acknowledgements and per-connection rate limiting
- Local SQLite storage for peers and message history, with a forward-only migration and
  corrupt-file preservation
- 1:1 text chat with delivery status, unread counts, paging and a virtualised message list
- Presence tracking derived from discovery, connection and heartbeat signals
- System tray with unread tooltip, close-to-tray and start-minimised behaviour
- Native notifications with per-peer muting
- Settings screen: profile, appearance, locale, notifications, system and data
- Material Design 3 token layer with accent-colour palette generation and light/dark/system themes
- Interface languages: English and Russian
- Two-crate Rust workspace: a Tauri host and a Tauri-free core, so the protocol and services are
  testable without a web view
