//! A seeded pseudo-random generator, owned here rather than shared or
//! vendored.
//!
//! SplitMix64, the same twenty lines `docs/decisions/0002-repository-layout-and-ci.md`
//! points at when it says a seeded PRNG is exactly the case where this
//! repository builds rather than vendors: a fixture's whole value is that it
//! reproduces byte for byte, so the generator is part of the fixture and not an
//! implementation detail a dependency bump can move.
//!
//! `crates/sync` carries the same generator. That duplication is deliberate and
//! is the point of this phase's ordering: the measurement rig is built before
//! the servo so that it cannot be written to agree with it, and a rig that
//! imported the servo simulator's crate to make its own fixtures would have
//! taken a dependency on the thing it exists to measure. Twenty lines is a
//! cheaper price than that edge.
//!
//! Reference: Steele, Lea and Flood, "Fast splittable pseudorandom number
//! generators" (2014). The constants below are that paper's.

/// A SplitMix64 generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator at the given seed. Every seed is valid.
    pub fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A double in `[0, 1)`, using the top 53 bits so that every value is
    /// exactly representable.
    pub fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
        (self.next_u64() >> 11) as f64 * SCALE
    }

    /// A double in `[-1, 1)`.
    pub fn next_symmetric(&mut self) -> f64 {
        self.next_f64() * 2.0 - 1.0
    }

    /// A draw from a standard normal, by the Box-Muller transform.
    ///
    /// Injected timing jitter is modelled as Gaussian because that is what a
    /// sum of many small independent delays looks like, and because a fixture
    /// whose noise is uniform would understate the tails the confidence bound
    /// exists to notice.
    pub fn next_normal(&mut self) -> f64 {
        // next_f64 can return exactly zero, whose logarithm is not finite.
        let u1 = 1.0 - self.next_f64();
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn a_seed_reproduces_its_stream() {
        let mut a = Rng::new(0x5EED_0026);
        let mut b = Rng::new(0x5EED_0026);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn symmetric_draws_stay_in_their_interval_and_centre_on_zero() {
        let mut rng = Rng::new(11);
        let mut sum = 0.0;
        for _ in 0..100_000 {
            let x = rng.next_symmetric();
            assert!((-1.0..1.0).contains(&x), "{} is outside [-1, 1)", x);
            sum += x;
        }
        assert!((sum / 100_000.0).abs() < 0.01, "mean drifted to {}", sum);
    }

    #[test]
    fn normal_draws_have_about_unit_variance() {
        let mut rng = Rng::new(23);
        let n = 200_000;
        let mut sum = 0.0;
        let mut sum_sq = 0.0;
        for _ in 0..n {
            let x = rng.next_normal();
            sum += x;
            sum_sq += x * x;
        }
        let mean = sum / f64::from(n);
        let variance = sum_sq / f64::from(n) - mean * mean;
        assert!(mean.abs() < 0.02, "mean is {}", mean);
        assert!((variance - 1.0).abs() < 0.03, "variance is {}", variance);
    }
}
