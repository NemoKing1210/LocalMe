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
use tauri::Runtime;
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;

/// Everything the decision needs, gathered by the caller.
///
/// A struct of plain booleans rather than `&AppState` so the rule can be tested on its own: the
/// four inputs are the whole of it, and a test that has to build a running core to check a
/// four-way condition would not be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyInput {
    /// Whether the user has notifications switched on at all.
    pub enabled: bool,
    /// Whether this peer is muted.
    pub muted: bool,
    /// Whether the message is incoming.
    pub incoming: bool,
    /// Whether the window is focused *and* showing this peer's conversation.
    pub already_reading: bool,
}

/// Whether this message should be shown as a notification.
#[must_use]
pub const fn should_notify(input: NotifyInput) -> bool {
    input.enabled && !input.muted && input.incoming && !input.already_reading
}

/// Gathers the inputs for a message from the host's state.
#[must_use]
pub fn notify_input(state: &AppState, peer: &PeerView, message: &ChatMessage) -> NotifyInput {
    NotifyInput {
        enabled: state.settings_snapshot().notifications.enabled,
        muted: peer.notify_muted,
        incoming: matches!(message.direction, Direction::Incoming),
        // The one case where a native notification is actively annoying: the message is already
        // on screen, in the conversation that is open, in a focused window.
        already_reading: state.is_window_active() && state.active_chat() == Some(peer.device_id),
    }
}

/// Shows a notification for a message, if [`should_notify`] agrees.
///
/// Returns whether one was shown. The caller uses that to record the peer as the last
/// notified, which drives the tray-click behaviour.
pub fn show_message<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    peer: &PeerView,
    message: &ChatMessage,
) -> bool {
    if !should_notify(notify_input(state, peer, message)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The decision, with everything switched on unless a case says otherwise.
    fn input() -> NotifyInput {
        NotifyInput {
            enabled: true,
            muted: false,
            incoming: true,
            already_reading: false,
        }
    }

    #[test]
    fn an_incoming_message_to_an_unmuted_peer_is_announced() {
        assert!(should_notify(input()));
    }

    #[test]
    fn a_muted_peer_is_never_announced() {
        assert!(!should_notify(NotifyInput {
            muted: true,
            ..input()
        }));
    }

    #[test]
    fn notifications_switched_off_silence_everything() {
        assert!(!should_notify(NotifyInput {
            enabled: false,
            ..input()
        }));
    }

    #[test]
    fn the_users_own_message_is_not_announced_back_to_them() {
        assert!(!should_notify(NotifyInput {
            incoming: false,
            ..input()
        }));
    }

    #[test]
    fn a_message_already_on_screen_is_not_announced() {
        // The acceptance criterion: no notification when the window is focused and this same
        // conversation is open.
        assert!(!should_notify(NotifyInput {
            already_reading: true,
            ..input()
        }));
    }

    #[test]
    fn the_suppression_applies_only_to_the_conversation_on_screen() {
        // `already_reading` is computed per peer by the caller: a message from somebody else,
        // while this conversation is open, is still announced.
        assert!(should_notify(NotifyInput {
            already_reading: false,
            ..input()
        }));
    }

    #[test]
    fn every_condition_is_necessary() {
        // The baseline does notify, and flipping any single input must silence it: if one of
        // them did not change the answer, that condition would not be doing anything.
        assert!(should_notify(input()));
        for changed in [
            NotifyInput {
                enabled: false,
                ..input()
            },
            NotifyInput {
                muted: true,
                ..input()
            },
            NotifyInput {
                incoming: false,
                ..input()
            },
            NotifyInput {
                already_reading: true,
                ..input()
            },
        ] {
            assert!(!should_notify(changed), "{changed:?} must be silenced");
        }
    }
}
