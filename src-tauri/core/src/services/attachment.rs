//! The file-transfer engine: reading a source in chunks, assembling a destination on disk, and
//! the two places a digest is computed.
//!
//! Everything here is filesystem work and arithmetic. Policy — when a transfer starts, whom to
//! ask, what to persist — lives in the session, so this module can be exercised on its own: no
//! channels, no clock, no network.
//!
//! Two invariants hold the design together:
//!
//! * **The file on disk is the authority on how much has arrived.** A `transferred` column can
//!   be one acknowledgement behind; the length of the part file cannot.
//! * **An incoming file is append-only.** A chunk that does not continue the file exactly is
//!   refused, and the refusal reports the real position, which is what lets the sender rewind.

use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256 as Hasher};

use crate::domain::attachment::{Attachment, FileName, Sha256};
use crate::domain::ids::AttachmentId;
use crate::protocol::limits::{FILE_CHUNK_BYTES, FILE_WINDOW_BYTES};

/// Suffix of a file that is still being received. It keeps the real extension so the file is
/// still recognisable if the application never completes the transfer: `photo.part.jpg`.
pub const PART_SUFFIX: &str = ".part";

/// The directory that holds one attachment's file, named after its identifier rather than its
/// name, so nothing a peer sends can choose a path.
#[must_use]
pub fn attachment_directory(files_root: &Path, id: AttachmentId) -> PathBuf {
    files_root.join(id.to_string())
}

/// Where a finished incoming file lives.
#[must_use]
pub fn stored_path(files_root: &Path, id: AttachmentId, name: &FileName) -> PathBuf {
    attachment_directory(files_root, id).join(name.as_str())
}

/// Where an incoming file is assembled.
#[must_use]
pub fn part_path(files_root: &Path, id: AttachmentId, name: &FileName) -> PathBuf {
    attachment_directory(files_root, id).join(name.with_extension(PART_SUFFIX))
}

/// Digests a whole file.
///
/// Blocking, and deliberately so: it is the one operation on a large file that cannot be
/// interleaved with anything else, and the caller runs it on a blocking task so the session's
/// actor keeps answering while a half-gigabyte is read.
///
/// # Errors
///
/// [`io::Error`] if the file cannot be opened or read.
pub fn hash_file(path: &Path) -> io::Result<Sha256> {
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path)?);
    let mut hasher = Hasher::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(Sha256::from_bytes(hasher.finalize().into()));
        }
        if let Some(filled) = buffer.get(..read) {
            hasher.update(filled);
        }
    }
}

/// A file on its way out.
///
/// The handle is kept open and its position tracked, so a transfer is a sequence of reads rather
/// than an open per chunk; [`OutgoingTransfer::rewind`] is the only thing that moves it backwards,
/// and it exists because the recipient's file is the authority on where the stream continues.
pub struct OutgoingTransfer {
    pub attachment: Attachment,
    pub path: PathBuf,
    /// Bytes the recipient has confirmed, which is where the window is measured from.
    pub acked: u64,
    /// Bytes handed to the socket, which is where the next read starts.
    pub sent: u64,
    /// Set once the last chunk is out and [`OutgoingTransfer::done`] has been sent.
    pub finished: bool,
    file: Option<File>,
    position: u64,
}

impl OutgoingTransfer {
    #[must_use]
    pub fn new(attachment: Attachment, path: PathBuf) -> Self {
        let transferred = attachment.transferred;
        Self {
            attachment,
            path,
            acked: transferred,
            sent: transferred,
            finished: false,
            file: None,
            position: transferred,
        }
    }

    #[must_use]
    pub fn id(&self) -> AttachmentId {
        self.attachment.id
    }

    /// Whether the window has room for another chunk.
    ///
    /// A transfer whose digest is still being computed has nothing to send: the digest has to be
    /// in the `file_done` that follows the last chunk, and the recipient checks the file it
    /// assembled against it.
    #[must_use]
    pub fn wants_chunk(&self) -> bool {
        self.attachment.sha256.is_some()
            && !self.finished
            && self.sent.saturating_sub(self.acked) < FILE_WINDOW_BYTES
    }

