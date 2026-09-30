//! Time as a dependency, injected rather than read from the environment.
//!
//! Two kinds of time matter here and they are never mixed:
//!
//! * **Monotonic** (`Instant`) drives timeouts, heartbeats and rate limiting. It cannot jump.
//! * **Wall clock** (`UnixMillis`) is only ever written down as an observation — `sent_at`,
//!   `last_seen` — and displayed. It is never used for ordering decisions that must be
//!   correct, because the user's clock may be wrong and the network's clocks are not ours
//!   to trust (`docs/ARCHITECTURE.md` §13.2).

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch, as observed by the local clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixMillis(pub i64);

impl UnixMillis {
    /// Reads the system clock.
    #[must_use]
    pub fn now() -> Self {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(since_epoch) => {
                let millis = since_epoch.as_millis();
                // Saturating rather than wrapping: a clock set past year 292 278 994 should
                // pin, not silently become 1970.
                Self(i64::try_from(millis).unwrap_or(i64::MAX))
            }
            // A clock set before 1970 is technically possible; report the epoch rather than
            // panicking, since this value is only ever displayed.
            Err(_) => Self(0),
        }
    }

    /// The raw millisecond count.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

/// The clock the services read, so tests can control time.
///
/// Implementations must be cheap to clone and safe to share: the session actor clones it
/// into every connection task.
pub trait Clock: Clone + Send + Sync + 'static {
    /// Monotonic instant used for all timeout arithmetic.
    fn now(&self) -> Instant;

    /// Wall-clock instant used for values that are stored and displayed.
    fn wall(&self) -> UnixMillis;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> UnixMillis {
        UnixMillis::now()
    }
}

/// A clock that only moves when a test tells it to.
///
/// Wall time and monotonic time advance together, so a test can assert on both without
/// keeping two fictions in sync.
#[derive(Debug, Clone)]
pub struct ManualClock {
    base: Instant,
    elapsed: std::sync::Arc<std::sync::atomic::AtomicU64>,
    wall_start: i64,
}

impl ManualClock {
    /// Creates a clock positioned at `wall_start` milliseconds since the epoch.
    #[must_use]
    pub fn new(wall_start: i64) -> Self {
        Self {
            base: Instant::now(),
            elapsed: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            wall_start,
        }
    }

    /// Advances every reading of this clock by `millis`.
    pub fn advance(&self, millis: u64) {
        self.elapsed
            .fetch_add(millis, std::sync::atomic::Ordering::Relaxed);
    }

    fn elapsed(&self) -> u64 {
        self.elapsed.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.base + std::time::Duration::from_millis(self.elapsed())
    }

    fn wall(&self) -> UnixMillis {
        let elapsed = i64::try_from(self.elapsed()).unwrap_or(i64::MAX);
        UnixMillis(self.wall_start.saturating_add(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn system_clock_reads_a_plausible_wall_time() {
        // Later than 2020-01-01 and earlier than 2100-01-01: a sanity bound that fails if
        // the epoch conversion is ever wrong.
        let now = UnixMillis::now().as_i64();
        assert!(now > 1_577_836_800_000, "clock reads {now}");
        assert!(now < 4_102_444_800_000, "clock reads {now}");
    }

    #[test]
    fn manual_clock_only_moves_when_advanced() {
        let clock = ManualClock::new(1_000);
        let start = clock.now();
        assert_eq!(clock.wall().as_i64(), 1_000);
        assert_eq!(clock.now(), start);

        clock.advance(250);
        assert_eq!(clock.wall().as_i64(), 1_250);
        assert_eq!(
            clock.now().duration_since(start),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn manual_clock_is_shared_between_clones() {
        let clock = ManualClock::new(0);
        let clone = clock.clone();
        clock.advance(10);
        assert_eq!(clone.wall().as_i64(), 10);
    }
}
