//! Logging setup: one subscriber for the process, one file per day under `logs/` in the data
//! directory (UTC day key). Each record is appended by opening, writing and closing the file, so
//! clearing and pruning work while the process runs; every filesystem step is best-effort,
//! because a log that cannot be written must not take the process down.

use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use localme_core::services::{
    DEFAULT_LOG_RETENTION_DAYS, LogLevel, LoggingSettings, MAX_LOG_RETENTION_DAYS,
    MIN_LOG_RETENTION_DAYS,
};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Registry;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;

pub const LOG_DIRECTORY: &str = "logs";

const FILE_PREFIX: &str = "localme";

const FILE_EXTENSION: &str = "log";

/// Longest record field the front end may send; longer ones are cut, not rejected.
const MAX_FRONTEND_MESSAGE_CHARS: usize = 4_096;

/// Filter used when neither `RUST_LOG` nor the settings document has an opinion.
const DEFAULT_FILTER: &str =
    "localme=info,localme_core=info,tauri=warn,tauri_runtime=warn,mdns_sd=warn";

static LOGS: OnceLock<Arc<Logs>> = OnceLock::new();

static LEVEL: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

/// Whether `RUST_LOG` was set, in which case the settings document does not override it.
static ENV_OVERRIDE: OnceLock<bool> = OnceLock::new();

/// One file in the log directory, as the settings screen sees it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub name: String,
    pub size_bytes: u64,
    /// Milliseconds since the Unix epoch.
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsInfo {
    /// Absolute path, so it can be read out in a bug report.
    pub directory: String,
    /// Newest first.
    pub files: Vec<LogFile>,
    pub total_bytes: u64,
    pub retention_days: u32,
}

/// The log directory and the policy applied to it, shared behind an `Arc` by the writer and the
/// commands that describe and clear it.
#[derive(Debug)]
pub struct Logs {
    directory: PathBuf,
    retention_days: AtomicU32,
    /// Day key of the last prune, so pruning happens once a day rather than once a record.
    last_prune: Mutex<String>,
}

impl Logs {
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            retention_days: AtomicU32::new(DEFAULT_LOG_RETENTION_DAYS),
            last_prune: Mutex::new(String::new()),
        }
    }

    /// Where the files are.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Applies retention and prunes immediately, so a lowered retention is visible at once.
    pub fn set_retention(&self, days: u32) {
        self.retention_days
            .store(clamp_retention(days), Ordering::Relaxed);
        self.prune();
    }

    #[must_use]
    pub fn retention_days(&self) -> u32 {
        self.retention_days.load(Ordering::Relaxed)
    }

    /// The daily files, newest first, with their sizes.
    #[must_use]
    pub fn files(&self) -> Vec<LogFile> {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Vec::new();
        };

        let mut files: Vec<LogFile> = entries
            .flatten()
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                if !metadata.is_file() {
                    return None;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                parse_date_key(&name)?;
                let modified_ms = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|age| age.as_millis() as u64);
                Some(LogFile {
                    name,
                    size_bytes: metadata.len(),
                    modified_ms,
                })
            })
            .collect();

        // The name carries the date, so a plain reverse sort is newest first.
        files.sort_by(|left, right| right.name.cmp(&left.name));
        files
    }

    #[must_use]
    pub fn info(&self) -> LogsInfo {
        let files = self.files();
        let total_bytes = files.iter().map(|file| file.size_bytes).sum();
        LogsInfo {
            directory: self.directory.display().to_string(),
            files,
            total_bytes,
            retention_days: self.retention_days(),
        }
    }

    /// Deletes every log file, returning the number of bytes freed.
    ///
    /// A file that cannot be removed is left alone; the next record recreates today's file.
    pub fn clear(&self) -> u64 {
        let mut freed = 0;
        for file in self.files() {
            if fs::remove_file(self.directory.join(&file.name)).is_ok() {
                freed += file.size_bytes;
            }
        }
        freed
    }

    pub fn prune(&self) {
        let today = days_since_epoch(SystemTime::now());
        let today_key = date_key(today);
        if let Ok(mut last) = self.last_prune.lock() {
            // Once a day is enough: the window moves by a day at a time.
            if *last == today_key {
                return;
            }
            *last = today_key;
        }

        let retention = i64::from(self.retention_days());
        let cutoff = date_key(today - (retention - 1));
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(key) = parse_date_key(&name) else {
                // Anything else in the directory belongs to somebody else.
                continue;
            };
            // ISO dates compare correctly as strings, which is the whole reason for the format.
            if key.as_str() < cutoff.as_str() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// # Errors
    ///
    /// [`io::Error`] if no file manager could be started, which is what a headless Linux session
    /// or a locked-down desktop will report.
    pub fn open_directory(&self) -> io::Result<()> {
        let mut command = opener(&self.directory);
        command.spawn().map(|_| ())
    }

    fn open_today(&self) -> Option<File> {
        self.prune();
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(
                self.directory
                    .join(file_name(days_since_epoch(SystemTime::now()))),
            )
            .ok()
    }
}

