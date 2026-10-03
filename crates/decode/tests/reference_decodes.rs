//! The decoders against reference decodes, for every settled format (goal 16's
//! line A: "The P9 decoders match reference decodes for every settled format").
//!
//! `fixtures/decode` holds one short file per format with what a reference
//! decoder made of it (`fixtures/README.md` says which program, version and
//! package hash). What "match" means, per format
//! (docs/decisions/0000-the-server-decoders.md):
//!
//! - WAV, FLAC, ALAC: bit-exact. The integers recovered from the `f32` output
//!   hash to the reference decode's sha256, and the frame count is exact.
//! - Ogg Opus: bit-exact against the decode of the same libopus build made
//!   outside this crate (`exact_sha256`), and libopus's own judge,
//!   opus_compare, passes the decode against opusdec's (RFC 6716 section 6).
//! - MP3 and Ogg Vorbis: within the full-accuracy bounds of ISO/IEC 11172-4
//!   against the committed reference decode (mpg123 for MP3, libvorbis for
//!   Vorbis): "the rms level of the difference signal between the output of
//!   the decoder under test and the supplied reference output is less than
//!   2^-15/sqrt(12)" and "the difference signal shall have a maximum absolute
//!   value of at most 2^-14 relative to full-scale", with outputs "normalized
//!   to be between -1.0 and +1.0" (the standard as quoted at
//!   <https://www.underbit.com/resources/mpeg/audio/compliance>, read
//!   2026-10-03; the standard's own text, <https://www.iso.org/standard/22691.html>,
//!   is paywalled and was not read). Two honest limits: the reference here is
//!   mpg123's decode of chorus's own test signals, not the standard's
//!   bitstreams and supplied output; and for Vorbis the bound is chorus's own
//!   choice, because the Vorbis I specification sets no numeric bound (it asks
//!   a decoder to be "entirely mathematically equivalent to the
//!   specification", <https://xiph.org/vorbis/doc/Vorbis_I_spec.html> section
//!   1.3.2, read 2026-10-03 by the goal's research).
//! - Gapless pairs: one continuous signal cut in two and encoded as two
//!   tracks decodes, joined, to exactly the original frame count, and stays
//!   within a stated bound of the original across the join.
//! - AAC: refused by name.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor, Read, SeekFrom};
use std::path::PathBuf;

use chorus_decode::{Codec, DecodeError, Decoder, Format, Hint, Media, Tags};
use sha2::{Digest, Sha256};

// --- the bounds ---------------------------------------------------------------------

/// ISO/IEC 11172-4 full accuracy: the rms of the difference, full scale 1.0.
const ISO_FULL_ACCURACY_RMS: f64 = 1.0 / 32768.0 / 3.464_101_615_137_754_4; // 2^-15 / sqrt(12)
/// ISO/IEC 11172-4 full accuracy: the largest absolute difference.
const ISO_FULL_ACCURACY_MAX: f64 = 1.0 / 16384.0; // 2^-14

/// The join of a lossy gapless pair, against the original signal: the error in
/// the window around the join may be at most this many times the error of the
/// whole pair (the two halves are encoded independently, so the join is where
/// each encoder knew least about its neighbour), and at most `JOIN_MAX` at any
/// sample. A missing trim (hundreds of frames of offset) or a click fails both
/// by a wide margin; the measured figures are printed by the test and recorded
/// in the decision record.
const JOIN_RMS_RATIO: f64 = 3.0;
const JOIN_MAX: f64 = 0.12;
/// Frames on each side of the join that make the window.
const JOIN_WINDOW: usize = 2048;

/// After a seek a lossy decoder restarts with a run-up of 4096 frames instead
/// of the whole history, so what follows is close to the straight decode, not
/// equal to it. The bound is chorus's own, set above what the fixtures
/// measure (the test prints each figure).
const LOSSY_SEEK_RMS: f64 = 2e-2;

// --- the fixtures --------------------------------------------------------------------

