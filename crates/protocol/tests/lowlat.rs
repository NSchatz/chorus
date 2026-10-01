//! The low-latency path (goal 13): the shared vectors under
//! `fixtures/protocol/lowlat/`, read here and by the C endpoint's
//! `firmware/tests/test_lowlat.c`, and the wire rules of `low_latency_offer`,
//! `low_latency_accept` and `capabilities.features` that the v2 vectors do
//! not already hold.
//!
//! The vectors are READ and never written; `examples/lowlat_vectors.rs`
//! made them, with each case's expectation written by hand.

mod common;

use std::fs;
use std::path::PathBuf;

use chorus_protocol::v2::lowlat::{
    FecDecoder, FecEncoder, FecParams, Kind, Opener, Sealer, MAX_DATAGRAM_LEN,
};
use chorus_protocol::v2::{
    decode_frame, encode, features, Capabilities, Codec, DecodeError, Message, Outcome, Problem,
};

use common::{fixture_dir, parse_hex, Fields};

fn lowlat_dir() -> PathBuf {
    fixture_dir().join("lowlat")
}

fn load(stem: &str) -> Fields {
    let path = lowlat_dir().join(format!("{}.fields", stem));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is unreadable: {}", path.display(), e));
    Fields::parse(&format!("lowlat/{}.fields", stem), &text)
}

fn stems(prefix: &str) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(lowlat_dir())
        .expect("fixtures/protocol/lowlat exists")
        .map(|e| e.expect("a readable entry").path())
        .map(|p| {
            assert_eq!(
                p.extension().and_then(|x| x.to_str()),
                Some("fields"),
                "{}: only .fields files live in fixtures/protocol/lowlat",
                p.display()
            );
            p.file_stem().unwrap().to_string_lossy().into_owned()
        })
        .filter(|s| s.starts_with(prefix))
        .collect();
    out.sort();
    out
}

fn numbers(f: &Fields, key: &str) -> Vec<u64> {
    f.str(key)
        .split_whitespace()
        .map(|w| w.parse().unwrap_or_else(|_| panic!("{}: {}", key, w)))
        .collect()
}

struct Stream {
    key: [u8; 32],
    tag: u32,
    params: FecParams,
    chunks: u64,
    datagrams: Vec<Vec<u8>>,
    data_plaintexts: Vec<Vec<u8>>,
}

fn stream(stem: &str) -> Stream {
    let f = load(stem);
    let mut key = [0u8; 32];
    key.copy_from_slice(&f.bytes("key"));
    let n = f.u64("datagrams") as usize;
    let datagrams: Vec<Vec<u8>> = (0..n)
        .map(|j| f.bytes(&format!("datagram.{}", j)))
        .collect();
    // A datagram's kind is its header's byte 3 (1 data, 2 parity).
    let data_plaintexts = (0..n)
        .filter(|&j| datagrams[j][3] == Kind::Data.to_wire())
        .map(|j| f.bytes(&format!("plaintext.{}", j)))
        .collect();
    Stream {
        key,
        tag: f.u64("stream_tag") as u32,
        params: FecParams::new(f.u64("fec_k") as u8, f.u64("fec_depth") as u8).unwrap(),
        chunks: f.u64("chunks"),
        datagrams,
        data_plaintexts,
    }
}

#[test]
fn the_encoder_and_the_sealer_reproduce_every_stream_byte_for_byte() {
    let all = stems("stream_");
    assert!(all.len() >= 3, "the streams are committed: {:?}", all);
    let mut datagrams = 0;
    for stem in &all {
        let f = load(stem);
        let s = stream(stem);
        let mut enc = FecEncoder::new(s.params);
        let mut key = [0u8; 32];
        key.copy_from_slice(&f.bytes("key"));
        let mut sealer = Sealer::starting_at(key, s.tag, f.u64("first_counter")).unwrap();
        let mut j = 0;
        for i in 0..s.chunks {
            let mut payload = f.bytes(&format!("chunk.{}", i));
            for d in enc.push_payload(&mut payload).unwrap() {
                assert_eq!(
                    d.plaintext,
                    f.bytes(&format!("plaintext.{}", j)),
                    "{} plaintext.{}",
                    stem,
                    j
                );
                let sealed = sealer.seal(&d).unwrap();
                assert!(sealed.len() <= MAX_DATAGRAM_LEN);
                assert_eq!(sealed, s.datagrams[j], "{} datagram.{}", stem, j);
                j += 1;
            }
        }
        assert_eq!(
            j,
            s.datagrams.len(),
            "{}: every committed datagram made",
            stem
        );
        datagrams += j;
        println!("{}: {} chunks, {} datagrams, identical", stem, s.chunks, j);
    }
    println!(
        "lowlat streams: {} streams, {} datagrams byte for byte",
        all.len(),
        datagrams
    );
}

