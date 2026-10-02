//! Native notifications: a message is announced only when notifications are enabled, the peer
//! is unmuted, the message is incoming, and the user is not already looking at that conversation.
//! Desktop notifications do not report clicks back, so the last-notified peer is recorded instead.

use localme_core::domain::message::{ChatMessage, Direction, MessagePreview};
use localme_core::domain::peer::PeerView;
use tauri::Runtime;
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;

/// The inputs to the notification decision, as plain booleans so the rule is testable alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyInput {
    pub enabled: bool,
    pub muted: bool,
    pub incoming: bool,
    pub already_reading: bool,
}

#[must_use]
pub const fn should_notify(input: NotifyInput) -> bool {
    input.enabled && !input.muted && input.incoming && !input.already_reading
}

#[must_use]
pub fn notify_input(state: &AppState, peer: &PeerView, message: &ChatMessage) -> NotifyInput {
    NotifyInput {
        enabled: state.settings_snapshot().notifications.enabled,
        muted: peer.notify_muted,
        incoming: matches!(message.direction, Direction::Incoming),
        already_reading: state.is_window_active() && state.active_chat() == Some(peer.device_id),
    }
}

/// Returns whether one was shown, which the caller uses to record the last-notified peer.
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
    let body = if !settings.notifications.show_text {
        labels.new_message
    } else {
        match message.body.as_ref() {
            Some(text) => text.as_str().to_owned(),
            // A message with no text has nothing to show: name the file, which is what the
            // reader would otherwise be guessing at.
            None => MessagePreview::for_message(message).body,
        }
    };

    let mut builder = app
        .notification()
        .builder()
        .title(peer.nickname.as_str())
        .body(body);

    if settings.notifications.sound {
        // Only the sound name is honoured on desktop; leaving it unset approximates silence.
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
        assert!(!should_notify(NotifyInput {
            already_reading: true,
            ..input()
        }));
    }

    #[test]
    fn the_suppression_applies_only_to_the_conversation_on_screen() {
        // A message from somebody else, while this conversation is open, is still announced.
        assert!(should_notify(NotifyInput {
            already_reading: false,
            ..input()
        }));
    }

    #[test]
    fn every_condition_is_necessary() {
        // Flipping any single input must silence the notifying baseline.
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