/// Every fixture, with the test that reads it. `every_fixture_is_read_by_a_test`
/// holds the directory to this table, and each test below walks its own rows.
const FIXTURES: &[(&str, &str)] = &[
    ("wav-tone44-s16", "wav_matches_the_reference_decode"),
    ("wav-sweep48-s24", "wav_matches_the_reference_decode"),
    ("flac-tone44-s16", "flac_matches_the_reference_decode"),
    ("flac-sweep48-s24", "flac_matches_the_reference_decode"),
    (
        "flac-gap44-a",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    (
        "flac-gap44-b",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    (
        "alac-tone44-s16-moov-first",
        "alac_in_mp4_matches_the_reference_decode",
    ),
    (
        "alac-sweep48-s24-moov-last",
        "alac_in_mp4_matches_the_reference_decode",
    ),
    ("mp3-tone44", "mp3_matches_the_reference_decode"),
    ("mp3-sweep48", "mp3_matches_the_reference_decode"),
    (
        "mp3-gap44-a",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    (
        "mp3-gap44-b",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    ("vorbis-tone44", "ogg_vorbis_matches_the_reference_decode"),
    ("vorbis-sweep48", "ogg_vorbis_matches_the_reference_decode"),
    (
        "vorbis-gap44-a",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    (
        "vorbis-gap44-b",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    ("opus-tone44", "ogg_opus_matches_the_reference_decode"),
    ("opus-sweep48", "ogg_opus_matches_the_reference_decode"),
    (
        "opus-gap48-a",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    (
        "opus-gap48-b",
        "the_gapless_pairs_join_with_no_gap_and_no_overlap",
    ),
    ("aac-in-mp4", "aac_is_refused_by_name"),
    ("aac-adts", "aac_is_refused_by_name"),
];

fn names_for(test: &str) -> Vec<&'static str> {
    let names: Vec<_> = FIXTURES
        .iter()
        .filter(|(_, t)| *t == test)
        .map(|(n, _)| *n)
        .collect();
    assert!(!names.is_empty(), "no fixture is listed for {test}");
    names
}

fn dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/decode"
    ))
}

struct Fixture {
    name: String,
    fields: BTreeMap<String, String>,
    bytes: Vec<u8>,
}

impl Fixture {
    fn load(name: &str) -> Fixture {
        let text = std::fs::read_to_string(dir().join(format!("{name}.fields")))
            .unwrap_or_else(|e| panic!("{name}.fields: {e}"));
        let fields: BTreeMap<String, String> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let (k, v) = l
                    .split_once(" = ")
                    .unwrap_or_else(|| panic!("{name}.fields: `{l}`"));
                (k.to_string(), v.to_string())
            })
            .collect();
        let bytes =
            std::fs::read(dir().join(&fields["file"])).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            hex(&Sha256::digest(&bytes)),
            fields["file_sha256"],
            "{name}: the file is not the one its fields describe"
        );
        Fixture {
            name: name.to_string(),
            fields,
            bytes,
        }
    }

    fn get(&self, key: &str) -> &str {
        self.fields
            .get(key)
            .unwrap_or_else(|| panic!("{}.fields has no `{key}`", self.name))
    }

    fn number(&self, key: &str) -> u64 {
        self.get(key)
            .parse()
            .unwrap_or_else(|_| panic!("{}.fields: `{key}` is not a number", self.name))
    }

    fn reference(&self) -> Vec<u8> {
        let bytes = std::fs::read(dir().join(format!("{}.ref", self.name)))
            .unwrap_or_else(|e| panic!("{}.ref: {e}", self.name));
        assert_eq!(
            hex(&Sha256::digest(&bytes)),
            self.get("reference_sha256"),
            "{}.ref is not the reference its fields describe",
            self.name
        );
        bytes
    }

    fn extension(&self) -> String {
        self.get("file")
            .rsplit('.')
            .next()
            .expect("an extension")
            .to_string()
    }

    fn codec(&self) -> Codec {
        match self.get("codec") {
            "mp3" => Codec::Mp3,
            "flac" => Codec::Flac,
            "vorbis" => Codec::Vorbis,
            "opus" => Codec::Opus,
            "alac" => Codec::Alac,
            "pcm" => Codec::Pcm,
            other => panic!("{}: codec {other}", self.name),
        }
    }

    fn want_tags(&self) -> Tags {
        Tags {
            title: self.fields.get("title").cloned(),
            artist: self.fields.get("artist").cloned(),
            album: self.fields.get("album").cloned(),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Media that cannot seek and does not know its length: a stream.
struct Streamed(Cursor<Vec<u8>>);

impl Read for Streamed {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // In small pieces, as a socket would deliver them.
        let n = buf.len().min(1000);
        self.0.read(&mut buf[..n])
    }
}

impl Media for Streamed {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a stream does not seek",
        ))
    }
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}

fn seekable(bytes: &[u8]) -> Box<dyn Media> {
    Box::new(Cursor::new(bytes.to_vec()))
}

fn streamed(bytes: &[u8]) -> Box<dyn Media> {
    Box::new(Streamed(Cursor::new(bytes.to_vec())))
}

struct Decoded {
    format: Format,
    tags: Tags,
    pcm: Vec<f32>,
}

impl Decoded {
    fn frames(&self) -> u64 {
        (self.pcm.len() / usize::from(self.format.channels)) as u64
    }
}

fn decode(media: Box<dyn Media>, hint: &Hint) -> Result<Decoded, DecodeError> {
    let mut decoder = Decoder::open(media, hint)?;
    let opened = decoder.format().clone();
    let mut pcm = Vec::new();
    while decoder.read(&mut pcm)? > 0 {
        assert_eq!(
            decoder.format(),
            &opened,
            "the format changed inside one stream"
        );
    }
    Ok(Decoded {
        format: opened,
        tags: decoder.tags().clone(),
        pcm,
    })
}