#[test]
fn every_case_delivers_rebuilds_and_counts_exactly_as_committed() {
    let all = stems("case_");
    assert!(all.len() >= 10, "the cases are committed: {:?}", all);
    for stem in &all {
        let f = load(stem);
        let s = stream(f.str("stream"));
        let tag = if f.has("receiver_stream_tag") {
            f.u64("receiver_stream_tag") as u32
        } else {
            s.tag
        };
        let tamper = f.has("tamper").then(|| numbers(&f, "tamper"));
        let mut opener = Opener::new(s.key, tag);
        let mut dec = FecDecoder::new(s.params);
        let mut delivered = Vec::new();
        let mut recovered = Vec::new();
        for j in numbers(&f, "deliver") {
            let mut bytes = s.datagrams[j as usize].clone();
            if let Some(t) = &tamper {
                if t[0] == j {
                    bytes[t[1] as usize] ^= t[2] as u8;
                }
            }
            let Ok(o) = opener.open(&bytes) else {
                continue;
            };
            for d in dec.push(o.kind, &o.plaintext) {
                assert_eq!(
                    d.payload, s.data_plaintexts[d.chunk_index as usize],
                    "{}: chunk {} is the chunk that was sent, byte for byte",
                    stem, d.chunk_index
                );
                delivered.push(d.chunk_index);
                if d.recovered {
                    recovered.push(d.chunk_index);
                }
            }
        }
        dec.finish(s.chunks);
        delivered.sort_unstable();
        let o = opener.stats();
        let fec = dec.stats();
        assert_eq!(delivered, numbers(&f, "expect_delivered"), "{}", stem);
        assert_eq!(recovered, numbers(&f, "expect_recovered"), "{}", stem);
        let got = [
            ("opened", o.opened),
            ("malformed", o.malformed),
            ("wrong_stream_tag", o.wrong_stream_tag),
            ("replayed", o.replayed),
            ("auth_failed", o.auth_failed),
            ("unrecoverable", fec.unrecoverable),
            ("duplicate", fec.duplicate),
        ];
        for (name, value) in got {
            assert_eq!(
                value,
                f.u64(&format!("expect_{}", name)),
                "{} {}",
                stem,
                name
            );
        }
        assert_eq!((fec.late, fec.rejected), (0, 0), "{}", stem);
        println!(
            "{}: delivered {}, recovered {:?}, unrecoverable {}, dropped {}",
            stem,
            delivered.len(),
            recovered,
            fec.unrecoverable,
            o.malformed + o.wrong_stream_tag + o.replayed + o.auth_failed
        );
    }
    println!("lowlat cases: {} of {} as committed", all.len(), all.len());
}

#[test]
fn a_datagram_of_a_data_kind_the_stream_never_sent_is_not_a_chunk() {
    // A parity pushed as data, and data with another stream's FEC shape, are
    // rejected, never handed on.
    let s = stream("stream_k4_d1");
    let mut opener = Opener::new(s.key, s.tag);
    let parity = opener.open(&s.datagrams[4]).unwrap();
    assert_eq!(parity.kind, Kind::Parity);
    let mut dec = FecDecoder::new(FecParams::new(2, 1).unwrap());
    assert!(dec.push(Kind::Data, &parity.plaintext).is_empty());
    assert!(dec.push(Kind::Data, &s.data_plaintexts[0]).is_empty());
    assert_eq!(dec.stats().rejected, 2);
}

fn caps(features_byte: u8) -> Capabilities {
    Capabilities {
        codecs: Codec::Pcm.bit(),
        sample_formats: 0b011,
        max_channels: 2,
        sample_rates_hz: vec![48_000],
        buffer_ms: 300,
        intrinsic_latency_ns: 0,
        led_count: 0,
        visualizer_bands: 0,
        features: features_byte,
    }
}

#[test]
fn features_is_a_trailing_byte_absent_reads_as_none_and_an_unknown_bit_is_kept() {
    let without = encode(&Message::Capabilities(caps(0))).unwrap();
    let with = encode(&Message::Capabilities(caps(features::LOW_LATENCY))).unwrap();
    assert_eq!(with.len(), without.len() + 1, "one byte, only when set");
    assert_eq!(&with[3..with.len() - 1], &without[3..]);
    // A later version's bit is an endpoint able to do more, not an error.
    let later = encode(&Message::Capabilities(caps(0x81))).unwrap();
    assert_eq!(
        decode_frame(&later).outcome,
        Outcome::Decoded(Message::Capabilities(caps(0x81)))
    );
    assert_eq!(
        decode_frame(&without).outcome,
        Outcome::Decoded(Message::Capabilities(caps(0)))
    );
}

#[test]
fn an_undefined_direction_or_status_is_rejected_as_that_field() {
    let committed =
        parse_hex(&fs::read_to_string(fixture_dir().join("v2/low_latency_offer.hex")).unwrap());
    let mut frame = committed.clone();
    frame[3] = 3;
    match decode_frame(&frame).outcome {
        Outcome::Rejected(DecodeError::InvalidField { error, .. }) => {
            assert_eq!(error.field, "direction");
            assert!(matches!(error.problem, Problem::Undefined(3)));
        }
        other => panic!("direction 3 must be rejected, got {:?}", other),
    }
    let accept =
        parse_hex(&fs::read_to_string(fixture_dir().join("v2/low_latency_accept.hex")).unwrap());
    let mut frame = accept.clone();
    frame[7] = 4;
    match decode_frame(&frame).outcome {
        Outcome::Rejected(DecodeError::InvalidField { error, .. }) => {
            assert_eq!(error.field, "status");
            assert!(matches!(error.problem, Problem::Undefined(4)));
        }
        other => panic!("status 4 must be rejected, got {:?}", other),
    }
}
