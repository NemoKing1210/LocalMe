//! Process-wide state shared by the commands, the event forwarder, the tray and the window
//! policy.
//!
//! Everything mutable here is either an atomic flag or a value behind a lock that is held for
//! the duration of a single field access. The real state — peers, presence, messages — lives
//! in the `localme-core` session actor; what is here is the host's own view of the world:
//! which window is visible, which chat is open, which settings have been cached, and what the
//! interface last asked the tray and notifications to say.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, RwLock};

use localme_core::domain::ids::DeviceId;
use localme_core::runtime::Core;
use localme_core::services::{SessionHandle, Settings, SettingsHandle};
use tauri::Manager;

/// Labels the interface supplies in the user's language.
///
/// The tray menu and a native notification are drawn by the operating system, outside the
/// web view, so their text cannot come from the front end's catalogue at the moment they are
/// shown. The front end pushes the strings once, and again whenever the language changes,
/// which keeps a single source of truth for translations instead of a second catalogue in
/// Rust.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLabels {
    /// The application's name, used as the window title base.
    pub app_name: String,
    /// Tray item that shows the window.
    pub open: String,
    /// Tray item that pauses notifications.
    pub mute: String,
    /// Tray item that resumes notifications.
    pub unmute: String,
    /// Tray item that quits.
    pub quit: String,
    /// Tray tooltip when nothing is unread.
    pub tooltip_idle: String,
    /// Tray tooltip with unread messages; `{count}` is replaced.
    pub tooltip_unread: String,
    /// Notification body when the message text is hidden.
    pub new_message: String,
}

impl Default for UiLabels {
    fn default() -> Self {
        // English defaults, so the tray is usable in the window between process start and the
        // front end's first render.
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
    /// The tooltip for a given unread count.
    #[must_use]
    pub fn tooltip(&self, unread: u32) -> String {
        if unread == 0 {
            return self.tooltip_idle.clone();
        }
        self.tooltip_unread.replace("{count}", &unread.to_string())
    }

    /// The window title for a given unread count.
    ///
    /// Deliberately just the name and a number: a title has no room for a sentence, and the
    /// count is the only part that has to be read at a glance.
    #[must_use]
    pub fn window_title(&self, unread: u32) -> String {
        if unread == 0 {
            self.app_name.clone()
        } else {
            format!("{} ({unread})", self.app_name)
        }
    }
}

/// Derived state the host keeps so that hot paths do not have to ask the actors.
#[derive(Debug)]
pub struct Cached {
    /// The settings document as of the last change event.
    pub settings: Settings,
    /// The conversation the user is looking at, or `None` when the list is showing.
    ///
    /// Used for exactly one decision: whether a message that just arrived needs a native
    /// notification.
    pub active_chat: Option<DeviceId>,
    /// The peer whose arrival produced the most recent notification.
    ///
    /// Desktop notifications do not report clicks back to the application on every platform
    /// (see `docs/ARCHITECTURE.md` §12), so the nearest useful equivalent is offered instead:
    /// when the window is raised from the tray, the chat that notified last is opened.
    pub last_notified: Option<DeviceId>,
}

/// The application state, cloned out of a Tauri handle.
///
/// `AppState` is behind an `Arc`, so this copies a pointer. It returns `None` rather than
/// panicking when the state has not been installed yet: the window is created while the builder
/// is still running, so a window event can be delivered before `setup` has managed the state, and
/// a handler that panicked in that window would take the process down on startup.
#[must_use]
pub fn from_handle(app: &tauri::AppHandle) -> Option<std::sync::Arc<AppState>> {
    app.try_state::<std::sync::Arc<AppState>>()
        .map(|state| state.inner().clone())
}

/// Everything the host shares between its parts.
pub struct AppState {
    /// The running core.
    ///
    /// Behind a `Mutex` purely so it can be *taken* exactly once, by the shutdown path. It is
    /// never locked while an `.await` is in flight and never guards anything else.
    pub core: Mutex<Option<Core>>,
    /// Session commands and events.
    pub session: SessionHandle,
    /// The settings actor.
    pub settings: SettingsHandle,
    /// Cached settings and interface state.
    pub cached: RwLock<Cached>,
    /// Whether the main window is currently visible.
    pub window_visible: AtomicBool,
    /// Whether the main window currently has focus.
    pub window_focused: AtomicBool,
    /// Total unread messages, mirrored for the tray and the window title.
    pub unread: AtomicU32,
    /// Labels for the surfaces the operating system draws.
    pub labels: RwLock<UiLabels>,
}

impl AppState {
    /// Wraps a running core.
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

    /// The cached interface state.
    #[must_use]
    pub fn active_chat(&self) -> Option<DeviceId> {
        self.cached
            .read()
            .ok()
            .and_then(|cached| cached.active_chat)
    }

    /// Records which conversation the front end is showing.
    pub fn set_active_chat(&self, peer: Option<DeviceId>) {
        if let Ok(mut cached) = self.cached.write() {
            cached.active_chat = peer;
        }
    }

    /// Remembers the peer whose message produced the most recent notification.
    pub fn set_last_notified(&self, peer: Option<DeviceId>) {
        if let Ok(mut cached) = self.cached.write() {
            cached.last_notified = peer;
        }
    }

    /// Takes the peer the last notification was about, clearing it.
    pub fn take_last_notified(&self) -> Option<DeviceId> {
        self.cached
            .write()
            .ok()
            .and_then(|mut cached| cached.last_notified.take())
    }

    /// The labels the operating system surfaces are drawn with.
    #[must_use]
    pub fn labels_snapshot(&self) -> UiLabels {
        self.labels
            .read()
            .map(|labels| labels.clone())
            .unwrap_or_default()
    }

    /// Replaces the labels.
    pub fn set_labels(&self, labels: UiLabels) {
        if let Ok(mut current) = self.labels.write() {
            *current = labels;
        }
    }

    /// Whether the main window is visible and focused, i.e. the user is looking at it.
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
