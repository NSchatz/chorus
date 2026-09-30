//! Pairing two clients' free-run series into the relative series the free-run
//! fit reads (audit A-4).
//!
//! # What goes in
//!
//! Each client, run with `chorus-client --free-run --offsets-out <file>`,
//! writes its own playout offset against the SERVER timeline with correction
//! disabled: one `t_ns offset_ns` pair per sync tick, where `t_ns` is the
//! tick's instant on the server timeline and `offset_ns` is how far that
//! client's audible output is from where the server timeline puts it. The file
//! says so in two header keys, `correction = disabled` and `time_base =
//! server`, and a series without both is refused: a series taken with the
//! servo running is the servo's residual, not a crystal walking free, and a
//! series on a client's own clock cannot be lined up with another client's.
//!
//! # What comes out
//!
//! The two clients tick at their own moments, so their timestamps never
//! coincide. For every observation of client A that falls inside client B's
//! span, B's offset is linearly interpolated at A's instant and the relative
//! offset `A - B` is recorded at that instant. The server's own crystal cancels
//! in the difference, which is why the slope of the result is the relative
//! rate of the two clients' DACs and nothing else. An interpolation across a
//! gap wider than the run declared (`max_gap_ns`) is skipped rather than
//! bridged: a straight line drawn across a missing stretch would be a guess
//! presented as an observation.
//!
//! # Time
//!
//! Every figure comes from the timestamps the files carry. Nothing here reads a
//! clock.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::freerun::{self, Observation, OffsetSeries, OBSERVATIONS_SECTION};

/// The value of `correction` a client series has to carry.
pub const CORRECTION_DISABLED: &str = "disabled";

/// The value of `time_base` a client series has to carry.
pub const TIME_BASE_SERVER: &str = "server";

/// One client's series, and the two header keys that say what it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientSeries {
    /// The observations, as the free-run reader parses them.
    pub series: OffsetSeries,
    /// The `correction` key.
    pub correction: Option<String>,
    /// The `time_base` key.
    pub time_base: Option<String>,
}

/// Why two series could not be paired.
#[derive(Debug, Clone, PartialEq)]
pub enum PairError {
    /// A series file could not be read or parsed.
    Series(String),
    /// A series does not say it was taken with correction disabled.
    NotFreeRun {
        /// The file.
        path: PathBuf,
        /// What it said, if anything.
        correction: Option<String>,
    },
    /// A series is not on the server timeline.
    NotServerTimeBase {
        /// The file.
        path: PathBuf,
        /// What it said, if anything.
        time_base: Option<String>,
    },
    /// A series' timestamps do not strictly increase.
    NotIncreasing {
        /// The file.
        path: PathBuf,
        /// The index of the first observation that does not.
        index: usize,
    },
    /// Too few observations of A fall inside B's span to fit anything.
    TooFewPairs {
        /// How many were paired.
        paired: usize,
    },
}

impl PairError {
    /// A short, stable token naming which refusal this is.
    pub fn condition(&self) -> &'static str {
        match self {
            PairError::Series(_) => "series-unreadable",
            PairError::NotFreeRun { .. } => "not-free-run",
            PairError::NotServerTimeBase { .. } => "not-server-time-base",
            PairError::NotIncreasing { .. } => "timestamps-not-increasing",
            PairError::TooFewPairs { .. } => "too-few-pairs",
        }
    }
}