    /// Whether there is anything left to read at all.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.sent >= self.attachment.size
    }

    /// Moves the cursor to `offset`, which is what a recipient's correction asks for. The next
    /// read re-opens the file if the offset is behind the current position.
    pub fn rewind(&mut self, offset: u64) {
        self.sent = offset.min(self.attachment.size);
        self.acked = self.acked.min(offset);
        self.finished = false;
        if self.position > self.sent {
            self.file = None;
        }
    }

    /// Reads the next chunk, up to [`FILE_CHUNK_BYTES`], or `None` when the end is reached.
    ///
    /// # Errors
    ///
    /// [`io::Error`] if the source cannot be opened, seeked or read — including the case where
    /// the file has become shorter since it was attached, which is a real possibility on a
    /// machine the user is still using.
    pub fn read_chunk(&mut self) -> io::Result<Option<Vec<u8>>> {
        let remaining = self.attachment.size.saturating_sub(self.sent);
        if remaining == 0 {
            return Ok(None);
        }
        let want =
            usize::try_from(remaining.min(FILE_CHUNK_BYTES as u64)).unwrap_or(FILE_CHUNK_BYTES);
        if self.file.is_none() || self.position != self.sent {
            let mut file = File::open(&self.path)?;
            file.seek(SeekFrom::Start(self.sent))?;
            self.file = Some(file);
            self.position = self.sent;
        }
        let mut buffer = vec![0_u8; want];
        let file = self.file.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "the source file is not open")
        })?;
        file.read_exact(&mut buffer)?;
        self.sent += buffer.len() as u64;
        self.position = self.sent;
        Ok(Some(buffer))
    }
}

/// A file being assembled.
pub struct IncomingTransfer {
    pub attachment: Attachment,
    pub part: PathBuf,
    pub final_path: PathBuf,
    /// The recipient's real position: the length of the part file, plus what has been written
    /// and not yet flushed.
    pub received: u64,
    /// The last position reported to the sender.
    pub acked: u64,
    /// The digest the sender announced in `file_done`, kept while the file is being read back
    /// and verified. `Some` also means "do not start a second verification".
    pub expected: Option<Sha256>,
    file: Option<File>,
}

impl IncomingTransfer {
    /// Opens the part file, creating it if it is not there and *adopting* it if it is.
    ///
    /// Adoption is the whole point: a part file that survived a crash or a disconnect is the
    /// most precise statement of how much arrived, and starting from zero would throw it away.
    ///
    /// # Errors
    ///
    /// [`io::Error`] if the directory or the file cannot be created or opened.
    pub fn open(attachment: &Attachment, files_root: &Path) -> io::Result<Self> {
        let directory = attachment_directory(files_root, attachment.id);
        std::fs::create_dir_all(&directory)?;
        let part = part_path(files_root, attachment.id, &attachment.name);
        let file = OpenOptions::new().create(true).append(true).open(&part)?;
        let received = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        Ok(Self {
            attachment: attachment.clone(),
            part,
            final_path: stored_path(files_root, attachment.id, &attachment.name),
            received,
            // Nothing is confirmed until it is acknowledged; a resumed transfer is confirmed
            // again by the first chunk the sender sends.
            acked: 0,
            expected: None,
            file: Some(file),
        })
    }

    #[must_use]
    pub fn id(&self) -> AttachmentId {
        self.attachment.id
    }

