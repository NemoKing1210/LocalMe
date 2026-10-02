//! LocalMe core: domain, protocol, discovery, transport, storage and services.

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

pub mod discovery;
pub mod domain;
pub mod error;
pub mod ports;
pub mod protocol;
pub mod runtime;
pub mod services;
pub mod storage;
pub mod transport;

pub use error::CoreError;