impl fmt::Display for PairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PairError::Series(detail) => write!(f, "{}", detail),
            PairError::NotFreeRun { path, correction } => write!(
                f,
                "'{}' says correction = {}, and a free-run baseline needs both clients with \
                 correction DISABLED (chorus-client --free-run); a series taken with the servo \
                 running is the servo's residual and not a crystal walking free",
                path.display(),
                correction.as_deref().unwrap_or("<absent>")
            ),
            PairError::NotServerTimeBase { path, time_base } => write!(
                f,
                "'{}' says time_base = {}, and two clients' series can only be lined up on the \
                 server timeline (time_base = server)",
                path.display(),
                time_base.as_deref().unwrap_or("<absent>")
            ),
            PairError::NotIncreasing { path, index } => write!(
                f,
                "'{}': observation {} does not come after the one before it, and a monotonic \
                 series never goes backwards",
                path.display(),
                index
            ),
            PairError::TooFewPairs { paired } => write!(
                f,
                "only {} observation(s) of the first client fall inside the second client's \
                 span with no gap wider than the run declared, which is too few to pair",
                paired
            ),
        }
    }
}

impl std::error::Error for PairError {}

/// Read one client's series.
pub fn read_client_series(path: &Path) -> Result<ClientSeries, PairError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| PairError::Series(format!("'{}' could not be read: {}", path.display(), e)))?;
    parse_client_series(path, &text)
}

