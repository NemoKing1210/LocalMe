//! Application settings: a typed, versioned document plus the actor that owns it.
//!
//! The document lives here rather than in the front end (one owner, one file, one schema),
//! because the host needs `startMinimized`/`closeToTray` before the interface has loaded. The
//! nickname is deliberately not here: it lives in the database beside the device id, and
//! duplicating it would create two answers to "what am I called".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::error::CoreError;

const EVENT_CHANNEL_CAPACITY: usize = 16;

const COMMAND_CHANNEL_CAPACITY: usize = 32;

/// Version 2 added the `logging` group; every group carries `#[serde(default)]`, so no
/// migration step was needed.
pub const SETTINGS_VERSION: u32 = 2;

pub const MIN_LOG_RETENTION_DAYS: u32 = 1;

pub const MAX_LOG_RETENTION_DAYS: u32 = 365;

pub const DEFAULT_LOG_RETENTION_DAYS: u32 = 14;

/// The front end keeps the other half of this list in `src/i18n/locales.ts`; a variant added
/// here without a catalogue there would let the user pick a language the interface cannot speak,
/// so `messages.spec.ts` fails when a locale in that list has no label of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    En,
    Ru,
    Es,
    De,
    Fr,
    Pt,
    /// Chinese, Simplified.
    Zh,
}

impl Locale {
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ru => "ru",
            Self::Es => "es",
            Self::De => "de",
            Self::Fr => "fr",
            Self::Pt => "pt",
            Self::Zh => "zh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme: ThemeMode,
    /// Accent colour the Material 3 palette is generated from, as `#rrggbb`.
    pub accent: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            accent: "#6750A4".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationSettings {
    pub enabled: bool,
    pub show_text: bool,
    pub sound: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_text: true,
            sound: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    /// Everything, including per-frame protocol detail. Verbose and slow.
    Debug,
}

impl LogLevel {
    /// The dependency tree stays at `warn` whatever the user picks: a debug-level messenger that
    /// also logs every mDNS packet is a log nobody can read, and the user's choice is about
    /// *our* records.
    #[must_use]
    pub const fn filter(self) -> &'static str {
        match self {
            Self::Error => "localme=error,localme_core=error",
            Self::Warn => "localme=warn,localme_core=warn",
            Self::Info => "localme=info,localme_core=info",
            Self::Debug => "localme=debug,localme_core=debug",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LoggingSettings {
    pub level: LogLevel,
    /// Pruning happens at startup and whenever the log rolls over to a new day, so this is a
    /// bound on the directory rather than a scheduled job.
    pub retention_days: u32,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            retention_days: DEFAULT_LOG_RETENTION_DAYS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SystemSettings {
    pub autostart: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            autostart: false,
            start_minimized: false,
            close_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    /// Not derivable from the nickname: the first launch already writes a default nickname, so
    /// without a separate flag the welcome screen could not tell a fresh install from a user who
    /// kept the suggested name.
    pub onboarded: bool,
    pub appearance: AppearanceSettings,
    pub locale: Locale,
    pub notifications: NotificationSettings,
    pub system: SystemSettings,
    pub logging: LoggingSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            onboarded: false,
            appearance: AppearanceSettings::default(),
            locale: Locale::En,
            notifications: NotificationSettings::default(),
            system: SystemSettings::default(),
            logging: LoggingSettings::default(),
        }
    }
}

impl Settings {
    /// Unknown fields are *not* an error and are not preserved: the document is written back in
    /// full on every change, and silently keeping fields this build does not understand would
    /// make a downgrade look like it worked. A version from the future is refused rather than
    /// guessed at.
    ///
    /// # Errors
    ///
    /// [`CoreError::Storage`] if the file was written by a newer build.
    pub fn migrate(mut self) -> Result<Self, CoreError> {
        if self.version > SETTINGS_VERSION {
            return Err(CoreError::Storage(crate::error::StorageError::InvalidRow(
                format!(
                    "settings version {} is newer than this build understands ({SETTINGS_VERSION})",
                    self.version
                ),
            )));
        }
        self.version = SETTINGS_VERSION;
        self.normalise();
        Ok(self)
    }

