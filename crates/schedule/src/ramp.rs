//! Volume ramps: a straight line in amplitude, in integer thousandths.
//!
//! A ramp is `from`, `to` and a duration; sampled at an elapsed time it is
//!
//! ```text
//! from + (to - from) * elapsed / duration     (integer division, toward from)
//! ```
//!
//! so it is exactly `from` at 0, exactly `to` at and after the end, and
//! monotone in between (non-decreasing up, non-increasing down): truncating a
//! monotone quotient toward zero keeps it monotone. Linear in **amplitude**,
//! the same thing the `room_volume` message's `ramp_ms` means on the
//! endpoint, so a ramp the server plans and one an endpoint runs are the same
//! line. Integer arithmetic only, widened to 128 bits, so no input overflows
//! and every platform computes the same values.
//!
//! Elapsed time is a **monotonic** duration in nanoseconds: a ramp is a
//! length of time, and a wall clock stepped mid-ramp must not jump it.

/// The alarm's ramp length when the command names none, in seconds.
/// ASSUMED: thirty seconds from silence to the set volume; a gentle default,
/// not a measured one.
pub const DEFAULT_ALARM_RAMP_S: u32 = 30;

/// A ramp between two volumes in thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ramp {
    /// The value at elapsed 0.
    pub from: u16,
    /// The value at and after the end.
    pub to: u16,
    /// Its length, in nanoseconds; 0 is a step straight to `to`.
    pub duration_ns: u64,
}

impl Ramp {
    /// A ramp lasting `duration_ms` milliseconds.
    pub fn new(from: u16, to: u16, duration_ms: u64) -> Ramp {
        Ramp {
            from,
            to,
            duration_ns: duration_ms.saturating_mul(1_000_000),
        }
    }

    /// The value `elapsed_ns` nanoseconds after the ramp began.
    pub fn sample(&self, elapsed_ns: u64) -> u16 {
        if elapsed_ns >= self.duration_ns {
            return self.to;
        }
        let delta = i128::from(self.to) - i128::from(self.from);
        let step = delta * i128::from(elapsed_ns) / i128::from(self.duration_ns);
        (i128::from(self.from) + step) as u16
    }

    /// Whether the ramp has reached `to` by `elapsed_ns`.
    pub fn is_done(&self, elapsed_ns: u64) -> bool {
        elapsed_ns >= self.duration_ns
    }
}

/// The alarm's ramp: from silence up to `target` over `ramp_s` seconds.
pub fn alarm_ramp(target: u16, ramp_s: u32) -> Ramp {
    Ramp::new(0, target, u64::from(ramp_s) * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seeded linear congruential generator: many sampled points, the same
    /// ones every run (MMIX's constants).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 11
        }
    }

    #[test]
    fn endpoints_are_exact_and_every_ramp_is_monotone() {
        let mut rng = Lcg(0x5eed);
        for _ in 0..2000 {
            let from = (rng.next() % 1001) as u16;
            let to = (rng.next() % 1001) as u16;
            let dur_ms = rng.next() % 120_000;
            let r = Ramp::new(from, to, dur_ms);
            assert_eq!(r.sample(0), if dur_ms == 0 { to } else { from });
            assert_eq!(r.sample(r.duration_ns), to);
            assert_eq!(r.sample(r.duration_ns + 1 + rng.next() % 1_000_000_000), to);
            assert_eq!(r.sample(u64::MAX), to);
            let mut points: Vec<u64> = (0..64).map(|_| rng.next() % (r.duration_ns + 2)).collect();
            points.sort_unstable();
            let values: Vec<u16> = points.iter().map(|&e| r.sample(e)).collect();
            let (lo, hi) = (from.min(to), from.max(to));
            for w in values.windows(2) {
                if to >= from {
                    assert!(w[0] <= w[1], "{r:?} not non-decreasing: {values:?}");
                } else {
                    assert!(w[0] >= w[1], "{r:?} not non-increasing: {values:?}");
                }
            }
            assert!(values.iter().all(|v| (lo..=hi).contains(v)));
        }
    }

    #[test]
    fn linear_in_amplitude() {
        let r = Ramp::new(0, 1000, 30_000);
        assert_eq!(r.sample(15_000_000_000), 500);
        assert_eq!(r.sample(3_000_000_000), 100);
        // Truncation is toward `from` both ways.
        assert_eq!(Ramp::new(0, 1000, 3).sample(1_000_000), 333);
        assert_eq!(Ramp::new(1000, 0, 3).sample(1_000_000), 667);
    }

    #[test]
    fn the_alarm_ramp() {
        let r = alarm_ramp(400, DEFAULT_ALARM_RAMP_S);
        assert_eq!(r.sample(0), 0);
        assert_eq!(r.sample(29_999_999_999), 399);
        assert_eq!(r.sample(30_000_000_000), 400);
        assert!(r.is_done(30_000_000_000) && !r.is_done(29_999_999_999));
    }
}
