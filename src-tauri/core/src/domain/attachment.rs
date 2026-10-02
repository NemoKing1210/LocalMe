//! File attachments: the metadata that travels in a `chat` frame, the state of a transfer, and
//! the two values that make "the file arrived intact" checkable — a sanitised name and a digest.
//!
//! Nothing here touches the filesystem. The name a peer sends is input from a hostile network, so
//! [`FileName`] is a *sanitising* newtype: it is the only way a name becomes a path component, and
//! what it produces is safe to join onto a directory on Windows, macOS and Linux alike.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::domain::clock::UnixMillis;
use crate::domain::ids::{AttachmentId, DeviceId, MessageId};
use crate::domain::message::Direction;
use crate::error::DomainError;
use crate::protocol::limits::{MAX_ATTACHMENT_BYTES, MAX_FILE_NAME_CHARS};

/// What the interface can do with an attachment beyond offering it for download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    /// A raster image, so the bubble can show it instead of a file chip.
    Image,
    File,
}

impl AttachmentKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::File => "file",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "image" => Ok(Self::Image),
            "file" => Ok(Self::File),
            other => Err(DomainError::InvalidId {
                kind: "attachment kind",
                value: other.chars().take(16).collect(),
            }),
        }
    }

    /// Image formats a web view renders without a plugin.
    ///
    /// SVG is deliberately absent: it is the one image format that is also a document, and a
    /// preview must never be able to execute anything.
    #[must_use]
    pub fn of(name: &FileName) -> Self {
        let extension = name
            .as_str()
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "avif" => Self::Image,
            _ => Self::File,
        }
    }
}

/// Characters that cannot appear in a file name on at least one supported platform, plus the
/// separators that would otherwise let a peer choose the directory to write into.
const FORBIDDEN_IN_NAME: [char; 9] = ['/', '\\', '<', '>', ':', '"', '|', '?', '*'];

/// Names that Windows refuses outright, whatever the extension.
const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A file name that is safe to use as a path component.
///
/// The constructor never fails: a name that cannot be used is repaired rather than refused,
/// because refusing would leave the user with a transfer they could not accept for a reason that
/// has nothing to do with them. Only the last path component survives, control characters and
/// platform-forbidden characters are dropped, reserved names are prefixed, and the result is
/// truncated with its extension intact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileName(String);

impl FileName {
    /// The name used when nothing usable survives sanitising.
    pub const FALLBACK: &'static str = "file";

    #[must_use]
    pub fn sanitise(raw: &str) -> Self {
        let tail = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
        let cleaned: String = tail
            .chars()
            .filter(|ch| !ch.is_control() && !FORBIDDEN_IN_NAME.contains(ch))
            .collect();
        let trimmed = cleaned.trim().trim_matches(|ch| ch == '.' || ch == ' ');
        let mut name = if trimmed.is_empty() {
            Self::FALLBACK.to_owned()
        } else {
            trimmed.to_owned()
        };

        if Self::is_reserved(&name) {
            name.insert(0, '_');
        }
        Self(fit_to_limit(name))
    }

    fn is_reserved(name: &str) -> bool {
        let stem = name.split('.').next().unwrap_or(name);
        RESERVED_STEMS
            .iter()
            .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn kind(&self) -> AttachmentKind {
        AttachmentKind::of(self)
    }

    /// The name with `suffix` appended, keeping the extension last: `photo.jpg` + `.part` reads
    /// `photo.part.jpg`, which stays recognisable in a file manager.
    #[must_use]
    pub fn with_extension(&self, suffix: &str) -> String {
        match self.0.rsplit_once('.') {
            Some((stem, extension)) if !stem.is_empty() => {
                format!("{stem}{suffix}.{extension}")
            }
            _ => format!("{}{suffix}", self.0),
        }
    }
}

/// Truncates to [`MAX_FILE_NAME_CHARS`], keeping the extension when there is one.
fn fit_to_limit(name: String) -> String {
    if name.chars().count() <= MAX_FILE_NAME_CHARS {
        return name;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && extension.chars().count() <= 16 => {
            (stem, Some(extension))
        }
        _ => (name.as_str(), None),
    };
    let room = match extension {
        Some(extension) => MAX_FILE_NAME_CHARS.saturating_sub(extension.chars().count() + 1),
        None => MAX_FILE_NAME_CHARS,
    };
    let head: String = stem.chars().take(room).collect();
    match extension {
        Some(extension) => format!("{head}.{extension}"),
        None => head,
    }
}

impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for FileName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for FileName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::sanitise(&raw))
    }
}

