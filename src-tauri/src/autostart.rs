//! Autostart registration. The operating system owns the flag, so it is not mirrored into the
//! settings file: the stored value is the user's intent, and the startup reconciliation makes the
//! platform registration match it. A registration that points at another executable — the app was
//! moved or reinstalled, or an older build left its own path behind — is rewritten rather than
//! trusted.
//!
//! Only a packaged build touches the registration. A debug build (`npm run tauri dev`) would put
//! `target\debug\localme.exe` into the login entry, and a release build pointed at an alternate
//! `LOCALME_DATA_DIR` is a two-instance test rather than the copy the user signs in with. Such a
//! process leaves the platform alone; the setting is still saved and the next packaged start
//! reconciles the entry.
//!
//! On Windows the value is written here instead of through `auto-launch`, the crate behind the
//! Tauri plugin: it stores the executable path unquoted, so an installation under a path with a
//! space (a user profile such as `C:\Users\John Smith`, or `Program Files`) is split at the first
//! space and never starts. The other platforms keep the plugin, whose `enable` rewrites the
//! LaunchAgent or desktop entry and therefore repairs a stale path on its own.

use tauri::{AppHandle, Runtime};

use crate::error::ApiError;

#[cfg(not(windows))]
use tauri_plugin_autostart::ManagerExt;

/// May this process change the platform's login entry?
#[must_use]
fn manages_registration() -> bool {
    !cfg!(debug_assertions)
        && std::env::var_os("LOCALME_NO_AUTOSTART").is_none()
        && crate::data_dir_override().is_none()
}

/// Registers or unregisters the application, repairing an entry that points at another
/// executable. Safe to call on every start: when the entry already matches, nothing is written.
///
/// # Errors
///
/// [`ApiError::Internal`] if the platform refused the change. The caller keeps the previous
/// setting in that case, so the screen does not claim a change that did not happen.
pub fn apply<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), ApiError> {
    if !manages_registration() {
        tracing::debug!(
            enabled,
            "the autostart registration is left alone by this build"
        );
        return Ok(());
    }
    sync(app, enabled)
}

#[cfg(windows)]
fn sync<R: Runtime>(_app: &AppHandle<R>, enabled: bool) -> Result<(), ApiError> {
    let in_sync = if enabled {
        platform::is_current()
    } else {
        !platform::is_enabled()
    };
    if in_sync {
        return Ok(());
    }
    platform::set(enabled).map_err(|error| ApiError::Internal {
        message: format!("could not change the autostart registration: {error}"),
    })
}

#[cfg(not(windows))]
fn sync<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), ApiError> {
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);
    if !enabled && !current {
        return Ok(());
    }
    // `enable` is an idempotent rewrite of the entry with the running executable, which is also
    // how a stale path is repaired, so an enabled request always writes.
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| ApiError::Internal {
        message: format!("could not change the autostart registration: {error}"),
    })
}

#[cfg(windows)]
mod platform {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    /// The per-user login key. No administrator rights are needed, and the Tauri NSIS hook in
    /// `installer-hooks.nsh` removes the value on uninstall.
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Must match the product name: the installer's pre-uninstall hook deletes this value.
    const VALUE: &str = "LocalMe";

    /// The command Windows runs at sign-in: one quoted executable path, and no arguments.
    /// Whether the window opens is decided by the `start_minimized` setting after launch.
    fn command() -> std::io::Result<String> {
        Ok(quote(&std::env::current_exe()?))
    }

    fn quote(path: &std::path::Path) -> String {
        format!("\"{}\"", path.display())
    }

    fn registered() -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(RUN_KEY)
            .and_then(|key| key.get_value::<String, _>(VALUE))
            .ok()
    }

    pub fn is_enabled() -> bool {
        registered().is_some()
    }

    /// True when the registered command points at the running executable. A stale path (the app
    /// was moved, reinstalled, or a dev build registered itself) counts as out of sync.
    pub fn is_current() -> bool {
        match (registered(), command()) {
            (Some(registered), Ok(command)) => registered == command,
            _ => false,
        }
    }

    pub fn set(enabled: bool) -> std::io::Result<()> {
        if enabled {
            let key = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?.0;
            key.set_value(VALUE, &command()?)
        } else {
            let Ok(key) =
                RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
            else {
                // No key means nothing is registered, which is the requested state.
                return Ok(());
            };
            match key.delete_value(VALUE) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::path::Path;

        use super::*;

        #[test]
        fn a_path_with_a_space_stays_one_argument() {
            // The reason this module exists: `auto-launch` writes the path bare, so Windows
            // would try to run `C:\Program` and never find the executable.
            let path = Path::new(r"C:\Program Files\LocalMe\localme.exe");
            assert_eq!(quote(path), r#""C:\Program Files\LocalMe\localme.exe""#);
        }
    }
}
