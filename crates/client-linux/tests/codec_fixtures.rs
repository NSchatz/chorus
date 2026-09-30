//! The Linux client's decoders against the shared fixtures in fixtures/codec,
//! the same files firmware/tests/test_codec.c reads (fixtures/README.md says
//! what each holds).
//!
//! FLAC must decode bit-exact with `flac -d`'s reference, and Symphonia's own
//! MD5 check must find the STREAMINFO's MD5. Opus is judged as RFC 6716
//! section 6 and RFC 8251 judge a decoder: every packet's final range equal to
//! the vector's, and libopus's opus_compare passing the decode against the
//! official one; the decode must also be exactly the one the endpoint's C
//! gives (`decode_fnv1a64`), since both run the same libopus. The decoded
//! stream is then run through the path a session uses, `CodedStream` into the
//! v1 receive path, and must come out as the same PCM in full chunks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chorus_client_linux::coded::CodedStream;
use chorus_client_linux::decode::Decoder;
use chorus_client_linux::receive::{Received, Receiver};
use chorus_client_linux::session::Announced;
use chorus_protocol::v2::session::Translation;
use chorus_protocol::v2::{ChannelPosition, Codec, CodedChunk, Message, StreamFormat};
use chorus_protocol::{SampleFormat, StreamEnd};

struct Fixture {
    name: String,
    fields: BTreeMap<String, String>,
    chunks: Vec<(u32, u32, Vec<u8>)>,
    reference: Vec<u8>,
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/codec")
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex"))
        .collect()
}

fn load(name: &str) -> Fixture {
    let text = fs::read_to_string(dir().join(format!("{}.fields", name))).expect("fields");
    let fields = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(" = "))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let raw = fs::read(dir().join(format!("{}.chunks", name))).expect("chunks");
    let mut chunks = Vec::new();
    let mut at = 0;
    while at < raw.len() {
        let word = |o: usize| u32::from_be_bytes(raw[at + o..at + o + 4].try_into().unwrap());
        let (frames, range, len) = (word(0), word(4), word(8) as usize);
        chunks.push((frames, range, raw[at + 12..at + 12 + len].to_vec()));
        at += 12 + len;
    }
    let reference = fs::read(dir().join(format!("{}.pcm", name))).expect("pcm");
    Fixture {
        name: name.to_string(),
        fields,
        chunks,
        reference,
    }
}

fn names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir())
        .expect("fixtures/codec")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".fields"))
                .map(str::to_string)
        })
        .collect();
    names.sort();
    names
}

impl Fixture {
    fn get(&self, key: &str) -> &str {
        self.fields
            .get(key)
            .unwrap_or_else(|| panic!("{}: no {}", self.name, key))
    }

    fn codec(&self) -> Codec {
        if self.get("codec") == "flac" {
            Codec::Flac
        } else {
            Codec::Opus
        }
    }

    fn format(&self, sample_format: SampleFormat) -> StreamFormat {
        let channels: usize = self.get("channels").parse().unwrap();
        let map = match channels {
            1 => vec![ChannelPosition::Mono],
            2 => vec![ChannelPosition::FrontLeft, ChannelPosition::FrontRight],
            // FLAC's order for six channels (RFC 9639 section 9.1.3).
            _ => vec![
                ChannelPosition::FrontLeft,
                ChannelPosition::FrontRight,
                ChannelPosition::FrontCenter,
                ChannelPosition::LowFrequency,
                ChannelPosition::BackLeft,
                ChannelPosition::BackRight,
            ],
        };
        StreamFormat {
            codec: self.codec(),
            sample_format,
            sample_rate_hz: self.get("sample_rate_hz").parse().unwrap(),
            channel_map: map,
            frames_per_chunk: self.get("frames_per_chunk").parse().unwrap(),
            codec_config: unhex(self.get("codec_config")),
        }
    }

    fn sample_format(&self) -> SampleFormat {
        if self.get("sample_format") == "pcm_s16le" {
            SampleFormat::PcmS16Le
        } else {
            SampleFormat::PcmS24Le
        }
    }

    /// Decode every chunk; the PCM, and how many final ranges matched.
    fn decode(&self, sample_format: SampleFormat, md5: bool) -> (Vec<u8>, usize, Option<bool>) {
        let mut d = Decoder::open_checked(&self.format(sample_format), md5)
            .unwrap_or_else(|e| panic!("{}: {}", self.name, e));
        let mut out = Vec::new();
        let mut ranges = 0;
        for (i, (frames, range, data)) in self.chunks.iter().enumerate() {
            d.decode(data, *frames, &mut out)
                .unwrap_or_else(|e| panic!("{}: chunk {}: {}", self.name, i, e));
            if d.final_range() == *range {
                ranges += 1;
            }
        }
        let md5_ok = d.md5_matches();
        (out, ranges, md5_ok)
    }
}

