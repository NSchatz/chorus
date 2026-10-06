# P9: Decoders per format

- Decisions: K79
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- Outcome: approved at Checkpoint K, Option A (K79; `docs/decisions/0044-the-vendored-decoders.md`, `0122-the-server-decoders.md`) (recorded 2026-10-06)
- If deferred: MP3, FLAC, Vorbis, Opus, ALAC and WAV as recommended; AAC off
- Builds on: goal 4 (§8, release notes say where the source of any MPL-licensed dependency is), goal 6 (§10, FLAC and Opus on the C endpoint and in the Linux client "per P9"), goal 16 (§20, "the settled formats and implementations", the MPL ADR and allowlist exception, `make image` green with any C decoder), goal 17 (§21, stored alarm stream URLs), goal 20 (§24, HA announcements and TTS through the ducking mixer)

## Question

Which audio formats does chorus decode, with which implementations, under which licences and
patent positions, and where does decoding run? K79: "goal 1 proposes per format (MP3, FLAC, Opus,
Vorbis, ALAC, WAV, AAC) the patent status under K60's strict bar, licence (MIT/Apache/BSD/public
domain vs MPL), vendor vs write, and size; needed for UPnP/DLNA renders, HA announcements/TTS and
alarm stream URLs even with K64's inputs-only scope. Not chosen as final: vendor permissive libs;
Symphonia; write clean-room."

Bounding decisions, quoted:

- K60: "Strict: open only: no receiver that violates a service's terms or reverse-engineers a DRM
  or authentication handshake". The brief's §5 table applies this strict bar to decoders and adds
  "AAC's patent position".
