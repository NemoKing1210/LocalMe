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

/// The same values are sent over every mechanism, and peers keep them until the handshake
/// confirms them: a nickname and an avatar seed taken from an announcement are
/// unauthenticated, good enough to draw a row in a peer list, and replaced by the
/// handshake's values as soon as a connection is established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnAnnouncement {
    pub device_id: DeviceId,
    pub nickname: Nickname,
    pub avatar_seed: AvatarSeed,
    pub port: u16,
}

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

    #[test]
    fn address_order_does_not_depend_on_input_order() {
        let v4_low = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let v4_high = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));
        let v6: IpAddr = "fe80::1".parse().expect("valid IPv6 address");

        let forward = ordered_addresses([v6, v4_low, v4_high], 9);
        let backward = ordered_addresses([v4_high, v4_low, v6], 9);

        assert_eq!(
            forward, backward,
            "the order is total, not insertion-dependent"
        );
        assert_eq!(
            forward,
            vec![
                SocketAddr::new(v4_low, 9),
                SocketAddr::new(v4_high, 9),
                SocketAddr::new(v6, 9),
            ]
        );
    }

    #[test]
    fn an_ipv6_only_peer_still_yields_its_address() {
        let v6: IpAddr = "2001:db8::1".parse().expect("valid IPv6 address");

        let ordered = ordered_addresses([v6], 47820);

        assert_eq!(ordered, vec![SocketAddr::new(v6, 47820)]);
    }

    #[test]
    fn lock_hands_out_the_guard_even_after_the_mutex_is_poisoned() {
        let mutex = Mutex::new(1_u8);
        assert_eq!(
            *lock(&mutex),
            1,
            "an unpoisoned lock behaves like Mutex::lock"
        );

        let poisoned = Mutex::new(7_u8);
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = poisoned.lock().expect("the mutex starts clean");
            panic!("poison the mutex on purpose");
        }));
        assert!(panicked.is_err(), "the guard was dropped during a panic");
        assert!(poisoned.is_poisoned());
        assert_eq!(
            *lock(&poisoned),
            7,
            "a poisoned lock still hands out the last value rather than panicking"
        );
    }
}
