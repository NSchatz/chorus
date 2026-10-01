//! A LAN's loss, burst and jitter for the TV path's UDP legs (goal 13, bench
//! session S8): the tally `chorus-udp-loss` keeps.
//!
//! The low-latency wire (ADR 0091) protects a stream with one XOR parity per
//! `k` chunks, with an optional column interleave of depth `D`: it repairs any
//! one loss in a group, and a burst of up to `D` consecutive losses. Whether
//! depth 1 is enough is a question about the owner's LAN, which ADR 0091 left
//! ASSUMED (independent loss, each leg 0.25 ms, a 2 ms jitter margin). This
//! tally answers it from a probe stream sent at the wire's own cadence (one
//! datagram per 2.5 ms chunk, 1472 bytes): how many datagrams were lost, how
//! the losses group into bursts (the histogram the depth is chosen from), how
//! many arrived out of order or twice, and how much the transit time varies
//! between consecutive datagrams.
//!
//! The variation is RFC 3550's: for datagrams i and j sent at S and received
//! at R, "D(i,j) = (Rj - Ri) - (Sj - Si) = (Rj - Sj) - (Ri - Si)", and the
//! running estimate "J(i) = J(i-1) + (|D(i-1,i)| - J(i-1))/16" (RFC 3550
//! section 6.4.1, https://www.rfc-editor.org/rfc/rfc3550.txt, read
//! 2026-10-01). It needs no clock agreement between the two hosts: each side
//! reads only its own monotonic clock, and a constant offset cancels. Every
//! `|D|` is also counted in power-of-two microsecond buckets.
//!
//! Pure arithmetic: no clock, no socket. The tool hands it each datagram's
//! sequence number, the sender's stamp and the receive stamp.

use std::collections::BTreeSet;

/// The probe datagram's first bytes: magic, then the sequence number and the
/// sender's monotonic stamp, both u64 big-endian. The rest is padding.
pub const MAGIC: [u8; 4] = *b"CLUP";
/// The header the tally reads: magic, sequence, send stamp.
pub const HEADER_LEN: usize = 4 + 8 + 8;
/// The wire's largest datagram (ADR 0091: a 1500-byte Ethernet MTU less the
/// IPv4 and UDP headers), the probe's default size.
pub const DATAGRAM_LEN: usize = 1472;
/// The wire's chunk cadence at its defaults (120 frames at 48 kHz, ADR 0091),
/// the probe's default interval.
pub const INTERVAL_US: u64 = 2_500;
/// Buckets of the `|D|` table: bucket 0 is [0, 1 us), bucket k [2^(k-1), 2^k) us.
const BUCKETS: usize = 32;

/// Write a probe datagram's header into `buf` (at least [`HEADER_LEN`] bytes).
pub fn encode(buf: &mut [u8], sequence: u64, sent_ns: u64) {
    buf[..4].copy_from_slice(&MAGIC);
    buf[4..12].copy_from_slice(&sequence.to_be_bytes());
    buf[12..20].copy_from_slice(&sent_ns.to_be_bytes());
}

/// A probe datagram's sequence number and send stamp; `None` for anything
/// else (a stray datagram on the port is not a probe's).
pub fn decode(buf: &[u8]) -> Option<(u64, u64)> {
    if buf.len() < HEADER_LEN || buf[..4] != MAGIC {
        return None;
    }
    let sequence = u64::from_be_bytes(buf[4..12].try_into().ok()?);
    let sent = u64::from_be_bytes(buf[12..20].try_into().ok()?);
    Some((sequence, sent))
}

/// The running tally of one probe stream.
#[derive(Debug, Clone, Default)]
pub struct Tally {
    /// The next sequence number expected in order.
    next: Option<u64>,
    /// The sequence numbers below `next` not (yet) received.
    missing: BTreeSet<u64>,
    received: u64,
    duplicates: u64,
    reordered: u64,
    foreign: u64,
    /// The previous in-order datagram's (receive, send) stamps.
    previous: Option<(u64, u64)>,
    /// RFC 3550's J, in ns, as f64 (the recurrence divides by 16).
    jitter_ns: f64,
    max_d_ns: u64,
    d_buckets: [u64; BUCKETS],
}

/// What a tally says at the end.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Datagrams received once (duplicates not counted).
    pub received: u64,
    /// Sequence numbers from the first received to the highest never seen.
    pub lost: u64,
    /// Datagrams that arrived after a higher sequence number did.
    pub reordered: u64,
    /// Datagrams received a second time.
    pub duplicates: u64,
    /// Datagrams on the port that were not the probe's.
    pub foreign: u64,
    /// Consecutive-loss runs: (length, how many runs of that length).
    pub bursts: Vec<(u64, u64)>,
    /// RFC 3550's interarrival jitter at the end, ns.
    pub jitter_ns: u64,
    /// The largest `|D|` between consecutive in-order datagrams, ns.
    pub max_d_ns: u64,
    /// `|D|` in power-of-two buckets: (upper bound in us, count), up to the last
    /// non-empty one.
    pub d_buckets_us: Vec<(u64, u64)>,
}

impl Report {
    /// The loss ratio over the datagrams the stream spanned.
    pub fn loss_ratio(&self) -> f64 {
        let span = self.received + self.lost;
        if span == 0 {
            0.0
        } else {
            self.lost as f64 / span as f64
        }
    }
}

impl Tally {
    /// An empty tally.
    pub fn new() -> Tally {
        Tally::default()
    }

    /// A datagram that was not a probe's.
    pub fn foreign(&mut self) {
        self.foreign += 1;
    }

