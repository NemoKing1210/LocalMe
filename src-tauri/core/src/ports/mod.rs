//! Ports: the traits the service layer depends on.
//!
//! This crate never names a concrete adapter above this module. The session actor is
//! generic over these traits, so a test can drive it with a different store or a different
//! discovery source without touching a line of the service code.
//!
//! The traits use return-position `impl Trait` rather than `async fn` so that the `Send`
//! bound on the returned future is explicit: the session actor is spawned onto a
//! multi-threaded runtime, and an implicit non-`Send` future would fail to compile at the
//! spawn site, far from the trait that caused it.

pub mod discovery;
pub mod store;

pub use discovery::{DiscoveredPeer, Discovery, DiscoveryEvent};
pub use store::{HistoryCursor, KnownDevice, Store, StoredPeer};
