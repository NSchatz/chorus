#!/usr/bin/env python3
"""Regenerate fixtures/decode: one short file per settled input format (MP3, FLAC,
Ogg Vorbis, Ogg Opus, ALAC in MP4, WAV), each with what a reference decoder made
of it, and two AAC files the decoders must refuse by name.

Run by `make decode-fixtures`, never by the gate. Standard library only, plus the
reference programs of a pinned conda-forge prefix (REFDEC_PINS below; the prefix
is checked against the pins before anything is written) and the host C compiler
for tools/decode-fixtures/opus_pkt.c. Every program is used as a program; none
of their source was opened (docs/clean-room.md).

Signals (deterministic: integer samples through `round`, no randomness):
- tone: 0.3 s at 44.1 kHz, four tones per channel, different per channel.
- sweep: 0.3 s at 48 kHz, a logarithmic sweep 20 Hz to 20 kHz, the right channel
  inverted at 0.7; made at 16 and at 24 bits.
- gap: 1.0 s of three tones per channel at non-integer frequencies, cut in two at
  the middle frame, so the cut is not at a period boundary; at 44.1 kHz, and at
  48 kHz for Opus. The two halves are encoded as two separate tracks.

Per fixture NAME the directory gets NAME.<ext> (the encoded file), NAME.fields
(key = value: what the file is, the counts, the hashes and tags a test checks,
with the provenance in comments) and, where a test needs the reference samples
and not only their hash, NAME.ref (the reference decode: see fixtures/README.md).
"""

import argparse
import glob
import hashlib
import json
import math
import os
import struct
import subprocess
import sys
import tempfile
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
OPUS = os.path.join(ROOT, "third_party", "opus")
OUT = os.path.join(ROOT, "fixtures", "decode")

# The reference programs: conda-forge packages by name, version, build string and
# the package archive's sha256, as micromamba recorded them in conda-meta/ on
# 2026-10-03. fixtures/README.md carries the same table and the install command.
REFDEC_PINS = {
    "ffmpeg": ("9.0.2", "lgpl_hdc5ac13_803", "cec32eda8d1b08499d999fb7de006b03f29b3e4227ecba8bf32ea085399286f2"),
    "lame": ("4.0", "h770b6ad_1", "560a8561c5cc1f3c05b1e91d93436eb14fe55beb29c27e02623b42960d64d91f"),
    "libflac": ("1.5.0", "he200343_1", "e755e234236bdda3d265ae82e5b0581d259a9279e3e5b31d745dc43251ad64fb"),
    "libogg": ("1.3.5", "hd0c01bc_1", "ffb066ddf2e76953f92e06677021c73c85536098f1c21fcd15360dbc859e22e4"),
    "libopus": ("1.6.1", "hebe6cf0_1", "82873b6c8478e19ab7aef0828ad3619b6633ad185a90a52f39dcb2c30c0c075e"),
    "libsndfile": ("1.2.2", "hbc6d301_3", "3503121a77d76e33f668916b69d4b20cb6a21f62aa4351d1506271ae9d184c61"),
    "libvorbis": ("1.3.7", "h54a6638_2", "ca494c99c7e5ecc1b4cd2f72b5584cef3d4ce631d23511184411abcbb90a21a5"),
    "mpg123": ("1.33.7", "h877a99e_1", "5afc3265622de6663f4179bc9023e1c1cac06b9c8620a0bf54aa0a0c232cf15a"),
    "opus-tools": ("0.2", "h916dfff_0", "63a8f9a187e4af87776096ac892aac82ed0c7778f700a0f9d2363f1896bd5dce"),
}

ARTIST, ALBUM = "the generator", "fixtures/decode"


def tags_of(name):
    """Every fixture carries its own name as its title, so a test can tell two links of a chain apart."""
    return {"title": f"chorus fixture {name}", "artist": ARTIST, "album": ALBUM}


def ff_tags(name):
    return [x for k, v in tags_of(name).items() for x in ("-metadata", f"{k}={v}")]

# ffmpeg writes no version string and no random stream serial with these.
FF_EXACT = ["-fflags", "+bitexact", "-flags:a", "+bitexact"]


