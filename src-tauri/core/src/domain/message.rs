//! Chat messages.

use serde::{Deserialize, Serialize};

use crate::domain::clock::UnixMillis;
use crate::domain::ids::{DeviceId, MessageId};
use crate::error::DomainError;
use crate::protocol::limits::MAX_BODY_CHARS;

/// Which way a stored message travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Received from the peer.
    Incoming,
    /// Sent by this device.
    Outgoing,
}

impl Direction {
    /// Whether this direction is outgoing.
    #[must_use]
    pub const fn is_outgoing(self) -> bool {
        matches!(self, Self::Outgoing)
    }

    /// Storage representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "in",
            Self::Outgoing => "out",
        }
    }

    /// Parses the storage representation.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidId`] for anything other than `in`/`out`.
    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "in" => Ok(Self::Incoming),
            "out" => Ok(Self::Outgoing),
            other => Err(DomainError::InvalidId {
                kind: "message direction",
                value: other.chars().take(16).collect(),
            }),
        }
    }
}

/// Delivery progress of a message we sent.
///
/// `Received` is what an incoming row stores; it exists so both directions share one type
/// and the UI can render a single status column without a special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageStatus {
    /// Written to the outbound queue, not yet acknowledged.
    Sending,
    /// Handed to a live socket. Distinct from `Delivered` only for diagnostics.
    Sent,
    /// The recipient acknowledged it after committing it to its own database.
    Delivered,
    /// The recipient's row.
    Received,
    /// The peer went offline (or its queue overflowed) before acknowledging.
    Failed,
}

impl MessageStatus {
    /// Storage representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Received => "received",
            Self::Failed => "failed",
        }
    }

    /// Parses the storage representation.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::InvalidId`] for an unrecognised status, which can only happen
    /// if the database was written by a future version.
    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "sending" => Ok(Self::Sending),
            "sent" => Ok(Self::Sent),
            "delivered" => Ok(Self::Delivered),
            "received" => Ok(Self::Received),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::InvalidId {
                kind: "message status",
                value: other.chars().take(16).collect(),
            }),
        }
    }
}

/// A validated message body.
///
/// Input normalisation happens exactly once, here:
///
/// * `\r\n` and lone `\r` become `\n`, so a paste from a Windows application does not carry
///   stray carriage returns into the database;
/// * trailing whitespace is removed (a body ending in newlines is almost always an accident
///   of the composer);
/// * control characters other than `\n` and `\t` are rejected — the body is rendered as
///   text, and forward-compatibility with terminal-ish renderers is not worth the ambiguity;
/// * the character count is bounded by [`MAX_BODY_CHARS`].
///
/// On the wire and over IPC it is a plain string, and deserialising it runs the same
/// validation as [`MessageBody::parse`], so an invalid body cannot enter through serde.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct MessageBody(String);

impl<'de> Deserialize<'de> for MessageBody {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl MessageBody {
    /// Validates and normalises a body.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::EmptyBody`], [`DomainError::BodyTooLong`] or
    /// [`DomainError::BodyControlChar`].
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let normalised = raw.replace("\r\n", "\n").replace('\r', "\n");
        let trimmed = normalised.trim_end();

        if trimmed.trim().is_empty() {
            return Err(DomainError::EmptyBody);
        }
        let len = trimmed.chars().count();
        if len > MAX_BODY_CHARS {
            return Err(DomainError::BodyTooLong {
                actual: len,
                max: MAX_BODY_CHARS,
            });
        }
        if trimmed
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(DomainError::BodyControlChar);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The normalised body text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Number of characters, for the composer's counter.
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.0.chars().count()
    }

    /// Rebuilds a body from storage, where it was already validated.
    ///
    /// # Errors
    ///
    /// Returns the validation error if the stored text would not be accepted today, which
    /// can only happen if the limits were tightened between versions.
    pub fn from_stored(value: String) -> Result<Self, DomainError> {
        Self::parse(&value)
    }
}

/// Characters kept from a body in a [`MessagePreview`].
///
/// The peer list is re-sent on every presence tick, so carrying a full [`MAX_BODY_CHARS`] body
/// per peer would put multiples of a conversation on the wire to draw one truncated line.
pub const MAX_PREVIEW_CHARS: usize = 160;

/// The newest message in a conversation, reduced to what the user list draws.
///
/// Deliberately not a [`ChatMessage`]: the list needs the direction and a short body, and
/// nothing else the message carries — its identifier, statuses and receive time are the chat
/// view's business.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePreview {
    /// Which way the newest message travelled.
    pub direction: Direction,
    /// The body, bounded to [`MAX_PREVIEW_CHARS`] with a trailing ellipsis when it was cut.
    pub body: String,
}

impl MessagePreview {
    /// Builds a preview from a message body.
    #[must_use]
    pub fn new(body: &MessageBody, direction: Direction) -> Self {
        Self {
            direction,
            body: summarise(body.as_str()),
        }
    }
}

