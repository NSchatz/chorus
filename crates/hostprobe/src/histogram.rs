//! Order statistics over a run's samples, and a power-of-two bucket table.
//!
//! Pure arithmetic: no clock, no I/O. A percentile is the nearest-rank one
//! (the smallest sample with at least `p` percent of the samples at or below it),
//! so every figure a report prints is a sample that was actually observed.

/// The order statistics of one set of samples, in the samples' own unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// How many samples.
    pub count: usize,
    /// The smallest sample.
    pub min: i64,
    /// Nearest-rank 50th percentile.
    pub p50: i64,
    /// Nearest-rank 99th percentile.
    pub p99: i64,
    /// Nearest-rank 99.9th percentile.
    pub p999: i64,
    /// The largest sample.
    pub max: i64,
    /// The arithmetic mean, rounded toward zero.
    pub mean: i64,
}

/// The nearest-rank percentile of already sorted samples; `per_mille` is 500 for p50,
/// 999 for p99.9. None for an empty slice.
pub fn nearest_rank(sorted: &[i64], per_mille: u32) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let n = sorted.len() as u128;
    // rank = ceil(p * n), 1-based, clamped to [1, n].
    let rank = ((u128::from(per_mille) * n).div_ceil(1000)).clamp(1, n) as usize;
    Some(sorted[rank - 1])
}

/// Summarise `samples` (sorted in place). None when there are none.
pub fn summarise(samples: &mut [i64]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let sum: i128 = samples.iter().map(|&s| i128::from(s)).sum();
    Some(Summary {
        count: samples.len(),
        min: samples[0],
        p50: nearest_rank(samples, 500)?,
        p99: nearest_rank(samples, 990)?,
        p999: nearest_rank(samples, 999)?,
        max: samples[samples.len() - 1],
        mean: (sum / samples.len() as i128) as i64,
    })
}

/// How many samples are at or above `threshold`.
pub fn count_at_or_above(samples: &[i64], threshold: i64) -> usize {
    samples.iter().filter(|&&s| s >= threshold).count()
}

/// Power-of-two buckets over non-negative nanosecond samples, in microseconds:
/// bucket 0 is [0, 1 us), bucket k is [2^(k-1), 2^k) us. Returns (upper bound in us,
/// count) for every bucket up to the last non-empty one. Negative samples count in
/// bucket 0.
pub fn log2_buckets_us(samples_ns: &[i64]) -> Vec<(u64, usize)> {
    let mut counts = [0usize; 40];
    for &s in samples_ns {
        let us = (s.max(0) as u64) / 1_000;
        let k = if us == 0 {
            0
        } else {
            (64 - us.leading_zeros()) as usize
        };
        counts[k.min(counts.len() - 1)] += 1;
    }
    let last = counts.iter().rposition(|&c| c > 0).unwrap_or(0);
    (0..=last).map(|k| (1u64 << k, counts[k])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_on_one_to_a_thousand() {
        let mut v: Vec<i64> = (1..=1000).rev().collect();
        let s = summarise(&mut v).unwrap();
        assert_eq!(
            (s.count, s.min, s.p50, s.p99, s.p999, s.max),
            (1000, 1, 500, 990, 999, 1000)
        );
        assert_eq!(s.mean, 500); // 500.5 rounded toward zero
    }

    #[test]
    fn one_outlier_in_a_thousand_is_the_max_but_not_the_p999() {
        let mut v = vec![10i64; 999];
        v.push(5_000);
        let s = summarise(&mut v).unwrap();
        assert_eq!((s.p99, s.p999, s.max), (10, 10, 5_000));
    }

    #[test]
    fn two_outliers_in_a_thousand_reach_the_p999() {
        let mut v = vec![10i64; 998];
        v.extend([5_000, 6_000]);
        let s = summarise(&mut v).unwrap();
        assert_eq!((s.p999, s.max), (5_000, 6_000));
    }

    #[test]
    fn empty_and_single() {
        assert_eq!(summarise(&mut []), None);
        let s = summarise(&mut [7]).unwrap();
        assert_eq!((s.p50, s.p999, s.max), (7, 7, 7));
    }

    #[test]
    fn buckets_are_powers_of_two_in_microseconds() {
        let b = log2_buckets_us(&[0, 999, 1_000, 1_999, 2_000, 5_000, -3]);
        assert_eq!(b, vec![(1, 3), (2, 2), (4, 1), (8, 1)]);
        assert_eq!(count_at_or_above(&[1, 2, 3], 2), 2);
    }
}