#[must_use]
pub fn logs() -> Option<&'static Arc<Logs>> {
    LOGS.get()
}

/// Installs the global subscriber, writing to `stderr` and to a daily file under `data_dir`.
/// Called once from `setup`; a second call leaves the first subscriber in place. Returns the
/// directory the files are written to.
pub fn init(data_dir: &Path) -> PathBuf {
    let logs = Arc::new(Logs::new(data_dir.join(LOG_DIRECTORY)));
    // Deliberately ignoring the error: if the directory cannot be created, `stderr` still gets
    // every record and the application still runs.
    let _ = fs::create_dir_all(logs.directory());
    logs.prune();
    let directory = logs.directory().to_path_buf();
    let _ = LOGS.set(logs.clone());

    let from_env = EnvFilter::try_from_default_env().ok();
    let _ = ENV_OVERRIDE.set(from_env.is_some());
    let filter = from_env.unwrap_or_else(|| EnvFilter::new(DEFAULT_FILTER));
    let (filter, handle) = reload::Layer::new(filter);
    let _ = LEVEL.set(handle);

    // ANSI escapes are a terminal affordance: a GUI launch and a log file must not contain
    // them, so they are on only when stderr is actually a terminal.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_ansi(io::stderr().is_terminal())
                .with_writer(LogsWriter { logs }),
        )
        .try_init();

    directory
}

/// With `panic = "abort"` in the release profile the hook is the last code that runs, so it must
/// record the panic itself.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic with a non-text payload".to_owned());
        let location = info
            .location()
            .map(|place| format!("{}:{}:{}", place.file(), place.line(), place.column()))
            .unwrap_or_else(|| "an unknown location".to_owned());
        tracing::error!(panic = %payload, location = %location, "the application panicked");
        previous(info);
    }));
}

/// `RUST_LOG` wins over the stored level, so the escape hatch keeps working when the interface
/// is what is broken.
pub fn set_level(level: LogLevel) {
    if ENV_OVERRIDE.get().copied().unwrap_or(false) {
        tracing::debug!(
            chosen = level.filter(),
            "RUST_LOG is set, so the stored log level is not applied"
        );
        return;
    }
    match LEVEL.get() {
        Some(handle) => {
            if let Err(error) = handle.reload(EnvFilter::new(level.filter())) {
                tracing::warn!(%error, "the log level could not be changed");
            }
        }
        None => tracing::debug!("logging was not initialised; the log level was not applied"),
    }
}

pub fn apply(settings: &LoggingSettings) {
    if let Some(logs) = logs() {
        logs.set_retention(settings.retention_days);
    }
    set_level(settings.level);
}

/// The web view has no filesystem access and its console is invisible in a packaged build, so an
/// error thrown by a component would otherwise exist only on a screen the user has already closed.
pub fn log_frontend(level: &str, message: &str, context: Option<&str>) {
    let message = truncate(message);
    let context = context.map(truncate).unwrap_or_default();
    match level {
        "error" => tracing::error!(source = "frontend", %context, "{message}"),
        "warn" => tracing::warn!(source = "frontend", %context, "{message}"),
        "info" => tracing::info!(source = "frontend", %context, "{message}"),
        _ => tracing::debug!(source = "frontend", %context, "{message}"),
    }
}

fn clamp_retention(days: u32) -> u32 {
    days.clamp(MIN_LOG_RETENTION_DAYS, MAX_LOG_RETENTION_DAYS)
}

