//! Process-wide state shared by the commands, the event forwarder, the tray and the window
//! policy. The real state lives in the `localme-core` session actor; this is the host's own
//! view of the world.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, RwLock};

use localme_core::domain::ids::DeviceId;
use localme_core::runtime::Core;
use localme_core::services::{SessionHandle, Settings, SettingsHandle};
use tauri::Manager;

/// Labels for surfaces the operating system draws (tray menu, notifications) outside the web
/// view; the front end pushes them so translations keep a single source of truth.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLabels {
    pub app_name: String,
    pub open: String,
    pub quit: String,
    pub tooltip_idle: String,
    /// `{count}` is replaced with the unread count.
    pub tooltip_unread: String,
    pub new_message: String,
    pub conversations: String,
    pub all_conversations: String,
    pub mark_all_read: String,
    pub notifications: String,
    pub close_to_tray: String,
    pub autostart: String,
    pub settings: String,
    pub open_logs: String,
}

impl Default for UiLabels {
    fn default() -> Self {
        // English defaults, so the tray is usable before the front end pushes translations.
        Self {
            app_name: "LocalMe".to_owned(),
            open: "Open LocalMe".to_owned(),
            quit: "Quit LocalMe".to_owned(),
            tooltip_idle: "LocalMe — no unread messages".to_owned(),
            tooltip_unread: "LocalMe — {count} unread".to_owned(),
            new_message: "New message".to_owned(),
            conversations: "Conversations".to_owned(),
            all_conversations: "All conversations".to_owned(),
            mark_all_read: "Mark all as read".to_owned(),
            notifications: "Notifications".to_owned(),
            close_to_tray: "Close to tray".to_owned(),
            autostart: "Start with the system".to_owned(),
            settings: "Settings…".to_owned(),
            open_logs: "Open logs folder".to_owned(),
        }
    }
}

impl UiLabels {
    #[must_use]
    pub fn tooltip(&self, unread: u32) -> String {
        if unread == 0 {
            return self.tooltip_idle.clone();
        }
        self.tooltip_unread.replace("{count}", &unread.to_string())
    }

    #[must_use]
    pub fn window_title(&self, unread: u32) -> String {
        if unread == 0 {
            self.app_name.clone()
        } else {
            format!("{} ({unread})", self.app_name)
        }
    }
}

/// One row of the tray's conversation list. Kept host-side so the menu can be rebuilt while the
/// window is hidden, when no `state_snapshot` is being delivered to the interface.
#[derive(Debug, Clone)]
pub struct TrayPeer {
    pub device_id: DeviceId,
    pub nickname: String,
    pub unread: u32,
}

/// Cached state so hot paths avoid the actors.
#[derive(Debug)]
pub struct Cached {
    /// The settings document as of the last change event.
    pub settings: Settings,
    /// The conversation list as of the last `peers` event, newest activity first.
    pub peers: Vec<TrayPeer>,
    /// Used to suppress a notification for the conversation already on screen.
    pub active_chat: Option<DeviceId>,
    /// The peer whose message produced the most recent notification.
    pub last_notified: Option<DeviceId>,
}

/// Returns `None` rather than panicking when the state has not been installed yet: a window event
/// can be delivered before `setup` has managed it.
#[must_use]
pub fn from_handle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Option<std::sync::Arc<AppState>> {
    app.try_state::<std::sync::Arc<AppState>>()
        .map(|state| state.inner().clone())
}

pub struct AppState {
    /// Behind a `Mutex` only so it can be taken exactly once, by the shutdown path.
    pub core: Mutex<Option<Core>>,
    pub session: SessionHandle,
    pub settings: SettingsHandle,
    pub cached: RwLock<Cached>,
    pub window_visible: AtomicBool,
    pub window_focused: AtomicBool,
    /// Mirrored for the tray and the window title.
    pub unread: AtomicU32,
    pub labels: RwLock<UiLabels>,
}

impl AppState {
    #[must_use]
    pub fn new(core: Core, settings: Settings) -> Self {
        Self {
            session: core.session.clone(),
            settings: core.settings.clone(),
            core: Mutex::new(Some(core)),
            cached: RwLock::new(Cached {
                settings,
                peers: Vec::new(),
                active_chat: None,
                last_notified: None,
            }),
            window_visible: AtomicBool::new(false),
            window_focused: AtomicBool::new(false),
            unread: AtomicU32::new(0),
            labels: RwLock::new(UiLabels::default()),
        }
    }

