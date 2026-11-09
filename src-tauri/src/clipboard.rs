//! Clipboard policy for attachments: a file copied in the file manager, or a screenshot taken
//! with the system's own shortcut, can be attached without leaving the keyboard.
//!
//! The clipboard is read here rather than in the interface because a paste event carries no
//! path: the web view sees a `File` object with no location, and a copied image has no path at
//! all. A copied file keeps its own path — nothing is copied, exactly as with a drop — while an
//! image that exists only on the clipboard is written once, under the data directory, so the
//! message that sends it keeps its preview after a restart. That directory is the only thing
//! this module is allowed to delete from.

use std::io;
use std::path::{Path, PathBuf};

use clipboard_rs::common::{ContentFormat, RustImage, RustImageData};
use clipboard_rs::{Clipboard, ClipboardContext};
use localme_core::protocol::limits::MAX_ATTACHMENT_BYTES;
use tauri::{AppHandle, Runtime};

use crate::files::{self, FilePick};

/// Where an image that arrived on the clipboard is written, inside the data directory.
pub const PASTED_DIRECTORY: &str = "pasted";

/// The clipboard's attachable contents, described exactly as the picker and the drop are.
///
/// Files win over an image: copying a file often puts a thumbnail on the clipboard too, and the
/// file is what the user copied. A clipboard that cannot be read, or holds nothing attachable,
/// is an empty answer rather than an error: the paste simply changes nothing, and the interface
/// tells the user why.
#[must_use]
pub fn read<R: Runtime>(app: &AppHandle<R>, data_dir: &Path) -> Vec<FilePick> {
    let clipboard = match ClipboardContext::new() {
        Ok(clipboard) => clipboard,
        Err(error) => {
            tracing::debug!(%error, "the clipboard is unavailable");
            return Vec::new();
        }
    };

    if clipboard.has(ContentFormat::Files) {
        match clipboard.get_files() {
            Ok(paths) if !paths.is_empty() => {
                return paths
                    .iter()
                    .map(|path| files::inspect(app, Path::new(path)))
                    .collect();
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, "the clipboard's file list could not be read"),
        }
    }

    if clipboard.has(ContentFormat::Image) {
        match clipboard.get_image() {
            Ok(image) => match stage_image(data_dir, &image) {
                Ok(path) => return vec![files::inspect(app, &path)],
                Err(error) => tracing::debug!(%error, "a copied image could not be stored"),
            },
            Err(error) => tracing::debug!(%error, "the clipboard's image could not be read"),
        }
    }

    Vec::new()
}

/// Writes a copied image as a PNG under the data directory, and answers where.
///
/// The pixel count is checked before the image is encoded: a decoded image costs four bytes per
/// pixel, so an enormous copy is refused instead of being written to disk and refused later.
///
/// # Errors
///
/// [`io::Error`] when the image is too large, the staging directory cannot be created, the data
/// directory is not valid UTF-8, or the image cannot be encoded.
fn stage_image(data_dir: &Path, image: &RustImageData) -> io::Result<PathBuf> {
    let (width, height) = image.get_size();
    if u64::from(width) * u64::from(height) * 4 > MAX_ATTACHMENT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the image is {width}×{height}, too large to attach"),
        ));
    }

    let directory = data_dir.join(PASTED_DIRECTORY);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("pasted-{}.png", uuid::Uuid::new_v4()));
    let target = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the data directory is not valid UTF-8",
        )
    })?;
    image
        .save_to_path(target)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(path)
}

/// Removes a staged image the user took out of the composer before sending it.
///
/// A path from the interface is only a request: anything outside the staging directory is
/// somebody's own file, and this must never become a way to delete arbitrary files. A file that
/// is already gone, or a staging directory that was never created, is not an error.
pub fn discard(data_dir: &Path, path: &Path) {
    let Ok(directory) = std::fs::canonicalize(data_dir.join(PASTED_DIRECTORY)) else {
        return;
    };
    let Ok(target) = std::fs::canonicalize(path) else {
        return;
    };
    if target.parent() != Some(directory.as_path()) {
        return;
    }
    if let Err(error) = std::fs::remove_file(&target) {
        tracing::debug!(path = %target.display(), %error, "a removed paste could not be deleted");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A one-pixel PNG: enough for the image pipeline, small enough to read in the test.
    const ONE_PIXEL_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    fn one_pixel_image() -> RustImageData {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(ONE_PIXEL_PNG)
            .expect("the fixture is base64");
        RustImageData::from_bytes(&bytes).expect("the fixture is a PNG")
    }

    #[test]
    fn a_copied_image_is_staged_as_a_png_under_the_data_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = stage_image(dir.path(), &one_pixel_image()).expect("stored");

        assert!(path.starts_with(dir.path().join(PASTED_DIRECTORY)));
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
        assert!(std::fs::metadata(&path).expect("the file exists").len() > 0);
    }

    #[test]
    fn every_staged_image_gets_its_own_name() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image = one_pixel_image();
        let first = stage_image(dir.path(), &image).expect("stored");
        let second = stage_image(dir.path(), &image).expect("stored");
        assert_ne!(first, second);
    }

    #[test]
    fn a_discarded_paste_is_deleted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = stage_image(dir.path(), &one_pixel_image()).expect("stored");

        discard(dir.path(), &path);

        assert!(!path.exists());
    }

    #[test]
    fn a_file_outside_the_staging_directory_is_left_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let own = dir.path().join("notes.txt");
        std::fs::write(&own, b"mine").expect("write");
        // Create the staging directory so the refusal is about the file, not its absence.
        std::fs::create_dir_all(dir.path().join(PASTED_DIRECTORY)).expect("mkdir");

        discard(dir.path(), &own);

        assert!(
            own.exists(),
            "the interface must not be able to delete a user's file"
        );
    }

    #[test]
    fn discarding_something_already_gone_is_quiet() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join(PASTED_DIRECTORY).join("gone.png");

        discard(dir.path(), &missing);
    }
}
