//! `--probe-media <path>`: open one local file with the server's decoders and
//! say what it is (goal 16; docs/decoders.md).
//!
//! A diagnostic, like `--health-check`: it runs before any thread or socket
//! exists, decodes the whole file, prints one line and exits. It answers "does
//! this build decode that file, and to what" for an operator, and it is how
//! the image test shows that the decoders inside the released binary run
//! (`tools/image.sh`): libopus is C compiled for the image's own target, and
//! the fixture tests of `crates/decode` run a different build of it.
//!
//! No clock is read: the figures are counts and a hash of the samples.

use chorus_decode::{Decoder, Hint};

/// FNV-1a 64 over bytes, the hash `fixtures/codec` and `fixtures/decode`
/// already use for a decode (64-bit offset basis and prime of the FNV
/// reference, as in `tools/codec-fixtures/opus_ref.c`).
fn fnv1a64(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// Decodes the file at `path` to its end and returns the one line that
/// describes it: codec, rate, channels, bits (`-` for a lossy codec), the
/// frames decoded, the tags, and the FNV-1a 64 of the decoded samples as
/// little-endian `f32`. An exact decode (WAV, FLAC, ALAC, Opus) has one hash
/// on every build; an MP3 or Vorbis decode is floating point and its last bit
/// may differ between builds, which is why those are held to a tolerance and
/// not a hash (`crates/decode/tests/reference_decodes.rs`).
///
/// The error is the refusal, by name: `unsupported: aac`, a file that does not
/// open, a stream that breaks.
pub fn probe(path: &str) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_string);
    let hint = Hint {
        mime: None,
        extension,
    };
    let mut decoder = Decoder::open(Box::new(file), &hint).map_err(|e| e.to_string())?;
    let opened = decoder.format().clone();
    let mut pcm = Vec::new();
    let mut frames = 0u64;
    let mut hash = FNV_OFFSET;
    loop {
        pcm.clear();
        let n = decoder.read(&mut pcm).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if decoder.format().rate != opened.rate || decoder.format().channels != opened.channels {
            return Err(format!(
                "{path}: the stream changes shape inside the file ({} Hz, {} channels after {} Hz, {})",
                decoder.format().rate,
                decoder.format().channels,
                opened.rate,
                opened.channels
            ));
        }
        frames += n as u64;
        for sample in &pcm {
            hash = fnv1a64(hash, &sample.to_le_bytes());
        }
    }
    let tags = decoder.tags();
    let tag = |v: &Option<String>| {
        v.as_deref()
            .map_or_else(|| "-".to_string(), |s| format!("{s:?}"))
    };
    Ok(format!(
        "probe-media: codec={} rate={} channels={} bits={} frames={} title={} artist={} album={} pcm_f32_fnv1a64={:016x}",
        opened.codec.name(),
        opened.rate,
        opened.channels,
        opened.bits.map_or_else(|| "-".to_string(), |b| b.to_string()),
        frames,
        tag(&tags.title),
        tag(&tags.artist),
        tag(&tags.album),
        hash
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        format!(
            "{}/../../fixtures/decode/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn field(name: &str, key: &str) -> String {
        let text =
            std::fs::read_to_string(fixture(&format!("{name}.fields"))).expect("the fields file");
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key} = ")))
            .unwrap_or_else(|| panic!("{name}.fields has no {key}"))
            .to_string()
    }

    #[test]
    fn a_lossless_file_probes_to_its_fixture_hash() {
        let line = probe(&fixture("flac-tone44-s16.flac")).expect("decodes");
        assert_eq!(
            line,
            format!(
                "probe-media: codec=flac rate=44100 channels=2 bits=16 frames=13230 \
                 title=\"chorus fixture flac-tone44-s16\" artist=\"the generator\" \
                 album=\"fixtures/decode\" pcm_f32_fnv1a64={}",
                field("flac-tone44-s16", "decode_f32_fnv1a64")
            )
        );
    }

    #[test]
    fn opus_probes_to_the_hash_of_libopus_as_chorus_builds_it() {
        let line = probe(&fixture("opus-sweep48.opus")).expect("decodes");
        assert!(
            line.starts_with("probe-media: codec=opus rate=48000 channels=2 bits=- frames=14400 "),
            "{line}"
        );
        assert!(
            line.ends_with(&format!(
                "pcm_f32_fnv1a64={}",
                field("opus-sweep48", "decode_f32_fnv1a64")
            )),
            "{line}"
        );
    }

    #[test]
    fn aac_is_refused_by_name() {
        assert_eq!(
            probe(&fixture("aac-in-mp4.m4a")),
            Err("unsupported: aac".to_string())
        );
        assert_eq!(
            probe(&fixture("aac-adts.aac")),
            Err("unsupported: aac".to_string())
        );
    }

    #[test]
    fn a_missing_file_is_a_named_refusal() {
        let e = probe("/nonexistent/chorus.flac").expect_err("no such file");
        assert!(e.starts_with("/nonexistent/chorus.flac: "), "{e}");
    }
}
