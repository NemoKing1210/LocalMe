//! Core events to the interface, plus the two host-side reactions.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use localme_core::domain::ids::DeviceId;
use localme_core::domain::{Attachment, AttachmentId, AttachmentState};
use localme_core::services::{CoreEvent, NoticeLevel, Settings};
use tauri::{AppHandle, Emitter, Runtime};

use crate::files;
use crate::state::{self, AppState};
use crate::{notifications, tray, window};

const SETTINGS_EVENT: &str = "settings_changed";

const SNAPSHOT_EVENT: &str = "state_snapshot";

const OPEN_CHAT_EVENT: &str = "open_chat";

const OPEN_SETTINGS_EVENT: &str = "open_settings";

/// Reports the result of copying a completed download to the path the user chose.
const ATTACHMENT_SAVED_EVENT: &str = "attachment_saved";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentSaved<'a> {
    attachment_id: &'a AttachmentId,
    /// `None` when the copy failed; the interface tells the user rather than staying silent.
    path: Option<&'a str>,
}

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
                        // A completion may have been in the skipped window: reconcile the saves
                        // the user already confirmed rather than lose them silently. This runs
                        // before the next event is read, so no completion can slip past it.
                        reconcile_pending_saves(&app, &state).await;
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
        CoreEvent::Attachment { attachment, .. } => {
            if let Some(target) = take_destination(state, attachment) {
                copy_and_report(app, attachment, target);
            }
        }
        CoreEvent::MessageStatus { .. } | CoreEvent::Stopped => {}
    }

    emit_core(app, state, &event);
}

/// The destination a completion event should copy to, if the user chose one.
///
/// A complete transfer takes its destination exactly once, so a duplicated event cannot copy
/// the same file twice. A cancelled or failed one discards it: asking again has to reopen the
/// dialog rather than copy to a place the user chose before changing their mind.
fn take_destination(state: &AppState, attachment: &Attachment) -> Option<PathBuf> {
    match attachment.state {
        AttachmentState::Complete => state.take_pending_save(attachment.id),
        AttachmentState::Cancelled | AttachmentState::Failed => {
            let _ = state.take_pending_save(attachment.id);
            None
        }
        _ => None,
    }
}

/// Copies a completed download to the path the user chose, off the event loop, and reports the
/// outcome through [`ATTACHMENT_SAVED_EVENT`] — once, whether the copy worked or not.
pub(crate) fn copy_and_report<R: Runtime>(
    app: &AppHandle<R>,
    attachment: &Attachment,
    target: PathBuf,
) {
    let app = app.clone();
    let attachment = attachment.clone();
    tauri::async_runtime::spawn(async move {
        let source = match files::stored_path(attachment.path.as_deref()) {
            Ok(source) => source,
            Err(error) => {
                tracing::warn!(%error, id = %attachment.id, "the completed file is not on disk");
                emit_saved(&app, attachment.id, None);
                return;
            }
        };
        let destination = target.clone();
        let copied =
            tauri::async_runtime::spawn_blocking(move || files::copy(&source, &destination)).await;
        match copied {
            Ok(Ok(bytes)) => {
                tracing::info!(file = %attachment.name, bytes, "a downloaded file was saved");
                emit_saved(&app, attachment.id, Some(&target));
            }
            Ok(Err(error)) => {
                tracing::warn!(file = %attachment.name, %error, "a downloaded file could not be saved");
                emit_saved(&app, attachment.id, None);
            }
            Err(error) => {
                tracing::warn!(file = %attachment.name, %error, "the copy task did not finish");
                emit_saved(&app, attachment.id, None);
            }
        }
    });
}

fn emit_saved<R: Runtime>(app: &AppHandle<R>, id: AttachmentId, path: Option<&Path>) {
    let path = path.map(|path| path.to_string_lossy().into_owned());
    let payload = AttachmentSaved {
        attachment_id: &id,
        path: path.as_deref(),
    };
    if let Err(error) = app.emit(ATTACHMENT_SAVED_EVENT, payload) {
        tracing::debug!(%error, "the save result could not be delivered");
    }
}

/// What a pending destination found after a lag needs: the file is complete, the transfer is
/// still running, or the attachment is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconcileAction {
    Copy,
    Keep,
    Drop,
}

fn reconcile_action(attachment: Option<&Attachment>) -> ReconcileAction {
    match attachment {
        Some(attachment) if attachment.state == AttachmentState::Complete => ReconcileAction::Copy,
        Some(_) => ReconcileAction::Keep,
        None => ReconcileAction::Drop,
    }
}