/// Cuts `text` to [`MAX_PREVIEW_CHARS`], marking the cut so a clipped word is not read as the
/// whole message.
fn summarise(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(MAX_PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// A stored chat message.
///
/// The field names here are the ones the interface receives, which is why the type is
/// serialised rather than copied into a separate DTO: a parallel type would be one more place
/// for a rename to be forgotten.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    /// Globally unique, time-ordered identifier.
    pub id: MessageId,
    /// The conversation this message belongs to: always the *other* device.
    pub peer: DeviceId,
    /// Which way it travelled.
    pub direction: Direction,
    /// Validated body.
    pub body: MessageBody,
    /// Sender's wall clock, displayed as the message time.
    pub sent_at: UnixMillis,
    /// Receiver's wall clock, used to order messages whose `sent_at` is untrustworthy.
    pub received_at: UnixMillis,
    /// Delivery state.
    pub status: MessageStatus,
    /// Whether the local user has seen it (incoming only).
    pub read: bool,
}

impl ChatMessage {
    /// Whether this message is unread and incoming, i.e. whether it counts towards a badge.
    #[must_use]
    pub const fn is_unread_incoming(&self) -> bool {
        matches!(self.direction, Direction::Incoming) && !self.read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_bodies() {
        assert_eq!(MessageBody::parse(""), Err(DomainError::EmptyBody));
        assert_eq!(MessageBody::parse("   "), Err(DomainError::EmptyBody));
        assert_eq!(MessageBody::parse("\n\n\t"), Err(DomainError::EmptyBody));
    }

    #[test]
    fn normalises_carriage_returns() {
        let body = MessageBody::parse("line one\r\nline two\rline three").expect("valid");
        assert_eq!(body.as_str(), "line one\nline two\nline three");
    }

    #[test]
    fn strips_trailing_whitespace_but_keeps_interior_newlines() {
        let body = MessageBody::parse("hello\n\nworld\n\n").expect("valid");
        assert_eq!(body.as_str(), "hello\n\nworld");
    }

    #[test]
    fn rejects_disallowed_control_characters() {
        assert_eq!(
            MessageBody::parse("bell\u{7}here"),
            Err(DomainError::BodyControlChar)
        );
        // Tab and newline are the allowed exceptions.
        assert!(MessageBody::parse("a\tb\nc").is_ok());
    }

    #[test]
    fn counts_characters_for_the_length_limit() {
        let max = "a".repeat(MAX_BODY_CHARS);
        assert!(MessageBody::parse(&max).is_ok());

        let over = "a".repeat(MAX_BODY_CHARS + 1);
        assert_eq!(
            MessageBody::parse(&over),
            Err(DomainError::BodyTooLong {
                actual: MAX_BODY_CHARS + 1,
                max: MAX_BODY_CHARS
            })
        );
    }

    #[test]
    fn accepts_multibyte_and_astral_characters() {
        let body = MessageBody::parse("Привет 👋 こんにちは 𝔘𝔫𝔦𝔠𝔬𝔡𝔢").expect("valid");
        assert_eq!(body.as_str(), "Привет 👋 こんにちは 𝔘𝔫𝔦𝔠𝔬𝔡𝔢");
        // 6 + 1 + 1 + 1 + 5 + 1 + 7 characters: astral characters count as one each.
        assert_eq!(body.char_count(), 22);
    }

    #[test]
    fn a_short_body_is_previewed_whole() {
        let body = MessageBody::parse("see you at six").expect("valid");
        let preview = MessagePreview::new(&body, Direction::Outgoing);
        assert_eq!(preview.body, "see you at six");
        assert_eq!(preview.direction, Direction::Outgoing);
    }

    #[test]
    fn a_long_body_is_cut_to_the_preview_limit_with_an_ellipsis() {
        let body = MessageBody::parse(&"я".repeat(MAX_PREVIEW_CHARS + 40)).expect("valid");
        let preview = MessagePreview::new(&body, Direction::Incoming);
        assert_eq!(preview.body.chars().count(), MAX_PREVIEW_CHARS + 1);
        assert!(preview.body.ends_with('…'));
    }

    #[test]
    fn char_count_is_used_for_the_limit_not_byte_length() {
        // 8000 three-byte characters is 24 KiB, four times the byte length of an ASCII body
        // of the same character count, and must still be accepted.
        let body = MessageBody::parse(&"я".repeat(MAX_BODY_CHARS)).expect("valid");
        assert_eq!(body.char_count(), MAX_BODY_CHARS);
        assert_eq!(body.as_str().len(), MAX_BODY_CHARS * 2);
    }

    #[test]
    fn status_and_direction_round_trip_through_storage_forms() {
        for status in [
            MessageStatus::Sending,
            MessageStatus::Sent,
            MessageStatus::Delivered,
            MessageStatus::Received,
            MessageStatus::Failed,
        ] {
            assert_eq!(
                MessageStatus::from_db(status.as_str()).expect("parses"),
                status
            );
        }
        for direction in [Direction::Incoming, Direction::Outgoing] {
            assert_eq!(
                Direction::from_db(direction.as_str()).expect("parses"),
                direction
            );
        }
        assert!(MessageStatus::from_db("nonsense").is_err());
        assert!(Direction::from_db("sideways").is_err());
    }
}
