# Decoders

What chorus-server decodes, with what, and what it refuses (goal 16). The decision is proposal
P9 (`docs/proposals/P9-decoders.md`, Option A, decision K79); the record of how it was built is
`docs/decisions/0122-the-server-decoders.md`. The code is `crates/decode` (`chorus-decode`): a
pure library, bytes in and PCM out, with no socket and no thread.

Endpoints never see any of this: the server decodes every input and sends PCM (K62; FLAC and
Opus on the wire are goal 6's and are decoded on the endpoint, ADR 0044).

## Formats

| Format | Container | Decoder | Licence | Match against a reference decode |
|---|---|---|---|---|
| MP3 (MPEG-1/2 Layer III) | bare frames, ID3v1/ID3v2 tags | Symphonia 0.6.1 | MPL-2.0 | within ISO/IEC 11172-4 full accuracy of mpg123 1.33.7 |
| FLAC | FLAC | Symphonia 0.6.1 | MPL-2.0 | bit-exact (`flac -d` 1.5.0) |
| Vorbis | Ogg | Symphonia 0.6.1 | MPL-2.0 | within the same bounds of libvorbis 1.3.7 |
| Opus | Ogg (RFC 7845), mapping family 0 | Symphonia's Ogg demuxer, then libopus 1.6.1 (`crates/opus-sys`, the endpoints' code) | MPL-2.0, BSD-3-Clause | bit-exact against the same libopus build, and opus_compare against opusdec |
| ALAC | MP4 (`.m4a`) | Symphonia 0.6.1 | MPL-2.0 | bit-exact (ffmpeg 9.0.2's ALAC decode) |
| WAV / LPCM | RIFF WAVE | Symphonia 0.6.1 | MPL-2.0 | bit-exact |
| raw L16 | none (`audio/L16;rate=..;channels=..`) | `Decoder::open_l16` | chorus | exact by construction |

`crates/decode/tests/reference_decodes.rs` holds every row to `fixtures/decode` on every
`cargo test`; `fixtures/README.md` says where each reference came from.

The decode is interleaved `f32`, full scale 1.0, at the media's own rate and channel count.
Sources of up to 24 bits are exact in it. `Resampler` (a windowed-sinc polyphase resampler,
measured in `docs/measurements/resampler-quality.md`) and `remix` take it to a stream's rate and
channels.

Gapless: an encoder's delay and padding are trimmed before a frame leaves the decoder (MP3 from
the Xing/LAME header, Vorbis and Opus from the Ogg granule positions and the Opus pre-skip), so
two tracks cut from one recording decode to exactly the recording's length. An MP3 with no
Xing/LAME header carries no such figures and is played as it is.

Tags: title, artist and album, from ID3, Vorbis comments, FLAC blocks, MP4 atoms and RIFF INFO.
A chained Ogg stream (an Icecast mount starting a new title) keeps decoding across the link
with the new link's tags.

## What is refused, by name

| Input | Refusal | Why |
|---|---|---|
| AAC, in MP4 or as ADTS, or bytes nothing recognises that the sender calls `audio/aac` | `unsupported: aac` | Not compiled, in any build: its patents are live and pooled (P9, "AAC stays off"). |
| MP4 whose `moov` box is after its audio, on media that cannot seek | `unsupported: mp4 with moov at the end on a non-seekable source` | The demuxer needs the index first. On seekable media (a file, an HTTP server that honours Range) the same file plays. |
| Ogg Opus with a channel mapping family other than 0 | `unsupported: opus mapping family <n>` | Families 1 and 255 need libopus's multistream decoder, which chorus does not build (ADR 0044, decision 7). |
| MPEG Layer I and II, AC-3, E-AC-3, anything else | `unsupported: <what>` | Outside the settled set. |
| A seek on media that cannot seek | `unsupported: seek on a non-seekable source` | |

## Asking a build what it decodes

```
chorus-server --probe-media <path>
```

decodes one local file to its end and prints one line: codec, rate, channels, bits, frames, the
tags, and the FNV-1a 64 of the decoded samples as little-endian `f32`. It starts nothing (no
thread, no socket) and exits 0; a file the build does not decode exits 1 naming why. `make image`
runs it inside the unpacked image on one fixture of every format and on both AAC files.

## The image

libopus is C, so the static musl server now has C in it. `tools/image.sh` compiles it with the
pinned zig (`tools/zig-musl-cc.sh`) and leaves the link to rustc, so the binary is still a
static position-independent executable, which the image test checks. Sizes of `chorus-server`
for `x86_64-unknown-linux-musl`, release profile, measured 2026-10-03 (ADR 0122 has the
commands):

| | bytes | stripped | OCI tarball |
|---|---|---|---|
| before (origin/main `e8d36c9`, no decoders) | 3,772,832 | 3,023,496 | 2812 KiB |
| with the decoders (branch commit `5481a70`) | 5,866,704 | 4,656,704 | 3652 KiB |

The image carries the notices its binary needs: `/usr/share/doc/chorus/libopus-COPYING` and
`/usr/share/doc/chorus/THIRD-PARTY-NOTICES.md` (where Symphonia's MPL-2.0 source is).
