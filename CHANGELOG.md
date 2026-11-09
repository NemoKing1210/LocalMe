# Changelog

All notable changes to LocalMe are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.1] - 2026-11-09

### Fixed

- **Autostart could be registered from a development build.** `npm run tauri dev` (and a release
  build pointed at a test `LOCALME_DATA_DIR`) wrote its own executable into the login entry, so a
  machine could try to start `target\debug\localme.exe` at sign-in. Only a packaged build manages
  the entry now; `LOCALME_NO_AUTOSTART` is an explicit opt-out.
- **A stale autostart path was never repaired.** The stored setting was overwritten from whatever
  the platform reported, so an entry left pointing at an old or moved executable stayed broken.
  The setting is now the user's intent and the startup reconciliation rewrites the registration,
  including one left with the old `--minimized` argument.
- **Autostart did not work from an installation path containing a space.** The plugin's
  `auto-launch` crate stores the executable path unquoted; Windows then split it at the first
  space. The login value is written directly on Windows, with the path quoted.
- **Uninstalling left the login entry behind.** The NSIS uninstaller now removes both the
  `HKCU\…\Run` value and its Task Manager startup-approval record.
- A refused autostart change no longer saves the new value first: the switch and the settings
  document keep the previous value when the platform rejects the registration.

## [0.12.0] - 2026-11-09

### Added

- **A viewer for every picture.** Clicking an image — in a message or on a file still waiting in
  the composer — opens it on its own, where it can be zoomed with the wheel, a trackpad pinch,
  the toolbar or the `+` and `-` keys, moved by dragging, turned in quarter turns with the
  toolbar or `R`, and fitted again with the Fit button or `0`. A double click zooms to detail
  and the next one brings the whole picture back; Escape, the close button and a click beside
  the picture close it. The image is shown as a modal dialog, so it is never clipped by a
  bubble's entrance and the keyboard stays inside it.

## [0.11.0] - 2026-11-09

### Added

- **Attachments from the clipboard.** Ctrl+V in the composer now attaches whatever the file
  manager copied, and a screenshot or any copied image becomes an attachment of its own. The
  clipboard is read by the host, because the web view is given a file with no path and an image
  with no file at all; a copied image is written once under the data directory, so the message
  that sends it keeps its preview, and it is deleted if the user takes it out before sending.
  Pasting prose still pastes prose, unchanged.

## [0.10.2] - 2026-11-09

### Changed

- **The window's title bar is now the application's own, not Windows'.** The native frame is
  removed on Windows and the interface draws the bar instead: Material-style minimise,
  maximise/restore and close buttons on the same surfaces and accent as everything below them,
  in light and dark mode, with the rest of the strip free for dragging. In the two-pane layout
  the bar sits above the conversation only, so the people column runs all the way to the top of
  the window instead of starting under an empty strip. It remains a normal window — dragging
  snaps it, double-clicking maximises, the edges resize, and closing still honours the "keep
  running in the tray" setting. Other platforms keep their native frame.

## [0.10.1] - 2026-11-09

### Changed

- **Cursor's workspace folder is ignored by git**, so editor-local rules and settings stay out
  of the repository instead of showing up as untracked files

## [0.10.0] - 2026-11-09

### Added

- **Automatic language.** The interface can now follow the operating system's language instead
  of a fixed choice, and switches with it while LocalMe is running. Languages the interface does
  not ship fall back to English. New installs start in this mode; a language picked by hand stays
  picked.

## [0.9.3] - 2026-11-09

### Changed

- **The composer is quieter.** The line spelling out Enter and Shift+Enter is gone; the keyboard
  still works the same, and the footer now appears only to warn about length or show the counter

## [0.9.2] - 2026-10-03

### Added

- **A settings screen that is one screen per concern** — profile, appearance, language,
  notifications, system, data, logging and about are their own components, so a change in one
  cannot disturb another
- **Error messages in the interface's own language** — a failure the host can name is now shown
  as the sentence from the catalogue, chosen by the host's own error tag, instead of the English
  text the wire carries
- Friendly names for the accent swatches, read out by a screen reader (`settings.accentName.*`)

### Fixed

