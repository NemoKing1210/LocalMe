//! The IPC surface: every command parses its arguments, calls one service method and lets the
//! error convert. Argument names are camelCase on the JavaScript side, as Tauri defaults to.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use localme_core::domain::message::ChatMessage;
use localme_core::domain::peer::{PeerProfile, PeerView};
use localme_core::domain::{Attachment, AttachmentId, AttachmentKind};
use localme_core::ports::store::{HistoryCursor, KnownDevice};
use localme_core::services::Settings;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::DialogExt;

use crate::args::{self, PageCursor};
use crate::clipboard;
use crate::error::ApiError;
use crate::files::{self, FilePick};
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
    /// The host removed the native frame, so the interface draws the title bar.
    pub custom_titlebar: bool,
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
        custom_titlebar: window::uses_custom_titlebar(),
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
pub async fn history<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    before: Option<PageCursor>,
    limit: Option<u32>,
) -> Result<Vec<ChatMessage>, ApiError> {
    let peer = args::device_id(&peer_id)?;
    let cursor = before.map(HistoryCursor::try_from).transpose()?;
    let limit = limit.unwrap_or(localme_core::protocol::limits::HISTORY_PAGE_SIZE);
    let page = state.session.history(peer, cursor, limit).await?;

    // A picture you sent is rendered from its original file, and the asset protocol's scope is
    // built afresh on every start — so without this, every image you have ever sent becomes a
    // broken placeholder the next time the application is opened. Only images are granted, and
    // only ones that are still on disk: this is the whole of what the interface will draw.
    for attachment in page
        .iter()
        .flat_map(|message| message.attachments.iter())
        .filter(|attachment| attachment.kind == AttachmentKind::Image)
    {
        if let Some(path) = attachment.path.as_deref() {
            let path = Path::new(path);
            if path.is_file() {
                files::allow_preview(&app, path);
            }
        }
    }

    Ok(page)
}