    /// One probe datagram: its sequence number, the sender's stamp and the
    /// receive stamp (each host's own monotonic ns).
    pub fn push(&mut self, sequence: u64, sent_ns: u64, received_ns: u64) {
        let next = *self.next.get_or_insert(sequence);
        if sequence >= next {
            // In order, or after a gap: everything skipped is missing until
            // it turns up.
            self.missing.extend(next..sequence);
            self.next = Some(sequence + 1);
            self.received += 1;
            if sequence == next {
                if let Some((r0, s0)) = self.previous {
                    let d = (received_ns as i128 - r0 as i128) - (sent_ns as i128 - s0 as i128);
                    let d = d.unsigned_abs().min(u128::from(u64::MAX)) as u64;
                    self.jitter_ns += (d as f64 - self.jitter_ns) / 16.0;
                    self.max_d_ns = self.max_d_ns.max(d);
                    let us = d / 1_000;
                    let k = if us == 0 {
                        0
                    } else {
                        (64 - us.leading_zeros()) as usize
                    };
                    self.d_buckets[k.min(BUCKETS - 1)] += 1;
                }
            }
            self.previous = Some((received_ns, sent_ns));
        } else if self.missing.remove(&sequence) {
            self.reordered += 1;
            self.received += 1;
        } else {
            self.duplicates += 1;
        }
    }

    /// The tally as it stands.
    pub fn report(&self) -> Report {
        let mut bursts: Vec<(u64, u64)> = Vec::new();
        let mut run: Option<(u64, u64)> = None; // (first, length)
        let close = |len: u64, bursts: &mut Vec<(u64, u64)>| match bursts
            .iter_mut()
            .find(|(l, _)| *l == len)
        {
            Some((_, n)) => *n += 1,
            None => bursts.push((len, 1)),
        };
        for &s in &self.missing {
            run = match run {
                Some((first, len)) if first + len == s => Some((first, len + 1)),
                Some((_, len)) => {
                    close(len, &mut bursts);
                    Some((s, 1))
                }
                None => Some((s, 1)),
            };
        }
        if let Some((_, len)) = run {
            close(len, &mut bursts);
        }
        bursts.sort_unstable();
        let last = self.d_buckets.iter().rposition(|&c| c > 0);
        let d_buckets_us = match last {
            Some(last) => (0..=last).map(|k| (1u64 << k, self.d_buckets[k])).collect(),
            None => Vec::new(),
        };
        Report {
            received: self.received,
            lost: self.missing.len() as u64,
            reordered: self.reordered,
            duplicates: self.duplicates,
            foreign: self.foreign,
            bursts,
            jitter_ns: self.jitter_ns.round() as u64,
            max_d_ns: self.max_d_ns,
            d_buckets_us,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: u64 = 2_500_000;

    #[test]
    fn a_clean_stream_loses_nothing_and_has_no_jitter() {
        let mut t = Tally::new();
        for s in 0..1000u64 {
            // A constant offset between the hosts' clocks cancels.
            t.push(s, s * STEP, 7_000_000_000 + s * STEP);
        }
        let r = t.report();
        assert_eq!(
            (r.received, r.lost, r.reordered, r.duplicates),
            (1000, 0, 0, 0)
        );
        assert!(r.bursts.is_empty());
        assert_eq!((r.jitter_ns, r.max_d_ns), (0, 0));
        assert_eq!(r.d_buckets_us, vec![(1, 999)]);
        assert_eq!(r.loss_ratio(), 0.0);
    }

    #[test]
    fn losses_group_into_bursts_by_length() {
        let mut t = Tally::new();
        // Lost: 10; 20, 21; 30, 31, 32; 40.
        let lost = [10u64, 20, 21, 30, 31, 32, 40];
        for s in (0..100u64).filter(|s| !lost.contains(s)) {
            t.push(s, s * STEP, s * STEP);
        }
        let r = t.report();
        assert_eq!((r.received, r.lost), (93, 7));
        assert_eq!(r.bursts, vec![(1, 2), (2, 1), (3, 1)]);
        assert!((r.loss_ratio() - 0.07).abs() < 1e-12);
    }

    #[test]
    fn a_late_datagram_is_reordered_not_lost_and_a_repeat_is_a_duplicate() {
        let mut t = Tally::new();
        for s in [0u64, 1, 3, 2, 4, 4, 1] {
            t.push(s, s * STEP, s * STEP);
        }
        let r = t.report();
        assert_eq!(
            (r.received, r.lost, r.reordered, r.duplicates),
            (5, 0, 1, 2)
        );
        assert!(r.bursts.is_empty());
    }

    #[test]
    fn transit_variation_follows_rfc_3550() {
        let mut t = Tally::new();
        // Every other datagram 1 ms late: |D| alternates 1 ms each step.
        for s in 0..200u64 {
            let late = if s % 2 == 1 { 1_000_000 } else { 0 };
            t.push(s, s * STEP, s * STEP + late);
        }
        let r = t.report();
        assert_eq!(r.max_d_ns, 1_000_000);
        // J converges on 1 ms: J_n = 1 ms (1 - (15/16)^n), n = 199.
        let want = 1_000_000.0 * (1.0 - (15.0f64 / 16.0).powi(199));
        assert!((r.jitter_ns as f64 - want).abs() <= 1.0, "{}", r.jitter_ns);
        // 1000 us falls in [512, 1024) us, the bucket whose bound is 1024.
        assert_eq!(r.d_buckets_us.last(), Some(&(1024, 199)));
    }

    #[test]
    fn the_header_round_trips_and_a_stranger_is_refused() {
        let mut buf = [0u8; DATAGRAM_LEN];
        encode(&mut buf, 42, 123_456_789);
        assert_eq!(decode(&buf), Some((42, 123_456_789)));
        assert_eq!(decode(&buf[..HEADER_LEN - 1]), None);
        buf[0] = b'X';
        assert_eq!(decode(&buf), None);
    }
}