- **History paging loaded the wrong page.** The host returns a page newest-first and the store
  treated its first row as the oldest, so "load earlier messages" re-fetched what was already on
  screen and never reached the beginning of a conversation
- **An interrupted file transfer could stall for ever, or trip the rate limit and start over.**
  An acknowledgement is a statement of what is on the recipient's disk, and the sender was
  treating it as an opinion: a stale one could drag the stream backwards, so the sender re-sent a
  window the recipient already had — every chunk refused, fast enough that the refusal traffic hit
  the recipient's inbound rate limit and dropped the connection. And because the window stayed
  measured from an older acknowledgement, a sender whose file was already complete on the
  recipient's disk had a full window and sent nothing at all: not even the `file_done` that
  finishes the transfer. Corrections now only ever move the stream forward, and the reported
  position becomes the window's floor
- **A nickname of 28 characters or more made a device unreachable.** The avatar seed derived from
  the device id and the nickname could exceed the seed's own limit, so the peer was dropped by
  discovery and refused at the handshake
- **`start_minimized` did nothing.** The setting was stored and shown but never read; a start with
  the system always stayed in the tray and a normal start always showed the window
- **A lowered log retention did not take effect until the next day**, because the startup prune
  had already stamped the day
- The remembered address of a peer is no longer wiped by a rename or a re-discovery, so the
  preferred dial address survives
- A message whose identifier belongs to another conversation is no longer acknowledged as
  delivered while being dropped
- A peer cannot open an unbounded number of part files: at most 16 transfers may arrive at once
- The window is only hidden at startup when the tray exists — a tray-only start on a desktop
  without a tray would have left no way back
- Attachment state and progress writes that fail are logged instead of discarded silently
- The ready signal is sent once, after the first paint, instead of twice

### Changed

- A failed startup is written to the daily log file as well as the browser console, with its
  stack: the error screen is exactly what a user reports, and until now it left no trace in the
  log they can open from the settings screen
- **File transfers no longer block the session.** Chunk reads, chunk writes and the inspection of
  a file about to be sent run on the blocking pool, so a slow disk cannot stall message delivery,
  acknowledgements or presence for every peer
- **The outbox drains in batches**: one query for a batch of waiting messages and one transaction
  to mark them in flight, instead of a query and a round trip per message
- **Frames are encoded and decoded once.** The version check and the frame decode share a single
  JSON parse, the encoder writes into one buffer, and the decoder no longer copies the whole
  remainder of its buffer for every frame
- One clock for the whole interface: every row shares one timer and one visibility listener
- The file picker and the save dialog are asynchronous, so a modal dialog no longer freezes the
  window, the tray and every other command while it is open
- The file-transfer state machine lives in `services/session/transfer.rs` beside the actor, and
  the chunk and completion handlers share one validator
- Every discovery cache is bounded, and the beacon decodes a limited number of datagrams per
  second, so one host on the network cannot grow them without bound
- Stored timestamps are validated on the wire: a message id must be a UUIDv7
- The bundle is split by library, keeping the entry chunk under the audit threshold

### Removed

- Seven commands and two events nothing called: `mark_read`, `is_autostart_enabled`,
  `show_window`, `hide_window`, `quit` and the `own_profile`/`onboarding_complete` events
- Unused icons, unused catalogue strings (and the plural families that went with them), and the
  store fields no component read
- The `process` plugin, the unused `thiserror` dependency in the host crate and six npm packages
  no module imports
- Most of `capabilities/default.json`: the web view is granted what it invokes (`core:default`
  and `os:allow-hostname`), and nothing for the notifications, dialogs, autostart and window
  policy that Rust drives directly

## [0.9.1] - 2026-10-02

### Added

- A test suite for the interface: the stores, the IPC boundary, the theme engine, the
  composables, the chat, people, onboarding and settings screens, and the design-system
  components. `npm run test` runs it; `npm run test:watch` reruns on change
- A shared test harness in `src/test/` — a mount helper that installs the real Pinia, router and
  i18n, DTO factories, a controllable `matchMedia`, and a per-test reset — so a new test states
  only the behaviour it checks
- Front-end coverage measurement and a gate: `npm run test:coverage` fails below the thresholds
  in `vite.config.ts`, so a new module with no test fails the build rather than going unnoticed
