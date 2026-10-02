//! Core events to the interface, plus the two host-side reactions.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use localme_core::domain::ids::DeviceId;
use localme_core::services::{CoreEvent, NoticeLevel, Settings};
use tauri::{AppHandle, Emitter, Runtime};

use crate::state::{self, AppState};
use crate::{notifications, tray, window};

const SETTINGS_EVENT: &str = "settings_changed";

const SNAPSHOT_EVENT: &str = "state_snapshot";

const OPEN_CHAT_EVENT: &str = "open_chat";

const OPEN_SETTINGS_EVENT: &str = "open_settings";

pub fn spawn_forwarder<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = state::from_handle(&app) else {
            return;
        };
        let mut core_events = state.session.subscribe();
        let mut settings_events = state.settings.subscribe();

        loop {
            tokio::select! {
                event = core_events.recv() => match event {
                    Ok(event) => {
                        let finished = matches!(event, CoreEvent::Stopped);
                        handle(&app, &state, event);
                        if finished {
                            tracing::debug!("the core stopped; the event forwarder is done");
                            return;
                        }
                    }
                    // Falling behind loses events, not correctness: every event describes
                    // state the interface can also read back through `state_snapshot`.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "the interface fell behind the event stream");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },

                event = settings_events.recv() => match event {
                    Ok(settings) => apply_settings(&app, &settings),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {}
                },
            }
        }
    });
}

fn handle<R: Runtime>(app: &AppHandle<R>, state: &Arc<AppState>, event: CoreEvent) {
    match &event {
        CoreEvent::Peers { peers } => {
            let unread: u32 = peers.iter().map(|peer| peer.unread).sum();
            state.unread.store(unread, Ordering::Relaxed);
            // The tray is drawn outside the web view, so it keeps its own copy of the list and
            // stays usable while the window is hidden.
            state.set_peers(
                peers
                    .iter()
                    .map(|peer| state::TrayPeer {
                        device_id: peer.device_id,
                        nickname: peer.nickname.as_str().to_owned(),
                        unread: peer.unread,
                    })
                    .collect(),
            );
            tray::refresh(app, state);
            window::refresh_title(app, unread);
        }
        CoreEvent::Message { peer, message } => {
            if notifications::show_message(app, state, peer, message) {
                state.set_last_notified(Some(peer.device_id));
            }
        }
        CoreEvent::Notice { level, message } => match level {
            NoticeLevel::Warning => tracing::warn!(notice = %message, "core notice"),
            NoticeLevel::Error => tracing::error!(notice = %message, "core notice"),
            NoticeLevel::Info => tracing::info!(notice = %message, "core notice"),
        },
        CoreEvent::MessageStatus { .. }
        | CoreEvent::Attachment { .. }
        | CoreEvent::OwnProfile { .. }
        | CoreEvent::Stopped => {}
    }

    emit_core(app, state, &event);
}

fn emit_core<R: Runtime>(app: &AppHandle<R>, state: &AppState, event: &CoreEvent) {
    if !window_is_visible(state) {
        // The next snapshot covers everything missed while hidden.
        return;
    }
    if let Err(error) = app.emit(event.name(), event) {
        tracing::debug!(%error, name = event.name(), "an event could not be delivered");
    }
}

#[must_use]
pub fn window_is_visible(state: &AppState) -> bool {
    state.window_visible.load(Ordering::Relaxed)
}

/// The settings actor is the source of truth; the tray, the title and the interface follow it.
pub fn apply_settings<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    let Some(state) = state::from_handle(app) else {
        return;
    };
    if let Ok(mut cached) = state.cached.write() {
        cached.settings = settings.clone();
    }
    // Applied here so a settings-screen change takes effect without a restart.
    crate::logging::apply(&settings.logging);
    tray::refresh(app, &state);
    window::refresh_title(app, state.unread.load(Ordering::Relaxed));
    if let Err(error) = app.emit(SETTINGS_EVENT, settings) {
        tracing::debug!(%error, "a settings change could not be delivered");
    }
}

/// Sends the whole visible state to the interface in one event, when the window becomes visible.
pub fn emit_snapshot<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = state::from_handle(&app) else {
            return;
        };
        let session = state.session.clone();
        let settings = state.settings.clone();
        let (peers, settings) = match (session.list_peers().await, settings.get().await) {
            (Ok(peers), Ok(settings)) => (peers, settings),
            (peers, settings) => {
                tracing::debug!(
                    peers_failed = peers.is_err(),
                    settings_failed = settings.is_err(),
                    "the state snapshot could not be read in full"
                );
                return;
            }
        };
        let payload = serde_json::json!({ "peers": peers, "settings": settings });
        if let Err(error) = app.emit(SNAPSHOT_EVENT, payload) {
            tracing::debug!(%error, "the state snapshot could not be delivered");
        }
    });
}

/// Used when the window is raised from the tray after a notification: desktop notifications do
/// not report clicks back to the application on every platform.
pub fn emit_open_chat<R: Runtime>(app: &AppHandle<R>, peer: DeviceId) {
    if let Err(error) = app.emit(OPEN_CHAT_EVENT, peer.to_string()) {
        tracing::debug!(%error, "the open-chat request could not be delivered");
    }
}

/// Raised by the tray's Settings item, after the window has been shown: the router owns which
/// page is on screen, so the host asks for one rather than setting a flag.
pub fn emit_open_settings<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.emit(OPEN_SETTINGS_EVENT, ()) {
        tracing::debug!(%error, "the open-settings request could not be delivered");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::test_support;

    /// Only `window_is_visible` is reachable without an `AppHandle`; the rest of the module
    /// emits through one, and the `tauri::test` mock runtime is deliberately not enabled (its
    /// test binary fails to start on Windows), so it cannot be built here.
    #[test]
    fn the_event_names_are_the_ones_the_interface_listens_for() {
        assert_eq!(SETTINGS_EVENT, "settings_changed");
        assert_eq!(SNAPSHOT_EVENT, "state_snapshot");
        assert_eq!(OPEN_CHAT_EVENT, "open_chat");
        assert_eq!(OPEN_SETTINGS_EVENT, "open_settings");
    }

    #[tokio::test]
    async fn visibility_tracks_the_window_flag() {
        let (state, _dir) = test_support::state().await;
        assert!(!window_is_visible(&state));
        state.window_visible.store(true, Ordering::Relaxed);
        assert!(window_is_visible(&state));
        state.window_visible.store(false, Ordering::Relaxed);
        assert!(!window_is_visible(&state));
    }
}
