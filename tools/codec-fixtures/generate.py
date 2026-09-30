#!/usr/bin/env python3
"""Regenerate fixtures/codec: FLAC and Opus streams as the wire carries them,
each with its reference decode.

Run by `make -C firmware codec-fixtures FLAC=... OPUS_VECTORS=...`, never by the
gate. Standard library only. Inputs, all pinned in fixtures/codec/README.md:

- FLAC: signals this script synthesises (deterministic, no randomness beyond a
  fixed linear congruential generator), encoded by the reference encoder `flac`
  (libFLAC 1.5.0) and decoded back by `flac -d`. The reference PCM is flac's
  decode, and it must equal the input signal (FLAC is lossless) and match the
  STREAMINFO MD5, or the script stops.
- Opus: excerpts of the official RFC 6716 / RFC 8251 test vectors (the first
  packets of a vector, whole packets, about one second), their per-packet final
  ranges as the vector carries them, and the matching excerpt of the official
  reference decode (`testvectorNN.dec`, or `testvectorNNm.dec` for mono). The
  exact decode of the excerpt by libopus 1.6.1 as chorus builds it
  (tools/codec-fixtures/opus_ref.c) is recorded as a FNV-1a 64 hash, and
  libopus's opus_compare must pass it against the official decode.

Output per fixture NAME: NAME.fields (key = value; the stream_format fields,
the codec setup in hex, counts, hashes, provenance in comments), NAME.chunks
(per coded_chunk: frames, final range and length as 32-bit big-endian, then the
FLAC frame or Opus packet) and NAME.pcm (the reference decode, interleaved
little-endian in the fixture's sample format).
"""

import argparse
import hashlib
import math
import os
import struct
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
OPUS = os.path.join(ROOT, "third_party", "opus")

FLAC_FIXTURES = [
    # name, rate, channels, bits, seconds, compression, block size
    ("flac-s16-stereo-44k1", 44100, 2, 16, 1.0, "-5", 4096),
    ("flac-s24-stereo-96k", 96000, 2, 24, 0.25, "-8", 4096),
    ("flac-s16-6ch-48k", 48000, 6, 16, 0.2, "-5", 1152),
]

OPUS_FIXTURES = [
    # name, vector, channels decoded, pre-skip written into the OpusHead
    ("opus-tv02-silk-mono", "02", 1, 0),
    ("opus-tv05-hybrid-stereo", "05", 2, 312),
    ("opus-tv10-celt-stereo", "10", 2, 0),
]

EXCERPT_FRAMES = 48000


def fnv1a64(data):
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw)


# --- FLAC --------------------------------------------------------------------


def signal(rate, channels, bits, seconds):
    """Tones, a sweep and noise at about -6 dBFS, one character per channel."""
    frames = int(rate * seconds) + 777  # never a whole number of blocks
    peak = (1 << (bits - 1)) - 1
    seed = 12345
    out = bytearray()
    width = bits // 8
    for n in range(frames):
        t = n / rate
        for c in range(channels):
            seed = (seed * 1103515245 + 12345) & 0x7FFFFFFF
            noise = (seed / 0x7FFFFFFF) * 2.0 - 1.0
            if c == 0:
                x = 0.5 * math.sin(2 * math.pi * (100 + 4000 * t) * t)
            elif c == 1:
                x = 0.3 * math.sin(2 * math.pi * 997 * t) + 0.2 * noise
            else:
                x = 0.45 * math.sin(2 * math.pi * (220 * (c + 1)) * t) + 0.05 * noise
            v = int(round(x * peak))
            out += v.to_bytes(4, "little", signed=True)[:width]
    return bytes(out), frames


def read_metadata(data):
    if data[:4] != b"fLaC":
        raise SystemExit("not a FLAC stream")
    at = 4
    streaminfo = None
    while True:
        head = data[at]
        length = int.from_bytes(data[at + 1 : at + 4], "big")
        if head & 0x7F == 0:
            streaminfo = data[at + 4 : at + 4 + length]
        at += 4 + length
        if head & 0x80:
            return streaminfo, at


def crc8(data):
    crc = 0
    for b in data:
        crc ^= b
        for _ in range(8):
            crc = ((crc << 1) ^ 0x07) & 0xFF if crc & 0x80 else (crc << 1) & 0xFF
    return crc


def crc16(data):
    crc = 0
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x8005) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def frame_header(data, at):
    """(header length, block size) of a frame header at `at`, or None when the
    bytes there are not one (RFC 9639 section 9.1)."""
    if data[at] != 0xFF or data[at + 1] not in (0xF8, 0xF9):
        return None
    code = data[at + 2] >> 4
    rate_code = data[at + 2] & 0x0F
    p = at + 4
    first = data[p]
    extra = 0
    while extra < 7 and first & (0x80 >> extra):
        extra += 1
    p += 1 + (extra - 1 if extra > 1 else 0)
    if code == 1:
        block = 192
    elif 2 <= code <= 5:
        block = 576 << (code - 2)
    elif code == 6:
        block = data[p] + 1
        p += 1
    elif code == 7:
        block = int.from_bytes(data[p : p + 2], "big") + 1
        p += 2
    elif code >= 8:
        block = 256 << (code - 8)
    else:
        return None
    if rate_code == 12:
        p += 1
    elif rate_code in (13, 14):
        p += 2
    if crc8(data[at:p]) != data[p]:
        return None
    return p + 1 - at, block