/// Cuts a front-end message to a length a log can hold, on a character boundary.
fn truncate(value: &str) -> String {
    if value.chars().count() <= MAX_FRONTEND_MESSAGE_CHARS {
        return value.to_owned();
    }
    let mut text: String = value.chars().take(MAX_FRONTEND_MESSAGE_CHARS).collect();
    text.push('…');
    text
}

#[must_use]
pub fn opener(directory: &Path) -> std::process::Command {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";

    // On Windows `explorer` reports a non-zero status even when it opened the folder, so the
    // caller checks only that the process started.
    let mut command = std::process::Command::new(program);
    command.arg(directory);
    command
}

#[derive(Clone)]
struct LogsWriter {
    logs: Arc<Logs>,
}

impl<'writer> MakeWriter<'writer> for LogsWriter {
    type Writer = MultiWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        MultiWriter {
            stderr: io::stderr(),
            file: self.logs.open_today(),
        }
    }
}

struct MultiWriter {
    stderr: io::Stderr,
    file: Option<File>,
}

impl Write for MultiWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        // `stderr` decides the result: it is the writer that cannot fail in a way the caller
        // could act on, and the subscriber ignores whatever comes back.
        let written = self.stderr.write(buffer)?;
        if let Some(file) = self.file.as_mut() {
            // Best effort on purpose: a full disk must not turn a log record into a panic.
            let _ = file.write_all(buffer);
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
        self.stderr.flush()
    }
}

/// `localme.YYYY-MM-DD.log`.
fn file_name(days: i64) -> String {
    format!("{FILE_PREFIX}.{}.{FILE_EXTENSION}", date_key(days))
}

/// The date in a file name, if the name follows our own convention.
fn parse_date_key(name: &str) -> Option<String> {
    let rest = name.strip_prefix(FILE_PREFIX)?.strip_prefix('.')?;
    let (date, extension) = rest.rsplit_once('.')?;
    if extension != FILE_EXTENSION || date.len() != 10 {
        return None;
    }
    let bytes = date.as_bytes();
    let shaped = bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    shaped.then(|| date.to_owned())
}

fn days_since_epoch(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|age| (age.as_secs() / 86_400) as i64)
        .unwrap_or(0)
}