impl FromStr for FileName {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::sanitise(s))
    }
}

/// A SHA-256 digest, carried as lowercase hex on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        use sha2::{Digest, Sha256 as Hasher};

        let mut hasher = Hasher::new();
        hasher.update(bytes);
        Self(hasher.finalize().into())
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
            out.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
        }
        out
    }

    /// # Errors
    ///
    /// Returns [`DomainError::InvalidDigest`] when the text is not 64 hex characters.
    pub fn from_hex(text: &str) -> Result<Self, DomainError> {
        let invalid = || DomainError::InvalidDigest {
            value: text.chars().take(80).collect(),
        };
        if text.len() != 64 || !text.is_ascii() {
            return Err(invalid());
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            let (Some(hi), Some(lo)) = (pair.first(), pair.get(1)) else {
                return Err(invalid());
            };
            let (Some(hi), Some(lo)) = (char::from(*hi).to_digit(16), char::from(*lo).to_digit(16))
            else {
                return Err(invalid());
            };
            let Some(slot) = bytes.get_mut(index) else {
                return Err(invalid());
            };
            *slot = u8::try_from(hi * 16 + lo).map_err(|_| invalid())?;
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Sha256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_hex(&raw).map_err(serde::de::Error::custom)
    }
}

/// Where an attachment is in its transfer.
///
/// `Queued`/`Sending` belong to the sender, `Receiving` to the recipient, and the last three are
/// terminal for both. Stored as text, so a future state is a migration rather than a code change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentState {
    /// The metadata is stored, the transfer has not started.
    Queued,
    Sending,
    Receiving,
    Complete,
    /// The user cancelled it, or the peer did.
    Cancelled,
    /// It could not be completed: the source vanished, the disk refused it, the digest did not
    /// match, or the peer no longer has it.
    Failed,
}

impl AttachmentState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sending => "sending",
            Self::Receiving => "receiving",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "queued" => Ok(Self::Queued),
            "sending" => Ok(Self::Sending),
            "receiving" => Ok(Self::Receiving),
            "complete" => Ok(Self::Complete),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::InvalidId {
                kind: "attachment state",
                value: other.chars().take(16).collect(),
            }),
        }
    }

    #[must_use]
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Complete | Self::Cancelled | Self::Failed)
    }

    #[must_use]
    pub const fn is_in_flight(self) -> bool {
        matches!(self, Self::Queued | Self::Sending | Self::Receiving)
    }
}

/// What a `chat` frame says about one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentMeta {
    pub id: AttachmentId,
    pub name: FileName,
    pub size: u64,
    pub kind: AttachmentKind,
}

impl AttachmentMeta {
    #[must_use]
    pub fn new(id: AttachmentId, name: FileName, size: u64) -> Self {
        let kind = name.kind();
        Self {
            id,
            name,
            size,
            kind,
        }
    }

    /// # Errors
    ///
    /// Returns [`DomainError::AttachmentTooLarge`] when the announced size is above the cap. The
    /// recipient still cancels such an offer rather than closing the connection: it is a policy
    /// refusal, not a protocol violation, and the user should see the message that carried it.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.size > MAX_ATTACHMENT_BYTES {
            return Err(DomainError::AttachmentTooLarge {
                size: self.size,
                max: MAX_ATTACHMENT_BYTES,
            });
        }
        Ok(())
    }
}

/// A stored attachment, as the database and the interface know it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: AttachmentId,
    pub message_id: MessageId,
    /// Always the *other* device, like [`ChatMessage::peer`](crate::domain::message::ChatMessage).
    pub peer: DeviceId,
    pub direction: Direction,
    pub name: FileName,
    pub size: u64,
    pub kind: AttachmentKind,
    pub state: AttachmentState,
    /// Bytes that have reached their destination, from the receiver's real file length and the
    /// sender's last acknowledgement.
    pub transferred: u64,
    /// The whole-file digest. The sender computes it before the first chunk leaves and keeps it
    /// here, so a resume after a restart does not have to read the source again; the recipient
    /// checks the file it assembled against it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<Sha256>,
    pub created_at: UnixMillis,
    /// Where the file is on this machine: the recipient's stored copy, or the sender's source.
    /// `None` while an incoming file is still being received and after a failure.
    pub path: Option<String>,
}

impl Attachment {
    #[must_use]
    pub fn meta(&self) -> AttachmentMeta {
        AttachmentMeta {
            id: self.id,
            name: self.name.clone(),
            size: self.size,
            kind: self.kind,
        }
    }