    /// Repairs values that are individually valid JSON but not usable, so a hand-edited file
    /// cannot put the application into a state the interface cannot render.
    pub fn normalise(&mut self) {
        if !is_hex_colour(&self.appearance.accent) {
            self.appearance.accent = AppearanceSettings::default().accent;
        }
        // Clamped rather than trusted: a hand-edited `0` would delete today's log on the next
        // start.
        self.logging.retention_days = self
            .logging
            .retention_days
            .clamp(MIN_LOG_RETENTION_DAYS, MAX_LOG_RETENTION_DAYS);
    }
}

/// Whether a string is a `#rrggbb` colour.
fn is_hex_colour(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some(digits) = bytes.strip_prefix(b"#") else {
        return false;
    };
    digits.len() == 6 && digits.iter().all(u8::is_ascii_hexdigit)
}

#[derive(Debug)]
enum Command {
    Get {
        reply: oneshot::Sender<Settings>,
    },
    Update {
        next: Settings,
        reply: oneshot::Sender<Result<Settings, CoreError>>,
    },
}

#[derive(Debug, Clone)]
pub struct SettingsHandle {
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<Settings>,
}

impl SettingsHandle {
    /// # Errors
    ///
    /// [`CoreError::Task`] if the actor has stopped.
    pub async fn get(&self) -> Result<Settings, CoreError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(Command::Get { reply })
            .await
            .map_err(|_| CoreError::ShuttingDown)?;
        receiver.await.map_err(|_| CoreError::ShuttingDown)
    }

    /// The actor writes the file before answering, so an `Ok` here means the change survives a
    /// restart.
    ///
    /// # Errors
    ///
    /// [`CoreError::Storage`] if the file could not be written; the previous document stays in
    /// effect.
    pub async fn update(&self, next: Settings) -> Result<Settings, CoreError> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .send(Command::Update { next, reply })
            .await
            .map_err(|_| CoreError::ShuttingDown)?;
        receiver.await.map_err(|_| CoreError::ShuttingDown)?
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Settings> {
        self.events.subscribe()
    }
}

/// A file that cannot be parsed is *not* fatal: the defaults are used and the reason is returned
/// so the host can tell the user, because refusing to start over a settings file is worse than
/// resetting it.
pub async fn spawn(path: PathBuf) -> (SettingsHandle, Option<String>) {
    let (settings, problem) = load(&path).await;
    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
    let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

    let handle = SettingsHandle {
        commands: commands_tx,
        events: events_tx.clone(),
    };
    tokio::spawn(actor(path, settings, commands_rx, events_tx));
    (handle, problem)
}

async fn load(path: &Path) -> (Settings, Option<String>) {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(parsed) => match parsed.migrate() {
                Ok(settings) => (settings, None),
                Err(error) => (
                    Settings::default(),
                    Some(format!("settings could not be used: {error}")),
                ),
            },
            Err(error) => (
                Settings::default(),
                Some(format!("settings file is not valid JSON: {error}")),
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Settings::default(), None),
        Err(error) => (
            Settings::default(),
            Some(format!("settings could not be read: {error}")),
        ),
    }
}

async fn actor(
    path: PathBuf,
    initial: Settings,
    mut commands: mpsc::Receiver<Command>,
    events: broadcast::Sender<Settings>,
) {
    let mut current = initial;
    while let Some(command) = commands.recv().await {
        match command {
            Command::Get { reply } => {
                let _ = reply.send(current.clone());
            }
            Command::Update { mut next, reply } => {
                next.version = SETTINGS_VERSION;
                next.normalise();
                match write(&path, &next).await {
                    Ok(()) => {
                        current = next.clone();
                        let _ = reply.send(Ok(next.clone()));
                        let _ = events.send(next);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "settings could not be saved");
                        let _ = reply.send(Err(error));
                    }
                }
            }
        }
    }
}

