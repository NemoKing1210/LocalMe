//! The tray icon, its menu and the unread indication.
//!
//! The menu is a view of the state the host already keeps: the settings document and the cached
//! conversation list. Every path that changes either one rebuilds it through [`refresh`], so the
//! check marks and the unread counts can never disagree with the window.

use localme_core::services::Settings;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use crate::state::{self, AppState, TrayPeer};

/// Tray icon id, used to find the icon again to update its menu and tooltip.
pub const TRAY_ID: &str = "localme-tray";

/// How many conversations the submenu lists. The window shows the full list, so the cap is only
/// about keeping the menu readable.
const MAX_TRAY_PEERS: usize = 6;

const MENU_STATUS: &str = "localme:status";
const MENU_OPEN: &str = "localme:open";
const MENU_CONVERSATIONS: &str = "localme:conversations";
const MENU_ALL_CONVERSATIONS: &str = "localme:all-conversations";
const MENU_MARK_ALL_READ: &str = "localme:mark-all-read";
const MENU_TOGGLE_NOTIFICATIONS: &str = "localme:toggle-notifications";
const MENU_TOGGLE_CLOSE_TO_TRAY: &str = "localme:toggle-close-to-tray";
const MENU_TOGGLE_AUTOSTART: &str = "localme:toggle-autostart";
const MENU_SETTINGS: &str = "localme:settings";
const MENU_OPEN_LOGS: &str = "localme:open-logs";
const MENU_QUIT: &str = "localme:quit";

/// One menu item per conversation; the device id follows the prefix.
const OPEN_PEER_PREFIX: &str = "localme:peer:";

/// # Errors
///
/// Returns the platform error if the tray cannot be created; on Linux this means no
/// StatusNotifier or AppIndicator host is running.
pub fn install<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> tauri::Result<()> {
    let menu = build_menu(
        app,
        &state.labels_snapshot(),
        &state.settings_snapshot(),
        &state.peers_snapshot(),
        unread(state),
    )?;
    let tooltip = state.labels_snapshot().tooltip(unread(state));

    // The window icon doubles as the tray icon, so there is only one asset to keep in sync.
    let icon = app.default_window_icon().cloned();

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(tooltip)
        // Left click raises the window; the menu is on the right button.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
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
    let peers = state.peers_snapshot();
    let unread = unread(state);

    match build_menu(app, &labels, &settings, &peers, unread) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                tracing::warn!(%error, "failed to replace the tray menu");
            }
        }
        Err(error) => tracing::warn!(%error, "failed to build the tray menu"),
    }
    if let Err(error) = tray.set_tooltip(Some(labels.tooltip(unread))) {
        tracing::warn!(%error, "failed to update the tray tooltip");
    }
}

fn unread(state: &AppState) -> u32 {
    state.unread.load(std::sync::atomic::Ordering::Relaxed)
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    if let Some(device) = id.strip_prefix(OPEN_PEER_PREFIX) {
        open_peer(app, device);
        return;
    }
    match id {
        MENU_OPEN | MENU_ALL_CONVERSATIONS => crate::window::reveal(app),
        MENU_MARK_ALL_READ => mark_all_read(app),
        MENU_TOGGLE_NOTIFICATIONS => edit_settings(app, |settings| {
            settings.notifications.enabled = !settings.notifications.enabled;
        }),
        MENU_TOGGLE_CLOSE_TO_TRAY => edit_settings(app, |settings| {
            settings.system.close_to_tray = !settings.system.close_to_tray;
        }),
        MENU_TOGGLE_AUTOSTART => edit_settings(app, |settings| {
            settings.system.autostart = !settings.system.autostart;
        }),
        MENU_SETTINGS => open_settings(app),
        MENU_OPEN_LOGS => open_logs(),
        MENU_QUIT => {
            tracing::info!("quit requested from the tray");
            app.exit(0);
        }
        // The status line and the submenu label never raise an event.
        other => tracing::debug!(menu_item = other, "unhandled tray menu item"),
    }
}

fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    labels: &state::UiLabels,
    settings: &Settings,
    peers: &[TrayPeer],
    unread: u32,
) -> tauri::Result<Menu<R>> {
    // A disabled item, so the header reads as information rather than something to click.
    let status = MenuItem::with_id(
        app,
        MENU_STATUS,
        labels.tooltip(unread),
        false,
        None::<&str>,
    )?;
    let open = MenuItem::with_id(app, MENU_OPEN, &labels.open, true, None::<&str>)?;
    let mut items: Vec<Box<dyn IsMenuItem<R>>> = vec![Box::new(status), Box::new(open)];

    if !peers.is_empty() {
        let submenu = Submenu::with_id(app, MENU_CONVERSATIONS, &labels.conversations, true)?;
        for peer in peers.iter().take(MAX_TRAY_PEERS) {
            let item = MenuItem::with_id(
                app,
                peer_item_id(peer),
                conversation_label(peer),
                true,
                None::<&str>,
            )?;
            submenu.append(&item)?;
        }
        submenu.append(&PredefinedMenuItem::separator(app)?)?;
        submenu.append(&MenuItem::with_id(
            app,
            MENU_ALL_CONVERSATIONS,
            &labels.all_conversations,
            true,
            None::<&str>,
        )?)?;
        items.push(Box::new(submenu));
    }

    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        MENU_MARK_ALL_READ,
        &labels.mark_all_read,
        unread > 0,
        None::<&str>,
    )?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));

    items.push(Box::new(CheckMenuItem::with_id(
        app,
        MENU_TOGGLE_NOTIFICATIONS,
        &labels.notifications,
        true,
        settings.notifications.enabled,
        None::<&str>,
    )?));
    items.push(Box::new(CheckMenuItem::with_id(
        app,
        MENU_TOGGLE_CLOSE_TO_TRAY,
        &labels.close_to_tray,
        true,
        settings.system.close_to_tray,
        None::<&str>,
    )?));
    items.push(Box::new(CheckMenuItem::with_id(
        app,
        MENU_TOGGLE_AUTOSTART,
        &labels.autostart,
        true,
        settings.system.autostart,
        None::<&str>,
    )?));

    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        MENU_SETTINGS,
        &labels.settings,
        true,
        None::<&str>,
    )?));
    items.push(Box::new(MenuItem::with_id(
        app,
        MENU_OPEN_LOGS,
        &labels.open_logs,
        true,
        None::<&str>,
    )?));

    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(
        app,
        MENU_QUIT,
        &labels.quit,
        true,
        None::<&str>,
    )?));

    let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(|item| &**item).collect();
    Menu::with_items(app, &refs)
}

