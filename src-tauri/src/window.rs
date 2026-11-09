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

/// Whether the native frame is removed and the interface draws its own title bar.
///
/// Windows only. It has the pieces a hand-drawn bar needs (a drag region that snaps, resize hit
/// testing on an undecorated window, and the standard window operations), and its title bar is
/// the one that otherwise ignores the palette. On Linux an undecorated window cannot be resized
/// by the window manager, so the native frame stays there; on macOS the traffic lights are part
/// of the platform's own contract and are left alone.
#[must_use]
pub const fn uses_custom_titlebar() -> bool {
    cfg!(windows)
}

/// Removes the native frame where [`uses_custom_titlebar`] says the interface owns the bar.
pub fn apply_frame<R: Runtime>(app: &AppHandle<R>) {
    if !uses_custom_titlebar() {
        return;
    }
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::warn!("main window is missing; the native frame stays");
        return;
    };

    // Shadow first: the resize hit testing Tauri installs when the decorations go away asks
    // whether the window has an undecorated shadow, and the window is still decorated here.
    if let Err(error) = window.set_shadow(true) {
        tracing::debug!(%error, "the window shadow could not be enabled");
    }
    if let Err(error) = window.set_decorations(false) {
        tracing::warn!(%error, "the native title bar could not be removed");
        return;
    }

    round_corners(&window);
}

/// Windows 11 gives a framed window rounded corners; a borderless one only gets them when asked.
#[cfg(windows)]
fn round_corners<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    use windows::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };

    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd,
        Err(error) => {
            tracing::debug!(%error, "the window has no native handle to round");
            return;
        }
    };
    let preference = DWMWCP_ROUND;
    // SAFETY: `hwnd` is this process's live window handle, reached from the event loop; the
    // attribute takes exactly one `DWM_WINDOW_CORNER_PREFERENCE`, and `preference` outlives the
    // call, so the pointer and the size the call is told to read match.
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::from_ref(&preference).cast(),
            size_of_val(&preference) as u32,
        )
    };
    if let Err(error) = result {
        // Expected on Windows 10, which predates the attribute: the corners stay square.
        tracing::debug!(%error, "the window corners could not be rounded");
    }
}

#[cfg(not(windows))]
fn round_corners<R: Runtime>(_window: &tauri::WebviewWindow<R>) {}

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

/// Whether the window should stay hidden once the front end is ready.
///
/// The `--minimized` flag or the `start_minimized` setting asks for it; the tray has to exist for
/// it to be honoured, because a hidden window with no tray could not be reopened.
#[must_use]
pub fn starts_hidden(flag: bool, setting: bool, tray_available: bool) -> bool {
    (flag || setting) && tray_available
}

/// With the `start_minimized` setting — or the `--minimized` flag — the window is not revealed
/// at startup. A hidden window with no tray could not be reopened, so a start is only tray-only
/// when the tray actually exists.
pub fn install_ready_gate<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    app.listen(READY_EVENT, move |_event| {
        // Read at ready time: the tray is installed during `setup`, after this listener is bound.
        let state = state::from_handle(&handle);
        let tray_available = state
            .as_ref()
            .is_some_and(|state| state.tray_available.load(Ordering::Relaxed));
        let hidden = starts_hidden(
            crate::started_minimized(),
            state
                .as_ref()
                .is_some_and(|state| state.settings_snapshot().system.start_minimized),
            tray_available,
        );
        if hidden {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::test_support;
    use localme_core::services::{Settings, SystemSettings};

    #[test]
    fn the_window_constants_match_the_configuration() {
        assert_eq!(MAIN_WINDOW, "main");
        assert_eq!(READY_EVENT, "localme://ready");
        assert_eq!(READY_FALLBACK, Duration::from_secs(15));
    }

    #[test]
    fn a_tray_only_start_needs_both_the_request_and_a_tray() {
        // The flag, the setting and the tray, in every combination that matters.
        assert!(starts_hidden(true, false, true), "the autostart flag alone");
        assert!(starts_hidden(false, true, true), "the setting alone");
        assert!(starts_hidden(true, true, true));
        assert!(!starts_hidden(false, false, true), "nothing asked for it");
        assert!(
            !starts_hidden(true, true, false),
            "a hidden window with no tray could not be reopened"
        );
        assert!(!starts_hidden(true, false, false));
    }

    #[test]
    fn revealing_is_reported_from_the_static() {
        REVEALED.store(false, Ordering::Relaxed);
        assert!(!is_revealed());
        REVEALED.store(true, Ordering::Relaxed);
        assert!(is_revealed());
        REVEALED.store(false, Ordering::Relaxed);
        assert!(!is_revealed());
    }

    #[tokio::test]
    async fn the_close_decision_follows_the_cached_setting() {
        let settings = Settings {
            system: SystemSettings {
                close_to_tray: true,
                ..SystemSettings::default()
            },
            ..Settings::default()
        };
        let (state, _dir) = test_support::state_with(settings).await;
        assert!(should_stay_in_tray(&state));

        let settings = Settings {
            system: SystemSettings {
                close_to_tray: false,
                ..SystemSettings::default()
            },
            ..Settings::default()
        };
        let (state, _dir) = test_support::state_with(settings).await;
        assert!(!should_stay_in_tray(&state));
    }

    #[test]
    fn the_custom_titlebar_is_a_windows_decision() {
        // The interface draws the bar exactly where the host removed the frame; the two must
        // not disagree, so both read this one answer.
        assert_eq!(uses_custom_titlebar(), cfg!(windows));
    }
}
