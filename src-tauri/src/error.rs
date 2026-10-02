//! The error shape crossing the IPC boundary: a tagged enum, serialised into the rejection the
//! front end receives, so the variant tags are part of the front end's API.

use localme_core::CoreError;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiError {
    InvalidInput {
        field: String,
        message: String,
    },
    UnknownPeer {
        device_id: String,
    },
    Storage {
        message: String,
    },
    Discovery {
        message: String,
    },
    Network {
        message: String,
    },
    ShuttingDown,
    /// A bug or unexpected environment condition; the front end shows a generic message, so the
    /// detail is for the log, not the user.
    Internal {
        message: String,
    },
}

impl ApiError {
    pub fn invalid_input(field: impl Into<String>, message: impl std::fmt::Display) -> Self {
        Self::InvalidInput {
            field: field.into(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput { field, message } => write!(f, "{field}: {message}"),
            Self::UnknownPeer { device_id } => write!(f, "unknown peer {device_id}"),
            Self::Storage { message } => write!(f, "storage: {message}"),
            Self::Discovery { message } => write!(f, "discovery: {message}"),
            Self::Network { message } => write!(f, "network: {message}"),
            Self::ShuttingDown => f.write_str("the application is shutting down"),
            Self::Internal { message } => write!(f, "internal: {message}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Domain(source) => Self::InvalidInput {
                field: "value".to_owned(),
                message: source.to_string(),
            },
            CoreError::Protocol(source) => Self::InvalidInput {
                field: "frame".to_owned(),
                message: source.to_string(),
            },
            CoreError::Storage(source) => Self::Storage {
                message: source.to_string(),
            },
            CoreError::Discovery(source) => Self::Discovery {
                message: source.to_string(),
            },
            CoreError::Transport(source) => Self::Network {
                message: source.to_string(),
            },
            CoreError::UnknownPeer(device_id) => Self::UnknownPeer { device_id },
            CoreError::ShuttingDown => Self::ShuttingDown,
            CoreError::Task(message) => Self::Internal { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use localme_core::error::{DomainError, StorageError};

    #[test]
    fn domain_failures_become_input_errors() {
        let error: ApiError = CoreError::Domain(DomainError::EmptyBody).into();
        assert!(matches!(error, ApiError::InvalidInput { .. }));
        assert!(error.to_string().contains("empty"));
    }

    #[test]
    fn unknown_peer_failures_keep_the_identifier() {
        let error: ApiError = CoreError::UnknownPeer("abc".to_owned()).into();
        assert!(matches!(error, ApiError::UnknownPeer { device_id } if device_id == "abc"));
    }

    #[test]
    fn storage_failures_are_tagged_for_the_front_end() {
        let error: ApiError = CoreError::Storage(StorageError::Unavailable).into();
        assert!(matches!(error, ApiError::Storage { .. }));
    }

    #[test]
    fn rejections_serialise_with_a_kind_tag() {
        // The wire shape the front end receives as a rejection; it switches on `kind`.
        let error = ApiError::UnknownPeer {
            device_id: "abc".to_owned(),
        };
        let value = serde_json::to_value(&error).expect("serialises");
        assert_eq!(value["kind"], "unknown_peer");
        assert_eq!(value["device_id"], "abc");
    }

    #[test]
    fn every_variant_serialises_with_a_distinct_tag() {
        let variants = [
            (
                ApiError::InvalidInput {
                    field: "f".to_owned(),
                    message: "m".to_owned(),
                },
                "invalid_input",
            ),
            (
                ApiError::UnknownPeer {
                    device_id: "d".to_owned(),
                },
                "unknown_peer",
            ),
            (
                ApiError::Storage {
                    message: "m".to_owned(),
                },
                "storage",
            ),
            (
                ApiError::Discovery {
                    message: "m".to_owned(),
                },
                "discovery",
            ),
            (
                ApiError::Network {
                    message: "m".to_owned(),
                },
                "network",
            ),
            (ApiError::ShuttingDown, "shutting_down"),
            (
                ApiError::Internal {
                    message: "m".to_owned(),
                },
                "internal",
            ),
        ];
        for (error, expected) in variants {
            let value = serde_json::to_value(&error).expect("serialises");
            assert_eq!(value["kind"], expected);
        }
    }
}