- K62: "the speaker protocol gains FLAC (lossless, for Wi-Fi) and Opus (lossy, weak Wi-Fi/many
  rooms) alongside PCM, with decoders on the C endpoint and Linux client (vendoring permissive codec
  libraries allowed per BRIEF §3.2 and K26's licences)".
- K26 (as refined): "MIT OR Apache-2.0 ... K95 lets a dependency with another licence in by ADR,
  e.g. MPL-2.0 Symphonia per P9."
- K64: "Nothing; inputs only". K80: a stored stream URL is "played through the UPnP decoder path".

## Constraints that bind every option

- **Licence allowlist:** MIT, Apache-2.0, BSD, ISC, Zlib, public domain. Anything else (MPL-2.0,
  CC0, MIT-0 read strictly) needs an ADR (K26, K95) and a narrow cargo-deny exception (goal 16).
  dr_libs offer "Public Domain (www.unlicense.org)" as their first alternative, which is on the
  list. CC0 (minimp3) is a public-domain dedication whose clause 4(a) says "No trademark or patent
  rights held by Affirmer are waived, abandoned, surrendered, licensed" [L6]; it is treated here as
  public domain for copyright, with the patent question answered per format below.
- **Patents are their own axis:** K60's text is about service terms and DRM handshakes; no decoder
  here touches either. The strict bar is applied to patents too: a format whose patents are live
  and pooled for a fee is off unless the owner decides otherwise. Not legal advice.
- **Where decoding runs (K62, K95):** chorus-server decodes every input format; endpoints only ever
  receive PCM, FLAC or Opus, so the C endpoint and the Linux client decode FLAC and Opus only.
- **Shared fixtures (CLAUDE.md rule 5):** every wire codec (FLAC, Opus) is checked against the same
  fixtures in Rust and C, so the two sides cannot drift.
- **BRIEF §3.2:** "An audio codec: vendor if ever needed." Codecs are gray-zone platform, not the
  project; writing one needs a reason beyond instruction.
- **The image:** chorus-server ships as a static musl binary in an OCI tarball (§0.11). A C decoder
  in the server needs a musl C cross compiler in the build (a musl gcc, `zig cc` through
  cargo-zigbuild, or a glibc distroless base), chosen by ADR in goal 16. Rule 8: the fact that no
  musl C compiler or cmake is on PATH today is not a reason for or against any option; the
  question is what the build must carry to stay reproducible.
- **Clean-room:** faad2 (GPL), FAAC (LGPL), FFmpeg and mp3lame (LGPL) are never opened or used.

## Re-verification of the planning research

`research-casting-decoders.md` §4 and `verify-ha-casting.md` rows 4, 11 and 12 (2026-09-29), re-read
on 2026-09-30:

| Claim | Re-check 2026-09-30 | Changed? |
|---|---|---|
| HA TTS default format is MP3 | HA core 2026.9.3 `tts/__init__.py` line 107 `_DEFAULT_FORMAT = "mp3"`; `ATTR_PREFERRED_FORMAT = "preferred_format"` line 99 [F1] | No |
| MP3 US patents expired April 2017 (Wikipedia) | **Primary sources added:** Google Patents US6009399 (Deutsche Thomson Brandt) "Expired - Lifetime", 2017-04-16 [M1]; Fraunhofer IIS: "On April 23, 2017, Technicolor's mp3 licensing program for certain mp3 related patents and software of Technicolor and Fraunhofer IIS has been terminated." [M2] | Stronger evidence |
| AAC needs a Via LA licence for decoder products; per-unit fees from US$0.98 | Via LA [A1]: "Manufacturers or developers of end-user encoder and/or decoder products" need a licence; "License fees are due on the sale of encoders and/or decoders only. There are no patent license fees due for the distribution of bit-streams encoded in AAC."; US$0.98 (1-500,000 units) down to US$0.10 (over 75,000,000); multichannel counts 1.5 units; covers AAC-LC, HE-AAC, HE-AAC v2, xHE-AAC, AAC-LD, AAC-ELD; five-year renewable term; a one-time initial fee of US$15,000 (US$1,000 for small entities) is due on signing. The page states **no exemption for free or open-source software** | No; the free-software question is now answered as far as the page goes |
| Baseline AAC patents to about 2028, extensions to about 2031 | Not re-fetched; verify-ha-casting.md row 11 confirmed the Wikipedia sentence "Based on the list of patents from the SEC terms, the last baseline AAC patent expires in 2028 ... 2031" (secondary) [A2] | Via LA's 2026 Q2 patent list [A3] names 586 US patents, numbered up to US 12,573,411 (a 2026 grant, `ASSUMED` from USPTO numbering), so the pool runs well past 2031; which patents read on AAC-LC decoding is unknown (no dates in the list), and the 2028 date stays secondary |
| Symphonia MPL-2.0, 0.6.1, no Opus decoder | crates.io [C1]: symphonia 0.6.1 (2026-08-13), MPL-2.0, rust_version 1.85; every per-format crate 0.6.1 MPL-2.0; `symphonia-codec-opus` "does not exist". README [C2]: AAC-LC "Great", no gapless, not default; ALAC "Great", gapless; FLAC, MP3, PCM, Vorbis "Excellent", gapless; HE-AAC and HE-AAC v2 no status; Opus row lists a crate that is not published. Default features: `opt-simd`, `all-meta`, `adpcm`, `flac`, `mkv`, `ogg`, `pcm`, `vorbis`, `wav` | No |
| `opus` crate MIT/Apache over libopus via opusic-sys (BSD-3) | crates.io [C1]: `opus` 0.4.0 (2026-08-23) "MIT/Apache-2.0", depends on `opusic-sys ^0.7.3`; opusic-sys 0.7.5 (2026-08-06) BSD-3-Clause, default feature `bundled` = `dep:cmake` (it builds libopus from source with cmake) | **Added:** the default build needs cmake and a C compiler for the target |
| Pure-Rust Opus options are young | crates.io [C1], [C3]: `ropus` 0.12.18 (2026-05-11), BSD-3-Clause, "Rust port of the xiph Opus audio codec (fixed-point), bit-exact against the reference", encoder and decoder, "No external C toolchain required", Rust 1.88, 5,444 downloads; `opus-decoder` 0.1.1 MIT OR Apache-2.0, decoder only, 156,383 downloads | **Added:** a pure-Rust encoder-plus-decoder now exists (young) |
| libopus BSD-3 with royalty-free patent licences | libopus COPYING [L1]: BSD-3 text, then "Opus is subject to the royalty-free patent licenses which are specified at" IETF IPR 1524 (Xiph), 1914 (Microsoft), 1526 (Broadcom). IPR 1524 [O1]: "perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable", terminating for anyone who sues over an implementation (defensive). RFC 6716 [O2]: September 2012, Proposed Standard, three IPR disclosures listed | No |
| dr_flac public domain or MIT-0 | dr_libs README "Public domain, single file audio decoding libraries"; dr_mp3.h footer "ALTERNATIVE 1 ... unlicense.org", "ALTERNATIVE 2 - MIT No Attribution" [L2] | No |
| FLAC royalty-free, no known patents (ASSUMED) | **Primary source added:** RFC 9639 "Free Lossless Audio Codec (FLAC)", December 2024, Proposed Standard, no IPR declarations on its datatracker page [FL1]; libFLAC COPYING.Xiph is BSD-3 [L3] | Upgraded from ASSUMED |
| Vorbis royalty-free (ASSUMED) | Xiph: "a fully open, non-proprietary, patent-and-royalty-free, general-purpose compressed audio format" [V1]; libvorbis COPYING BSD-3 [L4] | Upgraded from ASSUMED (a self-statement by the format's owner) |
| ALAC: Apple's reference decoder Apache-2.0 (patent clause ASSUMED) | macosforge/alac LICENSE is Apache-2.0 [L5]; its §3: "each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable ... patent license to make, have made, use, offer to sell, sell, import" for claims "necessarily infringed by their Contribution(s)". Repo archived, last push 2020-07-29 (GitHub API) | Patent clause now quoted, not ASSUMED |
| minimp3 CC0 | minimp3 LICENSE is CC0 1.0, with the patent carve-out quoted above [L6] | Added the carve-out |

Adversarially verified 2026-09-30 (goal-1 verifier 1): 11 claims confirmed, 1 refuted, 1 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Where compressed audio enters (from the planning research, re-checked where marked)

- UPnP AV renders: whatever a control point sends (MP3, FLAC, AAC/M4A, ALAC, Ogg, WAV); the
  renderer's `GetProtocolInfo` Sink list decides what control points offer (UPnP ConnectionManager,
  research-casting-decoders.md [R14], not re-read). A control point that cannot send a listed
  format may transcode (BubbleUPnP, [R19], not re-read).
- HA TTS and announcements: MP3 by default; the chorus integration may ask for `flac` or `wav`
  through `preferred_format` [F1].
- Stored alarm URLs (K80): internet radio as MP3, AAC/HE-AAC, Ogg Vorbis or Opus over HTTP, and
  HLS (AAC in TS or fMP4) (RFC 8216, research [R37]); the share of each is `ASSUMED`.
- Spotify Soloist delivers PCM through PipeWire; no decoder is involved.

## Per format

| Format | Patent status (US) | Server candidates (Rust) and licence | Endpoint (C) | Proposed |
|---|---|---|---|---|
| WAV / LPCM | None known (`ASSUMED`: LPCM is not a coded format) | Symphonia `wav`/`pcm` (MPL-2.0); `hound` 3.5.1 Apache-2.0 (2023); own RIFF parser (small) | not on the wire as WAV; PCM is native | On |
| MP3 | Expired: US6009399 2017-04-16 [M1]; licensing programme ended 2017-04-23 [M2] | Symphonia `mp3` (MPL-2.0, "Excellent", gapless); `nanomp3` 0.1.1 MIT OR Apache-2.0 (from minimp3, 2025-08-13); `minimp3` crate MIT over CC0 C | not needed (K62) | On |
| FLAC | RFC 9639, no IPR declarations [FL1] | Symphonia `flac` (MPL-2.0, "Excellent", gapless); `claxon` 0.4.3 Apache-2.0 (last release 2020) | dr_flac (Unlicense) or libFLAC (BSD-3) | On (also a wire codec) |
| Opus | Royalty-free IETF licences (Xiph, Microsoft, Broadcom), Xiph's with a defensive-termination clause [L1][O1][O2] | libopus via `opus` 0.4.0 (MIT/Apache-2.0) + opusic-sys (BSD-3, builds C with cmake); `ropus` 0.12.18 BSD-3 pure Rust (encoder and decoder, young); `opus-decoder` 0.1.1 MIT OR Apache-2.0 (decoder only). Symphonia has none | libopus fixed-point (BSD-3) | On (also a wire codec, and the server must **encode** it) |
| Vorbis | Declared patent- and royalty-free by Xiph [V1] | Symphonia `vorbis` (MPL-2.0, "Excellent", gapless); `lewton` 0.10.2 MIT OR Apache-2.0 (2021) | not needed | On |
| ALAC | Apple's decoder is Apache-2.0 with its patent grant [L5] | Symphonia `alac` + `isomp4` (MPL-2.0, "Great", gapless); `alac` 0.5.0 MIT/Apache-2.0 (2018) with `mp4` 0.14.0 MIT (2023) | not needed | On |
| AAC-LC, HE-AAC | **Live**: Via LA pools licences for "developers of end-user ... decoder products", per-unit fees on sale only plus a one-time initial fee on signing, no free-software exemption stated [A1]; last baseline patent about 2028, extensions about 2031 (Wikipedia, secondary) [A2], but the pool lists US patents granted as late as 2026 [A3] | Symphonia `aac` (MPL-2.0, AAC-LC only, no gapless); fdk-aac (Fraunhofer licence grants no patent rights, research [R44]); faad2 GPL (never) | not needed | **Off** (owner decides) |

What "off" means for AAC: the UPnP Sink list omits `audio/mp4` and `audio/aac`, so a control point
sends another format or transcodes; an AAC or HLS alarm URL fails with a clear error in the app,
chorusctl and HA; HA TTS is unaffected (MP3).

Does a software AAC decoder in chorus need a licence? Via LA's page says a licence is needed by
"manufacturers or developers of end-user ... decoder products" and that fees fall only on sales
[A1]. chorus is private and sold to no one (K9), so per-unit fees would be zero. The licence itself
is not free, though: 'There is an initial fee of $15,000 due upon execution of the license',
reduced to $1,000 for small entities (fifteen or fewer employees, under US$1 million revenue)
[A1]. The page covers end-user products 'when sold directly or through distribution to end users'
and says nothing about a build that is never sold or distributed. Under the strict reading,
sub-choice (ii) therefore means signing a licence at a one-time cost of at least US$1,000.
Signing one is an owner action (the contract forbids goals from creating accounts or agreements,
K4). Not legal advice.

## Options

### Option A: Symphonia for the input formats, libopus for Opus (the planning research's recommendation)

- What: one decode crate in chorus-server wrapping Symphonia 0.6.1 with features `mp3`, `flac`,
  `vorbis`, `alac`, `pcm`, `wav`, `ogg`, `isomp4` (AAC not compiled), and libopus through the
  `opus` crate for Opus decode and the wire encode. The Linux client uses the same crates for FLAC
  and Opus. The C endpoint vendors dr_flac (Unlicense) and libopus fixed-point (BSD-3), pinned by
  hash. Shared FLAC and Opus fixtures run through both sides.
- Costs:
  - Licence: one ADR for MPL-2.0 (Symphonia used unmodified as a crates.io dependency; the release
    notes say where its source is, goal 4), a cargo-deny exception for `symphonia*` only.
  - Build: libopus in the static musl server means a musl C toolchain and cmake in the image
    build (by ADR in goal 16). The endpoint builds libopus with ESP-IDF's own toolchain.
  - Effort: small; one wrapper over two libraries, fixtures, gapless trimming, HTTP stream reader.
  - Size: not measured here (see Open inputs). Published crate sources: symphonia-bundle-mp3 69 KB,
    -isomp4 54 KB, -format-ogg 33 KB, -codec-vorbis 32 KB, -bundle-flac 31 KB, -format-riff 30 KB,
    -codec-alac 15 KB, -codec-pcm 12 KB (compressed `.crate` sizes [C1], a proxy only);
    opusic-sys 10.4 MB of vendored libopus source.
- Risks: MPL-2.0 file-level copyleft only binds changes to Symphonia's own files (`ASSUMED`
  summary; the ADR quotes MPL-2.0 itself); Symphonia's AAC and Opus gaps do not matter here.
