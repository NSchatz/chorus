# 0122: the server decodes MP3, FLAC, Vorbis, ALAC and WAV with Symphonia and Ogg Opus with the endpoints' libopus, held to reference decodes; AAC is never compiled; zig compiles the image's C and rustc still links it

- Status: accepted (goal 16, 2026-10-03)
- Decided by: the owner at Checkpoint K (proposal P9, Option A, decision K79; K26 and K95 let a
  licence off the allowlist in by record); goal 16 took the choices left open below
- Implemented in: `crates/decode`, `fixtures/decode`, `tools/decode-fixtures/`,
  `crates/server/src/probe_media.rs`, `tools/image.sh`, `tools/zig-musl-cc.sh`,
  `crates/opus-sys/build.rs`, `deny.toml`; described in `docs/decoders.md`; measured in
  `docs/measurements/resampler-quality.md`

## Context

P9 settled the formats (MP3, FLAC, Ogg Vorbis, Ogg Opus, ALAC, WAV; AAC off) and the libraries
(Symphonia for the input formats, libopus for Opus) and left four things to goal 16: the MPL-2.0
record and its cargo-deny exceptions, what "the decoders match reference decodes" means per
format, how the static musl image is built once the server links C, and the binary size (P9's
open input). ADR 0044 already admitted four Symphonia crates for FLAC in the Linux client and
vendored libopus 1.6.1; this record extends both to the server.

## What was read

All read 2026-10-03 unless marked.

- `docs/proposals/P9-decoders.md`, ADR 0044, ADR 0069 (the endpoint package's zig build), the
  goal's research notes on Symphonia 0.6.1 and on the image build (their measurements were
  re-run here where this record quotes a figure).
- Symphonia 0.6.1 through its public API only: the rustdoc of `symphonia`, `symphonia-core` and
  `symphonia-format-ogg` 0.6.1 (the pages docs.rs serves under
  <https://docs.rs/symphonia-core/0.6.1/symphonia_core/>, generated locally by `cargo doc`):
  `io::MediaSource`, `formats::{FormatReader, Track, SeekTo, SeekMode, SeekedTo}`,
  `formats::probe::Hint`, `packet::Packet`, `errors::Error`, `units::{Timestamp, Duration,
  TimeBase}`, `codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions,
  well_known}`, `audio::GenericAudioBufferRef`, `meta::{Metadata, MetadataRevision, Tag,
  StandardTag}`. No Symphonia source file was opened.
- crates.io: <https://crates.io/api/v1/crates/symphonia/0.6.1> (features, MPL-2.0, published
  2026-08-13, read by the goal's research); <https://crates.io/api/v1/crates/extended> (MIT; the
  newest is 0.2.0 of 2026-09-16, Symphonia 0.6.1 asks for 0.1.0).
