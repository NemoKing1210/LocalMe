//! Window policy: the window label, and the readiness gate that decides when the window
//! becomes visible.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Listener, Manager};

/// Label of the single application window, as declared in `tauri.conf.json`.
pub const MAIN_WINDOW: &str = "main";

/// Event the front end emits once Vue has mounted and the theme is painted.
///
/// Kept in sync with `MAIN_WINDOW_READY_EVENT` in `src/app/ready.ts`.
pub const READY_EVENT: &str = "localme://ready";

/// How long to wait for the front end before showing the window anyway.
///
/// Without this, a JavaScript error during startup would leave a running process with no
/// visible window — the worst possible failure mode, because there is nothing for the user
/// to click and nothing to report.
const READY_FALLBACK: Duration = Duration::from_secs(15);

static REVEALED: AtomicBool = AtomicBool::new(false);

/// Shows the main window and gives it focus. Idempotent.
pub fn reveal(app: &AppHandle) {
    REVEALED.store(true, Ordering::Relaxed);

    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::warn!("main window is missing");
        return;
    };
    if let Err(error) = window.show().and_then(|()| window.set_focus()) {
        tracing::warn!(%error, "failed to show the main window");
    }
}

/// Whether the window has already been shown.
#[must_use]
pub fn is_revealed() -> bool {
    REVEALED.load(Ordering::Relaxed)
}

/// Wires the readiness gate.
///
/// With `--minimized` (an autostart launch) the window is never revealed at startup: the
/// application starts in the tray and the user opens it when they want it.
pub fn install_ready_gate(app: &AppHandle) {
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