def split_frames(data, at):
    frames = []
    while at < len(data):
        head = frame_header(data, at)
        if head is None:
            raise SystemExit("no frame header at byte %d" % at)
        end = at + head[0] + 1
        while True:
            if end >= len(data):
                end = len(data)
                break
            if frame_header(data, end) is not None and crc16(data[at:end]) == 0:
                break
            end += 1
        if crc16(data[at:end]) != 0:
            raise SystemExit("frame at byte %d fails its CRC-16" % at)
        frames.append((head[1], data[at:end]))
        at = end
    return frames


def make_flac(flac, name, rate, channels, bits, seconds, level, block, out, work):
    pcm, total = signal(rate, channels, bits, seconds)
    raw = os.path.join(work, name + ".raw")
    enc = os.path.join(work, name + ".flac")
    dec = os.path.join(work, name + ".dec")
    with open(raw, "wb") as f:
        f.write(pcm)
    raw_opts = ["--force-raw-format", "--endian=little", "--sign=signed"]
    run([flac, "-s", "-f", *raw_opts, "--channels=%d" % channels, "--bps=%d" % bits,
         "--sample-rate=%d" % rate, level, "--blocksize=%d" % block, "--no-padding",
         "--no-seektable", "-o", enc, raw])
    run([flac, "-s", "-f", "-d", *raw_opts, "-o", dec, enc])
    with open(enc, "rb") as f:
        data = f.read()
    with open(dec, "rb") as f:
        reference = f.read()
    if reference != pcm:
        raise SystemExit("%s: flac -d did not give back the input" % name)
    streaminfo, at = read_metadata(data)
    md5 = hashlib.md5(reference).hexdigest()
    if streaminfo[18:34].hex() != md5:
        raise SystemExit("%s: the reference decode does not match the STREAMINFO MD5" % name)
    frames = split_frames(data, at)
    if sum(n for n, _ in frames) != total:
        raise SystemExit("%s: the frames do not add up to the stream" % name)
    version = run([flac, "--version"]).stdout.strip()
    fields = {
        "codec": "flac",
        "sample_format": "pcm_s16le" if bits == 16 else "pcm_s24le",
        "sample_rate_hz": rate,
        "channels": channels,
        "frames_per_chunk": block,
        "codec_config": streaminfo.hex(),
        "chunks": len(frames),
        "chunk_frames": total,
        "reference_frames": total,
        "reference_channels": channels,
        "reference_md5": md5,
        "decode_fnv1a64": "%016x" % fnv1a64(reference),
    }
    notes = [
        "Synthesised by tools/codec-fixtures/generate.py (%d Hz, %d channels, %d bits, %d frames)."
        % (rate, channels, bits, total),
        "Encoded by %s (libFLAC, BSD-3-Clause) with %s --blocksize=%d, one frame per chunk."
        % (version, level, block),
        "Reference: the PCM `flac -d` gave back, equal to the input and to the STREAMINFO MD5.",
        "Channel order is FLAC's (RFC 9639 section 9.1.3); the decode must be bit-exact.",
    ]
    write(out, name, fields, notes, [(n, 0, b) for n, b in frames], reference)
    return name


# --- Opus --------------------------------------------------------------------


def packets(path):
    with open(path, "rb") as f:
        data = f.read()
    at = 0
    out = []
    while at < len(data):
        n, rng = struct.unpack(">II", data[at : at + 8])
        out.append((data[at + 8 : at + 8 + n], rng))
        at += 8 + n
    return out


def opus_frames(packet):
    """PCM frames at 48 kHz (RFC 6716 section 3.1: TOC and frame count)."""
    toc = packet[0]
    config = toc >> 3
    if config < 12:
        size = (480, 960, 1920, 2880)[config & 3]
    elif config < 16:
        size = (480, 960)[config & 1]
    else:
        size = (120, 240, 480, 960)[config & 3]
    count = toc & 3
    n = {0: 1, 1: 2, 2: 2}.get(count)
    if n is None:
        n = packet[1] & 0x3F
    return size * n


