//! LocalMe core: domain, protocol, discovery, transport, storage and services.
//!
//! This crate never depends on `tauri`. The Tauri host in `src-tauri/src` is a thin
//! adapter over [`runtime::CoreHandle`]; everything that has behaviour worth testing
//! lives here.
//!
//! Layering (see `docs/ARCHITECTURE.md` §3):
//!
//! * [`domain`] — pure types and rules; no I/O, no async, no clocks of its own.
//! * [`protocol`] — wire format, framing, validation, rate limiting.
//! * [`ports`] — traits the service layer depends on (storage, discovery).
//! * [`storage`], [`discovery`], [`transport`] — adapters implementing those ports.
//! * [`services`] — actors that own mutable state.
//! * [`runtime`] — wiring: builds the whole graph and hands out a [`runtime::CoreHandle`].

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