- MPL-2.0, <https://www.mozilla.org/en-US/MPL/2.0/>, sections 1.7, 1.10, 3.1, 3.2, 3.3.
- ISO/IEC 11172-4's accuracy bounds as quoted at
  <https://www.underbit.com/resources/mpeg/audio/compliance>. The standard itself
  (<https://www.iso.org/standard/22691.html>) is paywalled and was not read.
- The Vorbis I specification, <https://xiph.org/vorbis/doc/Vorbis_I_spec.html>, sections 1.1.5
  and 1.3.2 (read by the goal's research; the sentence quoted below is from its notes).
- RFC 7845, <https://www.rfc-editor.org/rfc/rfc7845>: pre-skip, end trimming, the seek run-up,
  output gain, mapping family 0.
- Julius O. Smith, "Digital Audio Resampling Home Page",
  <https://ccrma.stanford.edu/~jos/resample/>, the page "Theory of Ideal Bandlimited
  Interpolation"; and "Spectral Audio Signal Processing", "Kaiser Window",
  <https://ccrma.stanford.edu/~jos/sasp/Kaiser_Window.html>.
- Not read, and marked `ASSUMED` where used: the ADTS header layout (ISO/IEC 13818-7; held by
  the `aac-adts` fixture, which ffmpeg wrote), the MP4 box layout (held by the two ALAC
  fixtures), Apple's `ALACSpecificConfig` layout (held by the 16-bit and 24-bit ALAC fixtures),
  RFC 3551's L16 byte order, ITU-R BS.775's downmix coefficients, Kaiser's design formulas.
- Permissive source read: `third_party/opus/src/opus_decoder.c` (libopus 1.6.1, BSD-3-Clause),
  the output-gain path, after a test measured it clipping (decision 6).
- conda-forge package metadata in the reference prefix's `conda-meta/*.json` (versions, builds,
  licences, sha256): ffmpeg, lame, mpg123, libflac, libvorbis, libsndfile, libogg, libopus,
  opus-tools.
- Programs run as programs only: ffmpeg 9.0.2 (the LGPL build), lame 4.0, mpg123 1.33.7, flac
  1.5.0, opusdec (opus-tools 0.2), sndfile-convert (libsndfile 1.2.2), zig 0.16.0. No GPL or
  LGPL source was opened (no FFmpeg, mpg123, LAME or libsndfile source).

## Decisions

1. **The decoder set.** One crate, `chorus-decode`, pure (bytes in, PCM out, no socket, no
   thread, no clock).

   | Format | Implementation | Version |
   |---|---|---|
   | MP3 | `symphonia-bundle-mp3` (feature `mp3`; Layers I and II stay off) | 0.6.1 |
   | FLAC | `symphonia-bundle-flac` | 0.6.1 |
   | Ogg Vorbis | `symphonia-format-ogg`, `symphonia-codec-vorbis` | 0.6.1 |
   | Ogg Opus | `symphonia-format-ogg` for the container, then libopus through `crates/opus-sys` | 0.6.1, libopus 1.6.1 |
   | ALAC in MP4 | `symphonia-format-isomp4`, `symphonia-codec-alac` | 0.6.1 |
   | WAV / LPCM | `symphonia-format-riff`, `symphonia-codec-pcm` | 0.6.1 |
   | raw L16 | chorus's own (`Decoder::open_l16`) | |

   `symphonia = "=0.6.1"` with `default-features = false` and the features `mp3 flac vorbis alac
   pcm wav ogg isomp4 id3v1 id3v2`. AAC, MKV, ADPCM and CAF are not compiled. SIMD (`opt-simd`,
   five more crates) stays off until a measurement asks for it.

2. **MPL-2.0 for eight more Symphonia crates, those crates only (K26, K95).** `symphonia`,
   `symphonia-bundle-mp3`, `symphonia-codec-alac`, `symphonia-codec-pcm`,
   `symphonia-codec-vorbis`, `symphonia-format-isomp4`, `symphonia-format-ogg` and
   `symphonia-format-riff`, each 0.6.1, used unmodified from crates.io, beside the four ADR 0044
   admitted. What MPL-2.0 asks, in its own words:
   - Section 1.10, Modifications: "any file in Source Code Form that results from an addition
     to, deletion from, or modification of the contents of Covered Software; or any new file in
     Source Code Form that contains any Covered Software." chorus changes no Symphonia file and
     copies none into its own, so it makes no Modifications.
   - Section 3.1: "All distribution of Covered Software in Source Code Form, including any
     Modifications that You create or to which You contribute, must be under the terms of this
     License." The copyleft binds Symphonia's own files.
   - Section 3.3, with 1.7 (a Larger Work is "a work that combines Covered Software with other
     material, in a separate file or files, that is not Covered Software"): "You may create and
     distribute a Larger Work under terms of Your choice, provided that You also comply with
     the requirements of this License for the Covered Software." chorus stays MIT OR Apache-2.0.
   - Section 3.2, the obligation that does bind a chorus binary: "such Covered Software must
     also be made available in Source Code Form, as described in Section 3.1, and You must
     inform recipients of the Executable Form how they can obtain a copy of such Source Code
     Form by reasonable means in a timely manner, at a charge no more than the cost of
     distribution to the recipient".

   Where the source is: each crate at `https://crates.io/crates/<name>/0.6.1`, checksum in
   `Cargo.lock`. A release attaches every `.crate` and names it in its notes
   (`tools/release.sh`, whose `cargo tree` query already covers the server and now finds
   twelve); the image names each crate and version in
   `/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md`, which `make image` checks against the build.
   `deny.toml` carries one exception per crate, each citing this record. Not legal advice.

3. **`extended` 0.1.0 (MIT)** enters with `symphonia-format-riff` (80-bit floats in AIFF-style
   headers). MIT is on the allowlist; no exception. It is the only new non-Symphonia crate, and
   there is no second version of any crate (`cargo deny check bans` in the gate).

4. **What "match" means, per format.** `crates/decode/tests/reference_decodes.rs` against
   `fixtures/decode` (22 fixtures, 50 files, 895,476 bytes; `fixtures/README.md` has each
   reference program's version, build and package sha256):
   - **WAV, FLAC, ALAC: bit-exact.** The integers recovered from the `f32` output hash to the
     sha256 of the reference decode (`flac -d` 1.5.0; ffmpeg 9.0.2's ALAC decoder; the WAV's own
     samples), at 16 and at 24 bits, at 44.1 and 48 kHz, and the frame count is exact.
   - **Opus: bit-exact and conformant.** Equal, sample for sample at 24 bits, to the decode of
     libopus 1.6.1 as chorus builds it made outside the crate (the generator's own Ogg page
     reader and `tools/decode-fixtures/opus_pkt.c`, with pre-skip and end trim applied per RFC
     7845), and libopus's opus_compare passes the decode against opusdec's (a float build of the
     same libopus): measured "Opus quality metric" 99.5 % and 96.1 % on the two fixtures.
   - **MP3 and Vorbis: within ISO/IEC 11172-4's full-accuracy bounds** of the committed
     reference decode: "the rms level of the difference signal between the output of the decoder
     under test and the supplied reference output is less than 2^-15/sqrt(12)" and "the
     difference signal shall have a maximum absolute value of at most 2^-14 relative to
     full-scale", outputs "normalized to be between -1.0 and +1.0" (as quoted at
     underbit.com, above). Measured: MP3 against mpg123 1.33.7, rms 1.07e-7 and 1.59e-7 (bound
     8.81e-6), max 8.6e-7 and 1.03e-6 (bound 6.10e-5); Vorbis against libvorbis 1.3.7 through
     `sndfile-convert`, rms 4.6e-8 and 5.9e-8, max 2.1e-7 and 3.0e-7. Two limits, stated: the
     MP3 reference is mpg123's decode of chorus's own signals, not the standard's bitstreams
     and supplied outputs; and for Vorbis the bound is chorus's own choice, because the Vorbis I
     specification sets no numeric bound (it asks that a decoder be "entirely mathematically
     equivalent to the specification", section 1.3.2).
   - **Gapless pairs** (MP3, Vorbis, Opus, FLAC): one second of signal cut in two and encoded
     as two tracks decodes, joined, to exactly the original frame count. FLAC: the joined
     decode is the original bit for bit. Lossy: within 2048 frames of the join the error
     against the original is under 3 times the whole pair's rms error and under 0.12 at any
     sample. Measured (join rms / whole rms, join max): MP3 1.08e-2 / 1.03e-2, 8.8e-2; Vorbis
     4.4e-3 / 2.6e-3, 3.0e-2; Opus 7.5e-3 / 3.8e-3, 5.1e-2. The research proposed 0.06 for the
     max; MP3 at 128 kbit/s measures 0.088 on this signal, so the bound is 0.12 (the two
     halves are encoded with no knowledge of each other; the join is where each knew least). The test also shows the bound has teeth: the same halves
     joined 32 frames short fail it.
   - **A comparator that can fail**: `a_corrupted_reference_fails_the_comparison` corrupts the
     reference four ways (one sample, an offset, a one-frame shift, a short reference) and one
     bit of a lossless decode; each fails.
   - **Every fixture is read**: `every_fixture_is_read_by_a_test` fails on a file in
     `fixtures/decode` that no test lists.

5. **Refusals are named.** `unsupported: aac` for an MP4 AAC track (the demuxer reports the
   codec; no decoder exists to hand it to), for an ADTS stream (sniffed before the probe), and
   for unrecognised bytes a sender calls AAC. `unsupported: mp4 with moov at the end on a
   non-seekable source` when the first boxes show `mdat` before `moov` and the media cannot
   seek; on seekable media the same file plays. `unsupported: opus mapping family <n>` for
   n other than 0 (ADR 0044, decision 7).

6. **The Opus output gain is applied by chorus-decode, not by libopus.** A test set a gain of
   -6.02 dB in a fixture's OpusHead and measured the decode at 0.022 of its level, not 0.5. The
   cause is in libopus 1.6.1's fixed-point path with `ENABLE_RES24` (chorus's build, ADR 0044
   decision 3): `src/opus_decoder.c` applies the gain and then `pcm[i] = SATURATE(x, 32767)`
   on samples that are 24-bit there, so any non-zero gain clips at -48 dBFS. chorus-decode
   passes gain 0 to libopus and multiplies the `f32` output by `10^(gain/(20*256))` (RFC 7845
   section 5.1: "players and media frameworks SHOULD apply it by default"); with gain 0 the
   samples stay libopus's own, which is what the bit-exact test holds. The vendored tree is
   not edited (byte for byte upstream's). **Open, outside this record's files:**
   `crates/client-linux/src/decode.rs` and the firmware still hand a `stream_format`'s gain to
   `OPUS_SET_GAIN`; the server sends no Opus yet, so nothing reaches it today.

7. **Seeking.** Lossless formats land on the frame exactly. Lossy ones are decoded from 4096
   frames before the target and the run-up is dropped (RFC 7845 asks at least 3840 for Opus);
   measured on the fixtures, MP3 and Vorbis then equal the straight decode and Opus is within
   rms 6.8e-3 of it.

8. **The resampler** is chorus's own: bandlimited interpolation with a Kaiser-windowed sinc
   (Smith: "the lowpass cutoff must be placed below half the new lower sampling rate"), stored
   as one row of taps per output phase, 96 zero crossings a side, designed for 110 dB, the
   stopband edge at the lower rate's Nyquist frequency; ratios with more than 1024 phases
   interpolate between rows. The output is aligned with the input and `n` input frames give
   exactly `ceil(n * to / from)` output frames, so gapless lengths survive it. Measured
   (`docs/measurements/resampler-quality.md`, Source: synthetic) for 44.1 to 48, 96 to 48, 48 to
   44.1 and 8 to 48 kHz: passband ripple at most 0.00002 dB up to 0.45 of the lower rate,
   images and aliases at most -110.5 dB, sweep error at most -129 dB; the tests hold 0.01 dB
   and -100 dB. Why build it (BRIEF section 3.2): about 200 lines, no dependency, and the
   property the players need (exact, time-aligned lengths) is a design choice a general crate
   would not promise. The Kaiser design formulas are `ASSUMED` from memory; the measurement,
   not the formula, is what is held.

9. **`chorus-server --probe-media <path>`** decodes one local file and prints what it is; it
   runs before any thread or socket. It is how the image test runs the decoders inside the
   released binary.

10. **The image: zig is the C compiler of the musl target; rustc still links.**
    `tools/image.sh` sets `CC_x86_64_unknown_linux_musl` to `tools/zig-musl-cc.sh`
    (`zig cc -target x86_64-linux-musl`, zig at the `core:zig` pin of `mise.toml`, 0.16.0),
    and the two `cargo build --target x86_64-unknown-linux-musl` lines stay as they were. On
    fitness:
    - The binary stays the kind it was: a static position-independent executable over rustc's
      self-contained musl, with only the C objects coming from the new tool. The image test
      now holds that (`readelf`: type DYN, no NEEDED, no INTERP), and decodes one fixture of
      every format with the unpacked binary, so the zig-compiled libopus is run, not only
      linked (the Opus hashes equal the host build's).
    - Headers and libc agree: zig 0.16.0 carries musl 1.2.5's headers and rustc 1.98.1 links
      musl 1.2.5 (the goal's image research read both version strings; not re-read here).
    - The pin exists and is held: zig's version and sha256 are in `mise.toml` and `mise.lock`,
      checked by `check-pins.sh`; nothing new is pinned.
    - `crates/opus-sys/build.rs` no longer emits `rustc-link-lib=m` when the target
      environment is musl: musl has no separate libm, and asking for one made the link driver
      reach for the host's glibc `libm.a` (the research's measured failure).
    - `tools/release.sh` builds the bare server binary with the same four variables.
    - `deploy/Dockerfile` needs no musl C compiler: it is a glibc build (`rust:1.98.1-slim-bookworm`
      to `debian:bookworm-slim`) and already compiled libopus for the client with the base
      image's own compiler. It now also copies the two notice files. What is verified is what
      `tools/image.sh` already verified, that its build context compiles (`cargo check
      --locked --workspace --bins`); **a `docker build` of it was not run here (no daemon) and
      is unverified.**

11. **Notices in the image.** libopus's BSD-3-Clause licence asks binary redistributions to
    reproduce it: `/usr/share/doc/chorus/libopus-COPYING` is `third_party/opus/COPYING`.
    `/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md` (`deploy/THIRD-PARTY-NOTICES.md` plus the
    crate list of the build) says where Symphonia's source is. The image test checks both.

12. **Binary size (P9's open input).** `chorus-server`, `x86_64-unknown-linux-musl`, release:

    | | bytes | stripped | OCI tarball |
    |---|---|---|---|
    | before: origin/main `e8d36c9` | 3,772,832 | 3,023,496 | 2812 KiB |
    | after: branch commit `5481a70` | 5,866,704 | 4,656,704 | 3652 KiB |

    The decoders cost 2,093,872 bytes unstripped, 1,633,208 stripped, 840 KiB of tarball.
    Commands, 2026-10-03: before, in a worktree of origin/main, `cargo build --release --locked
    --target x86_64-unknown-linux-musl -p chorus-server --bin chorus-server`, then `stat -c %s`
    and `strip -o`, and `bash tools/image.sh` for the tarball; after, `make image`, whose test
    prints the two sizes and the tarball's. The image ships the unstripped binary, as before.

## Options not chosen

- **cargo-zigbuild for the image** (the endpoint package's tool, ADR 0069): it builds and runs,
  but zig then links too, and the research measured the result as a fixed-address `EXEC`, not a
  PIE: address-space randomisation of a network-facing server's main image would be lost.
- **A glibc distroless base** (`base-nossl-debian12` or `cc-debian12`): 5.0 to 8.5 MB more
  compressed base (the research's `crane manifest` figures) for nothing the server uses, a
  glibc floor to hold with the same zig anyway, and an image test that could no longer show
  the binary runs on the image's own libc.
- **A musl gcc from musl.cc**: one unversioned URL on one person's site; a digest of a mutable
  URL is not a pin chorus can keep.
- **The `opus` crate** (P9's wording) and **ropus** (P9's Option B): as ADR 0044 decided, a
  second libopus built by cmake, or a port, where the server can run the code the endpoints
  run and the fixtures hold both to one output.
- **`OPUS_SET_GAIN` for the output gain**: clips in chorus's libopus build (decision 6).
- **A resampling crate**: see decision 8.
- **Committing reference decodes for the lossy gapless halves**: about 0.5 MB more; the frame
  count and the join bound against the original hold what matters there.
- **0.5 s signals with every reference committed**: over 1.4 MB; the signals are 0.3 s (tone,
  sweep) and 0.5 s halves, and the directory stays under 1 MB.

## Consequences

- `make tier-fast` gains 22 integration tests in `crates/decode` (about 1 s for the reference
  decodes and 3.6 s for the resampler measurement, wall clock in the dev profile, measured
  2026-10-03) and 4 unit tests in the server.
- `make image`, `tools/release.sh` and CI's image step need mise's zig installed, as the
  endpoint packages already do.
- The measurement report names a branch commit; after the squash merge it must be re-pointed
  to the merge commit (rule 11), as goal 6's was.
- An upgrade of Symphonia or libopus is its own commit that keeps `fixtures/decode` and
  `fixtures/codec` green.
- Not done here, and named so nobody assumes it: no HTTP, no player thread, no wire encoder
  (other tracks of goal 16); `remix`'s downmix coefficients and channel order are `ASSUMED`
  and unmeasured against a multichannel reference; no fixture has more than two channels, an
  MP3 without a Xing/LAME header, or a non-zero Opus output gain made by an encoder.
