//! Window policy: the window label, the readiness gate, the close-to-tray rule and the title.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Listener, Manager, Runtime};

use crate::events;
use crate::state::{self, AppState};

/// Label of the single application window, as declared in `tauri.conf.json`.
pub const MAIN_WINDOW: &str = "main";

/// Event the front end emits once Vue has mounted and the theme is painted.
///
/// Kept in sync with `MAIN_WINDOW_READY_EVENT` in `src/app/ready.ts`.
pub const READY_EVENT: &str = "localme://ready";

/// How long to wait for the front end before showing the window anyway.
///
/// Without this, a JavaScript error during startup would leave a running process with no
/// visible window — the worst possible failure mode, because there is nothing for the user to
/// click and nothing to report.
const READY_FALLBACK: Duration = Duration::from_secs(15);

static REVEALED: AtomicBool = AtomicBool::new(false);

/// Shows the main window and gives it focus. Idempotent.
pub fn reveal<R: Runtime>(app: &AppHandle<R>) {
    REVEALED.store(true, Ordering::Relaxed);

    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::warn!("main window is missing");
        return;
    };
    if let Err(error) = window.show().and_then(|()| window.set_focus()) {
        tracing::warn!(%error, "failed to show the main window");
        return;
    }

    let Some(state) = state::from_handle(app) else {
        return;
    };
    let was_hidden = !state.window_visible.swap(true, Ordering::Relaxed);
    if was_hidden {
        // Everything that happened while the window was in the tray is delivered as one
        // snapshot rather than replayed: the web view draws from state, not from a log.
        events::emit_snapshot(app);
    }

    // A window raised after a notification should land on the conversation that notified,
    // which is what the user is looking for. Desktop notifications do not report clicks back
    // to us on every platform, so this is the documented substitute
    // (`docs/ARCHITECTURE.md` §12).
    if let Some(peer) = state.take_last_notified() {
        events::emit_open_chat(app, peer);
    }
}

/// Hides the main window, leaving the process and its connections alive.
pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "failed to hide the main window");
    } else if let Some(state) = state::from_handle(app) {
        state.window_visible.store(false, Ordering::Relaxed);
    }
}

/// Whether the window has already been shown.
#[must_use]
pub fn is_revealed() -> bool {
    REVEALED.load(Ordering::Relaxed)
}

/// Updates the window title so the unread count is visible without opening the window.
pub fn refresh_title<R: Runtime>(app: &AppHandle<R>, unread: u32) {
    let Some(state) = state::from_handle(app) else {
        return;
    };
    let labels = state.labels_snapshot();
    if let Some(window) = app.get_webview_window(MAIN_WINDOW)
        && let Err(error) = window.set_title(&labels.window_title(unread))
    {
        tracing::debug!(%error, "failed to update the window title");
    }
}

/// Wires the readiness gate.
///
/// With `--minimized` (an autostart launch) the window is never revealed at startup: the
/// application starts in the tray and the user opens it when they want it.
pub fn install_ready_gate<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    app.listen(READY_EVENT, move |_event| {
        if crate::started_minimized() {
            tracing::info!("front end is ready; staying in the tray as requested");
            REVEALED.store(true, Ordering::Relaxed);
            return;
        }
        tracing::info!("front end is ready; revealing the window");
        reveal(&handle);
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(READY_FALLBACK).await;
        if is_revealed() {
            return;
        }
        tracing::warn!(
            timeout_seconds = READY_FALLBACK.as_secs(),
            "front end did not report readiness; showing the window anyway"
        );
        reveal(&handle);
    });
}

/// Decides what a close request means.
///
/// Returns `true` when the window should hide and the application keep running. The setting is
/// read from the host's cache, which mirrors the settings actor, so this stays a synchronous
/// decision on the event loop rather than a round trip through a channel.
#[must_use]
pub fn should_stay_in_tray(state: &AppState) -> bool {
    state.settings_snapshot().system.close_to_tray
}
