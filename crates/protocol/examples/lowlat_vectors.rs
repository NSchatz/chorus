//! Writes the shared low-latency vectors, `fixtures/protocol/lowlat/`.
//!
//!   cargo run -p chorus-protocol --example lowlat_vectors -- fixtures/protocol/lowlat
//!
//! Never run by a test: the vectors are committed, and the Rust test
//! (`crates/protocol/tests/lowlat_vectors.rs`) and the C test
//! (`firmware/tests/test_lowlat.c`) only read them. Each case's expected
//! counts are written here by hand, as what the case is meant to show, and
//! this program refuses to write a case the Rust implementation does not
//! reproduce, so a committed expectation is never just "what the code did".

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use chorus_protocol::v2::lowlat::{
    Datagram, FecDecoder, FecEncoder, FecParams, Kind, Opener, Sealer,
};

/// A public test key: never a real stream's.
const KEY: [u8; 32] = [
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
    0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f,
];
const TAG: u32 = 0x0BAD_CAFE;
const FIRST_COUNTER: u64 = 7;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

/// An `audio_chunk` payload: sequence, stamp, 48 kHz, stereo, s16, the
/// reserved block zero (the encoder assigns it), then `frames` frames.
fn chunk(i: u32, frames: usize) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&(100 + i).to_be_bytes());
    p.extend_from_slice(&(1_000_000_000u64 + u64::from(i) * 83_333).to_be_bytes());
    p.extend_from_slice(&48_000u32.to_be_bytes());
    p.push(2);
    p.push(1);
    p.extend_from_slice(&[0u8; 14]);
    for s in 0..frames * 4 {
        p.push(
            (i as u8)
                .wrapping_mul(37)
                .wrapping_add((s as u8).wrapping_mul(11)),
        );
    }
    p
}

struct Stream {
    stem: &'static str,
    why: &'static str,
    k: u8,
    depth: u8,
    frames: Vec<usize>,
}

struct Made {
    chunks: Vec<Vec<u8>>,
    plain: Vec<Datagram>,
    sealed: Vec<Vec<u8>>,
}

fn make(s: &Stream) -> Made {
    let params = FecParams::new(s.k, s.depth).unwrap();
    let mut enc = FecEncoder::new(params);
    let mut sealer = Sealer::starting_at(KEY, TAG, FIRST_COUNTER).unwrap();
    let mut chunks = Vec::new();
    let mut plain = Vec::new();
    let mut sealed = Vec::new();
    for (i, &frames) in s.frames.iter().enumerate() {
        let c = chunk(i as u32, frames);
        let mut p = c.clone();
        for d in enc.push_payload(&mut p).unwrap() {
            sealed.push(sealer.seal(&d).unwrap());
            plain.push(d);
        }
        chunks.push(c);
    }
    Made {
        chunks,
        plain,
        sealed,
    }
}

