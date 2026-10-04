# Wake-word models and their runtime: sources, checksums and licences

The wake-word detector (`crates/wakeword`) runs only the model files in this directory.
`tools/conventions/check-wakeword.sh` fails the gate when a file here has no row below, when
its SHA-256 is not the one in its row, when a row's licence is not on the allowed list, or when
a model's name is on the excluded list. The choices are recorded in
`docs/decisions/0000-the-wake-word-runtime.md`.

## Vendored files

Every file is byte for byte the upstream file at the commit named. Read on the date in the last
column: the repository's `LICENSE` and `NOTICE` (copied here), its README's licence section, and
the licence field GitHub's API reports for it (`Apache-2.0`).

| File | What it is | Source | Commit | SHA-256 | Licence | Read |
|---|---|---|---|---|---|---|
| `okay_nabu.tflite` | the model for the phrase "Okay Nabu" (microWakeWord, version 2) | <https://github.com/esphome/micro-wake-word-models/blob/40ff33f57f8fc6ad71a75ef085abbf742495b225/models/v2/okay_nabu.tflite> | `40ff33f57f8fc6ad71a75ef085abbf742495b225` | `0689abe1912a95a3318a0d8cb2e67bad0cbcfe3e24dd6e050c75debddfb6f891` | Apache-2.0 | 2026-10-04 |
| `okay_nabu.json` | its manifest: the phrase, the probability cutoff, the sliding window | <https://github.com/esphome/micro-wake-word-models/blob/40ff33f57f8fc6ad71a75ef085abbf742495b225/models/v2/okay_nabu.json> | `40ff33f57f8fc6ad71a75ef085abbf742495b225` | `6dd65604f70fe5ea9d1af73a7bf239529d1fbabc363807f45d2b22ce464ddbed` | Apache-2.0 | 2026-10-04 |
| `LICENSE` | the Apache License 2.0 text the models are under | <https://github.com/esphome/micro-wake-word-models/blob/40ff33f57f8fc6ad71a75ef085abbf742495b225/LICENSE> | `40ff33f57f8fc6ad71a75ef085abbf742495b225` | `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4` | Apache-2.0 | 2026-10-04 |
| `NOTICE` | the attribution notice Apache-2.0 section 4(d) asks a redistributor to carry | <https://github.com/esphome/micro-wake-word-models/blob/40ff33f57f8fc6ad71a75ef085abbf742495b225/NOTICE> | `40ff33f57f8fc6ad71a75ef085abbf742495b225` | `bf6c416d88749af557faecbaf874861c6a808c5d45f4c28a2e25f9f613ce91ad` | Apache-2.0 | 2026-10-04 |

The upstream `NOTICE` says the Apache-2.0 terms "cover the entire contents of this repository,
including the trained model weights (.tflite) and their manifests (.json)", and that the licence
grants no right to any third party's trademark.

Allowed licences for a row: Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, CC0-1.0. No row is
CC BY-NC-SA or any other NonCommercial licence, and the check refuses one.

## The runtime

The models are run by chorus's own code, with no external crate and no linked library.

| Part | Where | Origin | Licence | Read |
|---|---|---|---|---|
| The TFLite file reader and the integer interpreter | `crates/wakeword/src/flatbuf.rs`, `interp.rs` | chorus's own, written from the FlatBuffers format description and TensorFlow Lite's quantization specification | MIT OR Apache-2.0 | n/a |
| The audio frontend | `crates/wakeword/src/frontend.rs` | a Rust port of TensorFlow Lite's micro frontend (`tensorflow/lite/experimental/microfrontend/lib`, "Copyright 2018 The TensorFlow Authors"), read in the copy at <https://github.com/rhasspy/pymicro-features> commit `96bd69cfad79aa67697e176570d3dd87052c3def`; the file carries the notice and says what changed | Apache-2.0 | 2026-10-04 |
| The fixed-point FFT | `crates/wakeword/src/fft.rs` | a Rust port of the 16-bit real FFT of KISS FFT ("Copyright (c) 2003-2010, Mark Borgerding"), read in the same copy (`kissfft/kiss_fft.cc`, `kissfft/tools/kiss_fftr.cc`, `kissfft/_kiss_fft_guts.h`); the file carries the notice and the three conditions | BSD-3-Clause | 2026-10-04 |
| The manifest reader and the detector | `crates/wakeword/src/manifest.rs`, `lib.rs` | chorus's own; the detection rule (a sliding mean of probabilities against the manifest's cutoff) follows <https://github.com/OHF-Voice/pymicro-wakeword> commit `333da550c54b95192af472cfe37d98dab2f2086e` (`pymicro_wakeword/microwakeword.py`, Apache-2.0) | MIT OR Apache-2.0 | 2026-10-04 |

Both ported files keep their upstream licence, which chorus's own licence (MIT OR Apache-2.0)
is compatible with; both licences are on `deny.toml`'s allowlist.

## Excluded names

A model whose file name is one of these, or starts with one followed by `_`, `-` or `.`, fails the
check whatever its row says. They are phrases that name another party's product, service or
character, and the names of the models openWakeWord bundles, which are all CC BY-NC-SA 4.0
(<https://github.com/dscripka/openWakeWord>, README section "License", read 2026-10-04: "All of
the included pre-trained models are licensed under the Creative Commons
Attribution-NonCommercial-ShareAlike 4.0 International license").

- `alexa`: Amazon's assistant; also an openWakeWord bundled model
- `hey_jarvis`: a character of Marvel's; also an openWakeWord bundled model
- `hey_mycroft`: Mycroft AI's assistant; also an openWakeWord bundled model
- `hey_rhasspy`: an openWakeWord bundled model
- `timer`: an openWakeWord bundled model
- `weather`: an openWakeWord bundled model
- `hey_peppa_pig`: a character of Hasbro's
- `hey_siri`: Apple's assistant
- `hey_google`: Google's assistant
- `ok_google`: Google's assistant
- `hey_cortana`: Microsoft's assistant

"Okay Nabu" is kept: it is the wake phrase the models' publisher (the Open Home Foundation, with
Nabu Casa) made for Home Assistant's own voice assistant, which is the assistant chorus hands the
audio to, and it is published under Apache-2.0 for this use. No trademark register was searched
for it; the decision record says so.
