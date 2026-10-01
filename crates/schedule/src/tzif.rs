//! The TZif zoneinfo reader: the bytes of `/etc/localtime` or
//! `/usr/share/zoneinfo/<name>`, parsed and checked.
//!
//! The format is RFC 9636, "The Time Zone Information Format (TZif)",
//! October 2024 (<https://www.rfc-editor.org/rfc/rfc9636.html>, read
//! 2026-10-01), which obsoletes RFC 8536 (February 2019,
//! <https://www.rfc-editor.org/rfc/rfc8536.html>, read 2026-10-01) and adds
//! version 4. Versions 1 to 4 are read:
//!
//! - **1** (version byte NUL): one header and data block with 32-bit times.
//! - **2** (`2`): the version 1 block, then a second header and block with
//!   64-bit times, then a footer `\n<TZ string>\n` that governs every
//!   instant after the last transition. This reader skips the first block and
//!   reads the second, as the RFC recommends for a version 2 reader.
//! - **3** (`3`): the footer's rule times may be -167 to 167 hours
//!   ([`crate::posix`] accepts that range).
//! - **4** (`4`): the leap-second table may be truncated or carry an expiry.
//!
//! # What is refused, and why
//!
//! Every count, index and length is checked before it is used; a short or
//! inconsistent file is a typed [`TzifError`], never a panic or a guess. Two
//! refusals are policy rather than malformation:
//!
//! - **A leap-second table.** The `right/` zoneinfo files count leap seconds,
//!   so their transition times are not POSIX time; the instants chorus
//!   schedules with are POSIX time (the server's clock), and mixing the two
//!   would put every alarm about 27 seconds off. A file with leap-second
//!   records is refused by name: use the ordinary (`posix/`) file.
//! - **A UT offset outside -89 999 to 93 599 seconds** (-24:59:59 to
//!   25:59:59), the range RFC 9636 section 3.2 says a reader can rely on. It
//!   is also what lets [`crate::zone`] search a bounded window for the inverse
//!   conversion.
//!
//! The time zone designations in the file (`EST`, `-00` for "unspecified")
//! are returned as they are; `-00` is not given a meaning here.

use std::fmt;

use crate::posix::{OffsetAt, PosixTz, PosixTzError};

/// The UT offset range a reader can rely on (RFC 9636 section 3.2).
pub const UTOFF_MIN: i32 = -89_999;
/// See [`UTOFF_MIN`].
pub const UTOFF_MAX: i32 = 93_599;

/// Why a TZif file was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TzifError {
    /// The first four bytes are not `TZif`.
    NotTzif,
    /// The version byte is not NUL, `2`, `3` or `4`.
    Version(u8),
    /// The file ends before the header or a data block it declares.
    Truncated,
    /// A count, index, time or offset breaks a rule of RFC 9636 section 3.
    Invalid(&'static str),
    /// The file carries leap-second records (a `right/` file).
    LeapSeconds,
    /// The footer's TZ string does not parse.
    Footer(PosixTzError),
}

impl fmt::Display for TzifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TzifError::NotTzif => write!(f, "not a TZif file (no TZif magic)"),
            TzifError::Version(v) => write!(f, "TZif version byte 0x{v:02x} is not 1 to 4"),
            TzifError::Truncated => write!(f, "TZif file is truncated"),
            TzifError::Invalid(why) => write!(f, "TZif file is invalid: {why}"),
            TzifError::LeapSeconds => write!(
                f,
                "TZif file counts leap seconds (a right/ zone); chorus schedules in POSIX time, use the posix zone file"
            ),
            TzifError::Footer(e) => write!(f, "TZif footer: {e}"),
        }
    }
}

impl std::error::Error for TzifError {}

/// One local time type: an offset, a DST flag and an abbreviation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTimeType {
    /// Seconds east of UTC.
    pub utc_offset_s: i32,
    /// Whether it is daylight-saving time.
    pub dst: bool,
    /// The designation, e.g. `CEST`.
    pub abbreviation: String,
}

/// A parsed TZif file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tzif {
    /// 1 to 4.
    pub version: u8,
    /// Transition instants, strictly ascending, UTC seconds.
    pub transitions: Vec<i64>,
    /// The local time type index in effect from each transition on.
    pub transition_types: Vec<u8>,
    /// The local time types; at least one.
    pub types: Vec<LocalTimeType>,
    /// The footer TZ string, if present and not empty (version 2 and later).
    pub footer: Option<PosixTz>,
}

struct Header {
    version: u8,
    isutcnt: usize,
    isstdcnt: usize,
    leapcnt: usize,
    timecnt: usize,
    typecnt: usize,
    charcnt: usize,
}