def die(msg):
    sys.exit(f"decode-fixtures: {msg}")


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, **kw)
    if r.returncode != 0:
        die(f"{' '.join(cmd)}\n{r.stderr.decode(errors='replace')[-2000:]}")
    return r


def check_prefix(prefix):
    for name, (version, build, sha) in REFDEC_PINS.items():
        metas = glob.glob(os.path.join(prefix, "conda-meta", f"{name}-{version}-{build}.json"))
        if len(metas) != 1:
            die(f"{prefix} does not hold {name} {version} {build}; see fixtures/README.md for the install command")
        got = json.load(open(metas[0])).get("sha256")
        if got != sha:
            die(f"{name} {version} {build} in {prefix} has sha256 {got}, pinned {sha}")


# --- the signals -----------------------------------------------------------------


def tone(rate, n):
    peak = 32767
    out = []
    for i in range(n):
        t = i / rate
        left = sum(math.sin(2 * math.pi * f * t) for f in (441, 1000, 3000, 7000)) / 4
        right = sum(math.sin(2 * math.pi * f * t) for f in (315, 1500, 5000, 11000)) / 4
        out.append((int(round(left * 0.5 * peak)), int(round(right * 0.5 * peak))))
    return out


def sweep(rate, n, bits):
    peak = (1 << (bits - 1)) - 1
    f0, f1 = 20.0, 20000.0
    length = n / rate
    k = math.log(f1 / f0)
    out = []
    for i in range(n):
        t = i / rate
        phase = 2 * math.pi * f0 * length / k * (math.exp(t / length * k) - 1)
        s = math.sin(phase) * 0.5
        out.append((int(round(s * peak)), int(round(-s * peak * 0.7))))
    return out


GAP_LEFT = (440.5, 997.3, 3001.7)
GAP_RIGHT = (311.3, 1499.1, 5003.9)


def gap(rate, n):
    out = []
    for i in range(n):
        t = i / rate
        left = sum(math.sin(2 * math.pi * f * t) for f in GAP_LEFT) / 3
        right = sum(math.sin(2 * math.pi * f * t) for f in GAP_RIGHT) / 3
        out.append((int(round(left * 0.5 * 32767)), int(round(right * 0.5 * 32767))))
    return out


def pack(frames, bits):
    """Interleaved little-endian PCM at the sample's own width (2 or 3 bytes)."""
    if bits == 16:
        return b"".join(struct.pack("<hh", *f) for f in frames)
    return b"".join(struct.pack("<i", s)[:3] for f in frames for s in f)


