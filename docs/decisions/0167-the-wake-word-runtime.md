# 0167: the wake word runs microWakeWord's Apache-2.0 "Okay Nabu" model in chorus's own integer interpreter and a port of TensorFlow Lite's micro frontend, held value for value to TensorFlow Lite's reference kernels; the models and their licences are a checked list

- Status: accepted, 2026-10-04
- Decided by: the owner for the path and the licence rule (proposal P8, Option A: chorus-server
  runs wake-word models with permissive licences, Apache-2.0 microWakeWord models, openWakeWord's
  bundled CC BY-NC-SA models excluded, and no trademarked word such as "alexa"; K73, K26, K95);
  the task for the runtime ("an ADR records the inference runtime choice on fitness alone, and
  the provenance and licence of the wake-phrase fixture"); this record for the runtime, the
  model, the exclusion list and the fixtures.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `crates/wakeword` (`frontend.rs`, `fft.rs`, `flatbuf.rs`, `interp.rs`,
  `manifest.rs`, `lib.rs`); `third_party/wakeword/` with its list `LICENCES.md`;
  `tools/conventions/check-wakeword.sh`; `fixtures/wakeword/`; held by
  `crates/wakeword/tests/reference.rs`.

## Context

P8 puts the wake word on the server: a speaker with a microphone sends 16 kHz mono 16-bit PCM
(ADR 0166), chorus-server decides when the phrase was said, and only then is audio handed to Home
Assistant. That needs three things this record settles: which models, what runs them, and what
proves the result. Server wiring, the endpoint and the choice of phrase in Home Assistant are
other tasks.

A microWakeWord model is a TFLite file of about 60 kB with a JSON manifest. It is a streaming
network: it takes three 10 ms feature frames per call and keeps its history in resource variables
inside the model. The vendored model uses thirteen operators (`CALL_ONCE`, `VAR_HANDLE`,
`READ_VARIABLE`, `ASSIGN_VARIABLE`, `RESHAPE`, `CONCATENATION`, `STRIDED_SLICE`, `SPLIT_V`,
`CONV_2D`, `DEPTHWISE_CONV_2D`, `FULLY_CONNECTED`, `LOGISTIC`, `QUANTIZE`), all on int8 tensors.
Its input is not audio but the output of TensorFlow Lite's micro frontend in one fixed
configuration (30 ms window, 10 ms step, 40 mel channels, noise reduction, PCAN gain, log scale),
which is fixed-point C code over a fixed-point FFT.

## What was read

All read 2026-10-04.

- `docs/proposals/P8-voice-path.md` (the wake-word licences row and its sources), `deny.toml`,
  `docs/conventions.md` rule 13, `docs/clean-room.md`, ADR 0044 (the precedent for vendored
  third-party files), ADR 0166.
- <https://github.com/esphome/micro-wake-word-models> at commit
  `40ff33f57f8fc6ad71a75ef085abbf742495b225`: `LICENSE` (Apache-2.0), `NOTICE`, `README.md`
  (section "License": the terms cover the model weights and manifests), `models/v2/*.json`, and
  the model file `models/v2/okay_nabu.tflite` through a schema dump of its operators, shapes and
  quantization. GitHub's API reports the repository as Apache-2.0. The repository holds data
  only; no ESPHome source file was opened.
- <https://github.com/kahrendt/microWakeWord>: the licence GitHub's API reports (Apache-2.0).
  No file of it was needed.
- <https://github.com/OHF-Voice/pymicro-wakeword> at commit
  `333da550c54b95192af472cfe37d98dab2f2086e` (Apache-2.0, `LICENSE`):
  `pymicro_wakeword/microwakeword.py` and `const.py` (how features are quantized, the stride and
  the sliding mean), `tests/test_microwakeword.py`, and `tests/okay_nabu/2.wav`.
- <https://github.com/rhasspy/pymicro-features> at commit
  `96bd69cfad79aa67697e176570d3dd87052c3def` (Apache-2.0, `LICENSE`): `src/micro_features.cpp`
  (the frontend's configuration), its copy of TensorFlow Lite's micro frontend
  (`tensorflow/lite/experimental/microfrontend/lib/*.cc` and `*.h`, Apache-2.0, "Copyright 2018
  The TensorFlow Authors") and its copy of KISS FFT (`kissfft/kiss_fft.cc`, `_kiss_fft_guts.h`,
  `tools/kiss_fftr.cc`, BSD-3-Clause, "Copyright (c) 2003-2010, Mark Borgerding").
- TensorFlow Lite's quantization specification,
  <https://ai.google.dev/edge/litert/models/quantization_spec>, and the FlatBuffers format,
  <https://flatbuffers.dev/internals/>; the field numbers of TensorFlow Lite's `schema.fbs`
  (Apache-2.0) as the generated accessors of the PyPI package `tflite` give them.
- <https://github.com/dscripka/openWakeWord> `README.md`, sections "Pre-Trained Models" and
  "License": the code is Apache-2.0 and "all of the included pre-trained models are licensed
  under the Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International license".
- <https://github.com/sonos/tract>: a code search of the repository for `VAR_HANDLE` (one hit,
  the schema file `tflite/schema/tflite.fbs`; no operator implementation).
- No GPL source was opened: no ESPHome C++ (`micro_wake_word`, `voice_assistant`), no Piper.

## Decision

### The model

One model is vendored: `okay_nabu.tflite` (version 2) with its manifest, from
`esphome/micro-wake-word-models`, Apache-2.0, with the repository's `LICENSE` and `NOTICE`
beside it. `third_party/wakeword/LICENCES.md` lists every file with its source URL, commit,
SHA-256, licence and the date it was read, and the runtime's parts with theirs.

Excluded, by name, in the same file: `alexa`, `hey_jarvis`, `hey_mycroft`, `hey_peppa_pig` and
the other assistants' phrases (another party's product, service or character), and the names of
the models openWakeWord bundles (CC BY-NC-SA 4.0). `tools/conventions/check-wakeword.sh` fails
when a file in the directory has no row or a different checksum, when a row's licence is not
Apache-2.0, MIT, BSD or CC0, when a name is excluded, and when the crate compiles in a model the
directory does not hold; it tests itself on five damaged copies each run.

