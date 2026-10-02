<p align="center">
  <img src="src-tauri/icons/icon.png" width="96" height="96" alt="LocalMe">
</p>

<h1 align="center">LocalMe</h1>

<p align="center">
  <strong>A messenger for the local network.</strong><br>
  No server, no account, nothing to configure.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-59d5cc?style=flat-square" alt="MIT License"></a>
  <a href="../../actions/workflows/ci.yml"><img src="../../actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Vue-3-42B883?style=flat-square&logo=vuedotjs&logoColor=white" alt="Vue 3">
  <img src="https://img.shields.io/badge/Rust-2024-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/UI-7_languages-111716?style=flat-square" alt="7 UI languages">
</p>

<p align="center">
  <a href="#running-it">Running it</a> ·
  <a href="#how-it-is-built">How it is built</a> ·
  <a href="#protocol">Protocol</a> ·
  <a href="#ports-and-the-firewall">Ports</a> ·
  <a href="#troubleshooting">Troubleshooting</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

---

Launch LocalMe on two computers that share a network and they find each other. Messages are stored
locally on both ends, in SQLite, and one written while the other person is away waits in an outbox
and is sent, in order, as soon as they are back. The interface exists in English, Russian, Spanish,
German, French, Portuguese and Chinese. Built with
[Tauri 2](https://tauri.app) (Rust) and Vue 3. Architecture for contributors and coding agents:
[AGENTS.md](AGENTS.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Requirements

- **Runtime:** Windows 10+, macOS 11+, or a Linux desktop with WebKitGTK 4.1 and a
  StatusNotifier/AppIndicator host for the tray icon.
- **Build:** Rust 1.85 or newer, Node.js 20.19 or newer, and the
  [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform
  (`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev` and `patchelf` on
  Debian/Ubuntu).

## Running it

```sh
npm install
npm run tauri dev          # development, with hot reload for the front end
npm run tauri build        # release build and installers
```

Other commands:

```sh
npm run test               # front-end unit tests (vitest)
npm run lint               # eslint, no warnings tolerated
npm run typecheck          # vue-tsc, strict
cargo test --workspace     # Rust unit tests and the two-instance loopback suite
cargo clippy --workspace --all-targets -- -D warnings
```

`cargo test` lives in `src-tauri`; the integration suite there starts two complete instances on
the loopback interface and has them discover each other. Discovery is injected in those tests, so
they do not depend on multicast; the real-mDNS test is opt-in for the same reason:

```sh
cargo test -p localme-core --test loopback -- --ignored
```

### Two instances on one computer

Two copies are normally impossible: the second launch focuses the first window. To reproduce the
two-device scenario on one machine — which is how the discovery and messaging paths are checked
by hand — build once and run the binary twice with different data directories:

```sh
npm run build
cargo build --manifest-path src-tauri/Cargo.toml
LOCALME_DATA_DIR=/tmp/localme-a src-tauri/target/debug/localme &
LOCALME_DATA_DIR=/tmp/localme-b src-tauri/target/debug/localme &
```

On Windows use `%TEMP%\localme-a` and set the variable with `set` or `$env:`. The two instances
advertise different device ids, discover each other over mDNS within a few seconds, and each
keeps its own database.

## How it is built

The Rust side is split into two crates. `localme-core` holds everything with behaviour — the
domain types, the wire protocol, discovery, the TCP transport, storage and the service actors —
and has no dependency on Tauri at all. `localme` is the Tauri host: it owns the window, the
tray, the single-instance guard and the IPC surface, and contains no protocol logic. That split
is what lets the integration suite drive two full instances without a web view, and it means the
interesting code is testable with `cargo test` alone.

Inside the core, the domain is pure — identity, presence and the peer ordering are functions of
their inputs, with no clock of their own — and the ports (`Discovery`, `Store`) are traits, with
mDNS, the UDP beacon and SQLite as adapters behind them. All mutable network state lives in one
actor, the session, which owns the peer table and receives commands, transport events and
discovery events on one mailbox; there is no lock anywhere in it. The front end is Vue 3 with a
hand-built Material 3 token layer over the official colour utilities, Pinia stores that hold
state and nothing else, and a single typed IPC module: components never call the host directly.
Conversations and settings are routes rather than overlays, so the back button means what it
says, and the transitions between them use `motion-v` — switched off when the system asks for
reduced motion. The interface speaks English, Russian, Spanish, German, French, Portuguese and
Chinese.

## Protocol

Peers talk over TCP with length-prefixed JSON frames: a big-endian `u32` length followed by that
many bytes of UTF-8. A frame is at most 64 KiB, and a declared length outside that range closes
the connection before anything is allocated for it.

```jsonc
{ "v": 1, "t": "heartbeat", "seq": 7 }
```

`v` is the protocol version, `t` the frame type.

| `t`         | Direction         | Fields                                                            | Meaning                                              |
| ----------- | ----------------- | ----------------------------------------------------------------- | ---------------------------------------------------- |
| `hello`     | dialer → acceptor | `device_id, nickname, avatar_seed, listen_port, protocol_version` | open a session                                       |
| `welcome`   | acceptor → dialer | the same shape                                                    | accept, and exchange identity symmetrically          |
| `heartbeat` | both              | `seq`                                                             | liveness, one frame every 5 s                        |
| `chat`      | both              | `id, body`                                                        | a message; the sender is the connection, not a field |
| `chat_ack`  | both              | `id`                                                              | "stored here" — sent only after the row is committed |
| `profile`   | both              | `nickname, avatar_seed`                                           | a rename, broadcast to every live connection         |
| `goodbye`   | both              | `reason`                                                          | `shutdown`, `superseded` or `error`                  |
| `error`     | both              | `code, message`                                                   | a protocol error, then the connection closes         |

A peer is online when discovery has seen it, a connection to it is established and it is still
sending heartbeats; silence for 15 s (three intervals) marks it offline. When two peers dial each
other at the same moment, the connection opened by the peer with the smaller device id wins and
the other is closed by both ends — a rule both sides evaluate identically, so exactly one
survives. Message identifiers are UUID v7, which makes the primary key a time order and lets a
retransmitted frame be absorbed by `INSERT OR IGNORE`; the acknowledgement is sent after that
insert, which is what makes "delivered" mean durably stored rather than written to a socket.

An outgoing message is written to the local outbox before it is attempted, which is what lets the
composer accept a message while the other person is offline: the row stays `queued` and is written
out, oldest first, once a connection exists. Draining is paced to stay under the recipient's inbound
rate limit, and a connection that ends before the acknowledgement returns its in-flight rows to the
outbox, so nothing is lost and the backlog survives a restart. A message that had to wait shows two
dates — when it was written and when it was delivered — because both come from the same local clock.

Discovery is DNS-SD over mDNS (`_localme._tcp.local.`), with a UDP beacon on port 47821 —
broadcast and multicast — for networks that filter mDNS. Peers announced by both mechanisms are
de-duplicated by device id, and our own device id is filtered out at the boundary.

## Ports and the firewall

| Port  | Protocol | Purpose                                                                                      |
| ----- | -------- | -------------------------------------------------------------------------------------------- |
| 47820 | TCP      | peer connections. If it is taken, an ephemeral port is used and advertised through discovery |
| 47821 | UDP      | the discovery beacon, broadcast to `255.255.255.255` and multicast to `239.255.77.77`        |
| 5353  | UDP      | mDNS, shared with the operating system's own responder                                       |

**Windows** shows a firewall prompt the first time the application listens. Accept it for private
networks. If you dismissed it, add an inbound rule:

```powershell
New-NetFirewallRule -DisplayName "LocalMe" -Direction Inbound -Profile Private `
  -Protocol TCP -LocalPort 47820 -Action Allow
New-NetFirewallRule -DisplayName "LocalMe discovery" -Direction Inbound -Profile Private `
  -Protocol UDP -LocalPort 47821 -Action Allow
```

**macOS** asks once, the first time an inbound connection arrives. Approve it in
System Settings → Network → Firewall → Options.

**Linux** depends on the distribution's firewall. With `firewalld`, put the interface in the
`home` or `work` zone, where these ports are reachable within the zone.

## Performance

Measured on Windows 11 (x64) with a release build — `opt-level = "s"`, LTO, `codegen-units = 1`,
`strip`, `panic = "abort"` — with the window hidden in the tray, one peer known and offline, and
nothing else running. The process tree is the application plus the WebView2 processes it owns.

|                                                 |                                                      |
| ----------------------------------------------- | ---------------------------------------------------- |
| CPU, one minute idle                            | 0.016 s — 0.03 % of one core, 0.002 % of the machine |
| Private memory, `localme.exe` (host + core)     | 6.4 MB                                               |
| Private memory, the WebView2 tree (6 processes) | ~165 MB                                              |
| Working set, whole tree                         | ~388 MB                                              |
| Release binary on disk                          | 6.4 MB                                               |
| Windows installers                              | 3.1 MB (MSI), 2.3 MB (NSIS)                          |
| Front-end bundle                                | 85 KB gzipped JavaScript, 7 KB gzipped CSS           |

Two things to keep in mind when reading those numbers. Working set counts a shared page once per
process, so summing it across a seven-process tree overstates what the machine actually gives up;
private bytes are the honest figure. And almost all of that memory is WebView2 rather than this
application: the Rust host and the whole core are 6.4 MB, and the web view is the price of any
Tauri application. Keeping the web view alive for a window nobody is looking at is the one part of
this design a different trade-off could move — `docs/ARCHITECTURE.md` §13.8 records why it was not
moved, and what it would cost.

The idle CPU figure is the interesting one: nothing polls. Between heartbeats there is a 2 s
presence tick, a 5 s heartbeat per connected peer, and a 3 s or 20 s discovery beacon — all of
which are sub-millisecond wake-ups that touch no front-end code while the window is hidden.

To reproduce:

```sh
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --release --features custom-protocol
LOCALME_DATA_DIR=/tmp/localme-measure src-tauri/target/release/localme --minimized
```

then measure the process tree over a minute of idle.

## Troubleshooting

**The other computer never appears.** In order of likelihood:

1. **Client isolation.** Many guest, hotel and corporate Wi-Fi networks prevent clients from
   talking to each other at all — the access point drops the traffic. Nothing in this
   application can work around that; check whether the two machines can `ping` each other, and
   if they cannot, use a wired network, a phone hotspot, or a network without isolation.
2. **A firewall on one side is dropping the traffic.** Outbound connections and inbound
   connections both have to be allowed. Test by temporarily disabling the firewall on one
   machine: if the peer appears, add the rule rather than leaving it off.
3. **A VPN is capturing the traffic.** A full-tunnel VPN routes everything, including LAN
   traffic, through the tunnel, so the two machines end up on different logical networks.
   Discovery then either finds nothing or finds peers it cannot reach. Exclude the local subnet
   from the VPN's routes, or turn the VPN off while using LocalMe.
4. **The two machines are on different subnets.** mDNS and the beacon are link-local: they do
   not cross a router. This includes "the same Wi-Fi" when one machine is on the 2.4 GHz SSID and
   the other on a different VLAN, and it includes virtual adapters — Docker, Hyper-V, WSL,
   VirtualBox and Tailscale each create networks that look local but are not the one you meant.
5. **The peer is shown but offline.** The device is being announced but the connection is not
   completing, which is almost always the TCP port being blocked rather than the UDP discovery
   ports. Check the firewall rule for 47820.

**The database was damaged.** On startup LocalMe runs `PRAGMA integrity_check`. If the file is
unusable it is renamed to `localme.db.corrupt-<timestamp>` beside the original and a fresh
database is created; the application tells you the name of the preserved file, so nothing is
lost silently. The data directory is:

| Platform | Path                                                |
| -------- | --------------------------------------------------- |
| Windows  | `%APPDATA%\dev.localme.desktop`                     |
| macOS    | `~/Library/Application Support/dev.localme.desktop` |
| Linux    | `~/.local/share/dev.localme.desktop`                |

**Reporting a problem.** LocalMe writes one log file per day to `logs/` inside that data
directory (`localme.YYYY-MM-DD.log`; the day is UTC). Settings → Logs shows the folder, how much
room the files take, how many days to keep and how much detail to record, and can open the folder
or delete the files — including an error thrown by the interface itself, which is written to the
same file because a packaged build has no console to show it in. Logging out of the box keeps a
fortnight of records and never grows past that without being asked.

**Two instances on one machine.** The second launch focuses the first window and exits. That is
deliberate: two copies would fight over the same database and advertise the same device id.

**The tray icon is missing on Linux.** GNOME does not show tray icons without an extension
("AppIndicator and KStatusNotifierItem Support"). Without it the application still runs and still
receives messages; only the tray menu is unavailable.

**Notification click behaviour.** On desktop, the notification plugin does not report clicks back
to the application — it documents that action-related fields are ignored. Clicking the **tray
icon** instead raises the window and opens the conversation that notified most recently, which is
the closest equivalent available on all three platforms.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow and [AGENTS.md](AGENTS.md) for the project
map. Every shipped change bumps the version and adds a [CHANGELOG.md](CHANGELOG.md) section;
`npm run check:versions` keeps the version files honest.

## Licence

[MIT](LICENSE). The application icon is generated from `tools/make-icon.mjs`; avatars come from
[blobatar](https://github.com/Alain00/blobatar) (MIT).
