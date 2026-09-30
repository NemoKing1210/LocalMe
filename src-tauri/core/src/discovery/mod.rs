//! Discovery adapters.
//!
//! Three [`Discovery`](crate::ports::discovery::Discovery) implementations live here:
//!
//! * [`MdnsDiscovery`] — the primary mechanism: DNS-SD for `_localme._tcp.local.` through the
//!   `mdns-sd` crate, which already solves query/response, caching, name conflicts and
//!   per-interface behaviour, and interoperates with the responder every desktop OS ships.
//! * [`UdpBeacon`] — the escape hatch for networks that filter mDNS. It is deliberately dumb
//!   (periodic announce plus a direct reply, no query) because it has to work exactly where
//!   the smart mechanism does not.
//! * [`CompositeDiscovery`] — merges any number of sources into one event stream, so a peer
//!   seen by both mechanisms is reported once and is only reported lost when every
//!   mechanism has given up on it.
//!
//! Every adapter filters our own device id at the [`DiscoveryEvent`] boundary, so our own
//! announcement never reaches the session, whichever mechanism found it.

mod beacon;
mod composite;
mod mdns;

pub use beacon::UdpBeacon;
pub use composite::CompositeDiscovery;
pub use mdns::MdnsDiscovery;

use std::net::{IpAddr, SocketAddr};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::domain::ids::{AvatarSeed, DeviceId};
use crate::domain::nickname::Nickname;

/// What this device tells the network about itself.
///
/// The same values are sent over every mechanism, and peers keep them until the handshake
/// confirms them: a nickname and an avatar seed taken from an announcement are
/// unauthenticated, good enough to draw a row in a peer list, and replaced by the
/// handshake's values as soon as a connection is established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnAnnouncement {
    /// Our stable device identifier, announced in full so a peer never parses a name.
    pub device_id: DeviceId,
    /// The nickname peers should display for us.
    pub nickname: Nickname,
    /// The avatar seed peers should render for us.
    pub avatar_seed: AvatarSeed,
    /// The TCP port our listener is bound to.
    pub port: u16,
}

/// Orders advertised addresses for dialling: every IPv4 address before any IPv6 address, in
/// a deterministic order within each family.
///
/// IPv4 goes first because a dual-stack host usually advertises link-local IPv6 addresses
/// that are only usable on one link, while its IPv4 address is reachable from the whole
/// LAN. The caller remembers which address worked last and tries that one first.
fn ordered_addresses(addresses: impl IntoIterator<Item = IpAddr>, port: u16) -> Vec<SocketAddr> {
    let mut unique: Vec<IpAddr> = addresses.into_iter().collect();
    // A total sort first, so deduplication and the resulting order do not depend on the
    // iteration order of whatever set the addresses came from.
    unique.sort_unstable();
    unique.dedup();
    unique.sort_by_key(|address| address.is_ipv6());
    unique
        .into_iter()
        .map(|address| SocketAddr::new(address, port))
        .collect()
}

/// Locks a mutex, recovering the value if a panic poisoned it.
///
/// The guarded state is only ever replaced, never half-updated, so a poisoned lock carries
/// nothing worth propagating a panic for — and panic-free production code is a rule here.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn addresses_are_ordered_ipv4_first_and_deduplicated() {
        let v6: IpAddr = "fe80::1".parse().expect("valid IPv6 address");
        let other_v6: IpAddr = "2001:db8::1".parse().expect("valid IPv6 address");
        let v4 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));

        let ordered = ordered_addresses([v6, v4, other_v6, v4], 47820);

        assert_eq!(
            ordered.len(),
            3,
            "a duplicate address must not produce a second socket address"
        );
        assert_eq!(ordered.first().map(SocketAddr::ip), Some(v4));
        assert!(
            ordered.iter().skip(1).all(|address| address.is_ipv6()),
            "every IPv4 address must come before the first IPv6 address"
        );
        assert!(
            ordered.iter().all(|address| address.port() == 47820),
            "every address must carry the announced port"
        );
    }

    #[test]
    fn no_advertised_address_yields_no_socket_address() {
        assert!(ordered_addresses([], 47820).is_empty());
    }
}