/// After the event stream lagged: copy the saves that completed in the skipped window, and put
/// the rest back to wait for their own completion event.
///
/// Runs inline, before the forwarder reads another event. A task would let a completion be
/// handled while this is still reading its row: the event would find the pending map drained,
/// this would re-insert the destination, and no later event would ever copy it.
async fn reconcile_pending_saves<R: Runtime>(app: &AppHandle<R>, state: &Arc<AppState>) {
    for (id, target) in state.take_all_pending_saves() {
        match state.session.attachment(id).await {
            Ok(found) => match reconcile_action(found.as_ref()) {
                ReconcileAction::Copy => {
                    if let Some(attachment) = found {
                        copy_and_report(app, &attachment, target);
                    }
                }
                ReconcileAction::Keep => state.set_pending_save(id, target),
                ReconcileAction::Drop => {}
            },
            Err(error) => {
                tracing::warn!(%error, %id, "a waiting save could not be reconciled");
                state.set_pending_save(id, target);
            }
        }
    }
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
    use localme_core::domain::ids::MessageId;
    use localme_core::domain::{AttachmentKind, Direction, FileName, UnixMillis};

    /// A received document in the state under test, with the fields the decisions read.
    fn attachment(state: AttachmentState) -> Attachment {
        Attachment {
            id: AttachmentId::generate(),
            message_id: MessageId::generate(),
            peer: DeviceId::generate(),
            direction: Direction::Incoming,
            name: FileName::sanitise("report.pdf"),
            size: 10,
            kind: AttachmentKind::File,
            state,
            transferred: 0,
            sha256: None,
            created_at: UnixMillis(1),
            path: None,
        }
    }

    #[tokio::test]
    async fn a_completed_transfer_takes_the_chosen_destination_exactly_once() {
        let (state, _dir) = test_support::state().await;
        let complete = attachment(AttachmentState::Complete);
        state.set_pending_save(complete.id, PathBuf::from("/saved/report.pdf"));

        assert_eq!(
            take_destination(&state, &complete),
            Some(PathBuf::from("/saved/report.pdf"))
        );
        // A duplicated event must not copy the same file to the same place twice.
        assert_eq!(take_destination(&state, &complete), None);
    }

    #[tokio::test]
    async fn a_cancelled_or_failed_transfer_forgets_the_destination() {
        let (state, _dir) = test_support::state().await;

        let cancelled = attachment(AttachmentState::Cancelled);
        state.set_pending_save(cancelled.id, PathBuf::from("/saved/report.pdf"));
        assert_eq!(take_destination(&state, &cancelled), None);

        let failed = attachment(AttachmentState::Failed);
        state.set_pending_save(failed.id, PathBuf::from("/saved/report.pdf"));
        assert_eq!(take_destination(&state, &failed), None);

        // Nothing is left to copy behind the user's back.
        assert!(state.take_all_pending_saves().is_empty());
    }

    #[tokio::test]
    async fn a_transfer_in_progress_keeps_its_destination() {
        let (state, _dir) = test_support::state().await;
        let receiving = attachment(AttachmentState::Receiving);
        state.set_pending_save(receiving.id, PathBuf::from("/saved/report.pdf"));

        assert_eq!(take_destination(&state, &receiving), None);
        assert_eq!(
            state.take_all_pending_saves(),
            vec![(receiving.id, PathBuf::from("/saved/report.pdf"))]
        );
    }

    #[test]
    fn a_lagged_pending_save_is_copied_only_once_the_file_is_complete() {
        assert_eq!(
            reconcile_action(Some(&attachment(AttachmentState::Complete))),
            ReconcileAction::Copy
        );
        assert_eq!(
            reconcile_action(Some(&attachment(AttachmentState::Receiving))),
            ReconcileAction::Keep
        );
        // The row is gone: the destination has nothing to be copied from.
        assert_eq!(reconcile_action(None), ReconcileAction::Drop);
    }

    /// The payload is what the interface reads; the emit itself needs an `AppHandle`, so the
    /// shape is pinned here instead.
    #[test]
    fn the_saved_event_carries_the_id_and_an_optional_path() {
        let id = AttachmentId::generate();

        let saved = serde_json::to_value(AttachmentSaved {
            attachment_id: &id,
            path: Some("/saved/report.pdf"),
        })
        .expect("the payload serialises");
        assert_eq!(
            saved,
            serde_json::json!({ "attachmentId": id.to_string(), "path": "/saved/report.pdf" })
        );

        let failed = serde_json::to_value(AttachmentSaved {
            attachment_id: &id,
            path: None,
        })
        .expect("the payload serialises");
        assert_eq!(
            failed,
            serde_json::json!({ "attachmentId": id.to_string(), "path": null })
        );
    }

    /// Only `window_is_visible` and the two decision helpers are reachable without an
    /// `AppHandle`; the rest of the module emits through one, and the `tauri::test` mock runtime
    /// is deliberately not enabled (its test binary fails to start on Windows), so it cannot be
    /// built here.
    #[test]
    fn the_event_names_are_the_ones_the_interface_listens_for() {
        assert_eq!(SETTINGS_EVENT, "settings_changed");
        assert_eq!(SNAPSHOT_EVENT, "state_snapshot");
        assert_eq!(OPEN_CHAT_EVENT, "open_chat");
        assert_eq!(OPEN_SETTINGS_EVENT, "open_settings");
        assert_eq!(ATTACHMENT_SAVED_EVENT, "attachment_saved");
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
