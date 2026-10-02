//! Filesystem policy for attachments: what may be attached, and what the user can do with a file
//! that has arrived.
//!
//! The interface never sees a path it did not choose and never gets one back without the host
//! having looked at it first. A path crossing the IPC boundary is only a *request* to look: the
//! size, the name and the verdict come from the filesystem, in one place, so the preview in the
//! composer and the metadata the core stores cannot disagree.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use localme_core::domain::attachment::{AttachmentKind, FileName};
use localme_core::protocol::limits::MAX_ATTACHMENT_BYTES;
use tauri::{AppHandle, Manager, Runtime};

/// One file the interface may attach, and whatever is wrong with it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePick {
    pub path: String,
    /// The name that will be shown and sent: sanitised exactly as the core will send it, so the
    /// chip the user sees is the name the recipient gets.
    pub name: String,
    pub size: u64,
    /// `"image"` or `"file"`.
    pub kind: &'static str,
    /// `None` when the file can be attached; otherwise a code the interface translates.
    pub problem: Option<&'static str>,
}

impl FilePick {
    fn unusable(path: &Path, problem: &'static str) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            name: path
                .file_name()
                .map(|name| {
                    FileName::sanitise(&name.to_string_lossy())
                        .as_str()
                        .to_owned()
                })
                .unwrap_or_else(|| FileName::FALLBACK.to_owned()),
            size: 0,
            kind: "file",
            problem: Some(problem),
        }
    }
}

/// Adds one file to the asset protocol's scope, so the interface may render it.
///
/// Best effort: a file the protocol cannot be told about still attaches and still transfers, it
/// just cannot be previewed.
pub fn allow_preview<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    if let Err(error) = app.asset_protocol_scope().allow_file(path) {
        tracing::debug!(path = %path.display(), %error, "the preview scope refused a file");
    }
}

/// Describes a path as an attachment, and — when it is usable — adds it to the asset protocol's
/// scope so the interface can render it.
///
/// The scope is granted here, once, rather than opened wholesale: the interface can display
/// exactly the files the user chose and the files that were received, and nothing else.
pub fn inspect<R: Runtime>(app: &AppHandle<R>, path: &Path) -> FilePick {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "an attachment path is not readable");
            return FilePick::unusable(path, "missing");
        }
    };
    if !metadata.is_file() {
        return FilePick::unusable(path, "directory");
    }
    let size = metadata.len();
    let raw = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = FileName::sanitise(&raw);
    let kind = match name.kind() {
        AttachmentKind::Image => "image",
        AttachmentKind::File => "file",
    };
    if size > MAX_ATTACHMENT_BYTES {
        return FilePick {
            path: path.to_string_lossy().into_owned(),
            name: name.as_str().to_owned(),
            size,
            kind,
            problem: Some("tooLarge"),
        };
    }
    // Best effort: a file the asset protocol cannot be told about still attaches and still
    // transfers, it just cannot be previewed.
    allow_preview(app, path);
    FilePick {
        path: path.to_string_lossy().into_owned(),
        name: name.as_str().to_owned(),
        size,
        kind,
        problem: None,
    }
}

/// Opens a stored file with the platform's default application.
///
/// # Errors
///
/// [`io::Error`] when no handler could be started. Spawning is not awaited: the application
/// outlives the file's reader, and a handler that fails after it started is its own problem.
pub fn open(path: &Path) -> io::Result<()> {
    let mut command = match std::env::consts::OS {
        "windows" => {
            let mut command = Command::new("cmd");
            command.args(["/C", "start", ""]);
            command
        }
        "macos" => Command::new("open"),
        _ => Command::new("xdg-open"),
    };
    command.arg(path).spawn().map(|_| ())
}

/// Shows a file in the platform's file manager, with the file selected where the platform can.
///
/// # Errors
///
/// [`io::Error`] when no file manager could be started.
pub fn reveal(path: &Path) -> io::Result<()> {
    match std::env::consts::OS {
        "windows" => {
            let mut command = Command::new("explorer");
            // `explorer` wants the selection as one argument, comma first.
            command.arg(format!("/select,{}", path.display()));
            command.spawn().map(|_| ())
        }
        "macos" => Command::new("open").arg("-R").arg(path).spawn().map(|_| ()),
        _ => {
            let directory = path.parent().unwrap_or(path);
            Command::new("xdg-open").arg(directory).spawn().map(|_| ())
        }
    }
}

/// Copies a received file to a place the user chose.
///
/// # Errors
///
/// [`io::Error`] when the source cannot be read or the destination cannot be written.
pub fn copy(source: &Path, target: &Path) -> io::Result<u64> {
    std::fs::copy(source, target)
}

/// The stored path of an attachment that is ready to be opened, or why it is not.
///
/// # Errors
///
/// Returns an [`io::ErrorKind::NotFound`] error when the file is not on disk yet: a transfer
/// that is still running has no path, and a failed one has none either.
pub fn stored_path(path: Option<&str>) -> io::Result<PathBuf> {
    let Some(path) = path else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the file has not arrived yet",
        ));
    };
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the file is no longer there",
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_and_a_directory_path_are_reported_rather_than_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = FilePick::unusable(&dir.path().join("nope.bin"), "missing");
        assert_eq!(missing.problem, Some("missing"));
        assert_eq!(missing.name, "nope.bin");
        assert_eq!(missing.size, 0);

        // The name of a path is sanitised the same way the core will sanitise it, so the chip
        // and the recipient's copy agree.
        let odd = FilePick::unusable(Path::new("/tmp/../etc/passwd"), "missing");
        assert_eq!(odd.name, "passwd");
    }

    #[test]
    fn only_a_file_that_exists_has_a_stored_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("a.bin");
        std::fs::write(&file, b"x").expect("write");
        assert!(stored_path(None).is_err());
        assert!(stored_path(Some(&dir.path().to_string_lossy())).is_err());
        assert_eq!(
            stored_path(Some(&file.to_string_lossy())).expect("a file"),
            file
        );
    }
}