- Fit: mature (symphonia 15.1 M downloads [C1]), one API, shared gapless handling; libopus is the
  reference implementation on both sides, which makes Rust/C drift least likely.

### Option B: Symphonia for the input formats, `ropus` (pure Rust) for Opus on the server

- What: as A, but the server and Linux client use `ropus` 0.12.18 (BSD-3) for Opus encode and
  decode; the C endpoint keeps libopus. ropus claims bit-exact output against the C reference
  [C3], which the shared fixtures would check on every gate run.
- Costs: no C in the server image at all (no musl C cross compiler, no cmake); the same MPL ADR as
  A; effort as A.
- Risks: ropus is young (0.12.x, 5,444 downloads, one repository [C3]); a bit-exactness gap would
  show only as a fixture failure; Rust 1.88 minimum.
- Fit: the simplest reproducible image; weaker maturity for the one codec chorus also encodes.

### Option C: all-permissive Rust crates, no MPL

- What: `nanomp3` (MP3), `claxon` (FLAC), `lewton` (Vorbis), `alac` + `mp4` (ALAC in M4A), `ogg`
  0.9.2 (BSD-3), `hound` or own WAV, plus Opus per A or B.
- Costs: no licence ADR; four decoder APIs and two demuxers glued by chorus; chorus writes its own
  gapless trimming and seeking; stale upstreams (claxon 2020, lewton 2021, alac 2018).