"Okay Nabu" is kept as the one phrase: its publisher made it for Home Assistant's own assistant,
which is where chorus sends the audio, and published the model under Apache-2.0. **Not
established:** whether "Nabu" is a registered trademark of its publisher. No trademark register
was searched. The upstream `NOTICE` says the licence grants no trademark right; chorus uses the
phrase only to wake the assistant it was made for and names no product after it.

### The runtime: chorus's own interpreter, no dependency

`crates/wakeword` reads the TFLite file itself (a bounds-checked FlatBuffers reader) and runs
the thirteen operators with the integer arithmetic of TensorFlow Lite's quantization
specification. It has no dependency and no unsafe code. The reasons, on fitness for chorus
alone:

1. **The whole computation is small and closed.** Thirteen operators over int8 tensors of a few
   thousand elements at most, with every shape static. The interpreter is about 900 lines; rule 3 of
   the working agreement prefers building what is small and instructive.
2. **It can be held to a specification, value for value.** Integer kernels have one right
   answer. The test compares every output with TensorFlow Lite's reference kernels and they are
   equal (below). A float or differently rounded runtime could only be held to "close".
3. **A pure library with shared fixtures (rule 5).** No file, thread or clock; a damaged model is
   an error and never a panic (the test loads truncations at every 37th length and about 3000
   corrupted copies, and runs the ones that still load).
   Everything is checked at load, so running allocates nothing.