/// Decodes a fixture from seekable media and from a stream, holds both to one
/// output and to the fixture's stated shape, and returns the decode.
fn decode_both_ways(f: &Fixture) -> Decoded {
    let hint = Hint::default();
    let a = decode(seekable(&f.bytes), &hint).unwrap_or_else(|e| panic!("{}: {e}", f.name));
    let b =
        decode(streamed(&f.bytes), &hint).unwrap_or_else(|e| panic!("{} as a stream: {e}", f.name));
    assert!(
        a.pcm == b.pcm,
        "{}: a stream decodes differently from a file",
        f.name
    );
    check_shape(f, &a, true);
    check_shape(f, &b, false);
    a
}

fn check_shape(f: &Fixture, d: &Decoded, from_a_file: bool) {
    assert_eq!(d.format.codec, f.codec(), "{}", f.name);
    assert_eq!(
        u64::from(d.format.rate),
        f.number("sample_rate_hz"),
        "{}",
        f.name
    );
    assert_eq!(
        u64::from(d.format.channels),
        f.number("channels"),
        "{}",
        f.name
    );
    assert_eq!(
        d.format.bits.map(u64::from),
        f.fields.get("bits").map(|_| f.number("bits")),
        "{}",
        f.name
    );
    assert_eq!(d.frames(), f.number("frames"), "{}: decoded frames", f.name);
    if from_a_file {
        // A file's length is known before it is decoded, after the gapless trim.
        assert_eq!(
            d.format.frames,
            Some(f.number("frames")),
            "{}: frames stated at open",
            f.name
        );
    } else if let Some(stated) = d.format.frames {
        assert_eq!(
            stated,
            f.number("frames"),
            "{}: frames stated at open, as a stream",
            f.name
        );
    }
    assert_eq!(d.tags, f.want_tags(), "{}: tags", f.name);
}

/// The integers an exact decode stands for, as little-endian bytes of
/// `bytes_each` (2 or 3) per sample: sample = f32 * 2^(bits-1).
fn integer_bytes(pcm: &[f32], bits: u32, bytes_each: usize) -> Vec<u8> {
    let scale = f64::from(1u32 << (bits - 1));
    let mut out = Vec::with_capacity(pcm.len() * bytes_each);
    for s in pcm {
        let exact = f64::from(*s) * scale;
        assert_eq!(exact, exact.round(), "a sample is not a {bits}-bit integer");
        out.extend_from_slice(&(exact as i32).to_le_bytes()[..bytes_each]);
    }
    out
}

fn assert_bit_exact(f: &Fixture, d: &Decoded) {
    let bits = f.number("bits") as u32;
    let got = hex(&Sha256::digest(integer_bytes(
        &d.pcm,
        bits,
        bits as usize / 8,
    )));
    assert_eq!(
        got,
        f.get("reference_sha256"),
        "{}: the decode is not the reference decode",
        f.name
    );
}

fn s24_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<3>()
        .0
        .iter()
        .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0)
        .collect()
}

// --- the comparator ------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Difference {
    rms: f64,
    max: f64,
}

/// The difference signal between a decode and a reference of the same length.
fn difference(decoded: &[f32], reference: &[f32]) -> Result<Difference, String> {
    if decoded.len() != reference.len() {
        return Err(format!(
            "{} samples against {}",
            decoded.len(),
            reference.len()
        ));
    }
    if decoded.is_empty() {
        return Err("nothing to compare".to_string());
    }
    let mut sum = 0.0f64;
    let mut max = 0.0f64;
    for (a, b) in decoded.iter().zip(reference) {
        let d = f64::from(*a) - f64::from(*b);
        sum += d * d;
        max = max.max(d.abs());
    }
    Ok(Difference {
        rms: (sum / decoded.len() as f64).sqrt(),
        max,
    })
}

/// ISO/IEC 11172-4 full accuracy, as quoted at the top of this file.
fn within_iso_full_accuracy(decoded: &[f32], reference: &[f32]) -> Result<Difference, String> {
    let d = difference(decoded, reference)?;
    if d.rms >= ISO_FULL_ACCURACY_RMS {
        return Err(format!(
            "rms {:.3e} is not below {:.3e}",
            d.rms, ISO_FULL_ACCURACY_RMS
        ));
    }
    if d.max > ISO_FULL_ACCURACY_MAX {
        return Err(format!(
            "max {:.3e} is above {:.3e}",
            d.max, ISO_FULL_ACCURACY_MAX
        ));
    }
    Ok(d)
}