    /// Appends one chunk.
    ///
    /// # Errors
    ///
    /// [`io::ErrorKind::InvalidInput`] when `offset` does not continue the file, which the caller
    /// answers with an acknowledgement rather than an error: it is how the sender learns to
    /// rewind. Other errors are real write failures.
    pub fn append(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        if offset != self.received {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the chunk does not continue the file",
            ));
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the part file is not open"))?;
        file.write_all(data)?;
        self.received += data.len() as u64;
        Ok(())
    }

    /// Flushes and closes the handle. The part file stays where it is.
    pub fn release(&mut self) {
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
        }
    }

    /// Moves the assembled file into place and drops the part name.
    ///
    /// # Errors
    ///
    /// [`io::Error`] if the flush or the rename fails.
    pub fn finish(&mut self) -> io::Result<()> {
        self.release();
        std::fs::rename(&self.part, &self.final_path)
    }

    /// Removes the part file. Used when the transfer is refused or its digest does not match, so
    /// an abandoned half-file does not sit in the data directory for ever.
    pub fn discard(&mut self) {
        self.release();
        if let Err(error) = std::fs::remove_file(&self.part) {
            tracing::debug!(%error, path = %self.part.display(), "no part file to remove");
        }
        self.received = 0;
        self.acked = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::attachment::AttachmentState;
    use crate::domain::clock::UnixMillis;
    use crate::domain::ids::{DeviceId, MessageId};
    use crate::domain::message::Direction;

    fn attachment(state: AttachmentState, size: u64, transferred: u64) -> Attachment {
        Attachment {
            id: AttachmentId::generate(),
            message_id: MessageId::generate(),
            peer: DeviceId::generate(),
            direction: Direction::Outgoing,
            name: FileName::sanitise("payload.bin"),
            size,
            kind: crate::domain::attachment::AttachmentKind::File,
            state,
            transferred,
            sha256: None,
            created_at: UnixMillis(1),
            path: None,
        }
    }

    fn write_source(dir: &Path, bytes: &[u8]) -> PathBuf {
        let path = dir.join("source.bin");
        std::fs::write(&path, bytes).expect("write");
        path
    }

    #[test]
    fn a_source_is_read_in_bounded_chunks_and_hashed_whole() {
        let dir = tempfile::tempdir().expect("temp dir");
        let size = (FILE_CHUNK_BYTES * 2 + 17) as u64;
        let bytes: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
        let path = write_source(dir.path(), &bytes);

        let mut transfer =
            OutgoingTransfer::new(attachment(AttachmentState::Sending, size, 0), path.clone());
        let mut read: Vec<u8> = Vec::new();
        while let Some(chunk) = transfer.read_chunk().expect("read") {
            assert!(chunk.len() <= FILE_CHUNK_BYTES);
            read.extend_from_slice(&chunk);
        }
        assert_eq!(read, bytes);
        assert_eq!(transfer.read_chunk().expect("read"), None);
        assert_eq!(hash_file(&path).expect("hash"), Sha256::of(&bytes));
    }

    #[test]
    fn a_window_limits_how_far_ahead_the_sender_may_run() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_source(dir.path(), &[0_u8; 16]);
        let size = (FILE_WINDOW_BYTES + FILE_CHUNK_BYTES as u64) * 2;
        let mut source = attachment(AttachmentState::Sending, size, 0);
        source.sha256 = Some(Sha256::of(b"anything"));
        let mut transfer = OutgoingTransfer::new(source, path);
        // The window is measured from what the recipient confirmed, not from the first chunk.
        transfer.acked = 0;
        transfer.sent = FILE_WINDOW_BYTES;
        assert!(!transfer.wants_chunk());
        transfer.rewind(0);
        assert!(transfer.wants_chunk());
        assert_eq!(transfer.sent, 0);
    }

    #[test]
    fn a_rewind_behind_the_cursor_re_opens_the_source() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bytes: Vec<u8> = (0..(FILE_CHUNK_BYTES * 3) as u32)
            .map(|i| i as u8)
            .collect();
        let path = write_source(dir.path(), &bytes);
        let mut transfer = OutgoingTransfer::new(
            attachment(AttachmentState::Sending, bytes.len() as u64, 0),
            path,
        );
        assert!(transfer.read_chunk().expect("read").is_some());
        assert!(transfer.read_chunk().expect("read").is_some());
        transfer.rewind(FILE_CHUNK_BYTES as u64);
        let chunk = transfer.read_chunk().expect("read").expect("a chunk");
        assert_eq!(chunk, bytes[FILE_CHUNK_BYTES..FILE_CHUNK_BYTES * 2]);
    }

    #[test]
    fn an_incoming_file_is_append_only_and_adopts_what_is_there() {
        let dir = tempfile::tempdir().expect("temp dir");
        let meta = attachment(AttachmentState::Receiving, 6, 0);
        let mut transfer = IncomingTransfer::open(&meta, dir.path()).expect("open");
        transfer.append(0, b"abc").expect("append");
        assert_eq!(transfer.received, 3);

        // Anything that is not the next chunk is refused, and the position is untouched.
        assert!(transfer.append(0, b"xyz").is_err());
        assert!(transfer.append(9, b"xyz").is_err());
        assert_eq!(transfer.received, 3);

        // A second transfer for the same attachment adopts the part file.
        transfer.release();
        let mut resumed = IncomingTransfer::open(&meta, dir.path()).expect("reopen");
        assert_eq!(resumed.received, 3);
        resumed.append(3, b"def").expect("append");
        assert_eq!(resumed.received, 6);
        resumed.finish().expect("finish");
        assert_eq!(
            std::fs::read(&resumed.final_path).expect("read"),
            b"abcdef".to_vec()
        );
        assert!(!resumed.part.exists());
    }

    #[test]
    fn a_discarded_transfer_leaves_nothing_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut transfer =
            IncomingTransfer::open(&attachment(AttachmentState::Receiving, 4, 0), dir.path())
                .expect("open");
        transfer.append(0, b"abcd").expect("append");
        transfer.discard();
        assert!(!transfer.part.exists());
        assert_eq!(transfer.received, 0);
    }

    #[test]
    fn a_part_file_keeps_the_real_extension() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut meta = attachment(AttachmentState::Receiving, 1, 0);
        meta.name = FileName::sanitise("holiday.jpg");
        let transfer = IncomingTransfer::open(&meta, dir.path()).expect("open");
        assert!(transfer.part.ends_with("holiday.part.jpg"));
        assert_eq!(
            transfer.final_path.file_name().and_then(|n| n.to_str()),
            Some("holiday.jpg")
        );
    }

    #[test]
    fn hashing_an_empty_file_is_the_empty_digest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_source(dir.path(), b"");
        assert_eq!(hash_file(&path).expect("hash"), Sha256::of(b""));
    }

    #[test]
    fn hashing_handles_exact_chunk_boundaries() {
        let dir = tempfile::tempdir().expect("temp dir");
        for size in [FILE_CHUNK_BYTES, FILE_CHUNK_BYTES + 1] {
            let bytes: Vec<u8> = (0..size).map(|index| (index % 253) as u8).collect();
            let path = write_source(dir.path(), &bytes);
            assert_eq!(
                hash_file(&path).expect("hash"),
                Sha256::of(&bytes),
                "size {size}"
            );
        }
    }

    #[test]
    fn hashing_a_file_that_is_not_there_is_an_io_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(hash_file(&dir.path().join("missing.bin")).is_err());
    }

    #[test]
    fn a_source_that_vanishes_before_the_read_is_an_io_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_source(dir.path(), b"abc");
        let mut transfer =
            OutgoingTransfer::new(attachment(AttachmentState::Sending, 3, 0), path.clone());
        std::fs::remove_file(&path).expect("remove");
        assert!(
            transfer.read_chunk().is_err(),
            "a vanished source must be an error, not a silent empty chunk"
        );
    }

    #[test]
    fn an_outgoing_transfer_reports_completion_and_waits_for_its_digest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = write_source(dir.path(), b"abc");
        let mut transfer = OutgoingTransfer::new(attachment(AttachmentState::Sending, 3, 0), path);

        // No digest yet: the sender must not start streaming.
        assert!(!transfer.wants_chunk());
        assert!(!transfer.is_complete());

        transfer.attachment.sha256 = Some(Sha256::of(b"abc"));
        assert!(transfer.wants_chunk());
        let _ = transfer.read_chunk().expect("read");
        assert!(transfer.is_complete(), "all bytes are out");
        assert_eq!(transfer.read_chunk().expect("read"), None);

        // Once the last chunk has been followed by `file_done`, nothing more is wanted.
        transfer.finished = true;
        assert!(!transfer.wants_chunk());
    }

    #[test]
    fn attachment_directories_are_named_after_the_identifier() {
        let dir = tempfile::tempdir().expect("temp dir");
        let id = AttachmentId::generate();
        let directory = attachment_directory(dir.path(), id);
        assert_eq!(directory, dir.path().join(id.to_string()));

        // A hostile name is sanitised before it ever becomes a path component, so the stored
        // path stays inside the attachment's own directory.
        let hostile = FileName::sanitise("../../escape.txt");
        let stored = stored_path(dir.path(), id, &hostile);
        assert!(
            stored.starts_with(&directory),
            "{stored:?} left {directory:?}"
        );
        assert_eq!(stored.parent(), Some(directory.as_path()));
        assert!(!hostile.as_str().contains(['/', '\\']));
    }

    #[test]
    fn an_incoming_transfer_reports_its_id_and_refuses_writes_after_release() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut transfer =
            IncomingTransfer::open(&attachment(AttachmentState::Receiving, 3, 0), dir.path())
                .expect("open");
        assert_eq!(transfer.id(), transfer.attachment.id);

        transfer.release();
        let error = transfer
            .append(0, b"xyz")
            .expect_err("the handle is closed");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn discarding_a_transfer_whose_part_file_is_already_gone_is_quiet() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut transfer =
            IncomingTransfer::open(&attachment(AttachmentState::Receiving, 3, 0), dir.path())
                .expect("open");
        transfer.release();
        std::fs::remove_file(&transfer.part).expect("remove");
        transfer.discard();
        assert_eq!(transfer.received, 0);
        assert_eq!(transfer.acked, 0);
    }
}
