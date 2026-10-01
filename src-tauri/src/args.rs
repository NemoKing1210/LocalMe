//! Argument handling for the IPC surface.
//!
//! Every command in `commands.rs` parses its string arguments into domain types through one of
//! these functions, which is why they are here rather than inline: they are the whole of the
//! layer's own logic, they are what the tests in this file exercise, and they are the only place
//! a caller-supplied value becomes a validated one.
//!
//! The `field` each error names is part of the contract: the interface uses it to decide which
//! input to mark, so a command that renamed `peerId` to `peer_id` would break the interface's
//! error display. Tauri's `#[tauri::command]` argument names are camelCase on the JavaScript
//! side by default, and the field names below match them.

use localme_core::domain::ids::DeviceId;
use localme_core::domain::message::MessageBody;
use localme_core::domain::nickname::Nickname;
use localme_core::ports::store::HistoryCursor;
use localme_core::protocol::MAX_BODY_CHARS;

use crate::error::ApiError;

/// A device identifier, as the interface sends it.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] naming the `peerId` argument.
pub fn device_id(value: &str) -> Result<DeviceId, ApiError> {
    value
        .parse()
        .map_err(|error| ApiError::invalid_input("peerId", error))
}

/// A nickname.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] naming the `nickname` argument.
pub fn nickname(value: &str) -> Result<Nickname, ApiError> {
    Nickname::parse(value).map_err(|error| ApiError::invalid_input("nickname", error))
}

/// An `#RRGGBB` colour, as the interface sends one.
///
/// The window frame is painted from the palette the front end generated, so a colour is one of
/// the few values that crosses the boundary as a string. Parsing it here keeps the frame code
/// free of string handling and turns a malformed value into an input error rather than a silent
/// default colour.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] naming `field`.
pub fn hex_color(field: &str, value: &str) -> Result<[u8; 3], ApiError> {
    let invalid = || ApiError::invalid_input(field, "expected a colour in #RRGGBB form");
    let digits = value.strip_prefix('#').ok_or_else(invalid)?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid());
    }

    let mut channels = [0u8; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        let pair = &digits[index * 2..index * 2 + 2];
        *channel = u8::from_str_radix(pair, 16).map_err(|_| invalid())?;
    }
    Ok(channels)
}

/// A message body.
///
/// The length error is enriched with the limit, because "message body has 9000 characters, the
/// limit is 8000" is what the interface shows next to the counter and the limit belongs with it.
///
/// # Errors
///
/// [`ApiError::InvalidInput`] naming the `body` argument.
pub fn message_body(value: &str) -> Result<MessageBody, ApiError> {
    MessageBody::parse(value).map_err(|error| {
        if matches!(error, localme_core::error::DomainError::BodyTooLong { .. }) {
            ApiError::InvalidInput {
                field: "body".to_owned(),
                message: format!("{error} ({MAX_BODY_CHARS} characters maximum)"),
            }
        } else {
            ApiError::invalid_input("body", error)
        }
    })
}

/// Where a page of a conversation should start, as the interface sends it.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageCursor {
    /// Timestamp of the last row already returned.
    pub sent_at_ms: i64,
    /// Identifier of the last row already returned.
    pub id: String,
}

impl TryFrom<PageCursor> for HistoryCursor {
    type Error = ApiError;

    fn try_from(value: PageCursor) -> Result<Self, Self::Error> {
        Ok(Self {
            sent_at_ms: value.sent_at_ms,
            id: value
                .id
                .parse()
                .map_err(|error| ApiError::invalid_input("cursor.id", error))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_id_round_trips() {
        let raw = "018f2b9c-0000-7000-8000-0000000000aa";
        assert_eq!(device_id(raw).expect("valid").to_string(), raw);
    }

    #[test]
    fn a_bad_device_id_names_the_argument_the_interface_sent() {
        // The field name is what the interface marks as invalid, so it has to be the JavaScript
        // argument name and not the Rust parameter name.
        let error = device_id("nope").expect_err("not an identifier");
        match error {
            ApiError::InvalidInput { field, .. } => assert_eq!(field, "peerId"),
            other => panic!("expected an input error, got {other:?}"),
        }
    }

    #[test]
    fn a_nickname_is_trimmed_and_bounded() {
        assert_eq!(
            nickname("  Аня  ").expect("valid").as_str(),
            "Аня",
            "the host trims, so the interface does not have to"
        );
        assert!(nickname("").is_err());
        assert!(nickname(&"x".repeat(33)).is_err());
        assert!(nickname("line\nbreak").is_err());

        let error = nickname(&"x".repeat(33)).expect_err("too long");
        match error {
            ApiError::InvalidInput { field, message } => {
                assert_eq!(field, "nickname");
                assert!(
                    message.contains("33"),
                    "the message says how long: {message}"
                );
            }
            other => panic!("expected an input error, got {other:?}"),
        }
    }

    #[test]
    fn a_message_body_is_normalised() {
        assert_eq!(
            message_body("line one\r\nline two")
                .expect("valid")
                .as_str(),
            "line one\nline two"
        );
        assert!(message_body("   ").is_err());
        assert!(message_body("bell\u{7}").is_err());
    }

    #[test]
    fn an_over_long_body_names_the_limit() {
        let body = "я".repeat(MAX_BODY_CHARS + 1);
        let error = message_body(&body).expect_err("too long");
        match error {
            ApiError::InvalidInput { field, message } => {
                assert_eq!(field, "body");
                assert!(
                    message.contains(&MAX_BODY_CHARS.to_string()),
                    "the message must name the limit the interface shows: {message}"
                );
                assert!(
                    message.contains(&(MAX_BODY_CHARS + 1).to_string()),
                    "and how long the body actually was: {message}"
                );
            }
            other => panic!("expected an input error, got {other:?}"),
        }
    }

    #[test]
    fn a_colour_parses_to_channels_in_the_order_the_interface_writes_them() {
        assert_eq!(
            hex_color("accent", "#6750A4").expect("valid"),
            [0x67, 0x50, 0xA4]
        );
        assert_eq!(hex_color("accent", "#00ff00").expect("valid"), [0, 255, 0]);
    }

    #[test]
    fn a_bad_colour_names_the_argument_the_interface_sent() {
        for value in ["6750A4", "#6750A", "#6750A44", "#6750AG", "", "#"] {
            let error = hex_color("onAccent", value).expect_err("not a colour");
            match error {
                ApiError::InvalidInput { field, .. } => assert_eq!(field, "onAccent"),
                other => panic!("expected an input error for {value:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_page_cursor_parses_either_half_of_the_key() {
        let parsed = HistoryCursor::try_from(PageCursor {
            sent_at_ms: 1_700_000_000_000,
            id: "018f2b9c-0000-7000-8000-0000000000aa".to_owned(),
        })
        .expect("valid cursor");
        assert_eq!(parsed.sent_at_ms, 1_700_000_000_000);
        assert_eq!(
            parsed.id.to_string(),
            "018f2b9c-0000-7000-8000-0000000000aa"
        );
    }

    #[test]
    fn a_page_cursor_with_a_bad_identifier_is_an_input_error() {
        let error = HistoryCursor::try_from(PageCursor {
            sent_at_ms: 0,
            id: "not-an-id".to_owned(),
        })
        .expect_err("not an identifier");
        match error {
            ApiError::InvalidInput { field, .. } => assert_eq!(field, "cursor.id"),
            other => panic!("expected an input error, got {other:?}"),
        }
    }
}
