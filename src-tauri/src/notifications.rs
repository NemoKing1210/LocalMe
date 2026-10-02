//! Native notifications: a message is announced only when notifications are enabled, the peer
//! is unmuted, the message is incoming, and the user is not already looking at that conversation.
//! Desktop notifications do not report clicks back, so the last-notified peer is recorded instead.

use localme_core::domain::message::{ChatMessage, Direction, MessagePreview};
use localme_core::domain::peer::PeerView;
use localme_core::services::Settings;
use tauri::Runtime;
use tauri_plugin_notification::NotificationExt;

use crate::state::{AppState, UiLabels};

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
    let body = body_for(&labels, &settings, message);

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

/// The one line a notification shows: the message text, the file it carries, or — when the
/// user asked for notifications without text — the generic label.
#[must_use]
fn body_for(labels: &UiLabels, settings: &Settings, message: &ChatMessage) -> String {
    if !settings.notifications.show_text {
        return labels.new_message.clone();
    }
    match message.body.as_ref() {
        Some(text) => text.as_str().to_owned(),
        // A message with no text has nothing to show: name the file, which is what the
        // reader would otherwise be guessing at.
        None => MessagePreview::for_message(message).body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::test_support;
    use localme_core::domain::attachment::{Attachment, AttachmentState, FileName};
    use localme_core::domain::clock::UnixMillis;
    use localme_core::domain::ids::{AttachmentId, AvatarSeed, DeviceId, MessageId};
    use localme_core::domain::message::{MessageBody, MessageStatus};
    use localme_core::domain::nickname::Nickname;
    use localme_core::services::{NotificationSettings, Settings};

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

    fn peer(device_id: DeviceId, muted: bool) -> PeerView {
        let nickname = Nickname::parse("Alice").expect("valid nickname");
        PeerView {
            device_id,
            avatar_seed: AvatarSeed::derive(device_id, &nickname),
            nickname,
            online: true,
            last_seen_ms: None,
            unread: 0,
            notify_muted: muted,
            last_activity_ms: None,
            last_message: None,
        }
    }

    fn message(peer: DeviceId, direction: Direction, body: Option<&str>) -> ChatMessage {
        ChatMessage {
            id: MessageId::generate(),
            peer,
            direction,
            body: body.map(|text| MessageBody::parse(text).expect("valid body")),
            attachments: Vec::new(),
            sent_at: UnixMillis(0),
            received_at: UnixMillis(0),
            delivered_at: None,
            status: MessageStatus::Received,
            read: false,
        }
    }

    fn attachment(peer: DeviceId) -> Attachment {
        let name = FileName::sanitise("report.pdf");
        Attachment {
            id: AttachmentId::generate(),
            message_id: MessageId::generate(),
            peer,
            direction: Direction::Incoming,
            kind: name.kind(),
            name,
            size: 10,
            state: AttachmentState::Complete,
            transferred: 10,
            sha256: None,
            created_at: UnixMillis(0),
            path: None,
        }
    }

    #[tokio::test]
    async fn the_input_follows_the_settings_and_the_peer() {
        let peer_id = DeviceId::generate();
        let incoming = message(peer_id, Direction::Incoming, Some("hi"));

        let (state, _dir) = test_support::state().await;
        assert_eq!(
            notify_input(&state, &peer(peer_id, false), &incoming),
            NotifyInput {
                enabled: true,
                muted: false,
                incoming: true,
                already_reading: false,
            }
        );

        let off = Settings {
            notifications: NotificationSettings {
                enabled: false,
                ..NotificationSettings::default()
            },
            ..Settings::default()
        };
        let (state, _dir) = test_support::state_with(off).await;
        assert!(!notify_input(&state, &peer(peer_id, false), &incoming).enabled);

        assert!(notify_input(&state, &peer(peer_id, true), &incoming).muted);
        let own = message(peer_id, Direction::Outgoing, Some("hi"));
        assert!(!notify_input(&state, &peer(peer_id, false), &own).incoming);
    }

    #[tokio::test]
    async fn a_message_in_the_open_conversation_is_suppressed() {
        use std::sync::atomic::Ordering;

        let peer_id = DeviceId::generate();
        let alice = peer(peer_id, false);
        let incoming = message(peer_id, Direction::Incoming, Some("hi"));
        let (state, _dir) = test_support::state().await;

        // Visible but not focused is not "reading".
        state.window_visible.store(true, Ordering::Relaxed);
        state.set_active_chat(Some(peer_id));
        assert!(!notify_input(&state, &alice, &incoming).already_reading);

        state.window_focused.store(true, Ordering::Relaxed);
        assert!(notify_input(&state, &alice, &incoming).already_reading);

        // Only the conversation on screen is suppressed: another peer still gets announced.
        let other = DeviceId::generate();
        assert!(
            !notify_input(
                &state,
                &peer(other, false),
                &message(other, Direction::Incoming, Some("yo")),
            )
            .already_reading
        );

        // And a hidden window is never "reading".
        state.window_visible.store(false, Ordering::Relaxed);
        assert!(!notify_input(&state, &alice, &incoming).already_reading);
    }

    #[test]
    fn the_body_is_the_message_text_when_text_is_shown() {
        let id = DeviceId::generate();
        let message = message(id, Direction::Incoming, Some("see you at six"));
        assert_eq!(
            body_for(&UiLabels::default(), &Settings::default(), &message),
            "see you at six"
        );
    }

    #[test]
    fn the_body_is_generic_when_text_is_hidden() {
        let id = DeviceId::generate();
        let message = message(id, Direction::Incoming, Some("secret"));
        let settings = Settings {
            notifications: NotificationSettings {
                show_text: false,
                ..NotificationSettings::default()
            },
            ..Settings::default()
        };
        assert_eq!(
            body_for(&UiLabels::default(), &settings, &message),
            "New message"
        );
    }

    #[test]
    fn a_message_without_text_names_the_file_it_carries() {
        let id = DeviceId::generate();
        let mut message = message(id, Direction::Incoming, None);
        message.attachments.push(attachment(id));
        let body = body_for(&UiLabels::default(), &Settings::default(), &message);
        assert!(body.contains("report.pdf"), "{body}");
    }

    #[tokio::test]
    async fn a_recorded_notification_is_consumed_once() {
        let (state, _dir) = test_support::state().await;
        let peer_id = DeviceId::generate();
        // The event forwarder records the peer only when a notification was shown...
        state.set_last_notified(Some(peer_id));
        // ... and revealing the window consumes it, so it opens that chat exactly once.
        assert_eq!(state.take_last_notified(), Some(peer_id));
        assert_eq!(state.take_last_notified(), None);
    }
}