- Risks: maintenance falls on chorus; lower-confidence decoders (nanomp3 27 K downloads).
- Fit: strictly inside the allowlist; more code to own for no user-visible gain.

### Option D: vendor C decoders into the server (dr_mp3, dr_flac, stb_vorbis, Apple's ALAC, libopus)

- What: permissive C (and Apple's C++) through FFI in chorus-server.
- Costs: every codec needs a musl C/C++ toolchain in the image build and `unsafe` FFI wrappers;
  stb_vorbis licence is MIT or public domain (research [R41], not re-read).
- Fit: allowlist-clean but the worst fit for a Rust server; only the endpoint needs C.

### Option E: write clean-room decoders

- K79 lists it as not chosen as final. FLAC is small enough to write, but BRIEF §3.2 says to
  vendor a codec, and MP3, Vorbis and Opus decoders are large and undifferentiated. Not proposed,
  except that the planning research's FLAC and Opus fixtures would let a written FLAC decoder be
  checked if one were ever wanted.

### The AAC sub-choice (any option above)

- (i) **Off**: not compiled (the If-deferred cell).
- (ii) Symphonia `aac` behind a chorus build feature, default off, enabled only in a build the
  owner makes after deciding on the Via LA question; AAC-LC only (Symphonia has no HE-AAC decoder
  [C2]; no other permissive or MPL HE-AAC decoder was searched), no gapless.
- (iii) On by default: not proposed under the strict bar.

## Comparison

| Criterion | A: Symphonia + libopus | B: Symphonia + ropus | C: permissive crates | D: vendored C |
|---|---|---|---|---|
| Licence ADR needed | Yes (MPL-2.0) | Yes (MPL-2.0) | No | No |
| C in the server image | Yes (libopus: musl C + cmake) | **No** | Opus only, per A or B | Yes, every codec |
| Maturity of decoders | High | High, except Opus (young) | Mixed, stale | High |
| Opus encoder (K62 wire) | Reference libopus | ropus (claims bit-exact) | per A or B | libopus |
| Same Opus code as the endpoint | Yes | No (checked by fixtures) | per A or B | Yes |
| Gapless (UPnP) | Shared, from Symphonia | Shared | chorus writes it | chorus writes it |
| Effort | Small | Small | Medium | Medium-large |

## Recommendation

**Recommendation:** Option A (Symphonia 0.6.1 under an MPL-2.0 ADR for MP3, FLAC, Vorbis, ALAC and WAV; libopus via the `opus` crate for Opus; dr_flac and libopus on the C endpoint) with AAC off, because it is the most mature decoder set with one API and gapless handling, and it runs the reference Opus code on both sides of the wire.

Why: every recommended format is patent-clear on primary sources (MP3 expired and its programme
ended; FLAC and Opus are IETF standards with no fee-bearing IPR; Vorbis and ALAC carry their owners'
royalty-free statements or grants). The costs are one MPL-2.0 ADR (unmodified dependency, source
location in the release notes) and a musl C toolchain in the server image build for libopus, which
goal 16 records by ADR. If the owner would rather keep C out of the server image, Option B swaps
libopus for ropus on the Rust side at the price of a young dependency, with the shared fixtures as
the guard. Decoding stays in chorus-server; endpoints decode only FLAC and Opus (K62).

AAC stays off: its patents are live and Via LA's terms name no free-software exemption. What the
owner gives up: AAC and HLS radio URLs as alarm sources, and direct AAC/M4A casting (control points
must send or transcode to another format). If the owner accepts the Via LA position, sub-choice
(ii) adds AAC-LC in a build the owner makes; revisit only after a patent search shows that no live
pool patent reads on AAC-LC decoding (the pool lists US patents granted as late as 2026).

## If the owner defers

Later goals build "MP3, FLAC, Vorbis, Opus, ALAC and WAV as recommended; AAC off": that is Option A
with AAC off. Goal 16 writes the MPL-2.0 ADR and the cargo-deny exception and keeps `make image`
green with libopus; goal 6 vendors dr_flac and libopus on the endpoint; goal 4's release notes say
where Symphonia's source is. The cost is the same as the recommendation; nothing is lost that the
owner could not add later by a proposal (AAC, or ropus instead of libopus).

## Open inputs

- **Binary size** (K79 asks for it): not measured in goal 1. A size probe (stripped, LTO, static
  musl: a hello-world baseline, Symphonia with the recommended features, the same plus `aac`, and
  ropus encode plus decode) was queued under the heavy lock, did not get the lock within this session and was withdrawn;
  goal 16 measures the real server image before and after and records it. Until then every size
  here is a source-size proxy, not a binary size.
- Endpoint flash cost of dr_flac and libopus fixed-point on the chosen target: `ASSUMED` to fit;
  goal 6 measures it with the P1 target.
- The owner's calls: MPL-2.0 by ADR (A, B) or all-permissive (C); libopus (A) or ropus (B) in the
  server; AAC off, or sub-choice (ii) after the owner's own reading of Via LA's terms.
- The share of AAC and HLS among the owner's alarm radio stations: `ASSUMED`; only matters if AAC
  stays off.
- AAC patent end dates: Via LA's 2026 Q2 patent list [A3] names 586 US patents, numbered up to
  US 12,573,411 (a 2026 grant, `ASSUMED` from USPTO numbering), so the pool runs well past 2031;
  which patents read on AAC-LC decoding is unknown (no dates in the list), and the 2028 date stays
  secondary; no Needs item.

## Sources

All read 2026-09-30 unless marked.

- [F1] Home Assistant core 2026.9.3, `homeassistant/components/tts/__init__.py`, https://raw.githubusercontent.com/home-assistant/core/2026.9.3/homeassistant/components/tts/__init__.py (Apache-2.0)
- [M1] Google Patents, US6009399A, https://patents.google.com/patent/US6009399A/en
- [M2] Fraunhofer IIS, mp3, https://www.iis.fraunhofer.de/en/ff/amm/consumer-electronics/mp3.html
- [A1] Via Licensing Alliance, AAC programme, https://www.via-la.com/licensing-programs/aac/
- [A2] Wikipedia, Advanced Audio Coding (licensing and patents), https://en.wikipedia.org/wiki/Advanced_Audio_Coding (read 2026-09-29 in verify-ha-casting.md, not re-read)
- [A3] Via LA, AAC Patent License Agreement Patent List (26Q2), https://www.via-la.com/wp-content/uploads/2026/09/26Q2-AAC-Patent-List-website.pdf, read 2026-09-30
- [C1] crates.io API, https://crates.io/api/v1/crates/<name> for symphonia and its format and codec crates, symphonia-codec-opus (absent), opus, opusic-sys (and its 0.7.5 dependencies and features), audiopus, audiopus_sys, ropus, opus-decoder, unsafe-libopus, nanomp3, minimp3, claxon, lewton, alac, hound, fdk-aac, ogg, mp4, re_mp4
- [C2] Symphonia README, https://raw.githubusercontent.com/pdeljanov/Symphonia/master/README.md (codec table); GitHub API https://api.github.com/repos/pdeljanov/Symphonia (MPL-2.0)
- [C3] ropus README, https://static.crates.io/readmes/ropus/ropus-0.12.18.html
- [O1] IETF IPR disclosure 1524 (Xiph.Org, Opus), https://datatracker.ietf.org/ipr/1524/
- [O2] RFC 6716 datatracker, https://datatracker.ietf.org/doc/rfc6716/
- [FL1] RFC 9639 datatracker, https://datatracker.ietf.org/doc/rfc9639/
- [V1] Xiph.Org, Vorbis, https://xiph.org/vorbis/
- [L1] libopus COPYING, https://raw.githubusercontent.com/xiph/opus/main/COPYING
- [L2] dr_libs README and dr_mp3.h licence footer, https://raw.githubusercontent.com/mackron/dr_libs/master/README.md, https://raw.githubusercontent.com/mackron/dr_libs/master/dr_mp3.h
- [L3] libFLAC COPYING.Xiph, https://raw.githubusercontent.com/xiph/flac/master/COPYING.Xiph
- [L4] libvorbis COPYING, https://raw.githubusercontent.com/xiph/vorbis/master/COPYING
- [L5] Apple ALAC LICENSE, https://raw.githubusercontent.com/macosforge/alac/master/LICENSE; GitHub API https://api.github.com/repos/macosforge/alac (archived, Apache-2.0)
- [L6] minimp3 LICENSE (CC0 1.0), https://raw.githubusercontent.com/lieff/minimp3/master/LICENSE; GitHub API https://api.github.com/repos/lieff/minimp3
- Planning research sources carried without re-reading (research-casting-decoders.md, read 2026-09-29): [R14] UPnP ConnectionManager:1, [R19] BubbleUPnP, [R37] RFC 8216, [R41] stb LICENSE, [R44] fdk-aac NOTICE.
- `LEAD`: one web search, "Via LA AAC license FAQ free software open source decoder", returned no Via LA FAQ; its summary (Wikipedia on FDK AAC) is not relied on.

## What was read

- Files: `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`,
  `/cache/tmp/chorus-g1/prompt-PB.md`; [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) (§0.3 releases,
  §0.8, §0.9, §0.11, §0.13, §1, §2, §3.2, §5, §10, §20, §21, §24);
  [`.claude/goals/2026-09-chorus-research/research-casting-decoders.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research/research-casting-decoders.md) (header, §4-§7),
  `verify-ha-casting.md` (whole), `research-theater.md` and `verify-theater-platform.md` (for P2);
  `/cache/wt/chorus/chorus/baseline/BRIEF.md` §2.3, §3, §5.7.
- URLs: every entry in Sources marked read 2026-09-30, and one web search (the `LEAD` above).
- Permissive files read: licence files only (libopus, libFLAC, libvorbis, minimp3, Apple ALAC,
  dr_libs README and dr_mp3.h footer), HA core `tts/__init__.py` (Apache-2.0, grepped), crate
  metadata and the ropus README. No GPL or LGPL source file was opened (no faad2, FAAC, FFmpeg,
  mp3lame, libcec).