/// Stores the message and hands it to the peer when it is reachable; a peer that is away is not
/// an error, because the message waits in the outbox and is sent, in order, once it returns.
///
/// `attachments` are paths the interface has already inspected; the core looks at them again
/// before it writes anything down.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] if the text is too long, a path is not a usable file, or there is
/// neither text nor a file; [`ApiError::UnknownPeer`] if the device is no longer in the list;
/// [`ApiError::Storage`] if the message could not be stored.
#[tauri::command]
pub async fn send_message(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    body: String,
    attachments: Option<Vec<String>>,
) -> Result<ChatMessage, ApiError> {
    let peer = args::device_id(&peer_id)?;
    // A body of nothing but whitespace is not a body: the message is then whatever files came
    // with it, which is what makes a file-only message expressible at all.
    let text = match body.trim().is_empty() {
        true => None,
        false => Some(args::message_body(&body)?),
    };
    let files: Vec<PathBuf> = attachments
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    Ok(state.session.send_message(peer, text, files).await?)
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
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn set_peer_pinned(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    pinned: bool,
) -> Result<(), ApiError> {
    let peer = args::device_id(&peer_id)?;
    Ok(state.session.set_pinned(peer, pinned).await?)
}

/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier, or when the list is not the pinned set.
#[tauri::command]
pub async fn reorder_pinned_peers(
    state: State<'_, Arc<AppState>>,
    peer_ids: Vec<String>,
) -> Result<(), ApiError> {
    let ids = peer_ids
        .iter()
        .map(|raw| {
            raw.parse::<localme_core::domain::ids::DeviceId>()
                .map_err(|error| ApiError::invalid_input("peerIds", error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(state.session.reorder_pinned(ids).await?)
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
pub async fn complete_onboarding(
    state: State<'_, Arc<AppState>>,
    nickname: String,
) -> Result<PeerProfile, ApiError> {
    let nickname = args::nickname(&nickname)?;
    let profile = state.session.set_nickname(nickname).await?;

    let mut settings = state.settings.get().await?;
    settings.onboarded = true;
    state.settings.update(settings).await?;

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
/// effect. [`ApiError::Internal`] if the platform refused the autostart change; the document is
/// not written in that case either.
#[tauri::command]
pub async fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<Settings, ApiError> {
    // The start-with-system setting is owned by the operating system, not by our file. It is
    // applied before the document is written: if the platform refuses the change, the screen
    // keeps the switch where it was instead of showing a change that did not happen.
    let previous = state.settings.get().await?;
    if previous.system.autostart != settings.system.autostart {
        crate::autostart::apply(&app, settings.system.autostart)?;
    }
    Ok(state.settings.update(settings).await?)
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

/// Opens the platform's file picker and describes what was chosen.
///
/// The paths never reach the interface unexamined: the name, the size and whether the file can
/// be attached at all come from here, so the chip in the composer and the metadata the core
/// stores cannot disagree.
///
/// # Errors
///
/// [`ApiError::Internal`] if the platform's picker could not be opened.
#[tauri::command]
pub async fn pick_files<R: Runtime>(app: AppHandle<R>) -> Result<Vec<FilePick>, ApiError> {
    // The plugin's picker is callback-based: bridging it through a oneshot keeps the command
    // async, so the main thread is never held while the modal dialog is open.
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_files(move |files| {
        let _ = sender.send(files);
    });
    let picked = receiver.await.map_err(|_| ApiError::Internal {
        message: "the file picker did not answer".to_owned(),
    })?;
    Ok(picked
        .unwrap_or_default()
        .iter()
        .filter_map(|file| file.as_path().map(Path::to_path_buf))
        .map(|path| files::inspect(&app, path.as_path()))
        .collect())
}

/// Describes paths the interface already has — the ones a drop produced.
#[tauri::command]
pub fn inspect_files<R: Runtime>(app: AppHandle<R>, paths: Vec<String>) -> Vec<FilePick> {
    paths
        .iter()
        .map(|path| files::inspect(&app, Path::new(path)))
        .collect()
}

/// Reads the clipboard and describes anything attachable on it.
///
/// A copied file is described from its own path, exactly as the picker and the drop are; an
/// image that exists only on the clipboard has no path at all, so it is stored under the data
/// directory first. Reading the clipboard can block (X11 sets its own timeout), so it happens
/// off the interface thread. Nothing attachable is an empty answer, not an error to dismiss.
#[tauri::command]
pub async fn paste_files<R: Runtime>(app: AppHandle<R>) -> Vec<FilePick> {
    let data_dir = match crate::resolve_data_dir(&app) {
        Ok(directory) => directory,
        Err(error) => {
            tracing::warn!(%error, "the clipboard was not read: no data directory");
            return Vec::new();
        }
    };

    let reader = app.clone();
    match tauri::async_runtime::spawn_blocking(move || clipboard::read(&reader, &data_dir)).await {
        Ok(picks) => picks,
        Err(error) => {
            tracing::warn!(%error, "the clipboard reader did not finish");
            Vec::new()
        }
    }
}

/// Deletes a pasted image the user removed from the composer before sending it.
#[tauri::command]
pub fn discard_pasted_file<R: Runtime>(app: AppHandle<R>, path: String) {
    match crate::resolve_data_dir(&app) {
        Ok(data_dir) => clipboard::discard(&data_dir, Path::new(&path)),
        Err(error) => tracing::debug!(%error, "a removed paste was not deleted"),
    }
}

/// Opens a received file with whatever the platform uses for its type.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] if the attachment is unknown or has not arrived yet,
/// [`ApiError::Internal`] if no application could be started.
#[tauri::command]
pub async fn open_attachment(
    state: State<'_, Arc<AppState>>,
    attachment_id: String,
) -> Result<(), ApiError> {
    let path = ready_path(&state, &attachment_id).await?;
    files::open(&path).map_err(|error| ApiError::Internal {
        message: format!("no application could open that file: {error}"),
    })
}

/// Shows a received file in the platform's file manager.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] if the attachment is unknown or has not arrived yet,
/// [`ApiError::Internal`] if no file manager could be started.
#[tauri::command]
pub async fn reveal_attachment(
    state: State<'_, Arc<AppState>>,
    attachment_id: String,
) -> Result<(), ApiError> {
    let path = ready_path(&state, &attachment_id).await?;
    files::reveal(&path).map_err(|error| ApiError::Internal {
        message: format!("the file manager could not be opened: {error}"),
    })
}

/// Copies a received file wherever the user asks for it.
///
/// Returns the chosen path, or `None` when the dialog was dismissed.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] if the attachment is unknown or has not arrived yet,
/// [`ApiError::Internal`] if the copy failed.
#[tauri::command]
pub async fn save_attachment<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<AppState>>,
    attachment_id: String,
) -> Result<Option<String>, ApiError> {
    let attachment = attachment_of(&state, &attachment_id).await?;
    let source = files::stored_path(attachment.path.as_deref())
        .map_err(|error| ApiError::invalid_input("attachmentId", error))?;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name(attachment.name.as_str())
        .save_file(move |file| {
            let _ = sender.send(file);
        });
    let target = receiver
        .await
        .map_err(|_| ApiError::Internal {
            message: "the save dialog did not answer".to_owned(),
        })?
        .and_then(|file| file.as_path().map(Path::to_path_buf));
    let Some(target) = target else {
        return Ok(None);
    };
    files::copy(&source, &target).map_err(|error| ApiError::Internal {
        message: format!("the file could not be saved: {error}"),
    })?;
    tracing::info!(file = %attachment.name, "a received file was saved");
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// Stops a transfer, from either end.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn cancel_attachment(
    state: State<'_, Arc<AppState>>,
    attachment_id: String,
) -> Result<(), ApiError> {
    let id = args::attachment_id(&attachment_id)?;
    state.session.cancel_attachment(id).await?;
    Ok(())
}

/// Asks for a transfer again, after a failure or a cancellation.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] for a malformed identifier.
#[tauri::command]
pub async fn retry_attachment(
    state: State<'_, Arc<AppState>>,
    attachment_id: String,
) -> Result<(), ApiError> {
    let id = args::attachment_id(&attachment_id)?;
    state.session.retry_attachment(id).await?;
    Ok(())
}

/// The stored file of an attachment that is ready to be opened.
async fn ready_path(state: &AppState, attachment_id: &str) -> Result<PathBuf, ApiError> {
    let attachment = attachment_of(state, attachment_id).await?;
    files::stored_path(attachment.path.as_deref())
        .map_err(|error| ApiError::invalid_input("attachmentId", error))
}

async fn attachment_of(state: &AppState, attachment_id: &str) -> Result<Attachment, ApiError> {
    let id: AttachmentId = args::attachment_id(attachment_id)?;
    state
        .session
        .attachment(id)
        .await?
        .ok_or_else(|| ApiError::invalid_input("attachmentId", "there is no such file"))
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
