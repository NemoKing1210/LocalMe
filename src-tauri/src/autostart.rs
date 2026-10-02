//! Autostart registration. The operating system owns the flag, so it is not mirrored into the
//! settings file; registration passes `--minimized` so a sign-in start does not steal focus.

use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

use crate::error::ApiError;

/// # Errors
///
/// [`ApiError::Internal`] if the platform refused to answer.
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, ApiError> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| ApiError::Internal {
            message: format!("could not read the autostart state: {error}"),
        })
}

/// # Errors
///
/// [`ApiError::Internal`] if the platform refused the change; the caller keeps the previous
/// setting in that case, so the screen does not claim a change that did not happen.
pub fn apply<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), ApiError> {
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