fn write_stream(dir: &Path, s: &Stream, m: &Made) {
    let mut t = String::new();
    writeln!(t, "# chorus low-latency vector: the stream {}.", s.stem).unwrap();
    writeln!(t, "#").unwrap();
    writeln!(
        t,
        "# Committed, and never written by a test (crates/protocol/examples/lowlat_vectors.rs\n\
         # wrote it). Pushing chunk.0 onward through the FEC encoder (fec_k, fec_depth) and\n\
         # sealing what it emits with `key` and `stream_tag`, counters from first_counter,\n\
         # must give plaintext.N and datagram.N byte for byte, in that order\n\
         # (docs/protocol.md, \"Low-latency path\"). The cases (case_*.fields) drop, reorder,\n\
         # repeat and alter these datagrams."
    )
    .unwrap();
    writeln!(t, "#").unwrap();
    writeln!(
        t,
        "# Format: one \"key = value\" per line, '#' starts a comment; bytes are hex."
    )
    .unwrap();
    writeln!(t, "#").unwrap();
    for line in s.why.lines() {
        writeln!(t, "# {}", line).unwrap();
    }
    writeln!(t).unwrap();
    writeln!(t, "key = {}", hex(&KEY)).unwrap();
    writeln!(t, "stream_tag = {}", TAG).unwrap();
    writeln!(t, "first_counter = {}", FIRST_COUNTER).unwrap();
    writeln!(t, "fec_k = {}", s.k).unwrap();
    writeln!(t, "fec_depth = {}", s.depth).unwrap();
    writeln!(t, "chunks = {}", m.chunks.len()).unwrap();
    for (i, c) in m.chunks.iter().enumerate() {
        writeln!(
            t,
            "# chunk {}: sequence {}, {} frames",
            i,
            100 + i,
            s.frames[i]
        )
        .unwrap();
        writeln!(t, "chunk.{} = {}", i, hex(c)).unwrap();
    }
    writeln!(t, "datagrams = {}", m.sealed.len()).unwrap();
    let mut data = 0;
    for (j, (p, d)) in m.plain.iter().zip(&m.sealed).enumerate() {
        let what = match p.kind {
            Kind::Data => {
                data += 1;
                format!("data, chunk {}", data - 1)
            }
            Kind::Parity => {
                let g = u32::from_be_bytes([
                    p.plaintext[0],
                    p.plaintext[1],
                    p.plaintext[2],
                    p.plaintext[3],
                ]);
                format!("parity of group {}", g)
            }
        };
        writeln!(
            t,
            "# datagram {}: {}, counter {}",
            j,
            what,
            FIRST_COUNTER + j as u64
        )
        .unwrap();
        writeln!(t, "plaintext.{} = {}", j, hex(&p.plaintext)).unwrap();
        writeln!(t, "datagram.{} = {}", j, hex(d)).unwrap();
    }
    fs::write(dir.join(format!("{}.fields", s.stem)), t).unwrap();
}

#[derive(Default)]
struct Expect {
    delivered: Vec<u64>,
    recovered: Vec<u64>,
    opened: u64,
    malformed: u64,
    wrong_stream_tag: u64,
    replayed: u64,
    auth_failed: u64,
    unrecoverable: u64,
    duplicate: u64,
}

struct Case {
    stem: &'static str,
    stream: usize,
    why: &'static str,
    deliver: Vec<usize>,
    tamper: Option<(usize, usize, u8)>,
    receiver_stream_tag: Option<u32>,
    expect: Expect,
}

/// Run a case through the Rust implementation; panics where it differs from
/// the hand-written expectation.
fn check(case: &Case, m: &Made, chunks: u64) {
    let tag = case.receiver_stream_tag.unwrap_or(TAG);
    let mut opener = Opener::new(KEY, tag);
    let k = m
        .plain
        .iter()
        .find(|d| d.kind == Kind::Data)
        .unwrap()
        .plaintext[19];
    let depth = m.plain[0].plaintext[20];
    let mut dec = FecDecoder::new(FecParams::new(k, depth).unwrap());
    let mut delivered = Vec::new();
    let mut recovered = Vec::new();
    let sent: Vec<&Vec<u8>> = m
        .plain
        .iter()
        .filter(|d| d.kind == Kind::Data)
        .map(|d| &d.plaintext)
        .collect();
    for &j in &case.deliver {
        let mut bytes = m.sealed[j].clone();
        if let Some((at, off, x)) = case.tamper {
            if at == j {
                bytes[off] ^= x;
            }
        }
        if let Ok(o) = opener.open(&bytes) {
            for d in dec.push(o.kind, &o.plaintext) {
                assert_eq!(&d.payload, sent[d.chunk_index as usize], "{}", case.stem);
                delivered.push(d.chunk_index);
                if d.recovered {
                    recovered.push(d.chunk_index);
                }
            }
        }
    }
    dec.finish(chunks);
    delivered.sort_unstable();
    let o = opener.stats();
    let f = dec.stats();
    let e = &case.expect;
    assert_eq!(delivered, e.delivered, "{} delivered", case.stem);
    assert_eq!(recovered, e.recovered, "{} recovered", case.stem);
    assert_eq!(
        (
            o.opened,
            o.malformed,
            o.wrong_stream_tag,
            o.replayed,
            o.auth_failed
        ),
        (
            e.opened,
            e.malformed,
            e.wrong_stream_tag,
            e.replayed,
            e.auth_failed
        ),
        "{} open counts",
        case.stem
    );
    assert_eq!(
        (f.unrecoverable, f.duplicate, f.late, f.rejected),
        (e.unrecoverable, e.duplicate, 0, 0),
        "{} fec counts",
        case.stem
    );
}

