//! Window policy: the window label, the readiness gate, the close-to-tray rule, the title and
//! the colour of the native frame.

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

/// Paints the native title bar in the accent colour.
///
/// The palette lives in the front end — Material's tonal algorithm runs there — so the host is
/// handed the resolved pair rather than an accent to reason about: `caption` is the bar and the
/// frame around the window, `text` the glyphs drawn on it, and the two must contrast.
///
/// Windows 11 draws the frame itself and colours it from `DWMWA_CAPTION_COLOR`, with
/// `DWMWA_BORDER_COLOR` for the outline and `DWMWA_TEXT_COLOR` for the label. A build older than
/// 22000 rejects those attributes and keeps its own frame, which is also what happens on any
/// other platform: the window manager owns the frame there and offers no such hook, so this is
/// a documented no-op rather than a second, non-native title bar.
#[cfg(windows)]
pub fn set_accent<R: Runtime>(app: &AppHandle<R>, caption: [u8; 3], text: [u8; 3]) {
    use windows::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DwmSetWindowAttribute,
    };

    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::warn!("main window is missing; the title bar keeps the system colour");
        return;
    };
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd,
        Err(error) => {
            tracing::warn!(%error, "the window has no native handle to paint");
            return;
        }
    };

    for (attribute, channel_order) in [
        (DWMWA_CAPTION_COLOR, caption),
        (DWMWA_BORDER_COLOR, caption),
        (DWMWA_TEXT_COLOR, text),
    ] {
        let color = colorref(channel_order);
        // SAFETY: `hwnd` is this process's live window handle, reached from the event loop;
        // `color` is a `u32` that outlives the call, and each attribute above takes exactly one
        // `COLORREF`, so the pointer and the size the call is told to read match.
        let result = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                attribute,
                std::ptr::from_ref(&color).cast(),
                size_of::<u32>() as u32,
            )
        };
        if let Err(error) = result {
            // Expected on a Windows build that predates the attribute; the system frame stays.
            tracing::debug!(%error, attribute = attribute.0, "the frame colour was refused");
        }
    }
}

/// A `COLORREF` — `0x00BBGGRR` — from the `#RRGGBB` order the interface speaks.
#[cfg(windows)]
fn colorref([red, green, blue]: [u8; 3]) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

/// Away from Windows the window manager draws the frame and decides its colour; there is no
/// application-facing hook, so the palette stops at the edge of the web view.
#[cfg(not(windows))]
pub fn set_accent<R: Runtime>(_app: &AppHandle<R>, _caption: [u8; 3], _text: [u8; 3]) {}

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
