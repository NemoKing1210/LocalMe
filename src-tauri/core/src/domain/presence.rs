//! The presence state machine.
//!
//! Presence is derived, never guessed. It is a function of three independent observations
//! (`docs/ARCHITECTURE.md` §4.2): the peer was discovered, a connection to it was
//! established and handshaken, and that connection is still receiving heartbeats.
//!
//! The machine has no clock: every transition takes `now`, so a test can drive a peer
//! through a full online → stalled → offline → online cycle in microseconds.

use std::time::{Duration, Instant};

use crate::protocol::limits::HEARTBEAT_TIMEOUT;

/// What the UI shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresenceStatus {
    /// Reachable now.
    Online,
    /// Listed, but not reachable.
    Offline,
}

/// The internal phase, which is finer-grained than what the UI shows.
///
/// `Connecting` and `Stalled` both report [`PresenceStatus::Offline`] but are kept apart so
/// logs and tests can tell "we are still dialling" from "the connection went quiet".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresencePhase {
    /// Nothing known about this device beyond a row in the database.
    Unknown,
    /// Discovered and dialling, or dialling again after a failure.
    Connecting,
    /// Handshaken and receiving heartbeats.
    Online,
    /// Connected, but no heartbeat within the timeout; the socket is being torn down.
    Stalled,
    /// Discovered before, not reachable now.
    Offline,
}

/// A state change worth telling the world about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceChange {
    /// Nothing the UI cares about changed.
    Unchanged,
    /// The peer became reachable.
    CameOnline,
    /// The peer stopped being reachable.
    WentOffline,
    /// The phase changed but the reported status did not (e.g. `Connecting` → `Stalled`).
    InternalOnly,
}

impl PresenceChange {
    /// Whether the reported online/offline status differs from before.
    #[must_use]
    pub const fn is_visible(self) -> bool {
        matches!(self, Self::CameOnline | Self::WentOffline)
    }
}

/// Presence of a single peer.
#[derive(Debug, Clone)]
pub struct PresenceMachine {
    phase: PresencePhase,
    last_heartbeat: Option<Instant>,
    /// Whether a dial is in flight, so repeated discovery events do not stack up dials.
    dial_in_flight: bool,
}