4. **The alternatives fit worse.**
   - *TensorFlow Lite's C library linked in* (what `pymicro-wakeword` does): the right
     semantics by definition, but a C++ build or a 4.8 MB prebuilt shared object
     (`lib/linux_amd64/libtensorflowlite_c.so` in that repository is 4,835,320 bytes) inside a
     static musl server image, an FFI boundary with unsafe code, and a parser for untrusted
     files that chorus does not hold to its own checks, for a 60 kB model.
   - *TensorFlow Lite Micro compiled in*: smaller, the same reference kernels, but still a C++
     toolchain and FFI in the server for thirteen operators.
   - *A general Rust inference crate (tract)*: the models keep their state in resource
     variables, and the search above found no implementation of `VAR_HANDLE` in tract (only the
     schema names it), so the model would have to be rewritten to stateless
     form first, which is a second, unverified artefact; and its dependency tree is large for
     what is used.
   - *A different detector* (openWakeWord's): its bundled models are CC BY-NC-SA, which the
     owner's rule excludes.

### The frontend: a port, with its notices

`frontend.rs` is a Rust port of TensorFlow Lite's micro frontend and `fft.rs` of the part of
KISS FFT it uses (the 512-point 16-bit real FFT, radix-4 stages only). A port, not a rewrite,
because the models were trained on this arithmetic: every butterfly's rounding and every 16-bit
wrap changes the features. The two files keep their upstream licences (Apache-2.0 with the
TensorFlow notice and a statement of changes; BSD-3-Clause with KISS FFT's notice and
conditions), both on `deny.toml`'s allowlist and both compatible with chorus's MIT OR
Apache-2.0. No `deny.toml` exception is needed: the crate has no dependency.

### Detection

As the upstream runner does: features are put on the input tensor's grid (ties to even), the
model runs once per three frames, and the mean of the last `sliding_window_size` probabilities is
compared with the manifest's `probability_cutoff` (5 and 0.97 for this model). Two choices are
chorus's own: a feature beyond the int8 range saturates (the upstream Python wraps it), and a
detection is reported once, when the mean rises above the cutoff, and not again until it has
fallen back, because the model's output stays high for several hundred milliseconds after a
phrase. Time is a sample count; there is no clock in the crate.

## What was measured

On 2026-10-04, by `fixtures/wakeword/make-reference.py` (pymicro-features 2.0.2,
ai-edge-litert 2.2.0, numpy 2.5.3) and `cargo test -p chorus-wakeword`:

- The Rust frontend's features equal the C frontend's on both recordings: 198 of 198 frames for
  `okay-nabu.wav`, 398 of 398 for `music.wav`, all 40 values of each.
- The Rust interpreter's outputs equal those of TensorFlow Lite's reference kernels (LiteRT's
  `BUILTIN_REF` resolver) on both: 66 of 66 and 132 of 132.
- TensorFlow Lite's own three kernel sets do not agree with each other on this model. Against
  the reference kernels on `okay-nabu.wav`, the optimized built-in kernels differ in 6 of 66
  outputs by at most 9/256, and the XNNPACK delegate in 11 of 66 by at most 10/256; on
  `music.wav` all three agree. All three give the same largest window mean (0.9961) and so the
  same detection. chorus follows the reference kernels because they are the specification's
  arithmetic and the ones TensorFlow Lite Micro builds on.
- Detection: "Okay Nabu" once in `okay-nabu.wav`, at sample 24320 of 32000, mean probability
  0.988; none in ten seconds of silence, ten seconds of quiet and of loud white noise, or the
  music fixture played once and four times over.

No timing claim is made here; the server-wiring task measures the cost on the server.

## The fixtures: provenance and licence

- `fixtures/wakeword/okay-nabu.wav`: the wake phrase, 2 s, 16 kHz mono 16-bit. It is
  `tests/okay_nabu/2.wav` of <https://github.com/OHF-Voice/pymicro-wakeword> at commit
  `333da550c54b95192af472cfe37d98dab2f2086e`, byte for byte (SHA-256
  `d26a300a7f76fd84cf9ce76d22c61d1a8253837be4dc65f8a5f13ab28d37f176`). Licence: Apache-2.0, the
  repository's `LICENSE`; the repository carries no separate statement for its test audio.
  **Not established:** who or what spoke it (a person or a synthesizer); the repository does not
  say.
- `fixtures/wakeword/music.wav`: 4 s of music synthesized by `make-music.py` from arithmetic
  alone (plucked chords, a bass line, a noise hi-hat), so no recording's licence is in question.
  chorus's own, MIT OR Apache-2.0.
- `*.features` and `*.outputs`: the reference values, written by `make-reference.py` from the
  two recordings and the vendored model. chorus's own.
- Silence and noise are generated in the test.

## Consequences

- A second phrase is a row in `LICENCES.md`, the files, and a line in `builtin()`; the check
  refuses it otherwise. A model that uses an operator outside the thirteen is refused at load
  with the operator's number, and adding the operator is a change to `interp.rs` held to the
  same reference.
- The frontend's tables are built with `f32` library functions (`cos`, `ln_1p`, `powf`), as the
  C original builds them. The reference test would catch a platform whose library rounds one of
  them across a table boundary.
- The reference files are regenerated only by the script, with the pinned versions in its
  header.
