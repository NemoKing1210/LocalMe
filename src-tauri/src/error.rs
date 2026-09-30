//! The error shape crossing the IPC boundary.
//!
//! Tauri serialises a command's `Err` value into the rejection the front end receives, so
//! this type *is* part of the public API. It is a tagged enum rather than a string because
//! the front end needs to react differently to "this peer is offline" (disable the composer,
//! explain why) and to "storage failed" (offer to report it) — and because a message the
//! user can act on has to be written per code, in their language, not by the Rust layer.

use localme_core::CoreError;
use serde::Serialize;

/// A failure returned from an IPC command.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiError {
    /// The caller supplied something invalid: a nickname, a message body, an identifier.
    InvalidInput {
        /// Which field was rejected.
        field: String,
        /// Human-readable detail, in English, for logs and developer tooling.
        message: String,
    },
    /// No peer with that device id is known to this installation.
    UnknownPeer {
        /// The identifier that was not found.
        device_id: String,
    },
    /// The peer exists but cannot be written to right now.
    PeerOffline {
        /// The identifier of the unreachable peer.
        device_id: String,
    },
    /// The local database refused an operation.
    Storage {
        /// Underlying detail.
        message: String,
    },
    /// Discovery could not start or was interrupted.
    Discovery {
        /// Underlying detail.
        message: String,
    },
    /// A network operation failed.
    Network {
        /// Underlying detail.
        message: String,
    },
    /// The application is shutting down and cannot accept new work.
    ShuttingDown,
    /// A bug or an unexpected environment condition. The front end shows a generic message
    /// and offers the log location; the detail is for the log, not for the user.
    Internal {
        /// Underlying detail.
        message: String,
    },
}

impl ApiError {
    /// Builds an input error for a named field.
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
            Self::PeerOffline { device_id } => write!(f, "peer {device_id} is offline"),
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
            CoreError::PeerOffline(device_id) => Self::PeerOffline { device_id },
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
        // This is the exact shape the front end receives as a command rejection, so the
        // front end can switch on `kind` rather than parse a message string.
        let error = ApiError::PeerOffline {
            device_id: "abc".to_owned(),
        };
        let value = serde_json::to_value(&error).expect("serialises");
        assert_eq!(value["kind"], "peer_offline");
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
                ApiError::PeerOffline {
                    device_id: "d".to_owned(),
                },
                "peer_offline",
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