impl Default for PresenceMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl PresenceMachine {
    /// A machine for a device we have never observed.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            phase: PresencePhase::Unknown,
            last_heartbeat: None,
            dial_in_flight: false,
        }
    }

    /// The current phase.
    #[must_use]
    pub const fn phase(&self) -> PresencePhase {
        self.phase
    }

    /// What the UI shows.
    #[must_use]
    pub const fn status(&self) -> PresenceStatus {
        match self.phase {
            PresencePhase::Online => PresenceStatus::Online,
            _ => PresenceStatus::Offline,
        }
    }

    /// Whether the peer is reachable right now.
    #[must_use]
    pub const fn is_online(&self) -> bool {
        matches!(self.status(), PresenceStatus::Online)
    }

    /// When the last heartbeat arrived, on the monotonic clock.
    #[must_use]
    pub const fn last_heartbeat(&self) -> Option<Instant> {
        self.last_heartbeat
    }

    /// The peer was seen by discovery.
    ///
    /// Returns `true` when the caller should start (or retry) a dial. The machine refuses
    /// to ask for a second dial while one is in flight or while the peer is already online,
    /// so a chatty discovery source cannot turn into a connection storm.
    pub fn discovered(&mut self) -> bool {
        match self.phase {
            PresencePhase::Unknown | PresencePhase::Offline => {
                self.phase = PresencePhase::Connecting;
                self.dial_in_flight = true;
                true
            }
            PresencePhase::Connecting | PresencePhase::Online | PresencePhase::Stalled => false,
        }
    }

    /// A dial attempt failed. Marks the peer offline but ready to be dialled again on the
    /// next discovery event.
    pub fn dial_failed(&mut self) -> PresenceChange {
        self.dial_in_flight = false;
        let before = self.status();
        if self.phase != PresencePhase::Online {
            self.phase = PresencePhase::Offline;
        }
        Self::change(before, self.status())
    }

    /// A handshake completed.
    pub fn connected(&mut self, now: Instant) -> PresenceChange {
        let before = self.status();
        self.phase = PresencePhase::Online;
        self.last_heartbeat = Some(now);
        self.dial_in_flight = false;
        Self::change(before, self.status())
    }

    /// A heartbeat arrived.
    pub fn heartbeat(&mut self, now: Instant) -> PresenceChange {
        let before = self.status();
        self.phase = PresencePhase::Online;
        self.last_heartbeat = Some(now);
        self.dial_in_flight = false;
        Self::change(before, self.status())
    }

    /// The connection ended, whether by `goodbye`, by an error or by the socket closing.
    pub fn disconnected(&mut self) -> PresenceChange {
        self.dial_in_flight = false;
        let before = self.status();
        // A peer that we still see announced stays `Offline` (not `Unknown`): the discovery
        // record keeps it listed, and the next announcement re-dials it.
        self.phase = PresencePhase::Offline;
        Self::change(before, self.status())
    }

    /// Periodic check for a connection that went quiet without closing.
    ///
    /// A half-open TCP connection — the peer slept, the cable was pulled, a NAT rule
    /// expired — produces no error, so silence has to be turned into a state change
    /// explicitly.
    pub fn tick(&mut self, now: Instant) -> PresenceChange {
        if self.phase != PresencePhase::Online {
            return PresenceChange::Unchanged;
        }
        let Some(last) = self.last_heartbeat else {
            return PresenceChange::Unchanged;
        };
        let elapsed = now.saturating_duration_since(last);
        if elapsed > HEARTBEAT_TIMEOUT {
            self.phase = PresencePhase::Stalled;
            return PresenceChange::WentOffline;
        }
        PresenceChange::Unchanged
    }

    /// How long the peer has been silent, if it is online.
    #[must_use]
    pub fn silence(&self, now: Instant) -> Option<Duration> {
        self.last_heartbeat
            .map(|last| now.saturating_duration_since(last))
    }

    /// Forgets the device entirely. A later `discovered` treats it as brand new, which is
    /// exactly the behaviour required after "forget this user".
    pub fn forget(&mut self) {
        *self = Self::new();
    }

    /// Whether the peer is online *and* therefore writable.
    #[must_use]
    pub const fn accepts_messages(&self) -> bool {
        self.is_online()
    }

    const fn change(before: PresenceStatus, after: PresenceStatus) -> PresenceChange {
        match (before, after) {
            (PresenceStatus::Offline, PresenceStatus::Online) => PresenceChange::CameOnline,
            (PresenceStatus::Online, PresenceStatus::Offline) => PresenceChange::WentOffline,
            _ => PresenceChange::InternalOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn a_fresh_machine_is_unknown_and_offline() {
        let machine = PresenceMachine::new();
        assert_eq!(machine.phase(), PresencePhase::Unknown);
        assert_eq!(machine.status(), PresenceStatus::Offline);
        assert!(!machine.accepts_messages());
    }

    #[test]
    fn discovery_asks_for_exactly_one_dial() {
        let mut machine = PresenceMachine::new();
        assert!(machine.discovered(), "first discovery should dial");
        assert_eq!(machine.phase(), PresencePhase::Connecting);
        assert!(
            !machine.discovered(),
            "a second discovery while dialling must not stack dials"
        );
    }

    #[test]
    fn connecting_reports_offline_until_the_handshake_finishes() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        machine.discovered();
        assert_eq!(machine.status(), PresenceStatus::Offline);
        assert_eq!(machine.connected(base), PresenceChange::CameOnline);
        assert_eq!(machine.status(), PresenceStatus::Online);
        assert!(machine.accepts_messages());
    }

    #[test]
    fn failed_dial_allows_a_later_retry() {
        let mut machine = PresenceMachine::new();
        machine.discovered();
        assert_eq!(machine.dial_failed(), PresenceChange::InternalOnly);
        assert_eq!(machine.phase(), PresencePhase::Offline);
        assert!(machine.discovered(), "offline peers are dialled again");
    }

    #[test]
    fn silence_beyond_the_timeout_stalls_the_peer() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        machine.discovered();
        machine.connected(base);

        // One heartbeat interval of silence is not enough to change anything.
        assert_eq!(
            machine.tick(at(base, HEARTBEAT_TIMEOUT.as_millis() as u64 - 1)),
            PresenceChange::Unchanged
        );
        assert_eq!(machine.status(), PresenceStatus::Online);

        // Past the timeout the peer goes offline without any socket error.
        assert_eq!(
            machine.tick(at(base, HEARTBEAT_TIMEOUT.as_millis() as u64)),
            PresenceChange::Unchanged,
            "exactly at the timeout is still within budget"
        );
        assert_eq!(
            machine.tick(at(base, HEARTBEAT_TIMEOUT.as_millis() as u64 + 1)),
            PresenceChange::WentOffline
        );
        assert_eq!(machine.phase(), PresencePhase::Stalled);
        assert_eq!(machine.status(), PresenceStatus::Offline);
        assert!(!machine.accepts_messages());
    }

    #[test]
    fn a_heartbeat_revives_a_peer_and_resets_the_budget() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        machine.discovered();
        machine.connected(base);
        machine.tick(at(base, 20_000));
        assert_eq!(machine.phase(), PresencePhase::Stalled);

        // The late heartbeat arrives before the socket error is observed.
        assert_eq!(
            machine.heartbeat(at(base, 20_100)),
            PresenceChange::CameOnline
        );
        assert_eq!(machine.phase(), PresencePhase::Online);
        assert_eq!(
            machine.tick(at(base, 20_100 + 15_000)),
            PresenceChange::Unchanged
        );
    }

    #[test]
    fn stalled_peers_are_not_dialled_until_the_connection_is_torn_down() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        machine.discovered();
        machine.connected(base);
        machine.tick(at(base, 20_000));

        assert!(
            !machine.discovered(),
            "discovery must not dial over a connection that has not been cleaned up"
        );
        machine.disconnected();
        assert!(machine.discovered(), "once torn down, dial again");
    }

    #[test]
    fn disconnect_is_reported_once() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        machine.discovered();
        machine.connected(base);
        assert_eq!(machine.disconnected(), PresenceChange::WentOffline);
        assert_eq!(machine.disconnected(), PresenceChange::InternalOnly);
        assert_eq!(machine.phase(), PresencePhase::Offline);
        assert_eq!(machine.last_heartbeat(), Some(base));
    }

    #[test]
    fn losing_a_connecting_peer_is_not_a_visible_change() {
        let mut machine = PresenceMachine::new();
        machine.discovered();
        assert_eq!(machine.disconnected(), PresenceChange::InternalOnly);
        assert_eq!(machine.status(), PresenceStatus::Offline);
    }

    #[test]
    fn forget_returns_the_machine_to_unknown() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        machine.discovered();
        machine.connected(base);
        machine.forget();
        assert_eq!(machine.phase(), PresencePhase::Unknown);
        assert_eq!(machine.last_heartbeat(), None);
        assert!(machine.discovered(), "a forgotten peer is dialled as new");
    }

    #[test]
    fn tick_does_nothing_for_offline_peers() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        assert_eq!(machine.tick(at(base, 60_000)), PresenceChange::Unchanged);
        machine.discovered();
        assert_eq!(machine.tick(at(base, 60_000)), PresenceChange::Unchanged);
        assert_eq!(machine.phase(), PresencePhase::Connecting);
    }

    #[test]
    fn silence_reports_the_elapsed_time_and_is_none_while_unknown() {
        let base = Instant::now();
        let mut machine = PresenceMachine::new();
        assert_eq!(machine.silence(base), None);
        machine.connected(base);
        assert_eq!(
            machine.silence(at(base, 2_500)),
            Some(Duration::from_millis(2_500))
        );
    }

    #[test]
    fn a_full_lifecycle_behaves_as_the_ui_expects() {
        let base = Instant::now();

        // Seen for the first time from discovery alone: listed, offline.
        let mut machine = PresenceMachine::new();
        machine.discovered();
        assert_eq!(machine.status(), PresenceStatus::Offline);

        // Handshake: online, writable.
        machine.connected(base);
        assert_eq!(machine.status(), PresenceStatus::Online);
        assert!(machine.accepts_messages());

        // The peer is killed without a goodbye frame. Within the timeout nothing changes.
        assert_eq!(machine.tick(at(base, 5_000)), PresenceChange::Unchanged);
        assert_eq!(machine.status(), PresenceStatus::Online);

        // After the timeout the peer reads as offline and cannot be written to.
        assert_eq!(machine.tick(at(base, 16_000)), PresenceChange::WentOffline);
        assert_eq!(machine.status(), PresenceStatus::Offline);
        assert!(!machine.accepts_messages());
    }
}
