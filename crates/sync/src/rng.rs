//! A seeded pseudo-random generator, written here rather than vendored.
//!
//! SplitMix64. Two reasons it lives in this repository instead of coming from
//! a crate: the simulator's whole value is that a scenario reproduces exactly,
//! which means the generator is part of the fixture and not an implementation
//! detail that a dependency bump can move; and the same 20 lines have to be
//! mirrored in C when the firmware wants to replay a scenario.
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
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn a_seed_reproduces_its_stream() {
        let mut a = Rng::new(0x5EED_0001);
        let mut b = Rng::new(0x5EED_0001);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let mut same = 0;
        for _ in 0..1000 {
            if a.next_u64() == b.next_u64() {
                same += 1;
            }
        }
        assert_eq!(same, 0, "two seeds produced overlapping streams");
    }

    #[test]
    fn doubles_stay_in_the_unit_interval() {
        let mut rng = Rng::new(42);
        for _ in 0..10_000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x), "{} is outside [0, 1)", x);
        }
    }

    #[test]
    fn doubles_are_spread_across_the_interval() {
        // A generator stuck near one value would still pass the bounds test.
        let mut rng = Rng::new(7);
        let mut buckets = [0u32; 10];
        for _ in 0..100_000 {
            let x = rng.next_f64();
            buckets[(x * 10.0) as usize] += 1;
        }
        for (i, count) in buckets.iter().enumerate() {
            assert!(
                *count > 8_000 && *count < 12_000,
                "bucket {} holds {} of 100000 draws",
                i,
                count
            );
        }
    }
}
