//! A file that is half prose and passes, which is what makes this tree the
//! discriminating one.
//!
//! A gate that refused every commented file and a gate that refuses only the
//! narrated ones leave identical evidence against the four trees beside this
//! one. So this file carries real documentation, sits in the warn band, is
//! named there, and does not fail the build.

/// The bound a caller may not exceed, in microseconds.
///
/// It is a constant rather than a configuration value because every reader of
/// a saved log has to be able to reproduce the arithmetic without the
/// configuration that produced it.
pub const BOUND_US: u64 = 300_000;

/// Clamps a requested fill to the bound above.
///
/// Returns the fill that was applied and whether the clamp bit was set, which
/// is what a caller records rather than the request it made: a log that carried
/// the request would say what was asked for and never what happened.
pub fn clamp(requested_us: u64) -> (u64, bool) {
    if requested_us > BOUND_US {
        (BOUND_US, true)
    } else {
        (requested_us, false)
    }
}

pub fn twice(value: u64) -> u64 {
    value * 2
}

pub fn thrice(value: u64) -> u64 {
    value * 3
}

pub fn sum(left: u64, right: u64) -> u64 {
    left + right
}

pub fn difference(left: u64, right: u64) -> u64 {
    left.saturating_sub(right)
}