fn fnv1a64(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

/// The decoded stream through the session's path: stream_format, the coded
/// chunks, stream_end, into `CodedStream` and out as v1 frames the receive
/// path reads. Returns the PCM it played and the chunk sizes.
fn through_the_session_path(fx: &Fixture) -> (Vec<u8>, Vec<u64>, Vec<u64>) {
    let format = fx.format(fx.sample_format());
    let announced = Arc::new(Mutex::new(Announced {
        stream_format: Some(format.clone()),
        ..Announced::default()
    }));
    let mut coded = CodedStream::new(Arc::clone(&announced));
    let mut bytes = Vec::new();
    let mut take = |t: Translation| match t {
        Translation::Pass => {}
        Translation::Before(v) | Translation::Replace(v) => bytes.extend(v),
    };
    let start_ns = 1_000_000_000u64;
    let mut at_frames = 0u64;
    for (i, (frames, _, data)) in fx.chunks.iter().enumerate() {
        let chunk = CodedChunk {
            sequence: 100 + i as u32,
            timestamp_ns: start_ns + at_frames * 1_000_000_000 / u64::from(format.sample_rate_hz),
            frames: *frames,
            data: data.clone(),
        };
        at_frames += u64::from(*frames);
        take(
            coded
                .translate(&Message::CodedChunk(chunk))
                .expect("decodes"),
        );
    }
    let end = StreamEnd {
        final_sequence: 0,
        end_timestamp_ns: 0,
    };
    take(coded.translate(&Message::StreamEnd(end)).expect("flushes"));
    assert_eq!(
        announced.lock().unwrap().decoded_chunks,
        fx.chunks.len() as u64
    );
    let mut receiver = Receiver::new();
    let mut pcm = Vec::new();
    let mut sizes = Vec::new();
    let mut stamps = Vec::new();
    for r in receiver.push(&bytes).expect("no framing error") {
        if let Received::Chunk { chunk, frames } = r {
            sizes.push(frames);
            stamps.push(chunk.timestamp_ns);
            pcm.extend(chunk.audio_data);
        }
    }
    (pcm, sizes, stamps)
}

#[test]
fn flac_and_opus_decode_to_the_reference_decodes() {
    let (mut flac, mut flac_ok, mut opus, mut opus_ok) = (0, 0, 0, 0);
    let out_dir = std::env::temp_dir().join(format!("chorus-codec-{}", std::process::id()));
    fs::create_dir_all(&out_dir).unwrap();
    for name in names() {
        let fx = load(&name);
        let chunks = fx.chunks.len();
        assert_eq!(chunks, fx.get("chunks").parse::<usize>().unwrap());
        if fx.codec() == Codec::Flac {
            flac += 1;
            let (pcm, _, md5) = fx.decode(fx.sample_format(), true);
            let exact = pcm == fx.reference;
            println!(
                "{}: {} chunks, {} bytes, bit-exact with flac -d: {}; Symphonia's MD5 check \
                 against STREAMINFO: {:?}",
                name,
                chunks,
                pcm.len(),
                exact,
                md5
            );
            assert!(exact, "{}: not bit-exact", name);
            assert_eq!(md5, Some(true), "{}: MD5", name);
            assert_eq!(
                format!("{:016x}", fnv1a64(&pcm)),
                fx.get("decode_fnv1a64"),
                "{}",
                name
            );
            flac_ok += 1;
        } else {
            opus += 1;
            let (pcm, ranges, _) = fx.decode(SampleFormat::PcmS16Le, false);
            let fnv = format!("{:016x}", fnv1a64(&pcm));
            let decoded = out_dir.join(format!("{}.pcm", name));
            fs::write(&decoded, &pcm).unwrap();
            let reference = dir().join(format!("{}.pcm", name));
            let stereo = fx.get("channels") == "2";
            let quality = chorus_opus_sys::opus_compare(
                reference.to_str().unwrap(),
                decoded.to_str().unwrap(),
                stereo,
            );
            println!(
                "{}: final range equal in {} of {} packets; FNV-1a {} (the endpoint's: {}); \
                 opus_compare passes: {}",
                name,
                ranges,
                chunks,
                fnv,
                fx.get("decode_fnv1a64"),
                quality
            );
            assert_eq!(ranges, chunks, "{}: final ranges", name);
            assert_eq!(fnv, fx.get("decode_fnv1a64"), "{}: exact decode", name);
            assert!(quality, "{}: opus_compare", name);
            // The 24-bit output rounds to the 16-bit one as libopus rounds.
            let (pcm24, _, _) = fx.decode(SampleFormat::PcmS24Le, false);
            assert_eq!(pcm24.len() / 3, pcm.len() / 2);
            for (s24, s16) in pcm24.chunks(3).zip(pcm.chunks(2)) {
                let v = i32::from_le_bytes([0, s24[0], s24[1], s24[2]]) >> 8;
                let r = ((v + 128) >> 8).clamp(-32768, 32767);
                assert_eq!(r, i32::from(i16::from_le_bytes([s16[0], s16[1]])));
            }
            opus_ok += 1;
        }
    }
    let _ = fs::remove_dir_all(&out_dir);
    println!(
        "codec fixtures: flac {} of {} bit-exact and MD5, opus {} of {} final-range, exact and \
         opus_compare pass",
        flac_ok, flac, opus_ok, opus
    );
    assert!(
        flac >= 3 && opus >= 2,
        "fixtures/codec holds {} FLAC and {} Opus",
        flac,
        opus
    );
}

#[test]
fn a_coded_stream_reaches_the_receive_path_as_full_chunks_of_the_same_pcm() {
    for name in names() {
        let fx = load(&name);
        let (pcm, sizes, stamps) = through_the_session_path(&fx);
        let (direct, _, _) = fx.decode(fx.sample_format(), false);
        assert_eq!(
            pcm, direct,
            "{}: the session path plays what the decoder gave",
            name
        );
        let nominal = sizes[0];
        assert!(
            sizes[..sizes.len() - 1].iter().all(|&n| n == nominal)
                && sizes[sizes.len() - 1] <= nominal,
            "{}: every chunk but the last is full: {:?}",
            name,
            sizes
        );
        // Contiguous: each chunk starts where the one before it ended.
        let rate: u64 = fx.get("sample_rate_hz").parse().unwrap();
        let skip = if fx.codec() == Codec::Opus {
            u64::from(u16::from_le_bytes([
                unhex(fx.get("codec_config"))[10],
                unhex(fx.get("codec_config"))[11],
            ]))
        } else {
            0
        };
        let start = 1_000_000_000u64 + skip * 1_000_000_000 / rate;
        let mut at = 0u64;
        for (i, (&n, &ts)) in sizes.iter().zip(&stamps).enumerate() {
            assert_eq!(
                ts,
                start + at * 1_000_000_000 / rate,
                "{}: chunk {} timestamp",
                name,
                i
            );
            at += n;
        }
        println!(
            "{}: {} coded chunks became {} chunks of {} frames (last {}), first at +{} frames",
            name,
            fx.chunks.len(),
            sizes.len(),
            nominal,
            sizes[sizes.len() - 1],
            skip
        );
    }
}

#[test]
fn a_bad_setup_or_chunk_is_refused_by_name() {
    let flac = load("flac-s16-stereo-44k1");
    let mut f = flac.format(SampleFormat::PcmS16Le);
    f.codec_config.pop();
    assert!(Decoder::open(&f)
        .unwrap_err()
        .contains("34-byte STREAMINFO"));
    let f = flac.format(SampleFormat::PcmF32Le);
    assert!(Decoder::open(&f).unwrap_err().contains("pcm_f32le"));
    let opus = load("opus-tv05-hybrid-stereo");
    let mut o = opus.format(SampleFormat::PcmS16Le);
    o.codec_config[18] = 1;
    assert!(Decoder::open(&o).unwrap_err().contains("mapping family 1"));

    let mut d = Decoder::open(&flac.format(SampleFormat::PcmS16Le)).unwrap();
    let (frames, _, data) = &flac.chunks[0];
    let mut out = Vec::new();
    let e = d.decode(data, frames - 1, &mut out).unwrap_err();
    assert!(e.contains("decoded to"), "{}", e);
    let mut bad = flac.chunks[1].2.clone();
    let middle = bad.len() / 2;
    bad[middle] ^= 0x5a;
    assert!(d.decode(&bad, flac.chunks[1].0, &mut out).is_err());
    let before = out.len();
    let (frames, _, data) = &flac.chunks[2];
    d.decode(data, *frames, &mut out)
        .expect("the next frame decodes");
    let start = (flac.chunks[0].0 as usize + flac.chunks[1].0 as usize) * 4;
    assert_eq!(
        &out[before..],
        &flac.reference[start..start + out.len() - before]
    );

    // A coded chunk under a PCM announcement is a framing error.
    let mut pcm_format = flac.format(SampleFormat::PcmS16Le);
    pcm_format.codec = Codec::Pcm;
    pcm_format.codec_config.clear();
    let announced = Arc::new(Mutex::new(Announced {
        stream_format: Some(pcm_format),
        ..Announced::default()
    }));
    let mut coded = CodedStream::new(announced);
    let chunk = CodedChunk {
        sequence: 0,
        timestamp_ns: 0,
        frames: flac.chunks[0].0,
        data: flac.chunks[0].2.clone(),
    };
    let e = coded.translate(&Message::CodedChunk(chunk)).unwrap_err();
    assert!(e.to_string().contains("framing error"), "{}", e);
}
