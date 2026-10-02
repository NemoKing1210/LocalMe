//! The tray icon, its menu and the unread indication.

use localme_core::services::Settings;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use crate::state::{self, AppState};

/// Tray icon id, used to find the icon again to update its menu and tooltip.
pub const TRAY_ID: &str = "localme-tray";

const MENU_OPEN: &str = "localme:open";
const MENU_TOGGLE_NOTIFICATIONS: &str = "localme:toggle-notifications";
const MENU_QUIT: &str = "localme:quit";

/// # Errors
///
/// Returns the platform error if the tray cannot be created; on Linux this means no
/// StatusNotifier or AppIndicator host is running.
pub fn install<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> tauri::Result<()> {
    let menu = build_menu(app, &state.labels_snapshot(), &state.settings_snapshot())?;
    let tooltip = state.labels_snapshot().tooltip(unread(state));

    // The window icon doubles as the tray icon, so there is only one asset to keep in sync.
    let icon = app.default_window_icon().cloned();

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(tooltip)
        // Left click raises the window; the menu is on the right button.
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

/// Flips the same flag the settings screen edits, so the two surfaces cannot disagree.
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
