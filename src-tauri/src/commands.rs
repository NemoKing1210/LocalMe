//! The IPC surface.
//!
//! Every command here does the same three things: parse its arguments into domain types,
//! call exactly one service method, and let the error convert. There is no protocol, storage
//! or presence logic in this file, and there is no command that reaches into the session's
//! state directly — the session's public methods *are* the API, and this layer only makes them
//! reachable from JavaScript.
//!
//! Argument names are camelCase on the JavaScript side (`peerId`), which is Tauri's default
//! and matches the TypeScript conventions in `src/ipc`.

use std::sync::Arc;

use localme_core::domain::ids::DeviceId;
use localme_core::domain::message::{ChatMessage, MessageBody};
use localme_core::domain::nickname::Nickname;
use localme_core::domain::peer::{PeerProfile, PeerView};
use localme_core::ports::store::{HistoryCursor, KnownDevice};
use localme_core::protocol::MAX_BODY_CHARS;
use localme_core::services::Settings;
use tauri::{AppHandle, Emitter, State};

use crate::error::ApiError;
use crate::state::{AppState, UiLabels};
use crate::{tray, window};

/// Everything the interface needs for its first paint.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    /// This device's identity.
    pub profile: PeerProfile,
    /// The settings document.
    pub settings: Settings,
    /// The user list, already arranged.
    pub peers: Vec<PeerView>,
    /// The port this instance listens on; shown in diagnostics.
    pub port: u16,
    /// Set when the database was unusable and has been preserved under this path.
    pub storage_recovered: Option<String>,
    /// Set when discovery could not be started.
    pub discovery_problem: Option<String>,
    /// The application version, for the About section.
    pub version: String,
}

/// Where a page of history should start.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageCursor {
    /// Timestamp of the last row already returned.
    pub sent_at_ms: i64,
    /// Identifier of the last row already returned.
    pub id: String,
}

impl TryFrom<PageCursor> for HistoryCursor {
    type Error = ApiError;

    fn try_from(value: PageCursor) -> Result<Self, Self::Error> {
        Ok(Self {
            sent_at_ms: value.sent_at_ms,
            id: value
                .id
                .parse()
                .map_err(|error| ApiError::invalid_input("cursor.id", error))?,
        })
    }
}

/// The identity and the user list, so a lost web view can resynchronise with one call.
///
/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn bootstrap(state: State<'_, Arc<AppState>>) -> Result<Bootstrap, ApiError> {
    let settings = state.settings.get().await?;
    let profile = state.session.own_profile().await?;
    let peers = state.session.list_peers().await?;
    let core = state.core.lock().ok();

    let (port, storage_recovered, discovery_problem) =
        match core.as_ref().and_then(|guard| guard.as_ref()) {
            Some(core) => (
                core.port,
                core.storage_recovered.clone(),
                core.discovery_problem.clone(),
            ),
            None => (0, None, None),
        };

    Ok(Bootstrap {
        profile,
        settings,
        peers,
        port,
        storage_recovered,
        discovery_problem,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

/// The arranged user list.
///
/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn list_peers(state: State<'_, Arc<AppState>>) -> Result<Vec<PeerView>, ApiError> {
    Ok(state.session.list_peers().await?)
}

/// One page of a conversation, newest first.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed cursor.
#[tauri::command]
pub async fn history(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    before: Option<PageCursor>,
    limit: Option<u32>,
) -> Result<Vec<ChatMessage>, ApiError> {
    let peer = parse_device_id(&peer_id)?;
    let cursor = before.map(HistoryCursor::try_from).transpose()?;
    let limit = limit.unwrap_or(localme_core::protocol::limits::HISTORY_PAGE_SIZE);
    Ok(state.session.history(peer, cursor, limit).await?)
}

/// Sends a message.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] if the body is empty or too long,
/// [`ApiError::PeerOffline`] if the recipient is not reachable. A message that was stored but
/// could not be queued comes back with status `failed` rather than as an error.
#[tauri::command]
pub async fn send_message(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    body: String,
) -> Result<ChatMessage, ApiError> {
    let peer = parse_device_id(&peer_id)?;
    let body = MessageBody::parse(&body).map_err(|error| {
        let message = error.to_string();
        if matches!(error, localme_core::error::DomainError::BodyTooLong { .. }) {
            ApiError::InvalidInput {
                field: "body".to_owned(),
                message: format!("{message} ({MAX_BODY_CHARS} characters maximum)"),
            }
        } else {
            ApiError::InvalidInput {
                field: "body".to_owned(),
                message,
            }
        }
    })?;
    Ok(state.session.send_message(peer, body).await?)
}

/// Marks a conversation as read.
///
/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn mark_read(state: State<'_, Arc<AppState>>, peer_id: String) -> Result<u32, ApiError> {
    let peer = parse_device_id(&peer_id)?;
    Ok(state.session.mark_read(peer).await?)
}

/// Forgets a device, optionally deleting its conversation.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn forget_peer(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    delete_history: bool,
) -> Result<(), ApiError> {
    let peer = parse_device_id(&peer_id)?;
    Ok(state.session.forget(peer, delete_history).await?)
}

/// Lets a forgotten device back into the list.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn restore_peer(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
) -> Result<(), ApiError> {
    let peer = parse_device_id(&peer_id)?;
    Ok(state.session.restore(peer).await?)
}

/// Suppresses or restores notifications for one device.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn set_peer_muted(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    muted: bool,
) -> Result<(), ApiError> {
    let peer = parse_device_id(&peer_id)?;
    Ok(state.session.set_muted(peer, muted).await?)
}