#[test]
fn a_corrupted_reference_fails_the_comparison() {
    let f = Fixture::load("mp3-tone44");
    let d = decode(seekable(&f.bytes), &Hint::default()).expect("decodes");
    let reference = s24_to_f32(&f.reference());
    within_iso_full_accuracy(&d.pcm, &reference).expect("the true reference passes");

    // One sample off by 2^-13 (twice the largest difference allowed).
    let mut one = reference.clone();
    one[1000] += 1.0 / 8192.0;
    let e = within_iso_full_accuracy(&d.pcm, &one).expect_err("one bad sample");
    assert!(e.contains("max"), "{e}");
    // Every sample off by 2^-15: under the max bound, over the rms bound.
    let offset: Vec<f32> = reference.iter().map(|s| s + 1.0 / 32768.0).collect();
    let e = within_iso_full_accuracy(&d.pcm, &offset).expect_err("an offset");
    assert!(e.contains("rms"), "{e}");
    // The reference one frame late: what a wrong gapless trim looks like.
    let mut late = vec![0.0f32; 2];
    late.extend_from_slice(&reference[..reference.len() - 2]);
    within_iso_full_accuracy(&d.pcm, &late).expect_err("a shifted reference");
    // A frame short.
    within_iso_full_accuracy(&d.pcm, &reference[2..]).expect_err("a short reference");

    // The lossless comparator: one flipped bit in one sample changes the hash.
    let f = Fixture::load("flac-tone44-s16");
    let mut d = decode(seekable(&f.bytes), &Hint::default()).expect("decodes");
    assert_bit_exact(&f, &d);
    d.pcm[500] += 1.0 / 32768.0;
    let got = hex(&Sha256::digest(integer_bytes(&d.pcm, 16, 2)));
    assert_ne!(got, f.get("reference_sha256"));
}

// --- per format ----------------------------------------------------------------------

#[test]
fn wav_matches_the_reference_decode() {
    for name in names_for("wav_matches_the_reference_decode") {
        let f = Fixture::load(name);
        assert_eq!(f.get("match"), "bit-exact");
        let d = decode_both_ways(&f);
        assert_bit_exact(&f, &d);
    }
}

#[test]
fn flac_matches_the_reference_decode() {
    for name in names_for("flac_matches_the_reference_decode") {
        let f = Fixture::load(name);
        assert_eq!(f.get("match"), "bit-exact");
        let d = decode_both_ways(&f);
        assert_bit_exact(&f, &d);
    }
}

#[test]
fn alac_in_mp4_matches_the_reference_decode() {
    for name in names_for("alac_in_mp4_matches_the_reference_decode") {
        let f = Fixture::load(name);
        assert_eq!(f.get("match"), "bit-exact");
        let d = match f.get("moov") {
            // The index first: plays from a file and from a stream.
            "first" => decode_both_ways(&f),
            // The index last: plays where the decoder can seek to it.
            _ => {
                let d = decode(seekable(&f.bytes), &Hint::default())
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                check_shape(&f, &d, true);
                d
            }
        };
        assert_bit_exact(&f, &d);
    }
}

#[test]
fn an_mp4_with_moov_at_the_end_is_refused_by_name_on_a_non_seekable_source() {
    let f = Fixture::load("alac-sweep48-s24-moov-last");
    assert_eq!(f.get("moov"), "last");
    match decode(streamed(&f.bytes), &Hint::default()) {
        Err(DecodeError::Unsupported(what)) => {
            assert_eq!(what, "mp4 with moov at the end on a non-seekable source")
        }
        Err(e) => panic!("refused, but not by name: {e}"),
        Ok(_) => panic!("an mp4 whose index is at its end cannot play from a stream"),
    }
}

#[test]
fn mp3_matches_the_reference_decode() {
    for name in names_for("mp3_matches_the_reference_decode") {
        let f = Fixture::load(name);
        assert_eq!(f.get("match"), "iso-11172-4-full-accuracy");
        let d = decode_both_ways(&f);
        let reference = s24_to_f32(&f.reference());
        let diff = within_iso_full_accuracy(&d.pcm, &reference)
            .unwrap_or_else(|e| panic!("{name} against mpg123: {e}"));
        println!(
            "{name}: against mpg123 1.33.7: rms {:.3e} (bound {:.3e}), max {:.3e} (bound {:.3e})",
            diff.rms, ISO_FULL_ACCURACY_RMS, diff.max, ISO_FULL_ACCURACY_MAX
        );
    }
}

#[test]
fn ogg_vorbis_matches_the_reference_decode() {
    for name in names_for("ogg_vorbis_matches_the_reference_decode") {
        let f = Fixture::load(name);
        assert_eq!(f.get("match"), "iso-11172-4-full-accuracy");
        let d = decode_both_ways(&f);
        let reference = s24_to_f32(&f.reference());
        let diff = within_iso_full_accuracy(&d.pcm, &reference)
            .unwrap_or_else(|e| panic!("{name} against libvorbis: {e}"));
        println!(
            "{name}: against libvorbis 1.3.7: rms {:.3e} (bound {:.3e}), max {:.3e} (bound {:.3e})",
            diff.rms, ISO_FULL_ACCURACY_RMS, diff.max, ISO_FULL_ACCURACY_MAX
        );
    }
}