/// Parse one client's series already in hand.
pub fn parse_client_series(path: &Path, text: &str) -> Result<ClientSeries, PairError> {
    let series = freerun::parse_series(path, text).map_err(|e| PairError::Series(e.to_string()))?;
    let mut correction = None;
    let mut time_base = None;
    for raw in text.lines() {
        let line = match raw.find('#') {
            Some(at) => &raw[..at],
            None => raw,
        }
        .trim();
        if line == OBSERVATIONS_SECTION {
            break;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "correction" => correction = Some(value.trim().to_string()),
                "time_base" => time_base = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }
    Ok(ClientSeries {
        series,
        correction,
        time_base,
    })
}

fn check(client: &ClientSeries) -> Result<(), PairError> {
    let path = client.series.path.clone();
    if client.correction.as_deref() != Some(CORRECTION_DISABLED) {
        return Err(PairError::NotFreeRun {
            path,
            correction: client.correction.clone(),
        });
    }
    if client.time_base.as_deref() != Some(TIME_BASE_SERVER) {
        return Err(PairError::NotServerTimeBase {
            path,
            time_base: client.time_base.clone(),
        });
    }
    for (index, pair) in client.series.observations.windows(2).enumerate() {
        if pair[1].t_ns <= pair[0].t_ns {
            return Err(PairError::NotIncreasing {
                path,
                index: index + 1,
            });
        }
    }
    Ok(())
}

/// Pair two clients' series into relative observations `A - B` at A's
/// instants.
pub fn pair(
    a: &ClientSeries,
    b: &ClientSeries,
    max_gap_ns: i64,
) -> Result<Vec<Observation>, PairError> {
    check(a)?;
    check(b)?;
    let bs = &b.series.observations;
    let mut out = Vec::new();
    let mut at = 0usize;
    for obs in &a.series.observations {
        while at + 1 < bs.len() && bs[at + 1].t_ns < obs.t_ns {
            at += 1;
        }
        if at + 1 >= bs.len() {
            break;
        }
        let (lo, hi) = (bs[at], bs[at + 1]);
        if obs.t_ns < lo.t_ns || obs.t_ns > hi.t_ns {
            continue;
        }
        if hi.t_ns - lo.t_ns > max_gap_ns {
            continue;
        }
        let span = i128::from(hi.t_ns - lo.t_ns);
        let into = i128::from(obs.t_ns - lo.t_ns);
        let rise = i128::from(hi.offset_ns - lo.offset_ns);
        // Round half away from zero, in integers, so the pairing is exact and
        // reproducible on every platform.
        let num = rise * into;
        let step = if num >= 0 {
            (num + span / 2) / span
        } else {
            (num - span / 2) / span
        };
        let b_at = i128::from(lo.offset_ns) + step;
        out.push(Observation {
            t_ns: obs.t_ns,
            offset_ns: (i128::from(obs.offset_ns) - b_at) as i64,
        });
    }
    if out.len() < 2 {
        return Err(PairError::TooFewPairs { paired: out.len() });
    }
    Ok(out)
}

/// Render the paired series in the `.offsets` format the free-run fit reads.
pub fn render_paired(
    label: &str,
    a: &ClientSeries,
    b: &ClientSeries,
    pairs: &[Observation],
) -> String {
    let mut out = String::new();
    out.push_str(
        "# Relative playout offset between two clients running with correction DISABLED\n\
         # (free-run), paired on the server timeline by `chorus-measure pair`: client A's\n\
         # offset minus client B's, interpolated at A's instants. The server's crystal cancels\n\
         # in the difference, so the slope is the two clients' relative rate.\n\
         #\n\
         # Format: key = value, then [observations], then one 't_ns offset_ns' pair per line.\n\n",
    );
    out.push_str(&format!("label = {}\n", label));
    out.push_str(&format!("correction = {}\n", CORRECTION_DISABLED));
    out.push_str(&format!("time_base = {}\n", TIME_BASE_SERVER));
    out.push_str(&format!("client_a = {}\n", a.series.label));
    out.push_str(&format!("client_b = {}\n", b.series.label));
    out.push_str(&format!("\n{}\n", OBSERVATIONS_SECTION));
    for obs in pairs {
        out.push_str(&format!("{} {}\n", obs.t_ns, obs.offset_ns));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(label: &str, body: &str) -> ClientSeries {
        let text = format!(
            "label = {}\ncorrection = disabled\ntime_base = server\n[observations]\n{}",
            label, body
        );
        parse_client_series(Path::new(label), &text).unwrap()
    }

    #[test]
    fn two_straight_lines_pair_into_their_difference() {
        // A walks +10 ns/s, B walks -5 ns/s and ticks half a second later.
        let a = client("a", "0 0\n1000000000 10\n2000000000 20\n3000000000 30\n");
        let b = client("b", "500000000 -2\n1500000000 -7\n2500000000 -12\n");
        let pairs = pair(&a, &b, 5_000_000_000).unwrap();
        // A at 1 s and 2 s fall inside B's span; B there is -4.5 -> -5 (half
        // away from zero) and -9.5 -> -10.
        assert_eq!(
            pairs,
            vec![
                Observation {
                    t_ns: 1_000_000_000,
                    offset_ns: 15
                },
                Observation {
                    t_ns: 2_000_000_000,
                    offset_ns: 30
                },
            ]
        );
    }

    #[test]
    fn a_series_with_the_servo_running_is_refused_by_name() {
        let a = client("a", "0 0\n1 1\n");
        let text =
            "label = b\ncorrection = enabled\ntime_base = server\n[observations]\n0 0\n1 1\n";
        let b = parse_client_series(Path::new("b"), text).unwrap();
        let err = pair(&a, &b, 10).unwrap_err();
        assert_eq!(err.condition(), "not-free-run");
    }

    #[test]
    fn a_series_on_a_client_clock_is_refused_by_name() {
        let a = client("a", "0 0\n1 1\n");
        let text = "label = b\ncorrection = disabled\n[observations]\n0 0\n1 1\n";
        let b = parse_client_series(Path::new("b"), text).unwrap();
        assert_eq!(
            pair(&a, &b, 10).unwrap_err().condition(),
            "not-server-time-base"
        );
    }

    #[test]
    fn a_gap_wider_than_declared_is_skipped_not_bridged() {
        let a = client("a", "1000 0\n2000 0\n6000 0\n");
        let b = client("b", "0 0\n1500 0\n2500 0\n9000 0\n");
        let pairs = pair(&a, &b, 1_500).unwrap();
        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().all(|p| p.t_ns != 6000));
    }

    #[test]
    fn timestamps_that_go_backwards_are_refused() {
        let a = client("a", "0 0\n5 0\n3 0\n");
        let b = client("b", "0 0\n9 0\n");
        assert_eq!(
            pair(&a, &b, 100).unwrap_err().condition(),
            "timestamps-not-increasing"
        );
    }
}