/// Every device this installation has seen, forgotten ones included.
///
/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn known_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<KnownDevice>, ApiError> {
    Ok(state.session.known_devices().await?)
}

/// Deletes every stored message.
///
/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn clear_history(state: State<'_, Arc<AppState>>) -> Result<u64, ApiError> {
    Ok(state.session.clear_history().await?)
}

/// This device's identity.
///
/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn own_profile(state: State<'_, Arc<AppState>>) -> Result<PeerProfile, ApiError> {
    Ok(state.session.own_profile().await?)
}

/// Renames this device and tells every connected peer.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] if the nickname is empty, too long, or contains control
/// characters.
#[tauri::command]
pub async fn set_nickname(
    state: State<'_, Arc<AppState>>,
    nickname: String,
) -> Result<PeerProfile, ApiError> {
    let nickname =
        Nickname::parse(&nickname).map_err(|error| ApiError::invalid_input("nickname", error))?;
    Ok(state.session.set_nickname(nickname).await?)
}

/// Completes the first-run screen: sets the nickname and records that the user was asked.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for a nickname that does not validate.
#[tauri::command]
pub async fn complete_onboarding(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    nickname: String,
) -> Result<PeerProfile, ApiError> {
    let nickname =
        Nickname::parse(&nickname).map_err(|error| ApiError::invalid_input("nickname", error))?;
    let profile = state.session.set_nickname(nickname).await?;

    let mut settings = state.settings.get().await?;
    settings.onboarded = true;
    state.settings.update(settings).await?;

    let _ = app.emit("onboarding_complete", ());
    Ok(profile)
}

/// The settings document.
///
/// # Errors
///
/// [`ApiError::ShuttingDown`] if the host has stopped.
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, ApiError> {
    Ok(state.settings.get().await?)
}

/// Replaces the settings document.
///
/// # Errors
///
/// [`ApiError::Storage`] if the file could not be written; the previous document stays in
/// effect.
#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<Settings, ApiError> {
    let saved = state.settings.update(settings).await?;
    // The start-with-system setting is owned by the operating system, not by our file, so it
    // is applied here rather than read from the document at startup.
    crate::autostart::apply(&app, saved.system.autostart)?;
    Ok(saved)
}

/// Whether the application is registered to start at sign-in.
///
/// # Errors
///
/// [`ApiError::Internal`] if the platform refused to answer.
#[tauri::command]
pub fn is_autostart_enabled(app: AppHandle) -> Result<bool, ApiError> {
    crate::autostart::is_enabled(&app)
}