fn list(v: &[impl ToString]) -> String {
    v.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn write_case(dir: &Path, case: &Case, stream: &Stream) {
    let mut t = String::new();
    writeln!(t, "# chorus low-latency vector: the case {}.", case.stem).unwrap();
    writeln!(t, "#").unwrap();
    writeln!(
        t,
        "# Committed, and never written by a test. Open the datagrams of {}.fields\n\
         # listed in `deliver`, in that order (an index twice is a repeat), each with the\n\
         # stream's key and the receiver's stream_tag (the stream's unless named here), XOR\n\
         # `tamper` (datagram, byte offset, mask) into one first, push what opens through the\n\
         # FEC decoder, then finish the stream after its `chunks`. The chunks handed on, the\n\
         # ones rebuilt and every count below must be exactly these.",
        stream.stem
    )
    .unwrap();
    writeln!(t, "#").unwrap();
    for line in case.why.lines() {
        writeln!(t, "# {}", line).unwrap();
    }
    writeln!(t).unwrap();
    writeln!(t, "stream = {}", stream.stem).unwrap();
    if let Some(tag) = case.receiver_stream_tag {
        writeln!(t, "receiver_stream_tag = {}", tag).unwrap();
    }
    writeln!(t, "deliver = {}", list(&case.deliver)).unwrap();
    if let Some((j, off, x)) = case.tamper {
        writeln!(t, "tamper = {} {} {}", j, off, x).unwrap();
    }
    let e = &case.expect;
    writeln!(t, "expect_delivered = {}", list(&e.delivered)).unwrap();
    writeln!(t, "expect_recovered = {}", list(&e.recovered)).unwrap();
    writeln!(t, "expect_opened = {}", e.opened).unwrap();
    writeln!(t, "expect_malformed = {}", e.malformed).unwrap();
    writeln!(t, "expect_wrong_stream_tag = {}", e.wrong_stream_tag).unwrap();
    writeln!(t, "expect_replayed = {}", e.replayed).unwrap();
    writeln!(t, "expect_auth_failed = {}", e.auth_failed).unwrap();
    writeln!(t, "expect_unrecoverable = {}", e.unrecoverable).unwrap();
    writeln!(t, "expect_duplicate = {}", e.duplicate).unwrap();
    fs::write(dir.join(format!("{}.fields", case.stem)), t).unwrap();
}

fn all_but(n: usize, drop: &[usize]) -> Vec<usize> {
    (0..n).filter(|i| !drop.contains(i)).collect()
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: lowlat_vectors <fixtures/protocol/lowlat>");
    let dir = Path::new(&dir);
    fs::create_dir_all(dir).unwrap();
    let streams = [
        Stream {
            stem: "stream_k4_d1",
            why:
                "The defaults' shape: one parity per 4 chunks, no interleave, two groups. Chunk 5\n\
                  has 2 frames and chunk 7 has 3 (the others 4), so a group's chunks differ in\n\
                  length and the parity's length_xor is what rebuilds a short one.",
            k: 4,
            depth: 1,
            frames: vec![4, 4, 4, 4, 4, 2, 4, 3],
        },
        Stream {
            stem: "stream_k3_d2",
            why: "Column interleave: k 3, depth 2. Each block of 6 chunks holds two groups, the\n\
                  even chunks and the odd ones, so two consecutive losses cost each group one.",
            k: 3,
            depth: 2,
            frames: vec![4; 12],
        },
        Stream {
            stem: "stream_nofec",
            why: "No FEC (fec_k 0): data only, each chunk its own group, nothing to rebuild.",
            k: 0,
            depth: 1,
            frames: vec![4; 4],
        },
    ];
    let made: Vec<Made> = streams.iter().map(make).collect();
    let n = |s: usize| made[s].sealed.len();
    let all = |s: usize| (0..made[s].chunks.len() as u64).collect::<Vec<u64>>();
    // stream_k4_d1's datagrams: c0 c1 c2 c3 P0 c4 c5 c6 c7 P1.
    // stream_k3_d2's: c0 c1 c2 c3 c4 P0 c5 P1 c6 c7 c8 c9 c10 P2 c11 P3.
    let full = |s: usize, recovered: Vec<u64>, dropped: usize| Expect {
        delivered: all(s),
        recovered,
        opened: (n(s) - dropped) as u64,
        ..Expect::default()
    };
    let mut cases = Vec::new();
    for (pos, j) in [0usize, 1, 2, 3].iter().enumerate() {
        cases.push(Case {
            stem: [
                "case_k4_lose_data0",
                "case_k4_lose_data1",
                "case_k4_lose_data2",
                "case_k4_lose_data3",
            ][pos],
            stream: 0,
            why: "One data datagram of group 0 lost: rebuilt from the other three and the parity.",
            deliver: all_but(n(0), &[*j]),
            tamper: None,
            receiver_stream_tag: None,
            expect: full(0, vec![pos as u64], 1),
        });
    }
    cases.push(Case {
        stem: "case_k4_lose_parity",
        stream: 0,
        why: "Group 0's parity lost: every chunk still arrives, nothing to rebuild.",
        deliver: all_but(n(0), &[4]),
        tamper: None,
        receiver_stream_tag: None,
        expect: full(0, vec![], 1),
    });
    cases.push(Case {
        stem: "case_k4_lose_short_chunk",
        stream: 0,
        why: "Chunk 7, the 3-frame one, lost: rebuilt at its own length (length recovery).",
        deliver: all_but(n(0), &[8]),
        tamper: None,
        receiver_stream_tag: None,
        expect: full(0, vec![7], 1),
    });
    cases.push(Case {
        stem: "case_k4_double_loss",
        stream: 0,
        why: "Chunks 0 and 1 lost: one parity cannot rebuild two. Both are counted\n\
              unrecoverable and nothing is guessed.",
        deliver: all_but(n(0), &[0, 1]),
        tamper: None,
        receiver_stream_tag: None,
        expect: Expect {
            delivered: vec![2, 3, 4, 5, 6, 7],
            opened: 8,
            unrecoverable: 2,
            ..Expect::default()
        },
    });
    cases.push(Case {
        stem: "case_k4_reordered",
        stream: 0,
        why: "Every datagram, each group's parity first and the groups interleaved: a group is\n\
              rebuilt as soon as its parity and three of its chunks are in (chunks 1 and 5),\n\
              and the real chunk, arriving after, is a duplicate. None is counted lost.",
        deliver: vec![4, 3, 0, 9, 2, 1, 5, 8, 7, 6],
        tamper: None,
        receiver_stream_tag: None,
        expect: Expect {
            duplicate: 2,
            ..full(0, vec![1, 5], 0)
        },
    });
    cases.push(Case {
        stem: "case_k3_d2_burst",
        stream: 1,
        why: "A burst of two: chunks 2 and 3 back to back. Chunk 2 is in the even group and\n\
              chunk 3 in the odd one, so each group lost one and both are rebuilt.",
        deliver: all_but(n(1), &[2, 3]),
        tamper: None,
        receiver_stream_tag: None,
        expect: full(1, vec![2, 3], 2),
    });
    cases.push(Case {
        stem: "case_k3_d2_double_in_group",
        stream: 1,
        why: "Chunks 0 and 2 lost: both in the even group of block 0, which one parity\n\
              cannot rebuild.",
        deliver: all_but(n(1), &[0, 2]),
        tamper: None,
        receiver_stream_tag: None,
        expect: Expect {
            delivered: vec![1, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            opened: 14,
            unrecoverable: 2,
            ..Expect::default()
        },
    });
    cases.push(Case {
        stem: "case_tampered_tag",
        stream: 0,
        why: "The last byte of datagram 2 (chunk 2's tag) altered: it fails authentication\n\
              and is dropped, and the FEC rebuilds chunk 2 from the rest.",
        deliver: all_but(n(0), &[]),
        tamper: Some((2, made[0].sealed[2].len() - 1, 0x01)),
        receiver_stream_tag: None,
        expect: Expect {
            delivered: all(0),
            recovered: vec![2],
            opened: 9,
            auth_failed: 1,
            ..Expect::default()
        },
    });
    cases.push(Case {
        stem: "case_tampered_header",
        stream: 0,
        why: "Datagram 5's counter (header byte 15) altered: the header is the AEAD's\n\
              associated data and the nonce, so the tag fails; chunk 4 is rebuilt.",
        deliver: all_but(n(0), &[]),
        tamper: Some((5, 15, 0x01)),
        receiver_stream_tag: None,
        expect: Expect {
            delivered: all(0),
            recovered: vec![4],
            opened: 9,
            auth_failed: 1,
            ..Expect::default()
        },
    });
    cases.push(Case {
        stem: "case_bad_magic",
        stream: 0,
        why: "Datagram 1's magic altered: dropped as malformed before any cryptography.",
        deliver: all_but(n(0), &[]),
        tamper: Some((1, 0, 0x20)),
        receiver_stream_tag: None,
        expect: Expect {
            delivered: all(0),
            recovered: vec![1],
            opened: 9,
            malformed: 1,
            ..Expect::default()
        },
    });
    cases.push(Case {
        stem: "case_replay",
        stream: 0,
        why: "Datagrams 1 and 6 delivered twice, the repeat of 1 after group 1 began: the\n\
              replay window drops both repeats before the FEC sees them.",
        deliver: vec![0, 1, 2, 3, 4, 5, 1, 6, 6, 7, 8, 9],
        tamper: None,
        receiver_stream_tag: None,
        expect: Expect {
            delivered: all(0),
            opened: 10,
            replayed: 2,
            ..Expect::default()
        },
    });
    cases.push(Case {
        stem: "case_wrong_stream_tag",
        stream: 0,
        why: "A receiver offered another stream: every datagram is dropped by its tag.",
        deliver: all_but(n(0), &[]),
        tamper: None,
        receiver_stream_tag: Some(TAG + 1),
        expect: Expect {
            wrong_stream_tag: 10,
            ..Expect::default()
        },
    });
    cases.push(Case {
        stem: "case_nofec_loss",
        stream: 2,
        why: "No FEC: chunk 1 lost is lost, and counted.",
        deliver: all_but(n(2), &[1]),
        tamper: None,
        receiver_stream_tag: None,
        expect: Expect {
            delivered: vec![0, 2, 3],
            opened: 3,
            unrecoverable: 1,
            ..Expect::default()
        },
    });
    for (s, m) in streams.iter().zip(&made) {
        write_stream(dir, s, m);
    }
    for c in &cases {
        check(c, &made[c.stream], made[c.stream].chunks.len() as u64);
        write_case(dir, c, &streams[c.stream]);
    }
    eprintln!(
        "wrote {} streams and {} cases to {}",
        streams.len(),
        cases.len(),
        dir.display()
    );
}