/// 16-bit little-endian, as opus_compare reads it.
fn s16_bytes(pcm: &[f32]) -> Vec<u8> {
    pcm.iter()
        .flat_map(|s| {
            ((f64::from(*s) * 32768.0).round().clamp(-32768.0, 32767.0) as i16).to_le_bytes()
        })
        .collect()
}

fn assert_opus_exact(f: &Fixture, d: &Decoded) {
    let got = hex(&Sha256::digest(integer_bytes(&d.pcm, 24, 3)));
    assert_eq!(
        got,
        f.get("exact_sha256"),
        "{}: not the decode of libopus as chorus builds it",
        f.name
    );
}

#[test]
fn ogg_opus_matches_the_reference_decode() {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    for name in names_for("ogg_opus_matches_the_reference_decode") {
        let f = Fixture::load(name);
        assert_eq!(f.get("match"), "opus");
        let d = decode_both_ways(&f);
        assert_eq!(d.format.rate, 48_000);
        // Bit for bit the decode made outside this crate by the same libopus build,
        // which also fixes the pre-skip, the end trim and the output gain.
        assert_opus_exact(&f, &d);
        // And libopus's own judge against opusdec's decode (RFC 6716 section 6).
        let reference = tmp.join(format!("{name}.reference.s16"));
        let decoded = tmp.join(format!("{name}.decoded.s16"));
        std::fs::write(&reference, f.reference()).expect("write the reference");
        std::fs::write(&decoded, s16_bytes(&d.pcm)).expect("write the decode");
        assert!(
            chorus_opus_sys::opus_compare(
                reference.to_str().expect("utf-8"),
                decoded.to_str().expect("utf-8"),
                true
            ),
            "{name}: opus_compare fails the decode against opusdec's"
        );
    }
}

// --- gapless --------------------------------------------------------------------------

/// The gap signal of tools/decode-fixtures/generate.py, from its formula.
fn gap_signal(rate: u32, frames: usize) -> Vec<f32> {
    const LEFT: [f64; 3] = [440.5, 997.3, 3001.7];
    const RIGHT: [f64; 3] = [311.3, 1499.1, 5003.9];
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = i as f64 / f64::from(rate);
        for tones in [LEFT, RIGHT] {
            let s: f64 = tones
                .iter()
                .map(|f| (2.0 * std::f64::consts::PI * f * t).sin())
                .sum::<f64>()
                / 3.0;
            out.push(((s * 0.5 * 32767.0).round() / 32768.0) as f32);
        }
    }
    out
}

#[test]
fn the_gapless_pairs_join_with_no_gap_and_no_overlap() {
    let names = names_for("the_gapless_pairs_join_with_no_gap_and_no_overlap");
    let mut pairs = BTreeSet::new();
    for name in &names {
        pairs.insert(
            name.trim_end_matches("-a")
                .trim_end_matches("-b")
                .to_string(),
        );
    }
    assert_eq!(
        pairs.len() * 2,
        names.len(),
        "every gapless fixture is half of a pair"
    );
    for pair in pairs {
        let a = Fixture::load(&format!("{pair}-a"));
        let b = Fixture::load(&format!("{pair}-b"));
        let da = decode_both_ways(&a);
        let db = decode_both_ways(&b);
        assert_eq!(
            da.format,
            Format {
                frames: da.format.frames,
                ..db.format.clone()
            },
            "{pair}: two halves, one shape"
        );
        let total = da.frames() + db.frames();
        let cut = da.frames() as usize;
        let mut joined = da.pcm.clone();
        joined.extend_from_slice(&db.pcm);
        let original = gap_signal(da.format.rate, total as usize);

        if a.fields.get("match").map(String::as_str) == Some("bit-exact") {
            // Lossless: the joined decode IS the original, bit for bit.
            assert_eq!(total, a.number("gap_total_frames"), "{pair}: frames");
            assert_eq!(cut as u64, a.number("gap_cut_frame"), "{pair}: the cut");
            let got = hex(&Sha256::digest(integer_bytes(&joined, 16, 2)));
            assert_eq!(
                got,
                a.get("gap_original_sha256"),
                "{pair}: the joined decode is not the original"
            );
            assert_eq!(a.get("gap_original_sha256"), b.get("gap_original_sha256"));
            assert_bit_exact(&a, &da);
            assert_bit_exact(&b, &db);
            println!("{pair}: {total} frames, bit-exact across the join");
            continue;
        }
        if a.codec() == Codec::Opus {
            assert_opus_exact(&a, &da);
            assert_opus_exact(&b, &db);
        }
        // Lossy: exactly the original's frame count (no gap, no overlap) ...
        let rate = u64::from(da.format.rate);
        assert_eq!(
            total, rate,
            "{pair}: one second was cut in two; the halves decode to {total} frames"
        );
        assert_eq!(cut as u64, rate / 2, "{pair}: the cut");
        // ... and close to the original across the join.
        let whole = difference(&joined, &original).expect("same length");
        let window = (cut - JOIN_WINDOW) * 2..(cut + JOIN_WINDOW) * 2;
        let join = difference(&joined[window.clone()], &original[window]).expect("same length");
        println!(
            "{pair}: {total} frames; error against the original: whole rms {:.3e} max {:.3e}; within {JOIN_WINDOW} frames of the join rms {:.3e} max {:.3e}",
            whole.rms, whole.max, join.rms, join.max
        );
        assert!(
            join.rms < JOIN_RMS_RATIO * whole.rms,
            "{pair}: the join's rms error {:.3e} is over {JOIN_RMS_RATIO} times the whole's {:.3e}",
            join.rms,
            whole.rms
        );
        assert!(
            join.max < JOIN_MAX,
            "{pair}: an error of {:.3e} at the join",
            join.max
        );
        // The check has teeth: the same halves joined with one frame missing fail it.
        let mut gapped = da.pcm[..da.pcm.len() - 64].to_vec();
        gapped.extend_from_slice(&db.pcm);
        gapped.extend_from_slice(&[0.0; 64]);
        let window = (cut - JOIN_WINDOW) * 2..(cut + JOIN_WINDOW) * 2;
        let bad = difference(&gapped[window.clone()], &original[window]).expect("same length");
        assert!(
            bad.max >= JOIN_MAX || bad.rms >= JOIN_RMS_RATIO * whole.rms,
            "{pair}: a 32-frame gap at the join passes the bound"
        );
    }
}