/// `YYYY-MM-DD` from days since the epoch (Howard Hinnant's `civil_from_days`), exact for every
/// date and leap year this application will see without a date dependency.
fn date_key(days: i64) -> String {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = month_position + if month_position < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A logs directory in a temporary folder.
    fn temporary() -> (tempfile::TempDir, Arc<Logs>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let logs = Arc::new(Logs::new(dir.path().join(LOG_DIRECTORY)));
        fs::create_dir_all(logs.directory()).expect("create");
        (dir, logs)
    }

    #[test]
    fn the_date_key_matches_known_dates() {
        assert_eq!(date_key(0), "1970-01-01");
        assert_eq!(date_key(1), "1970-01-02");
        assert_eq!(date_key(10_957), "2000-01-01");
        assert_eq!(date_key(11_016), "2000-02-29");
        assert_eq!(date_key(19_723), "2024-01-01");
        assert_eq!(date_key(20_000), "2024-10-04");
    }

    #[test]
    fn the_file_name_carries_its_date() {
        assert_eq!(file_name(0), "localme.1970-01-01.log");
    }

    #[test]
    fn only_our_own_file_names_are_recognised() {
        assert_eq!(
            parse_date_key("localme.2026-10-01.log").as_deref(),
            Some("2026-10-01")
        );
        assert!(parse_date_key("localme.2026-10-01.txt").is_none());
        assert!(parse_date_key("other.2026-10-01.log").is_none());
        assert!(parse_date_key("localme.log").is_none());
        assert!(parse_date_key("localme.2026-1-1.log").is_none());
        assert!(parse_date_key("localme.2026-10-01.log.1").is_none());
        assert!(parse_date_key("settings.json").is_none());
    }

    #[test]
    fn records_land_in_the_file_for_the_day() {
        let (_dir, logs) = temporary();
        let writer = LogsWriter { logs: logs.clone() };
        let mut record = writer.make_writer();
        record.write_all(b"a record for the test\n").expect("write");
        record.flush().expect("flush");

        let files = logs.files();
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(
            files[0].name,
            file_name(days_since_epoch(SystemTime::now()))
        );
        let text = fs::read_to_string(logs.directory().join(&files[0].name)).expect("read");
        assert!(text.contains("a record for the test"), "{text}");
    }

    #[test]
    fn the_writer_keeps_the_first_record_of_a_new_day_and_drops_the_old_ones() {
        let (_dir, logs) = temporary();
        let today = days_since_epoch(SystemTime::now());
        logs.set_retention(7);
        for offset in [0, 1, 30] {
            fs::write(logs.directory().join(file_name(today - offset)), b"x").expect("write");
        }
        fs::write(logs.directory().join("settings.json"), b"not ours").expect("write");

        // The marker was set by `set_retention`, so the day is cleared to make this a rollover.
        *logs.last_prune.lock().expect("lock") = String::new();
        logs.prune();

        assert!(logs.directory().join(file_name(today)).exists());
        assert!(logs.directory().join(file_name(today - 1)).exists());
        assert!(!logs.directory().join(file_name(today - 30)).exists());
        assert!(logs.directory().join("settings.json").exists());
    }

    #[test]
    fn a_silly_retention_is_clamped() {
        let (_dir, logs) = temporary();
        logs.set_retention(0);
        assert_eq!(logs.retention_days(), MIN_LOG_RETENTION_DAYS);
        logs.set_retention(u32::MAX);
        assert_eq!(logs.retention_days(), MAX_LOG_RETENTION_DAYS);
    }

    #[test]
    fn the_summary_counts_the_files_and_their_bytes() {
        let (_dir, logs) = temporary();
        let today = days_since_epoch(SystemTime::now());
        fs::write(logs.directory().join(file_name(today)), vec![b'x'; 16]).expect("write");

        let info = logs.info();
        assert_eq!(info.total_bytes, 16);
        assert_eq!(info.files.len(), 1);
        assert_eq!(info.directory, logs.directory().display().to_string());
    }

    #[test]
    fn clearing_removes_the_files_and_reports_the_space() {
        let (_dir, logs) = temporary();
        let today = days_since_epoch(SystemTime::now());
        fs::write(logs.directory().join(file_name(today)), vec![b'x'; 16]).expect("write");

        assert_eq!(logs.clear(), 16);
        assert!(logs.files().is_empty());
    }

    #[test]
    fn a_long_front_end_message_is_cut_rather_than_stored_whole() {
        let long = "x".repeat(MAX_FRONTEND_MESSAGE_CHARS + 10);
        let cut = truncate(&long);
        assert_eq!(cut.chars().count(), MAX_FRONTEND_MESSAGE_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(truncate("short"), "short");
    }

    #[test]
    fn every_level_has_its_own_filter() {
        assert_eq!(LogLevel::Error.filter(), "localme=error,localme_core=error");
        assert_eq!(LogLevel::Warn.filter(), "localme=warn,localme_core=warn");
        assert_eq!(LogLevel::Info.filter(), "localme=info,localme_core=info");
        assert_eq!(LogLevel::Debug.filter(), "localme=debug,localme_core=debug");
    }

    #[test]
    fn a_level_is_parsed_from_its_lowercase_name() {
        assert_eq!(
            serde_json::from_str::<LogLevel>("\"debug\"").expect("parses"),
            LogLevel::Debug
        );
        assert_eq!(
            serde_json::from_str::<LogLevel>("\"warn\"").expect("parses"),
            LogLevel::Warn
        );
        assert!(serde_json::from_str::<LogLevel>("\"trace\"").is_err());
        assert_eq!(
            serde_json::to_string(&LogLevel::Error).expect("serialises"),
            "\"error\""
        );
    }

    #[test]
    fn a_retention_request_is_clamped_to_the_supported_range() {
        assert_eq!(clamp_retention(0), MIN_LOG_RETENTION_DAYS);
        assert_eq!(clamp_retention(1), 1);
        assert_eq!(
            clamp_retention(DEFAULT_LOG_RETENTION_DAYS),
            DEFAULT_LOG_RETENTION_DAYS
        );
        assert_eq!(
            clamp_retention(MAX_LOG_RETENTION_DAYS),
            MAX_LOG_RETENTION_DAYS
        );
        assert_eq!(clamp_retention(u32::MAX), MAX_LOG_RETENTION_DAYS);
    }

    #[test]
    fn a_new_directory_starts_with_the_default_retention() {
        let (dir, logs) = temporary();
        assert_eq!(logs.retention_days(), DEFAULT_LOG_RETENTION_DAYS);
        assert_eq!(logs.directory(), dir.path().join(LOG_DIRECTORY));
    }

    #[test]
    fn lowering_the_retention_prunes_at_once() {
        let (_dir, logs) = temporary();
        let today = days_since_epoch(SystemTime::now());
        let old = logs.directory().join(file_name(today - 40));
        fs::write(&old, b"old").expect("write");

        logs.set_retention(7);
        assert_eq!(logs.retention_days(), 7);
        assert!(!old.exists());
    }

    #[test]
    fn pruning_happens_once_a_day() {
        let (_dir, logs) = temporary();
        let today = days_since_epoch(SystemTime::now());
        let old = logs.directory().join(file_name(today - 40));
        logs.set_retention(7); // Sets the day marker.
        fs::write(&old, b"old").expect("write");

        // The once-a-day guard makes a second prune on the same day a no-op...
        logs.prune();
        assert!(old.exists());

        // ... and clearing the marker lets the next prune remove it.
        *logs.last_prune.lock().expect("lock") = String::new();
        logs.prune();
        assert!(!old.exists());
    }

    #[test]
    fn files_ignore_anything_that_is_not_a_daily_log() {
        let (_dir, logs) = temporary();
        let today = days_since_epoch(SystemTime::now());
        fs::write(logs.directory().join(file_name(today)), b"x").expect("write");
        fs::write(logs.directory().join("notes.txt"), b"x").expect("write");
        fs::create_dir(logs.directory().join("archive")).expect("mkdir");

        let files = logs.files();
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].name, file_name(today));
    }

    #[test]
    fn clearing_counts_every_byte_it_frees_and_is_repeatable() {
        let (_dir, logs) = temporary();
        let today = days_since_epoch(SystemTime::now());
        fs::write(logs.directory().join(file_name(today)), vec![b'a'; 10]).expect("write");
        fs::write(logs.directory().join(file_name(today - 1)), vec![b'b'; 5]).expect("write");
        fs::write(logs.directory().join("notes.txt"), b"kept").expect("write");

        assert_eq!(logs.clear(), 15);
        assert!(logs.files().is_empty());
        // Anything that is not one of ours is left alone.
        assert!(logs.directory().join("notes.txt").exists());
        assert_eq!(logs.clear(), 0);
    }

    #[test]
    fn the_day_number_counts_whole_utc_days() {
        use std::time::Duration;
        assert_eq!(days_since_epoch(UNIX_EPOCH), 0);
        assert_eq!(
            days_since_epoch(UNIX_EPOCH + Duration::from_secs(86_399)),
            0
        );
        assert_eq!(
            days_since_epoch(UNIX_EPOCH + Duration::from_secs(86_400)),
            1
        );
    }

    #[test]
    fn a_negative_day_is_before_the_epoch() {
        assert_eq!(date_key(-1), "1969-12-31");
        assert_eq!(file_name(-1), "localme.1969-12-31.log");
    }

    #[test]
    fn the_opener_names_the_platforms_file_manager() {
        let command = opener(Path::new("/tmp/logs"));
        #[cfg(target_os = "windows")]
        assert_eq!(command.get_program().to_string_lossy().as_ref(), "explorer");
        #[cfg(target_os = "macos")]
        assert_eq!(command.get_program().to_string_lossy().as_ref(), "open");
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        assert_eq!(command.get_program().to_string_lossy().as_ref(), "xdg-open");
        assert!(
            command
                .get_args()
                .any(|arg| arg.to_string_lossy().as_ref() == "/tmp/logs")
        );
    }

    /// `init` installs the process-global subscriber and reload handle, so this is the one test
    /// that touches `LOGS`/`LEVEL`; it exercises the documented "apply without a restart" path.
    #[test]
    fn applying_settings_changes_the_retention_without_a_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let returned = init(dir.path());
        assert_eq!(returned, dir.path().join(LOG_DIRECTORY));
        assert!(logs().is_some(), "logging was initialised");

        apply(&LoggingSettings {
            level: LogLevel::Debug,
            retention_days: 3,
        });
        assert_eq!(logs().expect("initialised").retention_days(), 3);

        // Reloading to another level must not panic; `RUST_LOG`, when set, wins by design.
        set_level(LogLevel::Warn);
    }
}
