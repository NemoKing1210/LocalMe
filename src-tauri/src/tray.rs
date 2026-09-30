//! Modern integration: the tray icon, its menu and the unread indication.
//!
//! The tray is the application's persistent surface — the window is transient, the tray is
//! always there — so it carries the two things a user needs without opening anything: the
//! unread count, and the fastest way to stop being interrupted.
//!
//! The menu is rebuilt whenever a label or the pause state changes. That is a handful of
//! allocations on a rare event, and it is the only way to have a single item that says
//! "Pause notifications" or "Resume notifications" depending on the current state, which is
//! better than two items where one is always wrong.

use localme_core::services::Settings;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use crate::state::{self, AppState};

/// Identifier of the tray icon, so it can be found again to update the menu and tooltip.
pub const TRAY_ID: &str = "localme-tray";

const MENU_OPEN: &str = "localme:open";
const MENU_TOGGLE_NOTIFICATIONS: &str = "localme:toggle-notifications";
const MENU_QUIT: &str = "localme:quit";

/// Creates the tray icon.
///
/// # Errors
///
/// Returns the platform error if the tray cannot be created — on Linux this means no
/// StatusNotifier or AppIndicator host is running, which the caller reports rather than
/// treating as fatal.
pub fn install<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> tauri::Result<()> {
    let menu = build_menu(app, &state.labels_snapshot(), &state.settings_snapshot())?;
    let tooltip = state.labels_snapshot().tooltip(unread(state));

    // The window icon doubles as the tray icon: a second asset would be a second thing to
    // keep in sync, and every platform renders this one at the size it needs.
    let icon = app.default_window_icon().cloned();

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(tooltip)
        // Left click raises the window; the menu is on the right button. This matches what
        // every other tray application does, and it makes the icon a one-click "show me".
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => crate::window::reveal(app),
            MENU_TOGGLE_NOTIFICATIONS => toggle_notifications(app),
            MENU_QUIT => {
                tracing::info!("quit requested from the tray");
                app.exit(0);
            }
            other => tracing::debug!(menu_item = other, "unhandled tray menu item"),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::window::reveal(tray.app_handle());
            }
        });

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

/// Rebuilds the menu and the tooltip from the current labels, settings and unread count.
pub fn refresh<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let labels = state.labels_snapshot();
    let settings = state.settings_snapshot();

    match build_menu(app, &labels, &settings) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                tracing::warn!(%error, "failed to replace the tray menu");
            }
        }
        Err(error) => tracing::warn!(%error, "failed to build the tray menu"),
    }
    if let Err(error) = tray.set_tooltip(Some(labels.tooltip(unread(state)))) {
        tracing::warn!(%error, "failed to update the tray tooltip");
    }
}

/// The unread count as the tray shows it.
fn unread(state: &AppState) -> u32 {
    state.unread.load(std::sync::atomic::Ordering::Relaxed)
}

fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    labels: &crate::state::UiLabels,
    settings: &Settings,
) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, MENU_OPEN, &labels.open, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let toggle_text = if settings.notifications.enabled {
        &labels.mute
    } else {
        &labels.unmute
    };
    let toggle = MenuItem::with_id(
        app,
        MENU_TOGGLE_NOTIFICATIONS,
        toggle_text,
        true,
        None::<&str>,
    )?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, &labels.quit, true, None::<&str>)?;

    Menu::with_items(app, &[&open, &separator, &toggle, &separator_two, &quit])
}

/// Flips the global notification switch, from the tray.
///
/// The same flag the settings screen edits, so the two surfaces cannot disagree.
fn toggle_notifications<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = state::from_handle(&app) else {
            return;
        };
        let mut settings = match state.settings.get().await {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "failed to read the settings to toggle notifications");
                return;
            }
        };
        settings.notifications.enabled = !settings.notifications.enabled;
        let enabled = settings.notifications.enabled;
        match state.settings.update(settings).await {
            Ok(saved) => {
                tracing::info!(enabled, "notifications toggled from the tray");
                crate::events::apply_settings(&app, &saved);
            }
            Err(error) => tracing::warn!(%error, "failed to save the notification switch"),
        }
    });
}
