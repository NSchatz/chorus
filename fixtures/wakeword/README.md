# fixtures/wakeword

What `crates/wakeword/tests/reference.rs` holds the wake-word detector to. Rust-only by
declaration: the detector runs in chorus-server alone (proposal P8), no endpoint runs it and
there is no C implementation. Where each file comes from and under which licence is recorded
in `docs/decisions/0167-the-wake-word-runtime.md`.

| File | What it is | Origin and licence |
|---|---|---|
| `okay-nabu.wav` | "Okay Nabu" spoken once, 2 s, 16 kHz mono 16-bit | `tests/okay_nabu/2.wav` of <https://github.com/OHF-Voice/pymicro-wakeword> at commit `333da550c54b95192af472cfe37d98dab2f2086e`, byte for byte (SHA-256 `d26a300a7f76fd84cf9ce76d22c61d1a8253837be4dc65f8a5f13ab28d37f176`); Apache-2.0, the repository's `LICENSE`, read 2026-10-04 |
| `music.wav` | 4 s of synthesized music, 16 kHz mono 16-bit | written by `make-music.py` from arithmetic alone; chorus's own, MIT OR Apache-2.0 |
| `*.features` | per 10 ms frame, the 40 feature values of TensorFlow Lite's micro frontend for the recording | written by `make-reference.py` |
| `*.outputs` | per inference, the model's raw 8-bit output under TensorFlow Lite's reference kernels | written by `make-reference.py` |
| `make-music.py`, `make-reference.py` | the scripts that wrote the files above, with the pinned versions in their headers | chorus's own, MIT OR Apache-2.0 |

Silence and noise are generated in the test (zeros, and a linear congruential generator), so
they need no file.
