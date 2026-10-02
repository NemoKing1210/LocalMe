//! The IPC surface: every command parses its arguments, calls one service method and lets the
//! error convert. Argument names are camelCase on the JavaScript side, as Tauri defaults to.

use std::sync::Arc;

use localme_core::domain::message::ChatMessage;
use localme_core::domain::peer::{PeerProfile, PeerView};
use localme_core::ports::store::{HistoryCursor, KnownDevice};
use localme_core::services::Settings;
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::args::{self, PageCursor};
use crate::error::ApiError;
use crate::logging::{self, LogsInfo};
use crate::state::{AppState, UiLabels};
use crate::{tray, window};

/// Everything the interface needs for its first paint.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub profile: PeerProfile,
    pub settings: Settings,
    pub peers: Vec<PeerView>,
    pub port: u16,
    /// Set when the database was unusable and has been preserved under this path.
    pub storage_recovered: Option<String>,
    /// Set when discovery could not be started.
    pub discovery_problem: Option<String>,
    pub version: String,
}

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

    tracing::info!(
        peers = peers.len(),
        onboarded = settings.onboarded,
        locale = settings.locale.tag(),
        "the interface asked for its initial state"
    );

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
    let peer = args::device_id(&peer_id)?;
    let cursor = before.map(HistoryCursor::try_from).transpose()?;
    let limit = limit.unwrap_or(localme_core::protocol::limits::HISTORY_PAGE_SIZE);
    Ok(state.session.history(peer, cursor, limit).await?)
}

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
    let peer = args::device_id(&peer_id)?;
    let body = args::message_body(&body)?;
    Ok(state.session.send_message(peer, body).await?)
}

/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn mark_read(state: State<'_, Arc<AppState>>, peer_id: String) -> Result<u32, ApiError> {
    let peer = args::device_id(&peer_id)?;
    Ok(state.session.mark_read(peer).await?)
}

/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn forget_peer(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    delete_history: bool,
) -> Result<(), ApiError> {
    let peer = args::device_id(&peer_id)?;
    Ok(state.session.forget(peer, delete_history).await?)
}

/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn restore_peer(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
) -> Result<(), ApiError> {
    let peer = args::device_id(&peer_id)?;
    Ok(state.session.restore(peer).await?)
}

/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn set_peer_muted(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    muted: bool,
) -> Result<(), ApiError> {
    let peer = args::device_id(&peer_id)?;
    Ok(state.session.set_muted(peer, muted).await?)
}

/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn known_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<KnownDevice>, ApiError> {
    Ok(state.session.known_devices().await?)
}

/// # Errors
///
/// [`ApiError::ShuttingDown`] if the core has stopped.
#[tauri::command]
pub async fn clear_history(state: State<'_, Arc<AppState>>) -> Result<u64, ApiError> {
    Ok(state.session.clear_history().await?)
}

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
    let nickname = args::nickname(&nickname)?;
    Ok(state.session.set_nickname(nickname).await?)
}

/// # Errors
///
/// [`ApiError::InvalidInput`] for a nickname that does not validate.
#[tauri::command]
pub async fn complete_onboarding<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    nickname: String,
) -> Result<PeerProfile, ApiError> {
    let nickname = args::nickname(&nickname)?;
    let profile = state.session.set_nickname(nickname).await?;

    let mut settings = state.settings.get().await?;
    settings.onboarded = true;
    state.settings.update(settings).await?;

    let _ = app.emit("onboarding_complete", ());
    Ok(profile)
}

/// # Errors
///
/// [`ApiError::ShuttingDown`] if the host has stopped.
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, ApiError> {
    Ok(state.settings.get().await?)
}

/// # Errors
///
/// [`ApiError::Storage`] if the file could not be written; the previous document stays in
/// effect.
#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<Settings, ApiError> {
    let saved = state.settings.update(settings).await?;
    // The start-with-system setting is owned by the operating system, not by our file, so it is
    // applied here rather than read from the document at startup.
    crate::autostart::apply(&app, saved.system.autostart)?;
    Ok(saved)
}

/// # Errors
///
/// [`ApiError::Internal`] if the platform refused to answer.
#[tauri::command]
pub fn is_autostart_enabled<R: Runtime>(app: AppHandle<R>) -> Result<bool, ApiError> {
    crate::autostart::is_enabled(&app)
}