// --- refusals -------------------------------------------------------------------------

#[test]
fn aac_is_refused_by_name() {
    for name in names_for("aac_is_refused_by_name") {
        let f = Fixture::load(name);
        assert_eq!(f.get("match"), "refused");
        let hints = [
            Hint::default(),
            Hint {
                mime: Some("audio/mp4".into()),
                extension: Some(f.extension()),
            },
            Hint {
                mime: Some("audio/aac".into()),
                extension: None,
            },
            // A wrong type does not turn AAC into something else.
            Hint {
                mime: Some("audio/mpeg".into()),
                extension: Some("mp3".into()),
            },
        ];
        for hint in &hints {
            for media in [seekable(&f.bytes), streamed(&f.bytes)] {
                match decode(media, hint) {
                    Err(e @ DecodeError::Unsupported(_)) => {
                        assert_eq!(e.to_string(), f.get("refusal"), "{name} with {hint:?}")
                    }
                    Err(e) => panic!("{name} with {hint:?}: refused, but not by name: {e}"),
                    Ok(d) => panic!("{name} with {hint:?}: decoded {} frames of AAC", d.frames()),
                }
            }
        }
    }
    // Bytes nothing recognises, announced as AAC: still refused as AAC.
    let junk = vec![0x55u8; 20_000];
    let hint = Hint {
        mime: Some("audio/aacp".into()),
        extension: None,
    };
    assert_eq!(
        decode(streamed(&junk), &hint)
            .err()
            .map(|e| e.to_string())
            .as_deref(),
        Some("unsupported: aac")
    );
    // And announced as nothing: refused, not decoded and not a panic.
    assert!(matches!(
        decode(streamed(&junk), &Hint::default()),
        Err(DecodeError::Unsupported(_))
    ));
    assert!(matches!(
        decode(seekable(&[]), &Hint::default()),
        Err(DecodeError::Unsupported(_))
    ));
}

// --- Ogg specifics ----------------------------------------------------------------------