/// Replaces the labels the tray and the notifications are drawn with.
///
/// # Errors
///
/// [`ApiError::Internal`] if the tray could not be rebuilt.
#[tauri::command]
pub fn set_ui_labels(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    labels: UiLabels,
) -> Result<(), ApiError> {
    state.set_labels(labels);
    tray::refresh(&app, &state);
    Ok(())
}

/// Tells the host which conversation is on screen, so a notification is not raised for a
/// message the user is already reading.
#[tauri::command]
pub fn set_active_chat(
    state: State<'_, Arc<AppState>>,
    peer_id: Option<String>,
) -> Result<(), ApiError> {
    let peer = peer_id.as_deref().map(parse_device_id).transpose()?;
    state.set_active_chat(peer);
    // Opening a conversation is also how the user acknowledges the notification for it.
    if peer.is_some() {
        state.set_last_notified(None);
    }
    Ok(())
}

/// Shows and focuses the main window.
///
/// # Errors
///
/// [`ApiError::Internal`] if the window cannot be shown.
#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), ApiError> {
    window::reveal(&app);
    Ok(())
}

/// Hides the main window, leaving the application running in the tray.
///
/// # Errors
///
/// [`ApiError::Internal`] if the window cannot be hidden.
#[tauri::command]
pub fn hide_window(app: AppHandle) -> Result<(), ApiError> {
    window::hide(&app);
    Ok(())
}

/// Quits the application gracefully.
///
/// Returns immediately; the shutdown itself runs on the async runtime so that the web view is
/// not waiting on it. `RunEvent::Exit` performs the actual teardown.
///
/// # Errors
///
/// Never in practice; the `Result` keeps the signature uniform.
#[tauri::command]
pub fn quit(app: AppHandle) -> Result<(), ApiError> {
    tracing::info!("quit requested from the interface");
    app.exit(0);
    Ok(())
}

/// Ports, identity and versions, for the About section and for bug reports.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// The application version.
    pub version: String,
    /// The protocol version this build speaks.
    pub protocol_version: u16,
    /// The TCP port this instance listens on.
    pub tcp_port: u16,
    /// The device identifier, which is what another instance would dial.
    pub device_id: String,
    /// The platform, as reported by the OS plugin.
    pub platform: String,
}

/// Diagnostics for the About section.
#[tauri::command]
pub fn diagnostics(state: State<'_, Arc<AppState>>) -> Diagnostics {
    let core = state.core.lock().ok();
    let (port, device_id) = match core.as_ref().and_then(|guard| guard.as_ref()) {
        Some(core) => (core.port, core.device_id.to_string()),
        None => (0, String::new()),
    };
    Diagnostics {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        protocol_version: localme_core::protocol::PROTOCOL_VERSION,
        tcp_port: port,
        device_id,
        platform: tauri_plugin_os::platform().to_owned(),
    }
}

fn parse_device_id(value: &str) -> Result<DeviceId, ApiError> {
    value
        .parse()
        .map_err(|error| ApiError::invalid_input("peerId", error))
}

/// Every command this application exposes.
///
/// Listed once so the handler registration and the tests cannot drift apart.
#[macro_export]
macro_rules! ipc_commands {
    () => {
        tauri::generate_handler![
            $crate::commands::bootstrap,
            $crate::commands::list_peers,
            $crate::commands::history,
            $crate::commands::send_message,
            $crate::commands::mark_read,
            $crate::commands::forget_peer,
            $crate::commands::restore_peer,
            $crate::commands::set_peer_muted,
            $crate::commands::known_devices,
            $crate::commands::clear_history,
            $crate::commands::own_profile,
            $crate::commands::set_nickname,
            $crate::commands::complete_onboarding,
            $crate::commands::get_settings,
            $crate::commands::update_settings,
            $crate::commands::is_autostart_enabled,
            $crate::commands::set_ui_labels,
            $crate::commands::set_active_chat,
            $crate::commands::show_window,
            $crate::commands::hide_window,
            $crate::commands::quit,
            $crate::commands::diagnostics,
        ]
    };
}