const HEADER_LEN: usize = 44;

/// A data block: transition times, their type indices, the types.
type Block = (Vec<i64>, Vec<u8>, Vec<LocalTimeType>);

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn header(bytes: &[u8]) -> Result<Header, TzifError> {
    if bytes.len() < 4 || &bytes[..4] != b"TZif" {
        return Err(TzifError::NotTzif);
    }
    if bytes.len() < HEADER_LEN {
        return Err(TzifError::Truncated);
    }
    let version = match bytes[4] {
        0 => 1,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        v => return Err(TzifError::Version(v)),
    };
    let n = |i: usize| be32(&bytes[20 + 4 * i..]) as usize;
    Ok(Header {
        version,
        isutcnt: n(0),
        isstdcnt: n(1),
        leapcnt: n(2),
        timecnt: n(3),
        typecnt: n(4),
        charcnt: n(5),
    })
}

impl Header {
    /// The data block's length with `time_size`-byte times, or `None` on
    /// overflow (a hostile count).
    fn block_len(&self, time_size: usize) -> Option<usize> {
        let parts = [
            self.timecnt.checked_mul(time_size)?,
            self.timecnt,
            self.typecnt.checked_mul(6)?,
            self.charcnt,
            self.leapcnt.checked_mul(time_size + 4)?,
            self.isstdcnt,
            self.isutcnt,
        ];
        parts.iter().try_fold(0usize, |a, &p| a.checked_add(p))
    }
}

fn block(h: &Header, data: &[u8], time_size: usize) -> Result<Block, TzifError> {
    if h.typecnt == 0 {
        return Err(TzifError::Invalid("typecnt is zero"));
    }
    if h.charcnt == 0 {
        return Err(TzifError::Invalid("charcnt is zero"));
    }
    if h.isutcnt != 0 && h.isutcnt != h.typecnt {
        return Err(TzifError::Invalid("isutcnt is neither zero nor typecnt"));
    }
    if h.isstdcnt != 0 && h.isstdcnt != h.typecnt {
        return Err(TzifError::Invalid("isstdcnt is neither zero nor typecnt"));
    }
    if h.typecnt > 256 {
        return Err(TzifError::Invalid("more than 256 local time types"));
    }
    if h.leapcnt != 0 {
        return Err(TzifError::LeapSeconds);
    }
    let mut at = 0usize;
    let mut take = |len: usize| {
        let s = &data[at..at + len];
        at += len;
        s
    };
    let times = take(h.timecnt * time_size);
    let transitions: Vec<i64> = times
        .chunks_exact(time_size)
        .map(|c| {
            if time_size == 8 {
                i64::from_be_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])
            } else {
                i64::from(be32(c) as i32)
            }
        })
        .collect();
    if transitions.windows(2).any(|w| w[0] >= w[1]) {
        return Err(TzifError::Invalid(
            "transition times are not strictly ascending",
        ));
    }
    let transition_types = take(h.timecnt).to_vec();
    if transition_types
        .iter()
        .any(|&i| usize::from(i) >= h.typecnt)
    {
        return Err(TzifError::Invalid("a transition names a type past typecnt"));
    }
    let records = take(h.typecnt * 6);
    let chars = take(h.charcnt);
    let mut types = Vec::with_capacity(h.typecnt);
    for r in records.as_chunks::<6>().0 {
        let utc_offset_s = be32(r) as i32;
        if !(UTOFF_MIN..=UTOFF_MAX).contains(&utc_offset_s) {
            return Err(TzifError::Invalid(
                "a UT offset outside -89999 to 93599 seconds",
            ));
        }
        let dst = match r[4] {
            0 => false,
            1 => true,
            _ => return Err(TzifError::Invalid("a dst flag that is neither 0 nor 1")),
        };
        let idx = usize::from(r[5]);
        if idx >= chars.len() {
            return Err(TzifError::Invalid("a designation index past charcnt"));
        }
        let nul = chars[idx..]
            .iter()
            .position(|&c| c == 0)
            .ok_or(TzifError::Invalid("a designation with no NUL terminator"))?;
        let abbreviation = String::from_utf8_lossy(&chars[idx..idx + nul]).into_owned();
        types.push(LocalTimeType {
            utc_offset_s,
            dst,
            abbreviation,
        });
    }
    // The standard/wall and UT/local indicators only matter to a program that
    // applies a POSIX TZ string's default rules to the file (RFC 9636 section
    // 3.2); this reader never does, so they are length-checked and not used.
    let indicators = &data[at..at + h.isstdcnt + h.isutcnt];
    if indicators.iter().any(|&b| b > 1) {
        return Err(TzifError::Invalid("an indicator that is neither 0 nor 1"));
    }
    if h.isutcnt != 0 {
        let isut = &indicators[h.isstdcnt..];
        let isstd = &indicators[..h.isstdcnt];
        if isut
            .iter()
            .enumerate()
            .any(|(i, &u)| u == 1 && isstd.get(i) != Some(&1))
        {
            return Err(TzifError::Invalid(
                "a UT indicator set on a wall-clock type",
            ));
        }
    }
    Ok((transitions, transition_types, types))
}