    /// The cached settings, or the defaults if the lock is poisoned.
    #[must_use]
    pub fn settings_snapshot(&self) -> Settings {
        self.cached
            .read()
            .map(|cached| cached.settings.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn peers_snapshot(&self) -> Vec<TrayPeer> {
        self.cached
            .read()
            .map(|cached| cached.peers.clone())
            .unwrap_or_default()
    }

    pub fn set_peers(&self, peers: Vec<TrayPeer>) {
        if let Ok(mut cached) = self.cached.write() {
            cached.peers = peers;
        }
    }

    #[must_use]
    pub fn active_chat(&self) -> Option<DeviceId> {
        self.cached
            .read()
            .ok()
            .and_then(|cached| cached.active_chat)
    }

    pub fn set_active_chat(&self, peer: Option<DeviceId>) {
        if let Ok(mut cached) = self.cached.write() {
            cached.active_chat = peer;
        }
    }

    pub fn set_last_notified(&self, peer: Option<DeviceId>) {
        if let Ok(mut cached) = self.cached.write() {
            cached.last_notified = peer;
        }
    }

    pub fn take_last_notified(&self) -> Option<DeviceId> {
        self.cached
            .write()
            .ok()
            .and_then(|mut cached| cached.last_notified.take())
    }

    #[must_use]
    pub fn labels_snapshot(&self) -> UiLabels {
        self.labels
            .read()
            .map(|labels| labels.clone())
            .unwrap_or_default()
    }

    pub fn set_labels(&self, labels: UiLabels) {
        if let Ok(mut current) = self.labels.write() {
            *current = labels;
        }
    }

    #[must_use]
    pub fn is_window_active(&self) -> bool {
        self.window_visible.load(Ordering::Relaxed) && self.window_focused.load(Ordering::Relaxed)
    }

    /// Takes the core out, so shutdown can run exactly once.
    #[must_use]
    pub fn take_core(&self) -> Option<Core> {
        self.core.lock().ok().and_then(|mut guard| guard.take())
    }
}

/// A real, discovery-free core in a temporary data directory, shared by the host's test modules.
#[cfg(test)]
pub(crate) mod test_support {
    use super::{AppState, Settings};
    use localme_core::domain::nickname::Nickname;
    use localme_core::runtime::{Core, CoreConfig};

    /// Builds the host state over a freshly started core, with `settings` as the cached document.
    pub(crate) async fn state_with(settings: Settings) -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("a temporary data directory");
        let config = CoreConfig::without_discovery(
            dir.path().to_path_buf(),
            Nickname::parse("Tester").expect("a valid nickname"),
            0,
            0,
        );
        let core = Core::start(config).await.expect("the core starts");
        (AppState::new(core, settings), dir)
    }

    /// The same, with the default settings document.
    pub(crate) async fn state() -> (AppState, tempfile::TempDir) {
        state_with(Settings::default()).await
    }
}

#[cfg(test)]
mod tests {
    use super::test_support;
    use super::*;
    use localme_core::services::SystemSettings;

    fn labels() -> UiLabels {
        UiLabels::default()
    }

    #[test]
    fn the_default_labels_are_the_english_ones() {
        let labels = labels();
        assert_eq!(labels.app_name, "LocalMe");
        assert_eq!(labels.open, "Open LocalMe");
        assert_eq!(labels.quit, "Quit LocalMe");
        assert_eq!(labels.tooltip_idle, "LocalMe — no unread messages");
        assert_eq!(labels.tooltip_unread, "LocalMe — {count} unread");
        assert_eq!(labels.new_message, "New message");
        assert_eq!(labels.conversations, "Conversations");
        assert_eq!(labels.all_conversations, "All conversations");
        assert_eq!(labels.mark_all_read, "Mark all as read");
        assert_eq!(labels.notifications, "Notifications");
        assert_eq!(labels.close_to_tray, "Close to tray");
        assert_eq!(labels.autostart, "Start with the system");
        assert_eq!(labels.settings, "Settings…");
        assert_eq!(labels.open_logs, "Open logs folder");
    }

    #[test]
    fn the_tooltip_substitutes_the_count_only_when_there_is_one() {
        let labels = labels();
        assert_eq!(labels.tooltip(0), "LocalMe — no unread messages");
        assert_eq!(labels.tooltip(1), "LocalMe — 1 unread");
        assert_eq!(labels.tooltip(7), "LocalMe — 7 unread");
        assert_eq!(labels.tooltip(u32::MAX), "LocalMe — 4294967295 unread");
    }

    #[test]
    fn the_window_title_gains_the_count_only_when_something_is_unread() {
        let labels = labels();
        assert_eq!(labels.window_title(0), "LocalMe");
        assert_eq!(labels.window_title(1), "LocalMe (1)");
        assert_eq!(labels.window_title(12), "LocalMe (12)");
    }