/// # Errors
///
/// [`ApiError::Internal`] if the tray could not be rebuilt.
#[tauri::command]
pub fn set_ui_labels<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    labels: UiLabels,
) -> Result<(), ApiError> {
    state.set_labels(labels);
    tray::refresh(&app, &state);
    Ok(())
}

/// # Errors
///
/// [`ApiError::InvalidInput`] if either colour is not `#RRGGBB`.
#[tauri::command]
pub fn set_window_accent<R: Runtime>(
    app: AppHandle<R>,
    accent: String,
    on_accent: String,
) -> Result<(), ApiError> {
    let caption = args::hex_color("accent", &accent)?;
    let text = args::hex_color("onAccent", &on_accent)?;
    window::set_accent(&app, caption, text);
    Ok(())
}

/// Tells the host which conversation is on screen, so a notification is not raised for a
/// message the user is already reading.
#[tauri::command]
pub fn set_active_chat(
    state: State<'_, Arc<AppState>>,
    peer_id: Option<String>,
) -> Result<(), ApiError> {
    let peer = peer_id.as_deref().map(args::device_id).transpose()?;
    state.set_active_chat(peer);
    if peer.is_some() {
        state.set_last_notified(None);
    }
    Ok(())
}

/// # Errors
///
/// [`ApiError::Internal`] if the window cannot be shown.
#[tauri::command]
pub fn show_window<R: Runtime>(app: AppHandle<R>) -> Result<(), ApiError> {
    window::reveal(&app);
    Ok(())
}

/// # Errors
///
/// [`ApiError::Internal`] if the window cannot be hidden.
#[tauri::command]
pub fn hide_window<R: Runtime>(app: AppHandle<R>) -> Result<(), ApiError> {
    window::hide(&app);
    Ok(())
}

/// Returns immediately; the shutdown itself runs on the async runtime, and `RunEvent::Exit`
/// performs the actual teardown.
///
/// # Errors
///
/// Never in practice; the `Result` keeps the signature uniform.
#[tauri::command]
pub fn quit<R: Runtime>(app: AppHandle<R>) -> Result<(), ApiError> {
    tracing::info!("quit requested from the interface");
    app.exit(0);
    Ok(())
}

/// Ports, identity and versions, for the About section and for bug reports.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub version: String,
    pub protocol_version: u16,
    pub tcp_port: u16,
    /// What another instance would dial.
    pub device_id: String,
    pub platform: String,
}

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

/// # Errors
///
/// [`ApiError::Internal`] if logging has not been initialised, which would mean the settings
/// screen somehow outlived the startup sequence.
#[tauri::command]
pub fn logs_info() -> Result<LogsInfo, ApiError> {
    Ok(logs()?.info())
}

/// # Errors
///
/// [`ApiError::Internal`] if no file manager could be started.
#[tauri::command]
pub fn open_logs_folder() -> Result<(), ApiError> {
    logs()?
        .open_directory()
        .map_err(|error| ApiError::Internal {
            message: format!("the log directory could not be opened: {error}"),
        })
}

/// # Errors
///
/// [`ApiError::Internal`] if logging has not been initialised.
#[tauri::command]
pub fn clear_logs() -> Result<u64, ApiError> {
    let freed = logs()?.clear();
    tracing::info!(
        bytes = freed,
        "the log files were cleared from the settings screen"
    );
    Ok(freed)
}

/// The front end has no filesystem access and its console is invisible in a packaged build, so
/// this is how a component error reaches a file the user can send us.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for an unknown level or an empty message.
#[tauri::command]
pub fn log_frontend(
    level: String,
    message: String,
    context: Option<String>,
) -> Result<(), ApiError> {
    if !matches!(level.as_str(), "error" | "warn" | "info" | "debug") {
        return Err(ApiError::invalid_input(
            "level",
            "must be one of error, warn, info or debug",
        ));
    }
    if message.trim().is_empty() {
        return Err(ApiError::invalid_input("message", "must not be empty"));
    }
    logging::log_frontend(&level, &message, context.as_deref());
    Ok(())
}

fn logs() -> Result<&'static std::sync::Arc<logging::Logs>, ApiError> {
    logging::logs().ok_or_else(|| ApiError::Internal {
        message: "the log directory is not available".to_owned(),
    })
}