/// `muda` treats `&` as a mnemonic on Windows and Linux, so a nickname carrying one is escaped;
/// the unread count is appended so the menu says which conversation is waiting.
fn conversation_label(peer: &TrayPeer) -> String {
    let nickname = peer.nickname.replace('&', "&&");
    if peer.unread == 0 {
        nickname
    } else {
        format!("{nickname} ({})", peer.unread)
    }
}

fn peer_item_id(peer: &TrayPeer) -> String {
    format!("{OPEN_PEER_PREFIX}{}", peer.device_id)
}

/// Raises the window and asks the interface for that conversation. The reveal carries an
/// `open_chat` of its own when a notification is pending, so this request is emitted after it
/// and is the one the router applies last.
fn open_peer<R: Runtime>(app: &AppHandle<R>, device: &str) {
    crate::window::reveal(app);
    match crate::args::device_id(device) {
        Ok(peer) => crate::events::emit_open_chat(app, peer),
        Err(error) => tracing::warn!(%error, "the tray conversation could not be resolved"),
    }
}

fn open_settings<R: Runtime>(app: &AppHandle<R>) {
    crate::window::reveal(app);
    crate::events::emit_open_settings(app);
}

/// The log directory is only reachable once logging has started, which is a given by the time a
/// tray item can be clicked; a missing directory is a warning, not a failure.
fn open_logs() {
    let Some(logs) = crate::logging::logs() else {
        tracing::warn!("the log directory is unavailable");
        return;
    };
    if let Err(error) = logs.open_directory() {
        tracing::warn!(%error, "failed to open the log directory from the tray");
    }
}

/// Flips one field of the settings document and applies the same side effect the settings screen
/// does: the start-with-system setting is owned by the operating system, so it is reconciled
/// with the document after every save.
fn edit_settings<R: Runtime>(
    app: &AppHandle<R>,
    edit: impl FnOnce(&mut Settings) + Send + 'static,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = state::from_handle(&app) else {
            return;
        };
        let mut settings = match state.settings.get().await {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "failed to read the settings from the tray");
                return;
            }
        };
        edit(&mut settings);
        match state.settings.update(settings).await {
            Ok(saved) => {
                if let Err(error) = crate::autostart::apply(&app, saved.system.autostart) {
                    tracing::warn!(%error, "failed to apply the autostart registration");
                }
            }
            Err(error) => tracing::warn!(%error, "failed to save a settings change from the tray"),
        }
    });
}

/// Clears every unread conversation, oldest event first, on the actor that owns the database.
fn mark_all_read<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = state::from_handle(&app) else {
            return;
        };
        let waiting: Vec<TrayPeer> = state
            .peers_snapshot()
            .into_iter()
            .filter(|peer| peer.unread > 0)
            .collect();
        let mut cleared = 0u32;
        for peer in waiting {
            match state.session.mark_read(peer.device_id).await {
                Ok(_) => cleared += 1,
                Err(error) => tracing::warn!(%error, "a conversation could not be marked read"),
            }
        }
        tracing::info!(
            conversations = cleared,
            "conversations marked read from the tray"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use localme_core::domain::ids::DeviceId;

    fn peer(nickname: &str, unread: u32) -> TrayPeer {
        TrayPeer {
            device_id: DeviceId::generate(),
            nickname: nickname.to_owned(),
            unread,
        }
    }

    #[test]
    fn an_ampersand_in_a_nickname_is_escaped_for_the_menu() {
        // `muda` reads a lone `&` as a mnemonic, which would swallow the character and mark the
        // next one; a doubled one renders as a literal ampersand.
        assert_eq!(conversation_label(&peer("Tom & Jerry", 0)), "Tom && Jerry");
    }

    #[test]
    fn the_unread_count_is_shown_only_when_something_is_waiting() {
        assert_eq!(conversation_label(&peer("Alice", 0)), "Alice");
        assert_eq!(conversation_label(&peer("Alice", 3)), "Alice (3)");
    }

    #[test]
    fn an_item_id_names_the_conversation_it_opens() {
        let peer = peer("Alice", 0);
        assert_eq!(
            peer_item_id(&peer),
            format!("{OPEN_PEER_PREFIX}{}", peer.device_id)
        );
    }
}
