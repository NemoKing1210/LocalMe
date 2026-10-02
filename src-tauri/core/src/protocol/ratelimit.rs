//! Token bucket rate limiting for inbound frames.
//!
//! Pure arithmetic over caller-supplied instants, so it is unit-tested without sleeping and the
//! connection loop pays one comparison per frame.

use std::time::Instant;

/// A classic token bucket: `capacity` is the burst a peer may spend at once and
/// `refill_per_second` is the sustained rate.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_second: f64,
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// Starts full, so a connection may burst immediately.
    #[must_use]
    pub fn new(capacity: f64, refill_per_second: f64, now: Instant) -> Self {
        let capacity = if capacity.is_finite() && capacity > 0.0 {
            capacity
        } else {
            1.0
        };
        let refill_per_second = if refill_per_second.is_finite() && refill_per_second > 0.0 {
            refill_per_second
        } else {
            1.0
        };
        Self {
            capacity,
            refill_per_second,
            tokens: capacity,
            last_refill: now,
        }
    }

    /// Returns `true` when the frame is allowed. A denied frame costs nothing — the connection
    /// is closed on the first denial, so there is no reason to keep counting.
    pub fn try_acquire(&mut self, now: Instant) -> bool {
        self.try_acquire_n(1.0, now)
    }

    /// The same for a cost measured in something other than frames — file payload bytes, where
    /// one frame may spend forty kilobytes of the budget.
    ///
    /// An amount larger than the capacity can never be admitted, which is why the capacity of a
    /// byte budget is chosen above the largest single spend
    /// ([`FILE_CHUNK_BYTES`](crate::protocol::limits::FILE_CHUNK_BYTES)).
    pub fn try_acquire_n(&mut self, amount: f64, now: Instant) -> bool {
        let amount = if amount.is_finite() && amount > 0.0 {
            amount
        } else {
            1.0
        };
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_per_second).min(self.capacity);
            self.last_refill = now;
        }
        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }

    #[must_use]
    pub fn available(&self, now: Instant) -> f64 {
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        (self.tokens + elapsed * self.refill_per_second).min(self.capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn a_burst_is_allowed_then_refused() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(3.0, 1.0, base);

        assert!(bucket.try_acquire(base));
        assert!(bucket.try_acquire(base));
        assert!(bucket.try_acquire(base));
        assert!(
            !bucket.try_acquire(base),
            "the fourth frame in the same instant exceeds the burst"
        );
    }

    #[test]
    fn tokens_refill_at_the_configured_rate() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(2.0, 1.0, base);
        assert!(bucket.try_acquire(base));
        assert!(bucket.try_acquire(base));
        assert!(!bucket.try_acquire(base));

        assert!(!bucket.try_acquire(at(base, 500)));
        assert!(bucket.try_acquire(at(base, 1_000)));
        assert!(!bucket.try_acquire(at(base, 1_000)));
    }

    #[test]
    fn refill_never_exceeds_the_capacity() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(2.0, 10.0, base);
        let later = at(base, 60_000);
        assert!((bucket.available(later) - 2.0).abs() < f64::EPSILON);
        assert!(bucket.try_acquire(later));
        assert!(bucket.try_acquire(later));
        assert!(!bucket.try_acquire(later));
    }

    #[test]
    fn a_long_silence_does_not_hand_out_a_huge_burst() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(5.0, 5.0, base);
        let much_later = at(base, 3_600_000);
        for _ in 0..5 {
            assert!(bucket.try_acquire(much_later));
        }
        assert!(
            !bucket.try_acquire(much_later),
            "an hour of idleness must still cap the burst at the capacity"
        );
    }

    #[test]
    fn a_sustained_stream_at_the_limit_is_admitted() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(10.0, 20.0, base);
        for index in 0..100u64 {
            let now = at(base, index * 50);
            assert!(
                bucket.try_acquire(now),
                "frame {index} at {now:?} was refused"
            );
        }
    }

    #[test]
    fn a_stream_above_the_limit_is_cut_off_after_the_burst() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(10.0, 20.0, base);
        let mut admitted = 0_u64;
        for index in 0..200_u64 {
            if bucket.try_acquire(at(base, index * 10)) {
                admitted += 1;
            }
        }
        // The budget is the burst plus two seconds of refill: 10 + 40 = 50.
        assert!(
            (48..=52).contains(&admitted),
            "{admitted} of 200 frames admitted"
        );
        assert!(
            admitted < 200 / 2,
            "the limiter must cut the stream down, not merely trim it"
        );
    }

    #[test]
    fn nonsensical_parameters_degrade_to_a_usable_bucket() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(f64::NAN, 0.0, base);
        assert!(bucket.try_acquire(base));
        assert!(!bucket.try_acquire(base));
    }

    #[test]
    fn a_byte_budget_spends_and_refills_in_bytes() {
        let base = Instant::now();
        // 1 MiB burst, 1 MiB/s sustained: a 40 KiB chunk costs 40 KiB of the budget.
        let mut bucket = TokenBucket::new(1024.0 * 1024.0, 1024.0 * 1024.0, base);
        assert!(bucket.try_acquire_n(40.0 * 1024.0, base));
        assert_eq!(bucket.available(base), 1024.0 * 1024.0 - 40.0 * 1024.0);

        // An amount above the capacity is never admitted, however long it waits.
        assert!(!bucket.try_acquire_n(2.0 * 1024.0 * 1024.0, at(base, 10_000)));
        // Half a second of refill buys 512 KiB, so the next chunk fits.
        assert!(bucket.try_acquire_n(40.0 * 1024.0, at(base, 500)));
        // A denied spend costs nothing, which is what makes a retry free.
        let before = bucket.available(at(base, 500));
        assert!(!bucket.try_acquire_n(4.0 * 1024.0 * 1024.0, at(base, 500)));
        assert_eq!(bucket.available(at(base, 500)), before);
    }

    #[test]
    fn a_non_positive_or_non_finite_cost_is_treated_as_one() {
        let base = Instant::now();
        let mut bucket = TokenBucket::new(2.0, 1.0, base);

        // A nonsense amount must cost one token, not zero and not poison the bucket with NaN.
        assert!(bucket.try_acquire_n(f64::NAN, base));
        assert!(bucket.try_acquire_n(0.0, base));
        assert!(
            !bucket.try_acquire_n(-5.0, base),
            "two tokens are now spent"
        );
        // A second of refill buys one token, which a nonsense positive cost still consumes.
        assert!(bucket.try_acquire_n(f64::INFINITY, at(base, 1_000)));
    }
}