    #[test]
    fn the_labels_round_trip_through_camel_case_json() {
        let value = serde_json::to_value(labels()).expect("serialises");
        assert_eq!(value["appName"], "LocalMe");
        assert_eq!(value["tooltipIdle"], "LocalMe — no unread messages");
        assert_eq!(value["tooltipUnread"], "LocalMe — {count} unread");
        assert!(value.get("allConversations").is_some());
        assert!(value.get("markAllRead").is_some());
        assert!(value.get("closeToTray").is_some());
        assert!(value.get("openLogs").is_some());
        assert!(value.get("app_name").is_none());

        let back: UiLabels = serde_json::from_value(value.clone()).expect("deserialises");
        assert_eq!(serde_json::to_value(back).expect("re-serialises"), value);
    }

    #[tokio::test]
    async fn the_settings_snapshot_is_the_document_the_state_was_built_with() {
        let settings = Settings {
            system: SystemSettings {
                close_to_tray: false,
                ..SystemSettings::default()
            },
            ..Settings::default()
        };
        let (state, _dir) = test_support::state_with(settings.clone()).await;
        assert_eq!(state.settings_snapshot(), settings);
    }

    #[tokio::test]
    async fn peers_are_kept_in_the_order_they_were_set() {
        let (state, _dir) = test_support::state().await;
        assert!(state.peers_snapshot().is_empty());

        let peers = vec![
            TrayPeer {
                device_id: DeviceId::generate(),
                nickname: "Alice".to_owned(),
                unread: 2,
            },
            TrayPeer {
                device_id: DeviceId::generate(),
                nickname: "Bob".to_owned(),
                unread: 0,
            },
        ];
        state.set_peers(peers);

        let snapshot = state.peers_snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].nickname, "Alice");
        assert_eq!(snapshot[0].unread, 2);
        assert_eq!(snapshot[1].nickname, "Bob");
        assert_eq!(snapshot[1].unread, 0);
    }

    #[tokio::test]
    async fn the_active_chat_can_be_set_and_cleared() {
        let (state, _dir) = test_support::state().await;
        let peer = DeviceId::generate();
        assert_eq!(state.active_chat(), None);
        state.set_active_chat(Some(peer));
        assert_eq!(state.active_chat(), Some(peer));
        state.set_active_chat(None);
        assert_eq!(state.active_chat(), None);
    }

    #[tokio::test]
    async fn the_last_notified_peer_is_taken_rather_than_read() {
        let (state, _dir) = test_support::state().await;
        let peer = DeviceId::generate();
        assert_eq!(state.take_last_notified(), None);

        state.set_last_notified(Some(peer));
        assert_eq!(state.take_last_notified(), Some(peer));
        // Taking empties it, so a second reveal does not reopen the same conversation.
        assert_eq!(state.take_last_notified(), None);

        state.set_last_notified(None);
        assert_eq!(state.take_last_notified(), None);
    }

    #[tokio::test]
    async fn labels_can_be_replaced_wholesale() {
        let (state, _dir) = test_support::state().await;
        assert_eq!(state.labels_snapshot().app_name, "LocalMe");

        let custom = UiLabels {
            app_name: "LocalMe (ru)".to_owned(),
            open: "Открыть".to_owned(),
            ..UiLabels::default()
        };
        state.set_labels(custom);

        let snapshot = state.labels_snapshot();
        assert_eq!(snapshot.app_name, "LocalMe (ru)");
        assert_eq!(snapshot.open, "Открыть");
        assert_eq!(snapshot.quit, "Quit LocalMe");
    }

    #[tokio::test]
    async fn the_window_is_active_only_when_visible_and_focused() {
        let (state, _dir) = test_support::state().await;
        // Hidden and unfocused.
        assert!(!state.is_window_active());

        // Visible but unfocused.
        state.window_visible.store(true, Ordering::Relaxed);
        assert!(!state.is_window_active());

        // Visible and focused.
        state.window_focused.store(true, Ordering::Relaxed);
        assert!(state.is_window_active());

        // Focused but hidden.
        state.window_visible.store(false, Ordering::Relaxed);
        assert!(!state.is_window_active());
    }

    #[tokio::test]
    async fn the_core_is_handed_over_at_most_once() {
        let (state, _dir) = test_support::state().await;
        let core = state
            .take_core()
            .expect("the first take hands over the core");
        assert!(state.take_core().is_none());
        core.shutdown().await;
        assert!(state.take_core().is_none());
    }
}
