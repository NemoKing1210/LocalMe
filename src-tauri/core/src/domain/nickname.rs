//! Nickname validation.
//!
//! A nickname is *not* an identity — it is not unique and may change at any time. The only
//! reason it is a newtype is that an invalid nickname must not be able to reach the wire or
//! the database, and the only constructor is [`Nickname::parse`].

use std::fmt;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::DomainError;
use crate::protocol::limits::MAX_NICKNAME_CHARS;

/// A validated, trimmed display name of 1..=32 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Nickname(String);

impl Nickname {
    /// Trims surrounding whitespace and validates the result.
    ///
    /// Characters are counted as `char`s, not bytes, so a nickname of 32 emoji is accepted
    /// and a nickname of 33 ASCII letters is not.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::EmptyNickname`], [`DomainError::NicknameTooLong`] or
    /// [`DomainError::NicknameControlChar`]. Control characters are rejected because a
    /// nickname ends up in the window title, the tray tooltip and OS notifications, where
    /// a stray newline or escape sequence is a rendering and log-integrity problem.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let trimmed = raw.trim();
        let len = trimmed.chars().count();
        if len == 0 {
            return Err(DomainError::EmptyNickname);
        }
        if len > MAX_NICKNAME_CHARS {
            return Err(DomainError::NicknameTooLong {
                actual: len,
                max: MAX_NICKNAME_CHARS,
            });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(DomainError::NicknameControlChar);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The validated nickname without surrounding whitespace.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether a raw string would be accepted, for live form validation.
    #[must_use]
    pub fn is_valid(raw: &str) -> bool {
        Self::parse(raw).is_ok()
    }
}

impl fmt::Display for Nickname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Nickname {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Nickname {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        let nick = Nickname::parse("  Аня  ").expect("valid");
        assert_eq!(nick.as_str(), "Аня");
    }

    #[test]
    fn rejects_empty_and_whitespace_only() {
        assert_eq!(Nickname::parse(""), Err(DomainError::EmptyNickname));
        assert_eq!(Nickname::parse("   "), Err(DomainError::EmptyNickname));
        assert_eq!(Nickname::parse("\t\n"), Err(DomainError::EmptyNickname));
    }

    #[test]
    fn counts_characters_not_bytes() {
        let cyber = "я".repeat(32);
        assert!(Nickname::parse(&cyber).is_ok());
        let too_long = "я".repeat(33);
        assert_eq!(
            Nickname::parse(&too_long),
            Err(DomainError::NicknameTooLong {
                actual: 33,
                max: 32
            })
        );
    }

    #[test]
    fn accepts_emoji_up_to_the_limit() {
        let emoji: String = "\u{1F600}".repeat(32);
        let nick = Nickname::parse(&emoji).expect("valid");
        assert_eq!(nick.as_str().chars().count(), 32);
    }

    #[test]
    fn rejects_control_characters() {
        assert_eq!(
            Nickname::parse("An\u{7}na"),
            Err(DomainError::NicknameControlChar)
        );
        assert_eq!(
            Nickname::parse("line\nbreak"),
            Err(DomainError::NicknameControlChar)
        );
    }

    #[test]
    fn is_valid_matches_parse() {
        assert!(Nickname::is_valid("ok"));
        assert!(!Nickname::is_valid(""));
        assert!(!Nickname::is_valid(&"x".repeat(33)));
    }

    #[test]
    fn round_trips_through_json() {
        let nick = Nickname::parse("  Аня  ").expect("valid");
        let json = serde_json::to_string(&nick).expect("serialise");
        assert_eq!(json, "\"Аня\"");
        let back: Nickname = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, nick);
    }

    #[test]
    fn deserialising_an_invalid_nickname_fails() {
        let err = serde_json::from_str::<Nickname>("\"   \"").expect_err("must fail");
        assert!(err.to_string().contains("nickname"));
    }
}
