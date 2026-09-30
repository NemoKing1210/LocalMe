//! The event forwarder: core events to the interface, plus the two host-side reactions.
//!
//! This is where the architecture's "do not send events to a hidden window" rule lives. While
//! the window is in the tray the forwarder keeps only what the tray needs — the unread count —
//! and emits nothing to the web view; when the window comes back, one snapshot resynchronises
//! it instead of replaying a backlog it would have to reconcile.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use localme_core::services::{CoreEvent, NoticeLevel, Settings};
use tauri::{AppHandle, Emitter};

use crate::state::{self, AppState};
use crate::{notifications, tray, window};

/// Event name for a settings change.
const SETTINGS_EVENT: &str = "settings_changed";

/// Event name for the resynchronisation snapshot.
const SNAPSHOT_EVENT: &str = "state_snapshot";

/// Event name for "open this conversation", used after the window is raised from the tray.
const OPEN_CHAT_EVENT: &str = "open_chat";

/// Subscribes to the core's events and forwards them.
pub fn spawn_forwarder(app: &AppHandle) {
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

fn handle(app: &AppHandle, state: &Arc<AppState>, event: CoreEvent) {
    match &event {
        CoreEvent::Peers { peers } => {
            let unread: u32 = peers.iter().map(|peer| peer.unread).sum();
            state.unread.store(unread, Ordering::Relaxed);
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
        CoreEvent::MessageStatus { .. } | CoreEvent::OwnProfile { .. } | CoreEvent::Stopped => {}
    }

    emit_core(app, state, &event);
}

/// Delivers a core event to the web view, unless the window is hidden.
fn emit_core(app: &AppHandle, state: &AppState, event: &CoreEvent) {
    if !window_is_visible(state) {
        // Nothing to draw into, and waking the web view for every presence change is exactly
        // the cost this application is built to avoid. The next snapshot covers it.
        return;
    }
    if let Err(error) = app.emit(event.name(), event) {
        tracing::debug!(%error, name = event.name(), "an event could not be delivered");
    }
}

/// Whether the web view is currently showing.
#[must_use]
pub fn window_is_visible(state: &AppState) -> bool {
    state.window_visible.load(Ordering::Relaxed)
}

/// Mirrors a settings change: the settings actor is the source of truth, and the tray, the
/// window title and the interface all follow it.
pub fn apply_settings(app: &AppHandle, settings: &Settings) {
    let Some(state) = state::from_handle(app) else {
        return;
    };
    if let Ok(mut cached) = state.cached.write() {
        cached.settings = settings.clone();
    }
    tray::refresh(app, &state);
    window::refresh_title(app, state.unread.load(Ordering::Relaxed));
    if let Err(error) = app.emit(SETTINGS_EVENT, settings) {
        tracing::debug!(%error, "a settings change could not be delivered");
    }
}

/// Sends the whole visible state to the interface in one event.
///
/// Called when the window becomes visible again. One round trip beats replaying whatever
/// happened while it was hidden, and it is what makes the "no events to a hidden window" rule
/// invisible to the user.
pub fn emit_snapshot(app: &AppHandle) {
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

/// Asks the interface to open a conversation.
///
/// Used when the window is raised from the tray after a notification: desktop notifications do
/// not report clicks back to the application on every platform, so the tray click is the
/// documented path to "take me to what notified me".
pub fn emit_open_chat(app: &AppHandle, peer: localme_core::domain::ids::DeviceId) {
    if let Err(error) = app.emit(OPEN_CHAT_EVENT, peer.to_string()) {
        tracing::debug!(%error, "the open-chat request could not be delivered");
    }
}
