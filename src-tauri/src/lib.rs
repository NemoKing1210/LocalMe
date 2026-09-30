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

mod error;
mod logging;
mod window;

use std::process::ExitCode;

pub use error::ApiError;

/// Command-line flag set by the autostart entry when a tray-only launch was requested.
pub const FLAG_MINIMIZED: &str = "--minimized";

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
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![FLAG_MINIMIZED]),
        ))
        .setup(|app| {
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                minimized = started_minimized(),
                "LocalMe starting"
            );
            window::install_ready_gate(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())?
        .run(|_app, _event| {});

    Ok(())
}