/// The Ogg page checksum (RFC 3533 section 6: CRC-32 with the polynomial
/// 0x04c11db7, no reflection, zero start; `ASSUMED` from memory, and proven by
/// the fixtures' own pages below).
fn ogg_crc(page: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in page {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Rewrites bytes of the first page's body (the OpusHead) and fixes the page's
/// checksum, so the demuxer accepts the edited file.
fn with_opus_head(bytes: &[u8], edit: impl Fn(&mut [u8])) -> Vec<u8> {
    let mut out = bytes.to_vec();
    assert_eq!(&out[..4], b"OggS");
    let segments = usize::from(out[26]);
    let body = 27 + segments;
    let len: usize = out[27..body].iter().map(|b| usize::from(*b)).sum();
    let page = &mut out[..body + len];
    let stored = u32::from_le_bytes([page[22], page[23], page[24], page[25]]);
    page[22..26].fill(0);
    assert_eq!(
        ogg_crc(page),
        stored,
        "the checksum routine does not match the fixture's own page"
    );
    assert_eq!(&page[body..body + 8], b"OpusHead");
    edit(&mut page[body..]);
    let crc = ogg_crc(page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    out
}

#[test]
fn opus_output_gain_is_applied() {
    let f = Fixture::load("opus-tone44");
    let plain = decode(seekable(&f.bytes), &Hint::default()).expect("decodes");
    // -6.02 dB in Q7.8 (RFC 7845 section 5.1): 20 log10(0.5) * 256 = -1541.
    let quiet = with_opus_head(&f.bytes, |head| {
        head[16..18].copy_from_slice(&(-1541i16).to_le_bytes())
    });
    let quiet = decode(seekable(&quiet), &Hint::default()).expect("decodes");
    assert_eq!(quiet.pcm.len(), plain.pcm.len());
    let rms =
        |v: &[f32]| (v.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    let ratio = rms(&quiet.pcm) / rms(&plain.pcm);
    assert!(
        (ratio - 0.5).abs() < 0.005,
        "an output gain of -6.02 dB scaled the decode by {ratio}"
    );
}

#[test]
fn an_opus_mapping_family_other_than_0_is_refused_by_name() {
    let f = Fixture::load("opus-tone44");
    let surround = with_opus_head(&f.bytes, |head| head[18] = 1);
    match decode(seekable(&surround), &Hint::default()) {
        Err(DecodeError::Unsupported(what)) => assert_eq!(what, "opus mapping family 1"),
        Err(e) => panic!("refused, but not by name: {e}"),
        Ok(_) => panic!("mapping family 1 decoded"),
    }
}

#[test]
fn a_chained_ogg_stream_continues_with_refreshed_tags() {
    for (first, second) in [
        ("vorbis-gap44-a", "vorbis-sweep48"),
        ("opus-gap48-a", "opus-gap48-b"),
    ] {
        let a = Fixture::load(first);
        let b = Fixture::load(second);
        // Two complete Ogg streams end to end are one chained stream (what an
        // Icecast mount sends at each new title).
        let mut chain = a.bytes.clone();
        chain.extend_from_slice(&b.bytes);
        let mut decoder = Decoder::open(streamed(&chain), &Hint::default()).expect("opens");
        assert_eq!(decoder.tags(), &a.want_tags());
        let mut links = vec![(decoder.format().clone(), decoder.tags().clone(), 0u64)];
        let mut pcm = Vec::new();
        loop {
            let before = pcm.len();
            let n = decoder.read(&mut pcm).expect("reads across the link");
            if n == 0 {
                break;
            }
            let last = links.last_mut().expect("one link");
            if decoder.tags() != &last.1 || decoder.format().rate != last.0.rate {
                links.push((decoder.format().clone(), decoder.tags().clone(), 0));
            }
            let channels = usize::from(decoder.format().channels);
            assert_eq!((pcm.len() - before) / channels, n);
            links.last_mut().expect("one link").2 += n as u64;
        }
        assert_eq!(links.len(), 2, "{first} + {second}: {links:?}");
        assert_eq!(
            links[0].2,
            a.number("frames"),
            "{first}: frames of the first link"
        );
        assert_eq!(
            links[1].2,
            b.number("frames"),
            "{second}: frames of the second link"
        );
        assert_eq!(links[1].1, b.want_tags(), "the second link's tags");
        assert_eq!(
            u64::from(links[1].0.rate),
            b.number("sample_rate_hz"),
            "the second link's rate"
        );
        // Each link decodes as it does alone.
        let alone_a = decode(seekable(&a.bytes), &Hint::default()).expect("decodes");
        let alone_b = decode(seekable(&b.bytes), &Hint::default()).expect("decodes");
        assert!(
            pcm[..alone_a.pcm.len()] == alone_a.pcm[..],
            "{first}: the first link"
        );
        assert!(
            pcm[alone_a.pcm.len()..] == alone_b.pcm[..],
            "{second}: the second link"
        );
    }
}

// --- seeking, raw L16 -----------------------------------------------------------------

#[test]
fn seek_lands_on_the_frame_asked_for() {
    for (name, exact) in [
        ("wav-tone44-s16", true),
        ("flac-tone44-s16", true),
        ("alac-tone44-s16-moov-first", true),
        ("mp3-tone44", false),
        ("vorbis-tone44", false),
        ("opus-tone44", false),
    ] {
        let f = Fixture::load(name);
        let whole = decode(seekable(&f.bytes), &Hint::default()).expect("decodes");
        let channels = usize::from(whole.format.channels);
        for target in [0u64, 1, 4410, 9000, 9001] {
            let mut decoder = Decoder::open(seekable(&f.bytes), &Hint::default()).expect("opens");
            // Read a little first, so the seek is a real move.
            let mut pcm = Vec::new();
            decoder.read(&mut pcm).expect("reads");
            assert_eq!(
                decoder
                    .seek(target)
                    .unwrap_or_else(|e| panic!("{name} to {target}: {e}")),
                target
            );
            pcm.clear();
            while decoder.read(&mut pcm).expect("reads") > 0 {}
            let rest = &whole.pcm[target as usize * channels..];
            assert_eq!(
                pcm.len(),
                rest.len(),
                "{name}: frames after a seek to {target}"
            );
            if exact {
                assert!(
                    pcm == rest,
                    "{name}: a seek to {target} does not give the same samples"
                );
            } else {
                // A lossy decoder restarts from less history than a straight
                // decode had; it converges, it is not identical.
                let d = difference(&pcm, rest).expect("same length");
                println!("{name}: after a seek to {target}: rms {:.3e} max {:.3e} against the straight decode", d.rms, d.max);
                assert!(
                    d.rms < LOSSY_SEEK_RMS,
                    "{name}: after a seek to {target} the decode is off by rms {:.3e}",
                    d.rms
                );
            }
        }
    }
    let f = Fixture::load("flac-tone44-s16");
    let mut decoder = Decoder::open(streamed(&f.bytes), &Hint::default()).expect("opens");
    match decoder.seek(10) {
        Err(DecodeError::Unsupported(what)) => assert_eq!(what, "seek on a non-seekable source"),
        other => panic!(
            "a stream must refuse a seek by name, got {:?}",
            other.map_err(|e| e.to_string())
        ),
    }
}

#[test]
fn raw_l16_decodes_big_endian_frames() {
    let samples: [i16; 6] = [0, 1, -1, 32767, -32768, 12345];
    let bytes: Vec<u8> = samples
        .iter()
        .flat_map(|s| s.to_be_bytes())
        .chain([0x7f])
        .collect();
    for media in [seekable(&bytes), streamed(&bytes)] {
        let known = media.byte_len().is_some();
        let mut decoder = Decoder::open_l16(media, 44_100, 2).expect("opens");
        assert_eq!(
            decoder.format(),
            &Format {
                codec: Codec::Pcm,
                rate: 44_100,
                channels: 2,
                bits: Some(16),
                frames: known.then_some(3)
            }
        );
        let mut pcm = Vec::new();
        while decoder.read(&mut pcm).expect("reads") > 0 {}
        let want: Vec<f32> = samples.iter().map(|s| f32::from(*s) / 32768.0).collect();
        assert_eq!(pcm, want, "the trailing odd byte is not a frame");
        if known {
            assert_eq!(decoder.seek(2).expect("seeks"), 2);
            pcm.clear();
            while decoder.read(&mut pcm).expect("reads") > 0 {}
            assert_eq!(pcm, want[4..]);
        }
    }
    assert!(matches!(
        Decoder::open_l16(seekable(&bytes), 0, 2),
        Err(DecodeError::Malformed(_))
    ));
}

// --- the directory ----------------------------------------------------------------------

#[test]
fn every_fixture_is_read_by_a_test() {
    let listed: BTreeMap<&str, &str> = FIXTURES.iter().copied().collect();
    assert_eq!(listed.len(), FIXTURES.len(), "a fixture is listed twice");
    // Every listed test exists in this file under that name and walks its rows.
    let source = include_str!("reference_decodes.rs");
    for test in listed.values() {
        assert!(
            source.contains(&format!("fn {test}()")),
            "no test named {test}"
        );
        assert!(
            source.contains(&format!("names_for(\"{test}\")")),
            "{test} does not walk its fixtures"
        );
    }
    let mut on_disk = BTreeSet::new();
    for entry in std::fs::read_dir(dir()).expect("fixtures/decode") {
        let file = entry
            .expect("an entry")
            .file_name()
            .into_string()
            .expect("utf-8");
        let (stem, ext) = file
            .rsplit_once('.')
            .unwrap_or_else(|| panic!("{file}: no extension"));
        assert!(
            matches!(
                ext,
                "fields" | "ref" | "wav" | "flac" | "m4a" | "mp3" | "ogg" | "opus" | "aac"
            ),
            "{file}: a kind of file no test reads"
        );
        assert!(
            listed.contains_key(stem),
            "{file}: a fixture no test reads (add it to FIXTURES and to a test)"
        );
        on_disk.insert(stem.to_string());
    }
    for name in listed.keys() {
        assert!(on_disk.contains(*name), "{name} is listed and not on disk");
        let f = Fixture::load(name);
        // A reference file is read exactly where the fields say a test needs the samples.
        let has_ref = dir().join(format!("{name}.ref")).exists();
        let needs_ref = matches!(f.get("match"), "iso-11172-4-full-accuracy")
            || (f.get("match") == "opus" && !name.contains("-gap"));
        assert_eq!(
            has_ref, needs_ref,
            "{name}: a reference file without a reader, or a reader without its file"
        );
    }
}
