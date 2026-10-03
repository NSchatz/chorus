//! Durations and positions as AVTransport writes them.
//!
//! AVT1 section 2.2.14 (CurrentTrackDuration) gives the form
//! `H+:MM:SS[.F+]` or `H+:MM:SS[.F0/F1]`: one or more hour digits, two digits
//! of minutes and of seconds, and an optional fraction of a second, either
//! decimal digits or a ratio. The same form is used by CurrentMediaDuration,
//! RelativeTimePosition, AbsoluteTimePosition, the `REL_TIME` seek target
//! (AVT1 section 2.2.29: type "time") and DIDL-Lite's `res@duration`.

/// What a string variable holds when the feature behind it is not
/// implemented (AVT1 sections 2.2.14 to 2.2.23 use this value for durations,
/// metadata and positions alike).
pub const NOT_IMPLEMENTED: &str = "NOT_IMPLEMENTED";

/// What an `i4` position counter holds when counters are not supported: "the
/// maximum value of the i4 data type" (AVT1 section 2.2.25).
pub const NOT_IMPLEMENTED_I4: i32 = i32::MAX;

/// `H:MM:SS` for a time in milliseconds, with no fraction: the fraction is
/// optional in the form, and whole seconds are what every reader takes (the
/// millisecond part is dropped, not rounded, so a position never reads ahead
/// of what was played).
pub fn format(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// [`format`] for a known duration, [`NOT_IMPLEMENTED`] for an unknown one.
pub fn format_or_not_implemented(ms: Option<u64>) -> String {
    ms.map_or_else(|| NOT_IMPLEMENTED.to_string(), format)
}

/// Milliseconds for a time written in the specification's form, read with
/// tolerance: `H+:MM:SS`, with an optional `.F+` fraction (digits past the
/// third are dropped) or `.F0/F1` ratio; also `MM:SS` and plain seconds
/// (with or without a fraction), which the form does not allow but control
/// points send. `None` for anything else, [`NOT_IMPLEMENTED`] included, and
/// for minutes or seconds of 60 or more in a colon form.
pub fn parse(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || text.starts_with(['+', '-']) {
        return None;
    }
    let (whole, fraction) = match text.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (text, None),
    };
    let parts: Vec<&str> = whole.split(':').collect();
    let number = |p: &str| -> Option<u64> {
        if p.is_empty() || p.len() > 9 || !p.bytes().all(|b| b.is_ascii_digit()) {
            None
        } else {
            p.parse().ok()
        }
    };
    let seconds = match parts.as_slice() {
        [s] => number(s)?,
        [m, s] => {
            let (m, s) = (number(m)?, number(s)?);
            if s >= 60 {
                return None;
            }
            m * 60 + s
        }
        [h, m, s] => {
            let (h, m, s) = (number(h)?, number(m)?, number(s)?);
            if m >= 60 || s >= 60 {
                return None;
            }
            h * 3600 + m * 60 + s
        }
        _ => return None,
    };
    let millis = match fraction {
        None => 0,
        Some(f) => match f.split_once('/') {
            // F0/F1: F0 parts of F1, less than one second.
            Some((f0, f1)) => {
                let (f0, f1) = (number(f0)?, number(f1)?);
                if f1 == 0 || f0 >= f1 {
                    return None;
                }
                f0 * 1000 / f1
            }
            None => {
                if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let mut ms = 0;
                let mut scale = 100;
                for digit in f.bytes().take(3) {
                    ms += u64::from(digit - b'0') * scale;
                    scale /= 10;
                }
                ms
            }
        },
    };
    Some(seconds * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_written_as_h_mm_ss() {
        let cases = [
            (0, "0:00:00"),
            (999, "0:00:00"),
            (1000, "0:00:01"),
            (205_000, "0:03:25"),
            (3_599_999, "0:59:59"),
            (3_600_000, "1:00:00"),
            (36_000_000 + 61_000, "10:01:01"),
            (360_000_000, "100:00:00"),
        ];
        for (ms, text) in cases {
            assert_eq!(format(ms), text);
            assert_eq!(parse(text), Some(ms / 1000 * 1000));
        }
        assert_eq!(format_or_not_implemented(None), "NOT_IMPLEMENTED");
        assert_eq!(format_or_not_implemented(Some(2000)), "0:00:02");
    }

    #[test]
    fn times_are_read_with_fractions_and_short_forms() {
        let cases = [
            ("0:03:25.000", Some(205_000)),
            ("0:03:25.5", Some(205_500)),
            ("0:03:25.123456", Some(205_123)),
            ("0:00:01.1/2", Some(1500)),
            ("00:00:02", Some(2000)),
            ("03:25", Some(205_000)),
            ("90", Some(90_000)),
            ("1.25", Some(1250)),
            (" 0:00:02 ", Some(2000)),
            ("NOT_IMPLEMENTED", None),
            ("", None),
            ("0:60:00", None),
            ("0:00:60", None),
            ("-0:00:01", None),
            ("+5", None),
            ("1:2:3:4", None),
            ("0:00:01.", None),
            ("0:00:01.x", None),
            ("0:00:01.2/0", None),
            ("0:00:01.3/2", None),
            ("a:bb:cc", None),
            ("99999999999:00:00", None),
        ];
        for (text, ms) in cases {
            assert_eq!(parse(text), ms, "{text:?}");
        }
    }
}
