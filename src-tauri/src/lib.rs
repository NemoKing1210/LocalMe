//! Tauri host: window, tray, single-instance guard and IPC surface; all behaviour lives in
//! `localme-core`.

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

use std::process::ExitCode;
use std::sync::Arc;

use localme_core::domain::nickname::Nickname;
use localme_core::runtime::{Core, CoreConfig};
use tauri::{Manager, RunEvent, WindowEvent};

pub use error::ApiError;
use state::AppState;

mod args;
mod autostart;
mod commands;
mod error;
mod events;
mod logging;
mod notifications;
mod state;
mod tray;
mod webview;
mod window;

/// A macro, not a function: `tauri::generate_handler!` expands to a closure over the runtime.
macro_rules! ipc_handler {
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
            $crate::commands::set_window_accent,
            $crate::commands::set_active_chat,
            $crate::commands::show_window,
            $crate::commands::hide_window,
            $crate::commands::quit,
            $crate::commands::diagnostics,
            $crate::commands::logs_info,
            $crate::commands::open_logs_folder,
            $crate::commands::clear_logs,
            $crate::commands::log_frontend,
        ]
    };
}

/// Command-line flag set by the autostart entry when a tray-only launch was requested.
pub const FLAG_MINIMIZED: &str = "--minimized";

const FALLBACK_NICKNAME: &str = "LocalMe user";

/// Data directory override; also lifts the single-instance guard, which otherwise forbids two
/// instances sharing a database and advertising the same device id.
pub const FLAG_DATA_DIR: &str = "LOCALME_DATA_DIR";

#[must_use]
pub fn data_dir_override() -> Option<std::path::PathBuf> {
    std::env::var_os(FLAG_DATA_DIR).map(std::path::PathBuf::from)
}

#[must_use]
pub fn started_minimized() -> bool {
    std::env::args().any(|arg| arg == FLAG_MINIMIZED)
}

/// Runs the application, reporting a failed start as a non-zero exit code rather than a panic.
#[must_use]
pub fn run() -> ExitCode {
    if let Err(error) = run_inner() {
        tracing::error!(%error, "application failed to start");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run_inner() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = tauri::Builder::default();

    if data_dir_override().is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            window::reveal(app);
        }));
    } else {
        tracing::warn!(
            variable = FLAG_DATA_DIR,
            "the single-instance guard is off because a data directory was given explicitly"
        );
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![FLAG_MINIMIZED]),
        ))
        .invoke_handler(ipc_handler!())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = resolve_data_dir(&handle)?;
            logging::init(&data_dir);
            logging::install_panic_hook();
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                minimized = started_minimized(),
                path = %data_dir.display(),
                "LocalMe starting"
            );

            let state = tauri::async_runtime::block_on(start_core(&handle, data_dir))?;
            // The document owns the level and the retention; the subscriber follows it.
            logging::apply(&state.settings_snapshot().logging);
            app.manage(state);

            window::install_ready_gate(&handle);
            webview::disable_saved_info(&handle);
            events::spawn_forwarder(&handle);
            window::refresh_title(&handle, 0);

            // The tray is optional: a Linux desktop without a StatusNotifier host cannot show
            // one, and that must not stop the application.
            let Some(state) = state::from_handle(&handle) else {
                return Err("the application state was not installed".into());
            };
            match tray::install(&handle, &state) {
                Ok(()) => tray::refresh(&handle, &state),
                Err(error) => {
                    tracing::warn!(%error, "the tray icon is unavailable on this desktop");
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let app = window.app_handle().clone();
                // Neither `setup` nor the state exists yet; the safe reading of "no state" is
                // "quit", which is what closing does.
                let stays = state::from_handle(&app)
                    .is_some_and(|state| window::should_stay_in_tray(&state));
                if stays {
                    api.prevent_close();
                    window::hide(&app);
                } else {
                    tracing::info!("window closed; quitting");
                    app.exit(0);
                }
            }
            WindowEvent::Focused(focused) => {
                if let Some(state) = state::from_handle(&window.app_handle().clone()) {
                    state
                        .window_focused
                        .store(*focused, std::sync::atomic::Ordering::Relaxed);
                }
            }
            WindowEvent::Destroyed => {
                if let Some(state) = state::from_handle(&window.app_handle().clone()) {
                    state
                        .window_visible
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if let RunEvent::Exit = event
                && let Some(core) = state::from_handle(app).and_then(|state| state.take_core())
            {
                tracing::info!("shutting the core down");
                tauri::async_runtime::block_on(core.shutdown());
            }
        });

    Ok(())
}

/// The data directory, shared by logging and the core so the two cannot disagree.
fn resolve_data_dir(
    app: &tauri::AppHandle,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    match data_dir_override() {
        Some(override_dir) => {
            tracing::warn!(
                path = %override_dir.display(),
                "using an explicitly configured data directory"
            );
            Ok(override_dir)
        }
        None => Ok(app.path().app_data_dir()?),
    }
}

/// Opens the database, binds the listener and starts the session and discovery.
async fn start_core(
    app: &tauri::AppHandle,
    data_dir: std::path::PathBuf,
) -> Result<Arc<AppState>, Box<dyn std::error::Error>> {
    let hostname = tauri_plugin_os::hostname();
    let nickname = Nickname::parse(&hostname)
        .or_else(|_| Nickname::parse(FALLBACK_NICKNAME))
        .map_err(|error| format!("no usable default nickname: {error}"))?;

    let core = Core::start(CoreConfig::new(data_dir, nickname)).await?;
    let settings = core.settings.get().await?;
    let state = Arc::new(AppState::new(core, settings));

    // The platform's autostart state is authoritative over the stored setting.
    match autostart::is_enabled(app) {
        Ok(enabled) if enabled != state.settings_snapshot().system.autostart => {
            tracing::info!(
                stored = state.settings_snapshot().system.autostart,
                actual = enabled,
                "the autostart registration disagrees with the stored setting"
            );
            let mut settings = state.settings_snapshot();
            settings.system.autostart = enabled;
            if let Err(error) = state.settings.update(settings).await {
                tracing::warn!(%error, "failed to record the actual autostart state");
            }
        }
        Ok(_) => {}
        Err(error) => tracing::debug!(%error, "the autostart state could not be read"),
    }

    if let Some(path) = state.core.lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .and_then(|core| core.storage_recovered.clone())
    }) {
        tracing::warn!(%path, "the previous database was damaged and has been preserved");
    }

    Ok(state)
}