/// Writes the document atomically: a settings file half-written when the machine loses power
/// resets the user's preferences.
async fn write(path: &Path, settings: &Settings) -> Result<(), CoreError> {
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|error| CoreError::Task(format!("settings could not be serialised: {error}")))?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            CoreError::Storage(crate::error::StorageError::Sqlite(format!(
                "could not create {parent:?}: {error}"
            )))
        })?;
    }

    let temporary = path.with_extension("json.tmp");
    tokio::fs::write(&temporary, &json).await.map_err(|error| {
        CoreError::Storage(crate::error::StorageError::Sqlite(format!(
            "could not write {temporary:?}: {error}"
        )))
    })?;
    tokio::fs::rename(&temporary, path).await.map_err(|error| {
        CoreError::Storage(crate::error::StorageError::Sqlite(format!(
            "could not replace {path:?}: {error}"
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn settings_path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    #[test]
    fn defaults_are_sane() {
        let settings = Settings::default();
        assert_eq!(settings.version, SETTINGS_VERSION);
        assert!(
            !settings.onboarded,
            "a fresh install has not been onboarded"
        );
        assert_eq!(settings.locale, Locale::En);
        assert_eq!(settings.appearance.theme, ThemeMode::System);
        assert_eq!(settings.appearance.accent, "#6750A4");
        assert!(settings.notifications.enabled);
        assert!(settings.notifications.show_text);
        assert!(settings.notifications.sound);
        assert!(!settings.system.autostart);
        assert!(!settings.system.start_minimized);
        assert!(settings.system.close_to_tray);
        assert_eq!(settings.logging.level, LogLevel::Info);
        assert_eq!(settings.logging.retention_days, DEFAULT_LOG_RETENTION_DAYS);
    }

    #[test]
    fn every_shipped_locale_round_trips_through_its_tag() {
        // The tag is what the front end and `Intl` see, and what the settings file holds; a
        // variant whose tag did not survive serialisation would silently reset the language.
        let locales = [
            Locale::En,
            Locale::Ru,
            Locale::Es,
            Locale::De,
            Locale::Fr,
            Locale::Pt,
            Locale::Zh,
        ];
        for locale in locales {
            let json = serde_json::to_string(&locale).expect("serialises");
            assert_eq!(json, format!("\"{}\"", locale.tag()));
            let back: Locale = serde_json::from_str(&json).expect("parses");
            assert_eq!(back, locale);
        }
    }

    #[test]
    fn a_silly_log_retention_is_repaired() {
        let mut settings = Settings {
            logging: LoggingSettings {
                retention_days: 0,
                ..LoggingSettings::default()
            },
            ..Settings::default()
        };
        settings.normalise();
        assert_eq!(settings.logging.retention_days, MIN_LOG_RETENTION_DAYS);

        settings.logging.retention_days = u32::MAX;
        settings.normalise();
        assert_eq!(settings.logging.retention_days, MAX_LOG_RETENTION_DAYS);
    }

    #[test]
    fn every_log_level_installs_a_filter_for_our_own_crates() {
        for level in [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
        ] {
            let filter = level.filter();
            assert!(filter.contains("localme="), "{filter}");
            assert!(filter.contains("localme_core="), "{filter}");
        }
    }

    #[test]
    fn a_minimal_document_fills_in_the_rest() {
        // `#[serde(default)]` means a file written by an older build, or hand-edited, still
        // produces a complete document rather than a deserialisation error.
        let partial: Settings = serde_json::from_str(r#"{"locale":"ru"}"#).expect("parses");
        assert_eq!(partial.locale, Locale::Ru);
        assert_eq!(partial.appearance.accent, "#6750A4");
        assert!(partial.system.close_to_tray);
        assert_eq!(partial.logging, LoggingSettings::default());
    }

    #[test]
    fn the_document_round_trips_through_json() {
        let settings = Settings {
            locale: Locale::Ru,
            appearance: AppearanceSettings {
                theme: ThemeMode::Dark,
                accent: "#006A6A".to_owned(),
            },
            notifications: NotificationSettings {
                show_text: false,
                ..NotificationSettings::default()
            },
            system: SystemSettings {
                close_to_tray: false,
                ..SystemSettings::default()
            },
            ..Settings::default()
        };

        let json = serde_json::to_string(&settings).expect("serialises");
        let back: Settings = serde_json::from_str(&json).expect("parses");
        assert_eq!(back, settings);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let parsed: Settings = serde_json::from_str(r#"{"futureSetting":42,"locale":"en"}"#)
            .expect("unknown fields must not be fatal");
        assert_eq!(parsed.locale, Locale::En);
    }

    #[test]
    fn a_version_from_the_future_is_refused() {
        let raw = format!(r#"{{"version":{}}}"#, SETTINGS_VERSION + 1);
        let parsed: Settings = serde_json::from_str(&raw).expect("parses");
        assert!(parsed.migrate().is_err());
    }

    #[test]
    fn an_old_version_is_accepted_and_stamped() {
        let parsed: Settings =
            serde_json::from_str(r#"{"version":0,"locale":"ru"}"#).expect("parses");
        let migrated = parsed.migrate().expect("migrates");
        assert_eq!(migrated.version, SETTINGS_VERSION);
        assert_eq!(migrated.locale, Locale::Ru);
    }

    #[test]
    fn a_broken_accent_colour_is_repaired() {
        let mut settings = Settings {
            appearance: AppearanceSettings {
                accent: "not a colour".to_owned(),
                ..AppearanceSettings::default()
            },
            ..Settings::default()
        };
        settings.normalise();
        assert_eq!(settings.appearance.accent, "#6750A4");

        settings.appearance.accent = "#GGGGGG".to_owned();
        settings.normalise();
        assert_eq!(settings.appearance.accent, "#6750A4");

        settings.appearance.accent = "#00A0ff".to_owned();
        settings.normalise();
        assert_eq!(settings.appearance.accent, "#00A0ff");
    }

    #[tokio::test]
    async fn a_missing_file_starts_from_defaults() {
        let dir = tempdir().expect("tempdir");
        let (handle, problem) = spawn(settings_path(dir.path())).await;
        assert!(problem.is_none());
        assert_eq!(handle.get().await.expect("get"), Settings::default());
    }

    #[tokio::test]
    async fn changes_are_persisted_and_survive_a_restart() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());

        let (handle, _) = spawn(path.clone()).await;
        let mut next = handle.get().await.expect("get");
        next.locale = Locale::Ru;
        next.system.start_minimized = true;
        let saved = handle.update(next.clone()).await.expect("update");
        assert_eq!(saved, next);

        let (restarted, problem) = spawn(path).await;
        assert!(problem.is_none());
        assert_eq!(restarted.get().await.expect("get"), next);
    }

    #[tokio::test]
    async fn updates_notify_subscribers() {
        let dir = tempdir().expect("tempdir");
        let (handle, _) = spawn(settings_path(dir.path())).await;
        let mut events = handle.subscribe();

        let mut next = handle.get().await.expect("get");
        next.appearance.theme = ThemeMode::Dark;
        handle.update(next.clone()).await.expect("update");

        assert_eq!(events.recv().await.expect("an event"), next);
    }

    #[tokio::test]
    async fn a_corrupt_file_reports_the_problem_and_uses_defaults() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        tokio::fs::write(&path, b"{ this is not json")
            .await
            .expect("write");

        let (handle, problem) = spawn(path).await;
        assert!(
            problem.expect("a reported problem").contains("JSON"),
            "the reason must say what was wrong"
        );
        assert_eq!(handle.get().await.expect("get"), Settings::default());
    }

    #[tokio::test]
    async fn an_update_repairs_the_document_before_writing_it() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        let (handle, _) = spawn(path.clone()).await;

        let mut next = handle.get().await.expect("get");
        next.appearance.accent = "garbage".to_owned();
        let saved = handle.update(next).await.expect("update");
        assert_eq!(saved.appearance.accent, "#6750A4");

        let text = tokio::fs::read_to_string(&path).await.expect("read");
        assert!(text.contains("#6750A4"), "{text}");
    }

    #[tokio::test]
    async fn the_file_is_written_as_json_with_camel_case_keys() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        let (handle, _) = spawn(path.clone()).await;
        handle.update(Settings::default()).await.expect("update");

        let text = tokio::fs::read_to_string(&path).await.expect("read");
        assert!(text.contains("\"closeToTray\""), "{text}");
        assert!(text.contains("\"showText\""), "{text}");
        assert!(text.contains("\"startMinimized\""), "{text}");
        assert!(text.contains("\"retentionDays\""), "{text}");
        assert!(!text.contains("close_to_tray"), "{text}");
    }

    #[tokio::test]
    async fn the_temporary_file_does_not_survive_a_successful_write() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        let (handle, _) = spawn(path.clone()).await;
        handle.update(Settings::default()).await.expect("update");

        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[tokio::test]
    async fn a_handle_with_no_actor_left_reports_shutting_down() {
        // A command channel whose receiver has already gone: every call must be an error rather
        // than a hang, because the host treats it as "the core is stopping".
        let (commands, receiver) = mpsc::channel(1);
        drop(receiver);
        let (events, _) = broadcast::channel(1);
        let handle = SettingsHandle { commands, events };

        assert!(matches!(handle.get().await, Err(CoreError::ShuttingDown)));
        assert!(matches!(
            handle.update(Settings::default()).await,
            Err(CoreError::ShuttingDown)
        ));
    }

    #[tokio::test]
    async fn an_actor_that_never_replies_reports_shutting_down() {
        // The send succeeds, but the reply channel is dropped before an answer arrives.
        let (commands, mut receiver) = mpsc::channel(1);
        let (events, _) = broadcast::channel(1);
        let handle = SettingsHandle { commands, events };
        tokio::spawn(async move {
            let _ = receiver.recv().await;
        });

        assert!(matches!(
            handle.update(Settings::default()).await,
            Err(CoreError::ShuttingDown)
        ));
    }

    #[tokio::test]
    async fn a_settings_file_from_the_future_is_reported_and_reset() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        let raw = format!(r#"{{"version":{}}}"#, SETTINGS_VERSION + 1);
        tokio::fs::write(&path, raw).await.expect("write");

        let (handle, problem) = spawn(path).await;
        assert!(
            problem
                .expect("a reported problem")
                .contains("could not be used"),
            "a version this build cannot read must be reported, not guessed at"
        );
        assert_eq!(handle.get().await.expect("get"), Settings::default());
    }

    #[tokio::test]
    async fn an_unreadable_settings_path_is_reported_and_reset() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        // A directory where a file is expected: the read fails for a reason other than NotFound.
        tokio::fs::create_dir(&path).await.expect("create dir");

        let (handle, problem) = spawn(path).await;
        assert!(
            problem
                .expect("a reported problem")
                .contains("could not be read"),
            "an I/O failure must be reported"
        );
        assert_eq!(handle.get().await.expect("get"), Settings::default());
    }

    #[tokio::test]
    async fn an_update_fails_when_the_parent_cannot_be_created() {
        let dir = tempdir().expect("tempdir");
        // The parent of the settings path is a regular file, so `create_dir_all` must fail.
        let blocker = dir.path().join("blocker");
        tokio::fs::write(&blocker, b"not a directory")
            .await
            .expect("write");
        let (handle, _) = spawn(blocker.join("settings.json")).await;

        let error = handle
            .update(Settings::default())
            .await
            .expect_err("a write that cannot happen must be an error");
        assert!(matches!(error, CoreError::Storage(_)), "{error:?}");
    }

    #[tokio::test]
    async fn an_update_fails_when_the_temporary_file_cannot_be_written() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        // The temporary name is already a directory.
        tokio::fs::create_dir(path.with_extension("json.tmp"))
            .await
            .expect("create dir");
        let (handle, problem) = spawn(path).await;
        assert!(problem.is_none());

        let error = handle
            .update(Settings::default())
            .await
            .expect_err("the temporary file cannot be replaced by a directory");
        assert!(matches!(error, CoreError::Storage(_)), "{error:?}");
    }

    #[tokio::test]
    async fn an_update_fails_when_the_destination_is_a_directory() {
        let dir = tempdir().expect("tempdir");
        let path = settings_path(dir.path());
        // The destination path is a directory, so the rename must fail after the write succeeds.
        tokio::fs::create_dir(&path).await.expect("create dir");
        let (handle, _) = spawn(path).await;

        let error = handle
            .update(Settings::default())
            .await
            .expect_err("the rename onto a directory must fail");
        assert!(matches!(error, CoreError::Storage(_)), "{error:?}");
    }
}
