//! Service actors.
//!
//! A service is a long-lived task with a private mailbox. Nothing outside reaches into its
//! state: the only way in is a command, and the only way out is an event. That is what keeps
//! the mutable parts of this application — the peer table and the presence machines — free of
//! locks and races.

pub mod events;
pub mod session;
pub mod settings;

pub use events::{CoreEvent, NoticeLevel};
pub use session::{
    SessionCommand, SessionConfig, SessionHandle, SessionRuntime, spawn as spawn_session,
};
pub use settings::{
    AppearanceSettings, DEFAULT_LOG_RETENTION_DAYS, Locale, LogLevel, LoggingSettings,
    MAX_LOG_RETENTION_DAYS, MIN_LOG_RETENTION_DAYS, NotificationSettings, Settings, SettingsHandle,
    SystemSettings, ThemeMode, spawn as spawn_settings,
};
