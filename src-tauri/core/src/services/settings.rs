//! Application settings: a typed, versioned document plus the actor that owns it.
//!
//! The settings file is the one piece of state the *host* needs before the interface has
//! loaded — `startMinimized` and `closeToTray` decide what happens at startup and when the
//! window is closed. That is why the document and its persistence live here rather than in
//! the front end: one owner, one file, one schema, and no possibility of the Rust side and
//! the TypeScript side disagreeing about what was saved.
//!
//! The profile is deliberately *not* here. A nickname lives in the database beside the device
//! id it belongs to (see [`super::session`]), and duplicating it would create two answers to
//! "what am I called" that can drift apart.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::error::CoreError;

/// Capacity of the event channel. Settings change rarely; a consumer that falls this far
/// behind has stopped reading and does not need a backlog.
const EVENT_CHANNEL_CAPACITY: usize = 16;

/// Capacity of the command mailbox.
const COMMAND_CHANNEL_CAPACITY: usize = 32;

/// Current version of the settings schema.
pub const SETTINGS_VERSION: u32 = 1;

/// Interface languages this build ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    /// English.
    En,
    /// Russian.
    Ru,
}

impl Locale {
    /// The BCP 47 tag.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ru => "ru",
        }
    }
}

/// Which palette to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follow the operating system.
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Appearance settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    /// Light, dark, or follow the system.
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

/// Notification settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationSettings {
    /// Whether any native notification is shown.
    pub enabled: bool,
    /// Whether the notification contains the message text.
    pub show_text: bool,
    /// Whether a sound plays.
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

/// Operating-system integration settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SystemSettings {
    /// Launch at sign-in.
    pub autostart: bool,
    /// Launch hidden in the tray.
    pub start_minimized: bool,
    /// Keep running in the tray when the window is closed.
    ///
    /// Defaults to on: a messenger that stops receiving when its window is closed is not
    /// doing what the user expects. The settings screen states the consequence either way.
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

/// The settings document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Schema version, used to migrate a file written by an older build.
    pub version: u32,
    /// Whether the first-run screen has been completed.
    ///
    /// This is not derivable from the nickname: the first launch already writes a default
    /// nickname, so without a separate flag the welcome screen could not tell "a fresh
    /// install" from "a user who kept the name we suggested".
    pub onboarded: bool,
    /// Appearance.
    pub appearance: AppearanceSettings,
    /// Interface language.
    pub locale: Locale,
    /// Notifications.
    pub notifications: NotificationSettings,
    /// System integration.
    pub system: SystemSettings,
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
        }
    }
}

impl Settings {
    /// Brings a document loaded from disk up to the current schema version.
    ///
    /// Unknown fields are *not* an error and are not preserved: the document is written back
    /// in full on every change, and silently keeping fields this build does not understand
    /// would make a downgrade look like it worked. A version from the future is refused
    /// rather than guessed at.
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
        // Version 1 is the first schema, so there is nothing to translate yet; the mechanism
        // exists so that version 2 does not have to invent one.
        self.version = SETTINGS_VERSION;
        self.normalise();
        Ok(self)
    }

    /// Repairs values that are individually valid JSON but not usable.
    ///
    /// Applied on load and on update, so a hand-edited file cannot put the application into a
    /// state the interface cannot render.
    pub fn normalise(&mut self) {
        if !is_hex_colour(&self.appearance.accent) {
            self.appearance.accent = AppearanceSettings::default().accent;
        }
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

/// Commands the host application sends to the settings actor.
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

/// The host application's handle on the settings actor.
#[derive(Debug, Clone)]
pub struct SettingsHandle {
    commands: mpsc::Sender<Command>,
    events: broadcast::Sender<Settings>,
}

impl SettingsHandle {
    /// The current settings.
    ///
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

    /// Replaces the settings document.
    ///
    /// The actor writes the file before answering, so an `Ok` here means the change survives a
    /// restart — which is what a settings screen has to be able to promise.
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

    /// Subscribes to changes made through this handle.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Settings> {
        self.events.subscribe()
    }
}

/// Loads the settings file, repairs it if necessary, and starts the actor that owns it.
///
/// A file that cannot be parsed is *not* fatal: the defaults are used and the reason is
/// returned so the host can tell the user, because refusing to start over a settings file
/// would be a worse outcome than resetting it.
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // First launch: write nothing until the user changes something.
            (Settings::default(), None)
        }
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

/// Writes the document atomically: a settings file that is half-written when the machine
/// loses power is a file that resets the user's preferences.
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
    }

    #[test]
    fn a_minimal_document_fills_in_the_rest() {
        // `#[serde(default)]` means a file written by an older build, or hand-edited, still
        // produces a complete document rather than a deserialisation error.
        let partial: Settings = serde_json::from_str(r#"{"locale":"ru"}"#).expect("parses");
        assert_eq!(partial.locale, Locale::Ru);
        assert_eq!(partial.appearance.accent, "#6750A4");
        assert!(partial.system.close_to_tray);
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

        // A fresh actor over the same file sees the change.
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
        // The interface reads this shape; the TypeScript types are declared to match it.
        assert!(text.contains("\"closeToTray\""), "{text}");
        assert!(text.contains("\"showText\""), "{text}");
        assert!(text.contains("\"startMinimized\""), "{text}");
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
}