def write_wav(path, rate, frames, bits):
    w = wave.open(path, "wb")
    w.setnchannels(2)
    w.setsampwidth(bits // 8)
    w.setframerate(rate)
    w.writeframes(pack(frames, bits))
    w.close()


def wav_chunks(path):
    b = open(path, "rb").read()
    assert b[:4] == b"RIFF" and b[8:12] == b"WAVE", path
    pos, fmt, data = 12, None, None
    while pos + 8 <= len(b):
        tag, size = b[pos:pos + 4], struct.unpack("<I", b[pos + 4:pos + 8])[0]
        body = b[pos + 8:pos + 8 + size]
        if tag == b"fmt ":
            fmt = body
        elif tag == b"data":
            data = body if size != 0xFFFFFFFF else b[pos + 8:]
            break
        pos += 8 + size + (size & 1)
    code, channels, rate, _, _, bits = struct.unpack("<HHIIHH", fmt[:16])
    if code == 0xFFFE:
        code = struct.unpack("<H", fmt[24:26])[0]
    return code, channels, rate, bits, data


def s24_from_float(data):
    n = len(data) // 4
    out = bytearray()
    for (x,) in struct.iter_unpack("<f", data[:n * 4]):
        v = max(-8388608, min(8388607, int(round(x * 8388608.0))))
        out += struct.pack("<i", v)[:3]
    return bytes(out)


def s24_from_s32(data):
    out = bytearray()
    for (x,) in struct.iter_unpack("<i", data):
        v = max(-8388608, min(8388607, (x + 128) >> 8))
        out += struct.pack("<i", v)[:3]
    return bytes(out)


def fnv1a64(data):
    h = 0xCBF29CE484222325
    for byte in data:
        h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def f32_bytes_of_ints(pcm, width, bits):
    """The f32 little-endian bytes chorus-decode's exact output has for an integer decode."""
    scale = float(1 << (bits - 1))
    out = bytearray()
    for i in range(0, len(pcm), width):
        v = int.from_bytes(pcm[i:i + width], "little", signed=True)
        out += struct.pack("<f", v / scale)
    return bytes(out)


# --- Ogg, read here so the Opus reference owes nothing to the code under test -----


def ogg_packets(path):
    """The packets of a one-stream Ogg file, and the last page's granule position."""
    b = open(path, "rb").read()
    pos, packets, cur, granule = 0, [], bytearray(), 0
    while pos < len(b):
        assert b[pos:pos + 4] == b"OggS", f"{path}: no page at {pos}"
        granule = struct.unpack("<q", b[pos + 6:pos + 14])[0]
        nseg = b[pos + 26]
        lacing = b[pos + 27:pos + 27 + nseg]
        body = pos + 27 + nseg
        for seg in lacing:
            cur += b[body:body + seg]
            body += seg
            if seg < 255:
                packets.append(bytes(cur))
                cur = bytearray()
        pos = body
    return packets, granule


def build_opus_pkt(tmp):
    lines = open(os.path.join(OPUS, "chorus-build.txt")).read().splitlines()
    defs = [f"-D{x[len('define '):].strip()}" for x in lines if x.startswith("define ")]
    srcs = [os.path.join(OPUS, x[len("source "):].strip()) for x in lines if x.startswith("source ")]
    inc = [f"-I{os.path.join(OPUS, d)}" for d in ("include", "celt", "silk", "src")]
    flags = ["-std=c11", "-O2", "-ffp-contract=off", "-fno-fast-math", "-w"]
    exe = os.path.join(tmp, "opus_pkt")
    run(["cc", *flags, *defs, *inc, *srcs, os.path.join(HERE, "opus_pkt.c"), "-o", exe, "-lm"])
    return exe


# --- one fixture ------------------------------------------------------------------


class Gen:
    def __init__(self, prefix, tmp):
        self.bin = os.path.join(prefix, "bin")
        self.tmp = tmp
        self.opus_pkt = build_opus_pkt(tmp)
        self.written = []

    def tool(self, name):
        return os.path.join(self.bin, name)

    def path(self, name):
        return os.path.join(self.tmp, name)

    def emit(self, name, ext, encoded_path, fields, comment, ref=None):
        data = open(encoded_path, "rb").read()
        open(os.path.join(OUT, f"{name}.{ext}"), "wb").write(data)
        lines = [f"# {name}: {comment[0]}"] + [f"# {c}" for c in comment[1:]]
        lines.append("# Written by tools/decode-fixtures/generate.py (`make decode-fixtures`); do not edit by hand.")
        lines.append(f"file = {name}.{ext}")
        lines.append(f"file_sha256 = {hashlib.sha256(data).hexdigest()}")
        lines += [f"{k} = {v}" for k, v in fields]
        if ref is not None:
            open(os.path.join(OUT, f"{name}.ref"), "wb").write(ref)
        open(os.path.join(OUT, f"{name}.fields"), "w").write("\n".join(lines) + "\n")
        self.written.append(name)
        print(f"  {name}.{ext}: {len(data)} bytes" + (f", ref {len(ref)} bytes" if ref else ""))

    def lossless_fields(self, name, codec, container, rate, bits, pcm, tags=True):
        frames = len(pcm) // (2 * bits // 8)
        fields = [
            ("codec", codec), ("container", container), ("sample_rate_hz", rate), ("channels", 2),
            ("bits", bits), ("frames", frames), ("match", "bit-exact"),
            ("reference_sha256", hashlib.sha256(pcm).hexdigest()),
            ("decode_f32_fnv1a64", fnv1a64(f32_bytes_of_ints(pcm, bits // 8, bits))),
        ]
        if tags:
            fields += list(tags_of(name).items())
        return fields

    # FLAC: the reference encoder and decoder of libFLAC 1.5.0.
    def flac(self, name, rate, bits, frames, extra=()):
        src, enc, dec = self.path(f"{name}.wav"), self.path(f"{name}.flac"), self.path(f"{name}.dec.wav")
        write_wav(src, rate, frames, bits)
        tags = [x for k, v in tags_of(name).items() for x in ("-T", f"{k.upper()}={v}")]
        run([self.tool("flac"), "--silent", "--force", "-8", "--no-padding", *tags, "-o", enc, src])
        run([self.tool("flac"), "--silent", "--force", "-d", "-o", dec, enc])
        _, _, _, got_bits, pcm = wav_chunks(dec)
        if got_bits != bits or pcm != pack(frames, bits):
            die(f"{name}: flac -d did not give the input back")
        self.emit(name, "flac", enc, self.lossless_fields(name, "flac", "flac", rate, bits, pcm) + list(extra), [
            f"{bits}-bit stereo at {rate} Hz, `flac -8 --no-padding` (libFLAC 1.5.0).",
            "Reference: `flac -d` of the same build, equal to the input signal; reference_sha256 is the",
            f"sha256 of that decode as interleaved little-endian {bits}-bit samples ({bits // 8} bytes each).",
            "decode_f32_fnv1a64 is the FNV-1a 64 of the same samples as f32 (sample / 2^(bits-1)),",
            "little-endian: what chorus-server --probe-media prints for an exact decode.",
        ])

    # ALAC in MP4: ffmpeg's ALAC encoder and decoder.
    def alac(self, name, rate, bits, frames, faststart):
        src, enc = self.path(f"{name}.wav"), self.path(f"{name}.m4a")
        write_wav(src, rate, frames, bits)
        fmt = "s16p" if bits == 16 else "s32p"
        flags = ["-movflags", "+faststart"] if faststart else []
        run([self.tool("ffmpeg"), "-v", "error", "-y", "-i", src, *FF_EXACT, "-c:a", "alac", "-sample_fmt", fmt,
             *ff_tags(name), *flags, enc])
        raw = self.path(f"{name}.raw")
        run([self.tool("ffmpeg"), "-v", "error", "-y", "-i", enc, "-f", "s16le" if bits == 16 else "s24le", raw])
        pcm = open(raw, "rb").read()
        if pcm != pack(frames, bits):
            die(f"{name}: ffmpeg's ALAC decode did not give the input back")
        data = open(enc, "rb").read()
        moov, mdat = data.find(b"moov"), data.find(b"mdat")
        if (moov < mdat) != faststart:
            die(f"{name}: moov at {moov}, mdat at {mdat}: not the layout asked for")
        where = "first (faststart)" if faststart else "last (after mdat)"
        self.emit(name, "m4a", enc, self.lossless_fields(name, "alac", "mp4", rate, bits, pcm) + [
            ("moov", "first" if faststart else "last")], [
            f"ALAC, {bits}-bit stereo at {rate} Hz, in MP4 with the moov box {where}; ffmpeg 9.0.2's",
            "ALAC encoder. Reference: ffmpeg's own ALAC decode, equal to the input signal;",
            f"reference_sha256 is its sha256 as interleaved little-endian {bits}-bit samples.",
        ])

    # WAV: the bytes themselves.
    def wav(self, name, rate, bits, frames):
        src = self.path(f"{name}.wav")
        write_wav(src, rate, frames, bits)
        pcm = pack(frames, bits)
        self.emit(name, "wav", src, self.lossless_fields(name, "pcm", "wav", rate, bits, pcm, tags=False), [
            f"{bits}-bit stereo at {rate} Hz, written by Python's `wave` module.",
            "Reference: the samples themselves; reference_sha256 is their sha256 as interleaved",
            f"little-endian {bits}-bit samples.",
        ])

    # MP3: LAME 4.0 encodes (with its Xing/LAME header, so the delay and padding are
    # known), mpg123 1.33.7 decodes.
    def mp3(self, name, rate, frames, with_ref):
        src, enc, dec = self.path(f"{name}.wav"), self.path(f"{name}.mp3"), self.path(f"{name}.dec.wav")
        write_wav(src, rate, frames, 16)
        run([self.tool("lame"), "--quiet", "-b", "128", "--noreplaygain", "--add-id3v2",
             "--tt", tags_of(name)["title"], "--ta", ARTIST, "--tl", ALBUM, src, enc])
        run([self.tool("mpg123"), "-q", "-e", "f32", "-w", dec, enc])
        code, channels, got_rate, bits, data = wav_chunks(dec)
        if (code, channels, got_rate, bits) != (3, 2, rate, 32):
            die(f"{name}: mpg123 wrote format {code}, {channels} channels, {got_rate} Hz, {bits} bits")
        ref = s24_from_float(data)
        ref_frames = len(ref) // 6
        if ref_frames != len(frames):
            die(f"{name}: mpg123 decoded {ref_frames} frames of {len(frames)}: the gapless header was not honoured")
        fields = [("codec", "mp3"), ("container", "mp3"), ("sample_rate_hz", rate), ("channels", 2),
                  ("frames", ref_frames), ("match", "iso-11172-4-full-accuracy" if with_ref else "frames-and-join"),
                  ("reference_sha256", hashlib.sha256(ref).hexdigest())] + list(tags_of(name).items())
        self.emit(name, "mp3", enc, fields, [
            f"MPEG-1 Layer III, 128 kbit/s stereo at {rate} Hz, `lame -b 128 --noreplaygain` (LAME 4.0),",
            "with LAME's Xing/LAME header (encoder delay and padding) and an ID3v2 tag.",
            "Reference: `mpg123 -e f32 -w` (mpg123 1.33.7, gapless on), rounded to 24 bits;",
            "reference_sha256 is its sha256 as interleaved little-endian 24-bit samples (3 bytes each)"
            + (", and NAME.ref is those bytes." if with_ref else "."),
        ], ref if with_ref else None)

    # Ogg Vorbis: libvorbis 1.3.7 encodes (through ffmpeg) and decodes (through
    # libsndfile's sndfile-convert: ffmpeg's own libvorbis decode returned short
    # lengths, docs/decisions, the server decoders record).
    def vorbis(self, name, rate, frames, with_ref, serial):
        src, enc, dec = self.path(f"{name}.wav"), self.path(f"{name}.ogg"), self.path(f"{name}.dec.wav")
        write_wav(src, rate, frames, 16)
        run([self.tool("ffmpeg"), "-v", "error", "-y", "-i", src, *FF_EXACT, "-c:a", "libvorbis", "-q:a", "4",
             *ff_tags(name), "-serial_offset", str(serial), enc])
        run([self.tool("sndfile-convert"), "-pcm32", enc, dec])
        code, channels, got_rate, bits, data = wav_chunks(dec)
        if (code, channels, got_rate, bits) != (1, 2, rate, 32):
            die(f"{name}: sndfile-convert wrote format {code}, {channels} channels, {got_rate} Hz, {bits} bits")
        ref = s24_from_s32(data)
        ref_frames = len(ref) // 6
        if ref_frames != len(frames):
            die(f"{name}: libvorbis decoded {ref_frames} frames of {len(frames)}")
        fields = [("codec", "vorbis"), ("container", "ogg"), ("sample_rate_hz", rate), ("channels", 2),
                  ("frames", ref_frames), ("match", "iso-11172-4-full-accuracy" if with_ref else "frames-and-join"),
                  ("reference_sha256", hashlib.sha256(ref).hexdigest())] + list(tags_of(name).items())
        self.emit(name, "ogg", enc, fields, [
            f"Vorbis in Ogg, quality 4, stereo at {rate} Hz, libvorbis 1.3.7 through `ffmpeg -c:a libvorbis`.",
            "Reference: libvorbis 1.3.7's decode through `sndfile-convert -pcm32` (libsndfile 1.2.2),",
            "rounded to 24 bits; reference_sha256 is its sha256 as interleaved little-endian 24-bit samples"
            + (", and NAME.ref is those bytes." if with_ref else "."),
        ], ref if with_ref else None)

    # Ogg Opus: libopus 1.6.1 encodes (through ffmpeg; opusenc of opus-tools 0.2 does
    # not start in this prefix). Two references: opusdec's decode, for opus_compare,
    # and the exact decode of libopus 1.6.1 as chorus builds it, over packets this
    # script took out of the Ogg pages itself.
    def opus(self, name, rate, frames, with_ref, serial):
        src, enc, dec = self.path(f"{name}.wav"), self.path(f"{name}.opus"), self.path(f"{name}.dec.wav")
        write_wav(src, rate, frames, 16)
        run([self.tool("ffmpeg"), "-v", "error", "-y", "-i", src, *FF_EXACT, "-c:a", "libopus", "-b:a", "96k",
             *ff_tags(name), "-serial_offset", str(serial), enc])
        run([self.tool("opusdec"), "--quiet", "--rate", "48000", "--no-dither", enc, dec])
        code, channels, got_rate, bits, official = wav_chunks(dec)
        if (code, channels, got_rate, bits) != (1, 2, 48000, 16):
            die(f"{name}: opusdec wrote format {code}, {channels} channels, {got_rate} Hz, {bits} bits")
        packets, granule = ogg_packets(enc)
        head = packets[0]
        if head[:8] != b"OpusHead" or packets[1][:8] != b"OpusTags":
            die(f"{name}: not an Ogg Opus stream")
        channels, pre_skip, input_rate, gain, family = head[9], *struct.unpack("<HIh", head[10:18]), head[18]
        if (channels, family, gain) != (2, 0, 0):
            die(f"{name}: {channels} channels, mapping family {family}, output gain {gain}")
        pkts = self.path(f"{name}.pkts")
        with open(pkts, "wb") as f:
            for p in packets[2:]:
                f.write(struct.pack(">I", len(p)) + p)
        raw = self.path(f"{name}.s24")
        run([self.opus_pkt, pkts, "2", raw])
        exact = open(raw, "rb").read()
        total = len(exact) // 8
        want = granule - pre_skip  # RFC 7845 section 4.4: the PCM length is the last granule less the pre-skip
        if not 0 < want <= total - pre_skip:
            die(f"{name}: granule {granule}, pre-skip {pre_skip}, decoded {total}")
        exact = exact[pre_skip * 8:(pre_skip + want) * 8]
        exact24 = b"".join(exact[i:i + 3] for i in range(0, len(exact), 4))
        if len(official) // 4 != want:
            die(f"{name}: opusdec decoded {len(official) // 4} frames, the granule position says {want}")
        fields = [("codec", "opus"), ("container", "ogg"), ("sample_rate_hz", 48000), ("channels", 2),
                  ("frames", want), ("pre_skip", pre_skip), ("input_sample_rate_hz", input_rate),
                  ("output_gain_q8", gain), ("match", "opus"),
                  ("reference_sha256", hashlib.sha256(official).hexdigest()),
                  ("exact_sha256", hashlib.sha256(exact24).hexdigest()),
                  ("decode_f32_fnv1a64", fnv1a64(f32_bytes_of_ints(exact24, 3, 24)))] + list(tags_of(name).items())
        self.emit(name, "opus", enc, fields, [
            f"Opus in Ogg (RFC 7845), 96 kbit/s stereo from a {rate} Hz source, libopus 1.6.1 through",
            "`ffmpeg -c:a libopus`; mapping family 0. Decoded at 48 kHz, as Opus always is here.",
            "Reference (reference_sha256" + (", NAME.ref" if with_ref else "")
            + "): `opusdec --rate 48000 --no-dither` (opus-tools 0.2 over",
            "libopus 1.6.1, a float build), 16-bit little-endian: what opus_compare judges against.",
            "exact_sha256: the decode of libopus 1.6.1 as chorus builds it (fixed point,",
            "third_party/opus/chorus-build.txt) by tools/decode-fixtures/opus_pkt.c over the packets this",
            "script read out of the Ogg pages, after the pre-skip and the end trim of RFC 7845 section 4,",
            "as interleaved little-endian 24-bit samples (3 bytes each). decode_f32_fnv1a64 is the",
            "FNV-1a 64 of the same samples as little-endian f32 (sample / 2^23).",
        ], official if with_ref else None)

    # AAC, which chorus never decodes (proposal P9): two files to refuse by name.
    def aac(self, name, container, rate, frames):
        src = self.path(f"{name}.wav")
        write_wav(src, rate, frames, 16)
        ext = "m4a" if container == "mp4" else "aac"
        enc = self.path(f"{name}.{ext}")
        extra = ["-movflags", "+faststart"] if container == "mp4" else ["-f", "adts"]
        run([self.tool("ffmpeg"), "-v", "error", "-y", "-i", src, *FF_EXACT, "-c:a", "aac", "-b:a", "64k", *extra, enc])
        self.emit(name, ext, enc, [("codec", "aac"), ("container", container), ("match", "refused"),
                                   ("refusal", "unsupported: aac")], [
            f"AAC-LC in {'MP4' if container == 'mp4' else 'ADTS'}, ffmpeg 9.0.2's own AAC encoder. chorus never compiles",
            "an AAC decoder (docs/proposals/P9-decoders.md); the file exists to be refused by name.",
        ])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--prefix", required=True, help="the conda-forge prefix holding the reference programs")
    args = ap.parse_args()
    check_prefix(args.prefix)
    os.makedirs(OUT, exist_ok=True)
    for old in os.listdir(OUT):
        os.remove(os.path.join(OUT, old))
    with tempfile.TemporaryDirectory() as tmp:
        g = Gen(args.prefix, tmp)
        tone44 = tone(44100, 13230)
        sweep16 = sweep(48000, 14400, 16)
        sweep24 = sweep(48000, 14400, 24)
        gap44 = gap(44100, 44100)
        gap48 = gap(48000, 48000)
        gap_fields = lambda full, cut: [  # noqa: E731
            ("gap_total_frames", len(full)), ("gap_cut_frame", cut),
            ("gap_original_sha256", hashlib.sha256(pack(full, 16)).hexdigest())]

        g.wav("wav-tone44-s16", 44100, 16, tone44)
        g.wav("wav-sweep48-s24", 48000, 24, sweep24)
        g.flac("flac-tone44-s16", 44100, 16, tone44)
        g.flac("flac-sweep48-s24", 48000, 24, sweep24)
        g.flac("flac-gap44-a", 44100, 16, gap44[:22050], gap_fields(gap44, 22050))
        g.flac("flac-gap44-b", 44100, 16, gap44[22050:], gap_fields(gap44, 22050))
        g.alac("alac-tone44-s16-moov-first", 44100, 16, tone44, True)
        g.alac("alac-sweep48-s24-moov-last", 48000, 24, sweep24, False)
        g.mp3("mp3-tone44", 44100, tone44, True)
        g.mp3("mp3-sweep48", 48000, sweep16, True)
        g.mp3("mp3-gap44-a", 44100, gap44[:22050], False)
        g.mp3("mp3-gap44-b", 44100, gap44[22050:], False)
        g.vorbis("vorbis-tone44", 44100, tone44, True, 1)
        g.vorbis("vorbis-sweep48", 48000, sweep16, True, 2)
        g.vorbis("vorbis-gap44-a", 44100, gap44[:22050], False, 3)
        g.vorbis("vorbis-gap44-b", 44100, gap44[22050:], False, 4)
        g.opus("opus-tone44", 44100, tone44, True, 5)
        g.opus("opus-sweep48", 48000, sweep16, True, 6)
        g.opus("opus-gap48-a", 48000, gap48[:24000], False, 7)
        g.opus("opus-gap48-b", 48000, gap48[24000:], False, 8)
        g.aac("aac-in-mp4", "mp4", 44100, tone44[:4410])
        g.aac("aac-adts", "adts", 44100, tone44[:4410])
    total = sum(os.path.getsize(os.path.join(OUT, f)) for f in os.listdir(OUT))
    print(f"decode-fixtures: {len(g.written)} fixtures, {len(os.listdir(OUT))} files, {total} bytes in fixtures/decode")


if __name__ == "__main__":
    main()