impl Tzif {
    /// Parse and check a TZif file.
    pub fn parse(bytes: &[u8]) -> Result<Tzif, TzifError> {
        let h1 = header(bytes)?;
        let len1 = h1.block_len(4).ok_or(TzifError::Truncated)?;
        let end1 = HEADER_LEN.checked_add(len1).ok_or(TzifError::Truncated)?;
        if bytes.len() < end1 {
            return Err(TzifError::Truncated);
        }
        if h1.version == 1 {
            let (transitions, transition_types, types) = block(&h1, &bytes[HEADER_LEN..end1], 4)?;
            if end1 != bytes.len() {
                return Err(TzifError::Invalid("bytes after a version 1 data block"));
            }
            return Ok(Tzif {
                version: 1,
                transitions,
                transition_types,
                types,
                footer: None,
            });
        }
        // Version 2 and later: the 64-bit block is the one to read. The first
        // block's leap-second count still refuses a right/ file early.
        if h1.leapcnt != 0 {
            return Err(TzifError::LeapSeconds);
        }
        let rest = &bytes[end1..];
        let h2 = header(rest)?;
        if h2.version != h1.version {
            return Err(TzifError::Invalid(
                "the two headers disagree on the version",
            ));
        }
        let len2 = h2.block_len(8).ok_or(TzifError::Truncated)?;
        let end2 = HEADER_LEN.checked_add(len2).ok_or(TzifError::Truncated)?;
        if rest.len() < end2 {
            return Err(TzifError::Truncated);
        }
        let (transitions, transition_types, types) = block(&h2, &rest[HEADER_LEN..end2], 8)?;
        let footer_bytes = &rest[end2..];
        if footer_bytes.first() != Some(&b'\n') {
            return Err(TzifError::Truncated);
        }
        let close = footer_bytes[1..]
            .iter()
            .position(|&c| c == b'\n')
            .ok_or(TzifError::Truncated)?;
        if 1 + close + 1 != footer_bytes.len() {
            return Err(TzifError::Invalid("bytes after the footer"));
        }
        let tz = std::str::from_utf8(&footer_bytes[1..1 + close])
            .map_err(|_| TzifError::Invalid("the footer is not ASCII"))?;
        let footer = if tz.is_empty() {
            None
        } else {
            Some(PosixTz::parse(tz).map_err(TzifError::Footer)?)
        };
        Ok(Tzif {
            version: h1.version,
            transitions,
            transition_types,
            types,
            footer,
        })
    }

    fn type_offset(&self, i: usize) -> OffsetAt<'_> {
        let t = &self.types[i];
        OffsetAt {
            utc_offset_s: t.utc_offset_s,
            dst: t.dst,
            abbreviation: &t.abbreviation,
        }
    }

    /// The offset in effect at UTC instant `t`.
    ///
    /// Before the first transition, local time type 0 (RFC 9636 section 3.2).
    /// At or after the last transition, the footer's TZ string if there is
    /// one, else the last transition's type. A file with no transition is the
    /// footer throughout, or type 0 if it has none.
    pub fn offset_at(&self, t: i64) -> OffsetAt<'_> {
        let n = self.transitions.partition_point(|&x| x <= t);
        if n == 0 && !self.transitions.is_empty() {
            return self.type_offset(0);
        }
        if n == self.transitions.len() {
            if let Some(footer) = &self.footer {
                return footer.offset_at(t);
            }
        }
        match n {
            0 => self.type_offset(0),
            _ => self.type_offset(usize::from(self.transition_types[n - 1])),
        }
    }

    /// The first transition instant after `t`, from the table and then from
    /// the footer. It may change only the abbreviation or the DST flag, not
    /// the offset; callers that care compare offsets.
    pub fn next_transition_after(&self, t: i64) -> Option<i64> {
        let n = self.transitions.partition_point(|&x| x <= t);
        if n < self.transitions.len() {
            return Some(self.transitions[n]);
        }
        // Past the table: the footer governs, and only from the last
        // transition on (a file with no transition at all is the footer
        // throughout, RFC 9636 section 3.3).
        self.footer.as_ref()?.next_transition_after(t)
    }
}
