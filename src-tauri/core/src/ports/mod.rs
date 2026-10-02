//! Ports: the traits the service layer depends on.
//!
//! Traits use return-position `impl Trait` rather than `async fn` so the `Send` bound on the
//! returned future is explicit: the session actor is spawned onto a multi-threaded runtime, and
//! an implicit non-`Send` future would fail to compile at the spawn site, far from the trait.

pub mod discovery;
pub mod store;

pub use discovery::{DiscoveredPeer, Discovery, DiscoveryEvent};
pub use store::{HistoryCursor, KnownDevice, Store, StoredPeer};
