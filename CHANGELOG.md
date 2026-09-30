# Changelog

All notable changes to LocalMe are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
