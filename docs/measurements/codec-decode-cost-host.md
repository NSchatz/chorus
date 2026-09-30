# FLAC and Opus decode cost and decoder size, on a development host

Date: 2026-09-30
Source: host
Build measured: `23ae1e7622cd4dfd19caebba6b23a6acbb8c2978`
Build note: a commit of branch `chorus-g6/codecs` (PR #44); when it is squash-merged, the merge
commit carries the same decoder code and this line is re-pointed at it.
Timing evidence: none. These are wall-clock figures from a shared development container, not
the ESP32-S3; only a `hardware` report is timing evidence (BRIEF.md section 3.1 rule 3). They
answer the open input P9 and P1 left to goal 6 ("Endpoint flash cost of dr_flac and libopus
fixed-point ... goal 6 measures it") as far as a host can: the order of magnitude, and a baseline
the device run of goal 8 is compared with. Nothing here is a claim about the S3.

## The machine

```
kernel     Linux 6.12.107+deb13-amd64 x86_64
CPUs       56 visible, shared with other tenants (a container on a shared host)
compiler   cc (Debian 14.2.0-19) 14.2.0, -std=c11 -O2 -ffp-contract=off -fno-fast-math
decoders   dr_flac 0.13.3 (DR_FLAC_NO_SIMD), libopus 1.6.1 fixed point
           (third_party/opus/chorus-build.txt), both through firmware/src/codec*.c
```

## Decode cost

`firmware/build/test_codec firmware/build/opus_compare --bench` (built by `make -C firmware
build/test_codec build/opus_compare`, run under the repository's heavy-job lock): each fixture in
`fixtures/codec` decoded end to end through the endpoint's seam (open, every chunk, close) about
20 s of audio per run, five runs, the best run kept. Wall-clock time from `CLOCK_MONOTONIC`, per
second of audio decoded.

| fixture | content | ms per second of audio |
|---|---|---|
| flac-s16-stereo-44k1 | FLAC, 16-bit stereo, 44.1 kHz, 4096-frame blocks, `-5` | 1.769 |
| flac-s24-stereo-96k | FLAC, 24-bit stereo, 96 kHz, 4096-frame blocks, `-8` | 4.014 |
| flac-s16-6ch-48k | FLAC, 16-bit six channels, 48 kHz, 1152-frame blocks, `-5` | 4.948 |
| opus-tv02-silk-mono | Opus SILK (RFC 8251 vector 02), decoded mono | 0.898 |
| opus-tv05-hybrid-stereo | Opus hybrid (vector 05), decoded stereo | 3.464 |
| opus-tv10-celt-stereo | Opus CELT, mixed frame sizes (vector 10), decoded stereo | 3.695 |

On this host every stream decodes in well under 1 % of real time. The S3's figure is not
derivable from these (a different ISA, clock and memory system); goal 8 measures it on the
device with the P1 target.

## Size

Host (x86-64, the same flags plus `-ffunction-sections -fdata-sections`, linked with
`--gc-sections`): a program that opens and decodes through the seam (both codecs reachable)
against the same program without the decoders. The probe's `main` opens a decoder with
`chorus_codec_open`, decodes one chunk with `chorus_codec_decode`, reads
`chorus_codec_final_range` and closes it, choosing the codec from `argc` so neither can be
folded away; the baseline's `main` prints a number. Both link glibc dynamically, so the figures
are the program's own sections.

| build | text | data | bss |
|---|---|---|---|
| baseline, `-O2` | 1,302 | 576 | 8 |
| both decoders reachable, `-O2` | 191,715 | 1,744 | 8 |
| baseline, `-Os` | 1,302 | 576 | 8 |
| both decoders reachable, `-Os` | 152,801 | 1,736 | 8 |

So the seam plus dr_flac plus the libopus decoder is about 190 KB of x86-64 code at `-O2` and
150 KB at `-Os`. Unlinked objects, for reference: dr_flac.o 68,413 bytes, the 64 libopus
objects 154,075 bytes (`size -t`, `-O2 -g`).

ESP32-S3 image (ESP-IDF v6.1, the gate's configuration, `idf.py size`; a build output, not a
device run): `main` at `8eca054` (the endpoint's v2 session, no decoders) against this branch at
`23ae1e7` (the same session with `coded_chunk` decoded through the seam, so both decoders are
linked).

| | main `8eca054` | with the decoders `23ae1e7` | difference |
|---|---|---|---|
| `chorus-endpoint.bin`, bytes | 923,392 | 1,062,864 | +139,472 |
| Flash Code | 660,830 | 774,894 | +114,064 |
| Flash Data (`.rodata`) | 153,304 | 178,664 | +25,360 |
| DIRAM `.bss` | 174,520 | 209,080 | +34,560 |
| DIRAM `.data` | 21,847 | 21,927 | +80 |
| DIRAM used, of 341,760 | 267,234 (78.19 %) | 301,874 (88.33 %) | +34,640 |

The `.bss` growth is exactly the session's one static decode buffer (5760 frames, two channels,
three bytes: 34,560 bytes, `firmware/src/session.c`); the decoders' own state is allocated when
a stream opens and is not in these figures. The image outgrew ESP-IDF's default 1 MB app
partition (by 14,288 bytes), which is why the endpoint now uses the 1.5 MB single-app table
(ADR 0044). 88 % of DIRAM in use before any jitter buffer exists is a figure goal 8 has to plan
against (PSRAM is present, but rule 3 of `firmware/endpoint-units.conf` keeps DMA buffers out of
it).
