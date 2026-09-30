//! Validated identifier newtypes.
//!
//! `DeviceId` is the primary key of everything: it is generated once, stored once, and
//! never derived from hardware. See `docs/ARCHITECTURE.md` §4.

use std::fmt;
use std::str::FromStr;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::domain::nickname::Nickname;
use crate::error::DomainError;
use crate::protocol::limits::MAX_AVATAR_SEED_CHARS;

/// A stable, per-installation device identifier.
///
/// Generated as a random UUID v4 on first launch and persisted in the `meta` table. MAC
/// addresses, hostnames and IPs are all unstable — DHCP leases move, adapters change,
/// MACs are randomised by default on modern operating systems — so none of them can serve
/// as identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(Uuid);

impl DeviceId {
    /// Generates a fresh random identifier.
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wraps an already-validated UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }

    /// A short, human-readable form used for mDNS instance names and log lines.
    #[must_use]
    pub fn short(self) -> String {
        let mut out = String::with_capacity(20);
        for ch in self.0.simple().to_string().chars().take(20) {
            out.push(ch);
        }
        out
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for DeviceId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| DomainError::InvalidId {
                kind: "device id",
                value: s.chars().take(64).collect(),
            })
    }
}

impl Serialize for DeviceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(D::Error::custom)
    }
}

/// A message identifier: UUID v7, so identifiers sort by creation time.
///
/// The sortability is load-bearing: `ORDER BY id` is a time order, which gives the storage
/// layer a deterministic tie-break for messages that share a `sent_at` millisecond.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(Uuid);

impl MessageId {
    /// Generates a new time-ordered identifier.
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }

    /// Wraps an already-validated UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for MessageId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| DomainError::InvalidId {
                kind: "message id",
                value: s.chars().take(64).collect(),
            })
    }
}

impl Serialize for MessageId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MessageId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(D::Error::custom)
    }
}

/// The seed a peer's avatar is rendered from.
///
/// Owned by the announcing device and transmitted verbatim, because two devices must render
/// the same picture: anything derived locally (from an IP, from a localised nickname
/// comparison) would drift between machines.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AvatarSeed(String);

impl AvatarSeed {
    /// Validates a seed received from the network or produced locally.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::BadAvatarSeed`] when the value is empty or longer than
    /// [`MAX_AVATAR_SEED_CHARS`].
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let len = value.chars().count();
        if len == 0 || len > MAX_AVATAR_SEED_CHARS {
            return Err(DomainError::BadAvatarSeed {
                actual: len,
                max: MAX_AVATAR_SEED_CHARS,
            });
        }
        Ok(Self(value.to_owned()))
    }

    /// The canonical seed for a device: derived from the stable device id and the current
    /// nickname, so renaming a device is a visible avatar change for everyone.
    #[must_use]
    pub fn derive(device_id: DeviceId, nickname: &Nickname) -> Self {
        Self(format!("{device_id}:{nickname}"))
    }

    /// The seed string handed to the avatar renderer.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AvatarSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for AvatarSeed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AvatarSeed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::nickname::Nickname;

    #[test]
    fn device_ids_round_trip_through_json() {
        let id = DeviceId::generate();
        let json = serde_json::to_string(&id).expect("serialise");
        assert_eq!(json, format!("\"{id}\""));
        let back: DeviceId = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, id);
    }

    #[test]
    fn device_id_rejects_garbage() {
        let err = "not-a-uuid".parse::<DeviceId>().expect_err("must fail");
        assert!(matches!(
            err,
            DomainError::InvalidId {
                kind: "device id",
                ..
            }
        ));
    }

    #[test]
    fn device_id_deserialisation_rejects_garbage() {
        let err = serde_json::from_str::<DeviceId>("\"nope\"").expect_err("must fail");
        assert!(err.to_string().contains("device id"));
    }

    #[test]
    fn message_ids_increase_monotonically() {
        let first = MessageId::generate();
        let second = MessageId::generate();
        assert!(
            second > first,
            "uuid v7 must sort by creation order: {first} then {second}"
        );
    }

    #[test]
    fn short_form_is_twenty_characters_of_hex() {
        let id = DeviceId::generate();
        let short = id.short();
        assert_eq!(short.len(), 20);
        assert!(short.chars().all(|c| c.is_ascii_hexdigit()));
        // The dashed UUID contains the same first twenty hex characters.
        let simple = id.as_uuid().simple().to_string();
        assert!(simple.starts_with(&short));
    }

    #[test]
    fn avatar_seed_is_bounded() {
        assert!(AvatarSeed::parse("").is_err());
        assert!(AvatarSeed::parse(&"x".repeat(MAX_AVATAR_SEED_CHARS)).is_ok());
        assert!(AvatarSeed::parse(&"x".repeat(MAX_AVATAR_SEED_CHARS + 1)).is_err());
    }

    #[test]
    fn derived_seed_uses_device_id_and_nickname() {
        let id = DeviceId::from_uuid(
            "018f2b9c-0000-7000-8000-000000000000"
                .parse()
                .expect("uuid"),
        );
        let nick = Nickname::parse("Аня").expect("nickname");
        let seed = AvatarSeed::derive(id, &nick);
        assert_eq!(seed.as_str(), "018f2b9c-0000-7000-8000-000000000000:Аня");
        // 20 + 1 + 3 characters, comfortably inside the 64-character limit.
        assert!(seed.as_str().chars().count() <= MAX_AVATAR_SEED_CHARS);
    }
}
