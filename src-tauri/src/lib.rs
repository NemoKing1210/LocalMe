//! LocalMe: a zero-configuration messenger for the local network.
//!
//! This crate is the Tauri host. It owns the window, the tray, the single-instance guard and
//! the IPC surface, and it contains no protocol, discovery or storage logic: everything with
//! behaviour lives in `localme-core`, which has no Tauri dependency at all. See
//! `docs/ARCHITECTURE.md` §3 and §9.

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

mod autostart;
mod commands;
mod error;
mod events;
mod logging;
mod notifications;
mod state;
mod tray;
mod window;

use std::process::ExitCode;
use std::sync::Arc;

use localme_core::domain::nickname::Nickname;
use localme_core::runtime::{Core, CoreConfig};
use tauri::{Manager, RunEvent, WindowEvent};

pub use error::ApiError;
use state::AppState;

/// Command-line flag set by the autostart entry when a tray-only launch was requested.
pub const FLAG_MINIMIZED: &str = "--minimized";

/// Nickname used only if the operating system cannot tell us the computer's name.
const FALLBACK_NICKNAME: &str = "LocalMe user";

/// Whether this process was started minimised into the tray.
#[must_use]
pub fn started_minimized() -> bool {
    std::env::args().any(|arg| arg == FLAG_MINIMIZED)
}

/// Builds and runs the application.
///
/// Returns a process exit code rather than panicking, so a failed start is reported through
/// the log with a non-zero status instead of a Rust panic message.
#[must_use]
pub fn run() -> ExitCode {
    if let Err(error) = run_inner() {
        tracing::error!(%error, "application failed to start");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run_inner() -> Result<(), Box<dyn std::error::Error>> {
    logging::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch is a request to show the window that already exists.
            window::reveal(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![FLAG_MINIMIZED]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_peers,
            commands::history,
            commands::send_message,
            commands::mark_read,
            commands::forget_peer,
            commands::restore_peer,
            commands::set_peer_muted,
            commands::known_devices,
            commands::clear_history,
            commands::own_profile,
            commands::set_nickname,
            commands::complete_onboarding,
            commands::get_settings,
            commands::update_settings,
            commands::is_autostart_enabled,
            commands::set_ui_labels,
            commands::set_active_chat,
            commands::show_window,
            commands::hide_window,
            commands::quit,
            commands::diagnostics,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                minimized = started_minimized(),
                "LocalMe starting"
            );

            let state = tauri::async_runtime::block_on(start_core(&handle))?;
            app.manage(state);

            window::install_ready_gate(&handle);
            events::spawn_forwarder(&handle);
            window::refresh_title(&handle, 0);

            // The tray is optional: a minimal Linux desktop without a StatusNotifier host
            // cannot show one, and that must not stop the application.
            let state = state::from_handle(&handle);
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
                let state = state::from_handle(&app);
                if window::should_stay_in_tray(&state) {
                    // Closing the window is not quitting, when the user asked it not to be.
                    api.prevent_close();
                    window::hide(&app);
                } else {
                    tracing::info!("window closed; quitting");
                    app.exit(0);
                }
            }
            WindowEvent::Focused(focused) => {
                state::from_handle(&window.app_handle().clone())
                    .window_focused
                    .store(*focused, std::sync::atomic::Ordering::Relaxed);
            }
            WindowEvent::Destroyed => {
                state::from_handle(&window.app_handle().clone())
                    .window_visible
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        })
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if let RunEvent::Exit = event
                && let Some(core) = state::from_handle(app).take_core()
            {
                tracing::info!("shutting the core down");
                tauri::async_runtime::block_on(core.shutdown());
            }
        });

    Ok(())
}

/// Opens the database, binds the listener and starts the session and discovery.
///
/// The window is created hidden by the configuration, so the first thing the user sees is a
/// themed frame rather than a white one — the front end reports readiness and
/// [`window::install_ready_gate`] reveals it.
async fn start_core(app: &tauri::AppHandle) -> Result<Arc<AppState>, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let hostname = tauri_plugin_os::hostname();
    let nickname = Nickname::parse(&hostname)
        .or_else(|_| Nickname::parse(FALLBACK_NICKNAME))
        .map_err(|error| format!("no usable default nickname: {error}"))?;

    let core = Core::start(CoreConfig::new(data_dir, nickname)).await?;
    let settings = core.settings.get().await?;
    let state = Arc::new(AppState::new(core, settings));

    // The platform's answer about autostart is authoritative; if it disagrees with the file
    // (the user removed the entry, or a different user installed it), the file follows.
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