    /// A whole-file percentage, clamped so a hostile `transferred` cannot overflow the bar.
    #[must_use]
    pub fn percent(&self) -> u8 {
        if self.size == 0 {
            return 100;
        }
        let ratio = (self.transferred.min(self.size) as f64 / self.size as f64) * 100.0;
        // `as` on a float in 0..=100 is exact enough for a progress bar.
        ratio.round().clamp(0.0, 100.0) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_cannot_escape_its_directory() {
        for raw in [
            "../../etc/passwd",
            r"..\..\windows\system32\config",
            "/absolute/path/report.pdf",
            r"C:\Users\me\secret.txt",
        ] {
            let name = FileName::sanitise(raw);
            assert!(
                !name.as_str().contains(['/', '\\']),
                "{raw} kept a separator: {name}"
            );
            assert_ne!(name.as_str(), "..");
        }
        assert_eq!(FileName::sanitise("/tmp/report.pdf").as_str(), "report.pdf");
    }

    #[test]
    fn hostile_characters_are_dropped_rather_than_refused() {
        let name = FileName::sanitise("a\u{7}b<c>d:e\"f|g?h*i.png");
        assert_eq!(name.as_str(), "abcdefghi.png");
        assert_eq!(name.kind(), AttachmentKind::Image);
    }

    #[test]
    fn a_name_that_sanitises_to_nothing_falls_back() {
        for raw in ["", "   ", "...", "///"] {
            assert_eq!(FileName::sanitise(raw).as_str(), FileName::FALLBACK);
        }
    }

    #[test]
    fn reserved_device_names_are_prefixed() {
        assert_eq!(FileName::sanitise("CON.txt").as_str(), "_CON.txt");
        assert_eq!(FileName::sanitise("lpt1").as_str(), "_lpt1");
        assert_eq!(FileName::sanitise("console.log").as_str(), "console.log");
    }

    #[test]
    fn a_long_name_is_truncated_with_its_extension_intact() {
        let raw = format!("{}.jpeg", "a".repeat(400));
        let name = FileName::sanitise(&raw);
        assert_eq!(name.as_str().chars().count(), MAX_FILE_NAME_CHARS);
        assert!(name.as_str().ends_with(".jpeg"));
    }

    #[test]
    fn a_part_file_keeps_its_extension() {
        let name = FileName::sanitise("holiday.jpg");
        assert_eq!(name.with_extension(".part"), "holiday.part.jpg");
        assert_eq!(
            FileName::sanitise("README").with_extension(".part"),
            "README.part"
        );
    }

    #[test]
    fn a_digest_survives_the_wire_and_rejects_nonsense() {
        let digest = Sha256::of(b"hello");
        assert_eq!(
            digest.to_hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(Sha256::from_hex(&digest.to_hex()).expect("parses"), digest);
        assert!(Sha256::from_hex("nope").is_err());
        assert!(Sha256::from_hex(&"z".repeat(64)).is_err());
    }

    #[test]
    fn only_raster_images_are_previewed() {
        for name in ["a.PNG", "b.jpeg", "c.webp"] {
            assert_eq!(FileName::sanitise(name).kind(), AttachmentKind::Image);
        }
        for name in ["a.svg", "b.pdf", "c.tar.gz", "no-extension"] {
            assert_eq!(FileName::sanitise(name).kind(), AttachmentKind::File);
        }
    }

    #[test]
    fn progress_is_clamped() {
        let mut attachment = Attachment {
            id: AttachmentId::generate(),
            message_id: MessageId::generate(),
            peer: DeviceId::generate(),
            direction: Direction::Outgoing,
            name: FileName::sanitise("a.bin"),
            size: 200,
            kind: AttachmentKind::File,
            state: AttachmentState::Sending,
            transferred: 900,
            sha256: None,
            created_at: UnixMillis(1),
            path: None,
        };
        assert_eq!(attachment.percent(), 100);
        attachment.transferred = 0;
        assert_eq!(attachment.percent(), 0);
    }

    #[test]
    fn an_oversized_offer_is_rejected_by_the_meta_but_not_by_the_name() {
        let meta = AttachmentMeta::new(
            AttachmentId::generate(),
            FileName::sanitise("huge.iso"),
            MAX_ATTACHMENT_BYTES + 1,
        );
        assert!(meta.validate().is_err());
    }

    #[test]
    fn attachment_kind_labels_and_parsing_round_trip() {
        assert_eq!(AttachmentKind::Image.as_str(), "image");
        assert_eq!(AttachmentKind::File.as_str(), "file");
        assert_eq!(
            AttachmentKind::from_db("image").expect("image"),
            AttachmentKind::Image
        );
        assert_eq!(
            AttachmentKind::from_db("file").expect("file"),
            AttachmentKind::File
        );
        assert!(AttachmentKind::from_db("movie").is_err());
    }

    #[test]
    fn a_long_name_with_a_huge_extension_is_truncated_without_it() {
        // An extension over 16 characters is not worth keeping, so the whole name is simply cut.
        let raw = format!("{}.{}", "a".repeat(400), "e".repeat(20));
        let name = FileName::sanitise(&raw);
        assert_eq!(name.as_str().chars().count(), MAX_FILE_NAME_CHARS);
        assert!(name.as_str().chars().all(|c| c == 'a'));

        // A long name with no dot at all takes the same branch with no extension to keep.
        let plain = FileName::sanitise(&"b".repeat(MAX_FILE_NAME_CHARS * 2));
        assert_eq!(plain.as_str().chars().count(), MAX_FILE_NAME_CHARS);
    }

    #[test]
    fn a_file_name_is_a_string_on_the_wire_and_in_the_interface() {
        let name = FileName::sanitise("report.pdf");
        assert_eq!(name.to_string(), "report.pdf");
        let json = serde_json::to_string(&name).expect("serialise");
        assert_eq!(json, "\"report.pdf\"");
        let back: FileName = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, name);

        // Deserialising also sanitises, so a hostile wire value stays a safe path component.
        let hostile: FileName = serde_json::from_str(r#""..\\escape""#).expect("deserialise");
        assert_eq!(hostile.as_str(), "escape");

        let parsed: FileName = "photo.png".parse().expect("infallible");
        assert_eq!(parsed, FileName::sanitise("photo.png"));
    }

    #[test]
    fn a_digest_exposes_its_bytes_and_round_trips_through_serde() {
        let digest = Sha256::of(b"hello");
        assert_eq!(digest.as_bytes().len(), 32);
        assert_eq!(Sha256::from_bytes(*digest.as_bytes()), digest);
        assert_eq!(digest.to_string(), digest.to_hex());

        let json = serde_json::to_string(&digest).expect("serialise");
        assert_eq!(json, format!("\"{}\"", digest.to_hex()));
        let back: Sha256 = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, digest);
        assert!(serde_json::from_str::<Sha256>("\"nope\"").is_err());
    }

    #[test]
    fn attachment_states_round_trip_through_their_db_names() {
        for state in [
            AttachmentState::Queued,
            AttachmentState::Sending,
            AttachmentState::Receiving,
            AttachmentState::Complete,
            AttachmentState::Cancelled,
            AttachmentState::Failed,
        ] {
            assert_eq!(
                AttachmentState::from_db(state.as_str()).expect("parses"),
                state
            );
        }
        assert!(AttachmentState::from_db("teleporting").is_err());
    }

    #[test]
    fn only_the_in_flight_states_are_in_flight() {
        assert!(AttachmentState::Queued.is_in_flight());
        assert!(AttachmentState::Sending.is_in_flight());
        assert!(AttachmentState::Receiving.is_in_flight());
        assert!(!AttachmentState::Complete.is_in_flight());
        assert!(!AttachmentState::Cancelled.is_in_flight());
        assert!(!AttachmentState::Failed.is_in_flight());

        assert!(AttachmentState::Complete.is_finished());
        assert!(AttachmentState::Cancelled.is_finished());
        assert!(AttachmentState::Failed.is_finished());
        assert!(!AttachmentState::Queued.is_finished());
    }

    #[test]
    fn an_offer_at_the_size_cap_is_accepted() {
        let meta = AttachmentMeta::new(
            AttachmentId::generate(),
            FileName::sanitise("big.iso"),
            MAX_ATTACHMENT_BYTES,
        );
        assert!(meta.validate().is_ok());
    }

    #[test]
    fn an_empty_attachment_is_one_hundred_percent_done() {
        let attachment = Attachment {
            id: AttachmentId::generate(),
            message_id: MessageId::generate(),
            peer: DeviceId::generate(),
            direction: Direction::Outgoing,
            name: FileName::sanitise("empty"),
            size: 0,
            kind: AttachmentKind::File,
            state: AttachmentState::Complete,
            transferred: 0,
            sha256: None,
            created_at: UnixMillis(1),
            path: None,
        };
        assert_eq!(attachment.percent(), 100);
    }
}
