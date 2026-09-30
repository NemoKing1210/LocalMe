//! Native notifications.
//!
//! Two rules decide whether a message produces one:
//!
//! * it must not be something the user is already looking at — the window focused *and* that
//!   conversation open;
//! * the user must not have switched notifications off, globally or for that peer.
//!
//! What the platform will not do is tell us the user clicked the notification: on desktop the
//! plugin ignores the action-related fields, so there is no click callback to hook. The
//! substitute is the last-notified peer recorded in [`AppState`], which the window policy uses
//! to open that conversation when the window is raised from the tray — see
//! `docs/ARCHITECTURE.md` §12.

use localme_core::domain::message::{ChatMessage, Direction};
use localme_core::domain::peer::PeerView;
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;

/// Whether this message should be shown as a notification.
#[must_use]
pub fn should_notify(state: &AppState, peer: &PeerView, message: &ChatMessage) -> bool {
    let settings = state.settings_snapshot();

    if !settings.notifications.enabled {
        return false;
    }
    if peer.notify_muted {
        return false;
    }
    // Only incoming messages: the user typed the outgoing ones, and a notification for your
    // own message is noise.
    if !matches!(message.direction, Direction::Incoming) {
        return false;
    }
    // The one case where a native notification is actively annoying: the message is already
    // on screen, in the conversation that is open, in a focused window.
    let reading_this_chat = state.is_window_active() && state.active_chat() == Some(peer.device_id);
    !reading_this_chat
}

/// Shows a notification for a message, if [`should_notify`] agrees.
///
/// Returns whether one was shown. The caller uses that to record the peer as the last
/// notified, which drives the tray-click behaviour.
pub fn show_message(
    app: &tauri::AppHandle,
    state: &AppState,
    peer: &PeerView,
    message: &ChatMessage,
) -> bool {
    if !should_notify(state, peer, message) {
        return false;
    }

    let settings = state.settings_snapshot();
    let labels = state.labels_snapshot();
    let body = if settings.notifications.show_text {
        message.body.as_str().to_owned()
    } else {
        labels.new_message
    };

    let mut builder = app
        .notification()
        .builder()
        .title(peer.nickname.as_str())
        .body(body);

    if settings.notifications.sound {
        // Only the sound *name* is honoured, and only where the platform has one; the plugin
        // documents that scheduling, grouping and actions are ignored on desktop. Leaving the
        // sound unset when the user disabled it is the best available approximation of
        // silence — see `docs/ARCHITECTURE.md` §12.
        builder = builder.sound("Default");
    }

    match builder.show() {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "failed to show a notification");
            false
        }
    }
}
