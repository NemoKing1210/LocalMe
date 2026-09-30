//! Autostart registration.
//!
//! The operating system owns this flag — a registry key on Windows, a LaunchAgent on macOS, a
//! desktop entry on Linux — so it is deliberately *not* mirrored into the settings file. What
//! is stored is the user's intent; what is applied is the platform's state, and the settings
//! screen reads the platform's answer rather than ours.
//!
//! When the app is registered, it is registered with `--minimized`, so an automatic start on
//! sign-in does not steal focus from whatever the user is doing.

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::error::ApiError;

/// Whether the application is currently registered to start at sign-in.
///
/// # Errors
///
/// [`ApiError::Internal`] if the platform refused to answer.
pub fn is_enabled(app: &AppHandle) -> Result<bool, ApiError> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| ApiError::Internal {
            message: format!("could not read the autostart state: {error}"),
        })
}

/// Registers or unregisters the application.
///
/// # Errors
///
/// [`ApiError::Internal`] if the platform refused the change. The caller keeps the previous
/// setting in that case, so the screen does not claim a change that did not happen.
pub fn apply(app: &AppHandle, enabled: bool) -> Result<(), ApiError> {
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);
    if current == enabled {
        return Ok(());
    }

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| ApiError::Internal {
        message: format!("could not change the autostart registration: {error}"),
    })
}
