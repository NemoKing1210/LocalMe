//! Service actors.

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