def build_tools(work):
    lines = open(os.path.join(OPUS, "chorus-build.txt")).read().splitlines()
    defs = ["-D" + l.split()[1] for l in lines if l.startswith("define ")]
    srcs = [os.path.join(OPUS, l.split()[1]) for l in lines if l.startswith("source ")]
    inc = ["-I" + os.path.join(OPUS, d) for d in ("include", "celt", "silk", "src")]
    ref = os.path.join(work, "opus_ref")
    cmp_ = os.path.join(work, "opus_compare")
    flags = ["-std=c11", "-O2", "-ffp-contract=off", "-fno-fast-math"]
    run(["cc", *flags, *defs, *inc, *srcs, os.path.join(HERE, "opus_ref.c"), "-o", ref, "-lm"])
    run(["cc", "-O2", os.path.join(OPUS, "src", "opus_compare.c"), "-o", cmp_, "-lm"])
    return ref, cmp_


def make_opus(vectors, tools, name, vector, channels, skip, out, work):
    ref_tool, compare = tools
    bit = os.path.join(vectors, "testvector%s.bit" % vector)
    official = os.path.join(vectors, "testvector%s%s.dec" % (vector, "m" if channels == 1 else ""))
    chosen = []
    total = 0
    for packet, rng in packets(bit):
        if total >= EXCERPT_FRAMES:
            break
        n = opus_frames(packet)
        chosen.append((n, rng, packet))
        total += n
    # The official decodes are two-channel files either way: for a mono decode
    # (testvectorNNm.dec) opus_compare averages the two before comparing, so the
    # reference is kept as published and says so in `reference_channels`.
    with open(official, "rb") as f:
        reference = f.read()[skip * 4 : total * 4]
    exact = os.path.join(work, name + ".exact")
    got = run([ref_tool, bit, str(channels), str(len(chosen)), str(skip), exact]).stdout.split()
    stats = dict(kv.split("=") for kv in got)
    ref_file = os.path.join(work, name + ".ref")
    with open(ref_file, "wb") as f:
        f.write(reference)
    args = [compare] + (["-s"] if channels == 2 else []) + [ref_file, exact]
    verdict = subprocess.run(args, capture_output=True, text=True)
    if verdict.returncode != 0:
        raise SystemExit("%s: opus_compare fails: %s" % (name, verdict.stderr))
    quality = verdict.stderr.strip().splitlines()[-1]
    head = b"OpusHead" + bytes([1, channels]) + struct.pack("<HIhB", skip, 48000, 0, 0)
    sizes = {}
    for n, _, _ in chosen:
        sizes[n] = sizes.get(n, 0) + 1
    nominal = max((c, n) for n, c in sizes.items() if n in (120, 240, 480, 960, 1920, 2880))[1]
    fields = {
        "codec": "opus",
        "sample_format": "pcm_s16le",
        "sample_rate_hz": 48000,
        "channels": channels,
        "frames_per_chunk": nominal,
        "codec_config": head.hex(),
        "chunks": len(chosen),
        "chunk_frames": total,
        "reference_frames": total - skip,
        "reference_channels": 2,
        "decode_fnv1a64": stats["fnv1a64"],
    }
    notes = [
        "The first %d packets of testvector%s.bit, RFC 8251 test vectors" % (len(chosen), vector),
        "(https://opus-codec.org/static/testvectors/opus_testvectors-rfc8251.tar.gz), with the",
        "final range each packet carries there. Reference: the same span of testvector%s%s.dec,"
        % (vector, "m" if channels == 1 else ""),
        "the official decode, after the OpusHead pre-skip of %d frames; two channels even for" % skip,
        "a mono decode, which opus_compare averages (its read of file 1, libopus src/opus_compare.c).",
        "decode_fnv1a64 is libopus 1.6.1's exact decode as chorus builds it (fixed point,",
        "third_party/opus/chorus-build.txt), by tools/codec-fixtures/opus_ref.c; against the",
        "reference, opus_compare says: %s" % quality,
    ]
    write(out, name, fields, notes, chosen, reference)
    return name


# --- output ------------------------------------------------------------------


def write(out, name, fields, notes, chunks, reference):
    with open(os.path.join(out, name + ".fields"), "w") as f:
        f.write("# %s: a coded stream as the wire carries it, and its reference decode.\n" % name)
        f.write("# Written by tools/codec-fixtures/generate.py; do not edit by hand.\n")
        for line in notes:
            f.write("# %s\n" % line)
        for k, v in fields.items():
            f.write("%s = %s\n" % (k, v))
    with open(os.path.join(out, name + ".chunks"), "wb") as f:
        for n, rng, data in chunks:
            f.write(struct.pack(">III", n, rng, len(data)))
            f.write(data)
    with open(os.path.join(out, name + ".pcm"), "wb") as f:
        f.write(reference)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--flac", required=True)
    ap.add_argument("--vectors", required=True, help="the unpacked opus_newvectors directory")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    with tempfile.TemporaryDirectory() as work:
        for f in FLAC_FIXTURES:
            print("wrote", make_flac(a.flac, *f, a.out, work))
        tools = build_tools(work)
        for f in OPUS_FIXTURES:
            print("wrote", make_opus(a.vectors, tools, *f, a.out, work))
    return 0


if __name__ == "__main__":
    sys.exit(main())
