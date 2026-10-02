//! Peer identity, the handshake payload, and the projection the UI renders.
//!
//! [`PeerView`] is the only peer shape that crosses the IPC boundary, and it carries the
//! sort key with it so the ordering rule lives in one place.

use crate::domain::ids::{AvatarSeed, DeviceId};
use crate::domain::message::MessagePreview;
use crate::domain::nickname::Nickname;

/// The identity exchange at the start of every connection.
///
/// Sent by the dialer as `hello` and echoed by the acceptor as `welcome`, so both ends learn
/// the other's identity from the handshake rather than the unauthenticated discovery record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    pub protocol_version: u16,
    pub device_id: DeviceId,
    pub nickname: Nickname,
    pub avatar_seed: AvatarSeed,
    /// Zero when the sender could not determine it.
    pub listen_port: u16,
}

impl Handshake {
    #[must_use]
    pub fn new(protocol_version: u16, profile: &PeerProfile, listen_port: u16) -> Self {
        Self {
            protocol_version,
            device_id: profile.device_id,
            nickname: profile.nickname.clone(),
            avatar_seed: profile.avatar_seed.clone(),
            listen_port,
        }
    }

    #[must_use]
    pub fn with_nickname(&self, nickname: Nickname) -> Self {
        let profile = PeerProfile::new(self.device_id, nickname);
        Self {
            nickname: profile.nickname,
            avatar_seed: profile.avatar_seed,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerProfile {
    pub device_id: DeviceId,
    pub nickname: Nickname,
    pub avatar_seed: AvatarSeed,
}

impl PeerProfile {
    #[must_use]
    pub fn new(device_id: DeviceId, nickname: Nickname) -> Self {
        let avatar_seed = AvatarSeed::derive(device_id, &nickname);
        Self {
            device_id,
            nickname,
            avatar_seed,
        }
    }

    /// Applies a profile update received from the network.
    ///
    /// The seed is taken as announced, not re-derived: the remote device is the authority on
    /// its own avatar, and re-deriving locally is what makes two machines disagree.
    pub fn apply_update(&mut self, nickname: Nickname, avatar_seed: AvatarSeed) {
        self.nickname = nickname;
        self.avatar_seed = avatar_seed;
    }
}

/// One row of the user list.
///
/// Serialised directly as the interface renders it, so the ordering rule and the fields travel
/// together.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub device_id: DeviceId,
    pub nickname: Nickname,
    pub avatar_seed: AvatarSeed,
    pub online: bool,
    pub last_seen_ms: Option<i64>,
    pub unread: u32,
    pub notify_muted: bool,
    /// Newest of `last_seen_ms` and the last message timestamp, in milliseconds.
    ///
    /// Carried explicitly rather than recomputed in the front end so "last activity" cannot
    /// mean two different things in two places.
    pub last_activity_ms: Option<i64>,
    pub last_message: Option<MessagePreview>,
}

impl PeerView {
    /// Case-insensitive substring match over the nickname. The rule lives here so both sides
    /// agree, even though the front end applies it to the in-memory list.
    #[must_use]
    pub fn matches_query(&self, query: &str) -> bool {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        self.nickname.as_str().to_lowercase().contains(&needle)
    }

    fn activity(&self) -> i64 {
        self.last_activity_ms.unwrap_or(i64::MIN)
    }

    /// Unread first, then online, then most recent activity, then nickname **ascending**.
    /// The last term is not cosmetic: without it the order is not total, and two devices
    /// receiving the same events in a different order would show a different list.
    pub fn sort_for_display(peers: &mut [Self]) {
        peers.sort_by(|a, b| {
            let a_unread = u8::from(a.unread > 0);
            let b_unread = u8::from(b.unread > 0);
            b_unread
                .cmp(&a_unread)
                .then_with(|| b.online.cmp(&a.online))
                .then_with(|| b.activity().cmp(&a.activity()))
                .then_with(|| a.nickname.as_str().cmp(b.nickname.as_str()))
        });
    }

    #[must_use]
    pub fn arrange(peers: impl IntoIterator<Item = Self>, query: &str) -> Vec<Self> {
        let mut peers: Vec<Self> = peers
            .into_iter()
            .filter(|peer| peer.matches_query(query))
            .collect();
        Self::sort_for_display(&mut peers);
        peers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(id: u128, nick: &str, online: bool, unread: u32, activity: Option<i64>) -> PeerView {
        let uuid = uuid::Uuid::from_u128(id);
        let device_id = DeviceId::from_uuid(uuid);
        let nickname = Nickname::parse(nick).expect("valid nickname");
        PeerView {
            device_id,
            avatar_seed: AvatarSeed::derive(device_id, &nickname),
            nickname,
            online,
            last_seen_ms: activity,
            unread,
            notify_muted: false,
            last_activity_ms: activity,
            last_message: None,
        }
    }

    #[test]
    fn unread_peers_sort_above_online_peers_without_unread() {
        let mut peers = vec![
            peer(1, "online-no-unread", true, 0, Some(9_000)),
            peer(2, "offline-unread", false, 3, Some(1_000)),
        ];
        PeerView::sort_for_display(&mut peers);
        assert_eq!(
            peers.first().map(|p| p.nickname.as_str()),
            Some("offline-unread")
        );
    }

    #[test]
    fn online_peers_sort_above_offline_peers_otherwise() {
        let mut peers = vec![
            peer(1, "offline-recent", false, 0, Some(9_000)),
            peer(2, "online-stale", true, 0, Some(1_000)),
        ];
        PeerView::sort_for_display(&mut peers);
        assert_eq!(
            peers.first().map(|p| p.nickname.as_str()),
            Some("online-stale")
        );
    }

    #[test]
    fn activity_orders_within_the_same_bucket() {
        let mut peers = vec![
            peer(1, "older", true, 0, Some(1_000)),
            peer(2, "newer", true, 0, Some(5_000)),
            peer(3, "never", true, 0, None),
        ];
        PeerView::sort_for_display(&mut peers);
        let order: Vec<&str> = peers.iter().map(|p| p.nickname.as_str()).collect();
        assert_eq!(order, vec!["newer", "older", "never"]);
    }

    #[test]
    fn nickname_breaks_ties_so_the_order_is_total() {
        // Same bucket, same timestamp: only the nickname can decide, and it must decide the
        // same way on every device.
        let mut forward = vec![
            peer(7, "bob", true, 0, Some(1_000)),
            peer(8, "alice", true, 0, Some(1_000)),
        ];
        let mut backward = vec![
            peer(8, "alice", true, 0, Some(1_000)),
            peer(7, "bob", true, 0, Some(1_000)),
        ];
        PeerView::sort_for_display(&mut forward);
        PeerView::sort_for_display(&mut backward);
        assert_eq!(forward, backward);
        assert_eq!(forward.first().map(|p| p.nickname.as_str()), Some("alice"));
    }

    #[test]
    fn duplicate_nicknames_do_not_collide_in_the_order() {
        let mut peers = vec![
            peer(1, "Аня", false, 0, Some(1_000)),
            peer(2, "Аня", true, 0, Some(2_000)),
        ];
        PeerView::sort_for_display(&mut peers);
        assert_eq!(
            peers.first().map(|p| p.device_id),
            Some(DeviceId::from_uuid(uuid::Uuid::from_u128(2)))
        );
    }

    #[test]
    fn search_is_case_insensitive_and_substring_based() {
        let anya = peer(1, "Аня", true, 0, None);
        assert!(anya.matches_query(""));
        assert!(anya.matches_query("   "));
        assert!(anya.matches_query("ан"));
        assert!(anya.matches_query("АНЯ"));
        assert!(!anya.matches_query("борис"));

        let bob = peer(2, "Bob", true, 0, None);
        assert!(bob.matches_query("b"));
        assert!(bob.matches_query("OB"));
        assert!(!bob.matches_query("bobby"));
    }

    #[test]
    fn arrange_filters_then_orders() {
        let peers = vec![
            peer(1, "anna", false, 0, None),
            peer(2, "bob", true, 0, None),
            peer(3, "annabelle", true, 0, None),
        ];
        let arranged = PeerView::arrange(peers, "anna");
        let names: Vec<&str> = arranged.iter().map(|p| p.nickname.as_str()).collect();
        assert_eq!(names, vec!["annabelle", "anna"]);
    }

    #[test]
    fn profile_update_takes_the_announced_seed() {
        let id = DeviceId::from_uuid(uuid::Uuid::from_u128(42));
        let mut profile = PeerProfile::new(id, Nickname::parse("Old").expect("valid"));
        let announced = AvatarSeed::parse("peer-authoritative-seed").expect("valid");
        profile.apply_update(Nickname::parse("New").expect("valid"), announced.clone());
        assert_eq!(profile.nickname.as_str(), "New");
        assert_eq!(
            profile.avatar_seed, announced,
            "the peer's announced seed must win over a locally derived one"
        );
    }

    #[test]
    fn rename_derives_a_new_seed_locally() {
        let id = DeviceId::from_uuid(uuid::Uuid::from_u128(42));
        let profile = PeerProfile::new(id, Nickname::parse("Old").expect("valid"));
        let handshake = Handshake::new(1, &profile, 47820);
        let renamed = handshake.with_nickname(Nickname::parse("New").expect("valid"));
        assert_eq!(renamed.nickname.as_str(), "New");
        assert_eq!(renamed.avatar_seed.as_str(), format!("{id}:New"));
        assert_eq!(renamed.listen_port, 47820);
        assert_eq!(renamed.device_id, id);
    }
}
