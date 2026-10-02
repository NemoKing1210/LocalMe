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
    pub mute: String,
    pub unmute: String,
    pub quit: String,
    pub tooltip_idle: String,
    /// `{count}` is replaced with the unread count.
    pub tooltip_unread: String,
    pub new_message: String,
}

impl Default for UiLabels {
    fn default() -> Self {
        // English defaults, so the tray is usable before the front end pushes translations.
        Self {
            app_name: "LocalMe".to_owned(),
            open: "Open LocalMe".to_owned(),
            mute: "Pause notifications".to_owned(),
            unmute: "Resume notifications".to_owned(),
            quit: "Quit LocalMe".to_owned(),
            tooltip_idle: "LocalMe — no unread messages".to_owned(),
            tooltip_unread: "LocalMe — {count} unread".to_owned(),
            new_message: "New message".to_owned(),
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

/// Cached state so hot paths avoid the actors.
#[derive(Debug)]
pub struct Cached {
    /// The settings document as of the last change event.
    pub settings: Settings,
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