- Wider Rust coverage: the session actor (outbox drain, presence, every command and attachment
  path), the connection handshake and pump, discovery (beacon codec, mDNS translation, the
  composite source), the storage migrations and queries, and the host's Tauri-free state and
  window policy

### Changed

- CI measures both sides: the front end through `npm run test:coverage`, the Rust workspace
  through `cargo llvm-cov`

## [0.9.0] - 2026-10-02

### Added

- Send files, on their own or with a message: pick them with the paperclip, or drop them
  anywhere on the window. Pictures are shown in the conversation, everything else as a card
  with its name and size
- Every attached file is shown with an icon for its type — a PDF, a spreadsheet, an archive, a
  video — so a conversation full of files can be read at a glance rather than line by line
- Transfers show their progress, can be cancelled, and can be started again after a failure
- A file that is interrupted — the other computer goes away, or the application is closed —
  continues where it stopped when the conversation resumes, instead of starting over, and the
  received file is checked against the sender's digest before it is kept
- Received files can be opened, saved wherever you like, or shown in the file manager

### Changed

- Messages may now be files with no text at all; the message list and the notifications name
  the file in that case
- The database schema is version 3: a message body is optional, and attachments have their own
  table. An existing database is migrated on first start
- The wire protocol is version 2. Two installations must both be updated: a version 1 peer is
  politely refused rather than being sent frames it cannot read

### Fixed

- A peer no longer drops offline fifteen seconds after the last message: the liveness beats the
  two ends exchange were not reaching the part of the application that decides whether a peer is
  still there, so every idle conversation was declared stalled and reconnected, over and over
- Two instances on the same network could fall into a loop of connecting and dropping each
  other when both started dialling at the same moment: the connection that lost the tie-break
  took the surviving one down with it
- A picture you had sent showed as a broken placeholder after restarting the application: the
  permission to display a file was granted when it was chosen and forgotten on the next start

## [0.8.0] - 2026-10-02

### Added

- The tray menu is now a status view of the messenger: it shows the unread count and a
  Conversations submenu listing the recent conversations that are waiting, and opens the one
  you pick
- The tray can mark every conversation as read, open the settings screen, and open the log
  folder
- Notifications, close-to-tray and start-with-the-system can be switched from the tray; each
  shows a check mark for its current state and stays in sync with the settings screen

### Changed

- The tray's notification entry is now a checked "Notifications" switch instead of a text that
  said "Pause notifications" or "Resume notifications"

## [0.7.2] - 2026-10-02

### Changed

- The Language section in the settings no longer shows the note that the change applies immediately

## [0.7.1] - 2026-10-02

### Changed

- The Russian interface now calls the action that removes a person from the list "Удалить" instead of "Забыть"

## [0.7.0] - 2026-10-02

### Added

- Message text can be selected: a drag that starts anywhere in a bubble — its padding included —
  now selects the words, and Ctrl+C copies them as before
- Right-clicking a message opens a menu with "Copy message", or "Copy selection" when part of that
  message is already selected; a snackbar confirms the copy

## [0.6.1] - 2026-10-02

### Changed

- The notice that the other person is away is now a tonal banner with an icon above the message
  field, instead of a line of grey text

### Fixed

- Messages no longer sit against the edge of the window: the conversation keeps the same side
  gutter as the message field

## [0.6.0] - 2026-10-02

### Added

- Messages can be written to a person who is offline: they wait in an outbox and are sent, in the
  order they were written, as soon as the person is back, and they survive a restart
- A message that had to wait shows two times — when it was written and when it was delivered
- The delivery status of an outgoing message is now visible while it waits ("Waiting to send") and
  its delivery time is recorded when the acknowledgement arrives

### Changed

- Sending no longer fails when the other person is offline; the composer stays usable and the row
  waits instead of being refused
- Delivery is retried across reconnects rather than given up on: a connection that ends before the
  acknowledgement returns its in-flight messages to the outbox
- The outbox is drained at a pace the recipient's rate limit accepts, so a large backlog arrives
  instead of tripping the limit and being cut off

### Removed

- The `failed` and `sent` message states: an outgoing message is queued, in flight or delivered,
  and messages left in the old states by an earlier version are retried by the upgrade

## [0.5.3] - 2026-10-01

