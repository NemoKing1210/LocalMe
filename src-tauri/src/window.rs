use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Listener, Manager, Runtime};

use crate::events;
use crate::state::{self, AppState};

/// Window label, matching `tauri.conf.json`.
pub const MAIN_WINDOW: &str = "main";

/// Kept in sync with `MAIN_WINDOW_READY_EVENT` in `src/app/ready.ts`.
pub const READY_EVENT: &str = "localme://ready";

/// Fallback so a front-end error during startup cannot leave a running process with no visible
/// window to click or report.
const READY_FALLBACK: Duration = Duration::from_secs(15);

static REVEALED: AtomicBool = AtomicBool::new(false);

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
        // Everything missed while hidden is delivered as one snapshot, not a replayed backlog.
        events::emit_snapshot(app);
    }

    // Notifications do not report clicks back on every platform, so open the chat that notified
    // last instead.
    if let Some(peer) = state.take_last_notified() {
        events::emit_open_chat(app, peer);
    }
}

/// Paints the native title bar `caption` with `text` glyphs; a no-op on platforms (and pre-22000
/// Windows builds) whose window manager owns the frame.
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
            // Expected on Windows builds that predate the attribute; the system frame stays.
            tracing::debug!(%error, attribute = attribute.0, "the frame colour was refused");
        }
    }
}

/// A `COLORREF` — `0x00BBGGRR` — from the `#RRGGBB` order the interface speaks.
#[cfg(windows)]
fn colorref([red, green, blue]: [u8; 3]) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

/// No hook away from Windows: the window manager draws and colours the frame.
#[cfg(not(windows))]
pub fn set_accent<R: Runtime>(_app: &AppHandle<R>, _caption: [u8; 3], _text: [u8; 3]) {}

/// Hides the window; the process and its connections stay alive.
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

#[must_use]
pub fn is_revealed() -> bool {
    REVEALED.load(Ordering::Relaxed)
}

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

/// With `--minimized` (an autostart launch) the window is never revealed at startup.
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

/// The setting is read from the host's cache rather than the settings actor, so the close
/// decision stays synchronous on the event loop.
#[must_use]
pub fn should_stay_in_tray(state: &AppState) -> bool {
    state.settings_snapshot().system.close_to_tray
}
