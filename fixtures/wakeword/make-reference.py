#!/usr/bin/env python3
"""Writes the reference files beside each .wav here: what the upstream
implementations compute for it.

    <name>.features  one line per 10 ms frame: the 40 values of TensorFlow
                     Lite's micro frontend (pymicro-features, which compiles
                     the frontend's C sources)
    <name>.outputs   one line per inference (three frames): the model's raw
                     8-bit output under TensorFlow Lite's reference kernels
                     (LiteRT's interpreter with the BUILTIN_REF resolver)

crates/wakeword/tests/reference.rs holds the Rust frontend and interpreter to
these, value for value. The script also prints what TensorFlow Lite's two
other kernel sets give (the optimized built-in kernels and the XNNPACK
delegate): they round differently from the reference kernels and from each
other, which docs/decisions/0000-the-wake-word-runtime.md records.

The versions are pinned so a re-run gives the same files:

    uv run --with pymicro-features==2.0.2 --with ai-edge-litert==2.2.0 --with numpy==2.5.3 \
        python fixtures/wakeword/make-reference.py
"""
import json
import pathlib
import wave

import numpy as np
from ai_edge_litert import interpreter as litert
from pymicro_features import MicroFrontend

HERE = pathlib.Path(__file__).parent
MODEL = HERE.parent.parent / "third_party" / "wakeword" / "okay_nabu.tflite"
MANIFEST = json.loads(MODEL.with_suffix(".json").read_text())["micro"]
# The frontend hands out its 16-bit values times this.
FEATURE_SCALE = 0.0390625
STEP_BYTES = 160 * 2


def features_of(pcm):
    """The frontend's frames, as the runner in pymicro-wakeword feeds it: 10 ms at a time."""
    frontend = MicroFrontend()
    frames = []
    at = 0
    while at + STEP_BYTES <= len(pcm):
        result = frontend.process_samples(pcm[at : at + STEP_BYTES])
        at += result.samples_read * 2
        if result.features:
            frames.append([round(v / FEATURE_SCALE) for v in result.features])
    return frames


def outputs_of(frames, resolver):
    it = litert.Interpreter(model_path=str(MODEL), experimental_op_resolver_type=resolver)
    it.allocate_tensors()
    inp, out = it.get_input_details()[0], it.get_output_details()[0]
    scale, zero = inp["quantization"]
    stride = inp["shape"][1]
    outputs = []
    for k in range(0, len(frames) - stride + 1, stride):
        x = np.array(frames[k : k + stride], dtype=np.float64) * FEATURE_SCALE
        q = np.clip(np.round(x / scale + zero), -128, 127).astype(np.int8)
        it.set_tensor(inp["index"], q.reshape(inp["shape"]))
        it.invoke()
        outputs.append(int(it.get_tensor(out["index"]).reshape(-1)[0]))
    return outputs


def best_mean(outputs):
    """The largest sliding-window mean probability: what the cutoff is compared with."""
    n = MANIFEST["sliding_window_size"]
    return max(sum(outputs[i : i + n]) / n / 256 for i in range(len(outputs) - n + 1))


def main():
    resolvers = litert.OpResolverType
    for path in sorted(HERE.glob("*.wav")):
        with wave.open(str(path), "rb") as w:
            assert (w.getframerate(), w.getsampwidth(), w.getnchannels()) == (16000, 2, 1), path
            pcm = w.readframes(w.getnframes())
        frames = features_of(pcm)
        reference = outputs_of(frames, resolvers.BUILTIN_REF)
        path.with_suffix(".features").write_text("".join(" ".join(map(str, f)) + "\n" for f in frames))
        path.with_suffix(".outputs").write_text("".join(f"{o}\n" for o in reference))
        print(f"{path.name}: {len(frames)} frames, {len(reference)} inferences, cutoff {MANIFEST['probability_cutoff']}")
        for name, resolver in [
            ("reference kernels", resolvers.BUILTIN_REF),
            ("optimized kernels", resolvers.BUILTIN_WITHOUT_DEFAULT_DELEGATES),
            ("XNNPACK delegate", resolvers.AUTO),
        ]:
            got = outputs_of(frames, resolver)
            differ = sum(a != b for a, b in zip(got, reference))
            widest = max(abs(a - b) for a, b in zip(got, reference))
            print(f"  {name}: largest window mean {best_mean(got):.4f}; {differ} outputs differ from the reference kernels, by at most {widest}/256")


if __name__ == "__main__":
    main()