### Changed

- The contributor guide now asks for comments only where the code does not explain itself, instead
  of one on every change
- Comments across the codebase were trimmed to that rule: narrative, redundant and obvious
  comments are gone, and only notes about a real constraint, invariant or tradeoff remain

## [0.5.2] - 2026-10-01

### Changed

- The per-person overflow menu in the people list is now shown only while the row is hovered or
  focused, instead of a button on every row; with the button gone its width is returned to the
  name and the preview, which no longer ellipsise early
- The same menu opens on a right-click anywhere in a row, and on the keyboard's context-menu key,
  not only from the dots

### Fixed

- Opening the overflow menu no longer scrolls the people list or grows its scroll area: the menu
  is painted fixed and out of the list's flow, and the focused item does not drag the list to it

## [0.5.1] - 2026-10-01

### Changed

- A text field with no hint, error or counter no longer leaves an empty row under the input, so
  the people search sits flush against the list

## [0.5.0] - 2026-10-01

### Added

- Avatars animate where a single one is on screen: the profile picture in Settings and the
  avatar of the open conversation breathe, bob and blink on their own, and the known-devices
  list animates on hover. The people list keeps its static avatars, and all motion is dropped
  when the system asks for reduced motion.

## [0.4.0] - 2026-10-01

### Added

- The native title bar is painted in the accent colour, with the label on it in the matching
  contrast colour, and follows the accent and the light/dark mode as they change. The operating
  system still draws the frame, so the title, the window buttons and snap behaviour are unchanged.
  On Windows 11 this is the system title bar; on an older Windows and on macOS and Linux the
  window manager owns the frame and keeps its own colour.

## [0.3.1] - 2026-10-01

### Changed

- The Russian interface calls the people list «Пользователи» instead of «Люди», including the
  search placeholder and the per-person notification hint

## [0.3.0] - 2026-10-01

### Added

- The people list shows the newest message of each conversation under the name, prefixed with
  "You:" when it was the reader who sent it

## [0.2.4] - 2026-10-01

### Changed

- The floating label in a text field now rises out of the way as soon as the field is focused,
  not only once it contains text, so an empty search or nickname box shows where to type

## [0.2.3] - 2026-10-01

### Changed

- Avatar frames are now rounded squares rather than circles: the squircle backdrop blobatar
  draws is no longer clipped away by a full corner radius

## [0.2.2] - 2026-10-01

### Fixed

- A long people list no longer pushes the layout past the bottom of the window; the list column
  stays one window tall and scrolls instead of overflowing

## [0.2.1] - 2026-10-01

### Added

- Moving between two conversations is animated: the next one slides in from the side as the
  previous one leaves, instead of the two swapping in place
- A conversation whose history is still being read shows bubble-shaped placeholders where the
  messages will be, on the side each one belongs to, rather than a spinner in an empty log

### Removed

- The circular progress indicator. The message log was its last caller, and a placeholder with
  the shape of what is arriving says more than a spinner does

## [0.2.0] - 2026-10-01

### Added

- A daily log file, under `logs/` in the data directory, with the level of detail and the number of
  days to keep chosen in settings. The same screen shows where the files are, how much room they
  take, opens the folder in the file manager and deletes them — and an error thrown by the
  interface itself is written to the same file, so a problem survives the window being closed
- Five more interface languages: Spanish, German, French, Portuguese and Chinese, alongside English
  and Russian
- Chats and settings are pages with their own address now, so the back gesture and the back button
  return to where the user came from instead of the window forgetting it
- Motion throughout: page changes, the people list, a message arriving, its delivery status, unread
  badges, the theme change itself, and the first-run screen handing over to the conversation list.
  All of it is switched off when the operating system asks for reduced motion

### Changed

- The settings document gains a `logging` group (schema version 2); a file written by an earlier
  version is read and upgraded, with the default level and a fourteen-day retention

### Fixed

- The people list no longer empties itself a few seconds after launch, and a message that arrives
  while the application is running appears in the conversation immediately. The host was sending
  every event wrapped in its own name (`{"peers":{"peers":[…]}}`), so the interface read an empty
  list and an undefined message from each one; the payloads are now the shapes the interface
  declares, and a test pins each of them

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
