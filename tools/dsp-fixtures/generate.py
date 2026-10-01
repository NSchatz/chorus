#!/usr/bin/env python3
"""Regenerate fixtures/dsp: the DSP library's shared contract (goal 12).

Never run by the gate; run by hand from the repository root:
`python3 tools/dsp-fixtures/generate.py`. Standard library only.

Every expected value a block fixture carries is either a number printed in
the cited source (ITU-R BS.1770-5's K-weighting coefficients; the ISO 226:2003
parameter table as reproduced) or a property the cited source states (the RBJ
cookbook's magnitudes at f0, DC and Nyquist; Linkwitz-Riley's -6.02 dB, in
phase and flat sum; the Giannoulis et al. static curve; MathWorks' 10-90 %
time constants; a brickwall limiter's "never exceeds"). Where a value has to
be computed from the source (a curve at a given input, a magnitude of a
designed filter, a contour level), it is computed HERE, in Python, from the
source's formula, independently of crates/dsp and firmware/src/dsp.c.

The chain fixtures' `samples.<o>` lines are the exception and say so: they are
the Rust implementation's own output (the `chain_samples` example), there to
hold the C chain to the Rust chain sample for sample. Their analytic checks
(`amplitude.<o>`, `sum_amplitude`, `exact.<o>`) are computed here. An amplitude
is measured as sqrt(2 x mean square) over a whole number of cycles.
"""

import cmath
import math
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "fixtures", "dsp")
READ = "2026-10-01"

RBJ = "https://www.w3.org/TR/audio-eq-cookbook/"
ITU = "https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.1770-5-202311-I!!PDF-E.pdf"
PYLN = "https://github.com/csteinmetz1/pyloudnorm/blob/master/pyloudnorm/meter.py"
RANE = "https://www.ranecommercial.com/legacy/note160.html"
JOS = "https://ccrma.stanford.edu/~jos/pasp/Delay_Lines.html"
STEIN = ("https://www.steinberg.help/r/groove-agent/6.0/en/halion/topics/"
         "effects_reference/brickwalllimiter_r.html")
GMR = "https://secure.aes.org/forum/pubs/journal/?ID=174"
MW = "https://www.mathworks.com/help/audio/ref/compressor-system-object.html"
ISO = "https://www.iso.org/standard/34222.html"
ISO_TABLE = "https://www.dsprelated.com/showcode/174.php"
BASS = "https://en.wikipedia.org/wiki/Bass_management"
ATSC = "https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf"
GEIGER = "https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570096395.pdf"
SVS = ("https://www.svsound.com/blogs/subwoofer-setup-and-tuning/"
       "tips-for-setting-the-proper-crossover-frequency-for-a-subwoofer")
DSPDOC = "docs/dsp.md"


# --- the cookbook, written from the W3C note -------------------------------

def rbj(kind, fs, f0, q, gain_db):
    a = 10 ** (gain_db / 40)
    w0 = 2 * math.pi * f0 / fs
    c, s = math.cos(w0), math.sin(w0)
    al = s / (2 * q)
    sa = 2 * math.sqrt(a) * al
    t = {
        "lowpass": ((1 - c) / 2, 1 - c, (1 - c) / 2, 1 + al, -2 * c, 1 - al),
        "highpass": ((1 + c) / 2, -(1 + c), (1 + c) / 2, 1 + al, -2 * c, 1 - al),
        "bandpass": (al, 0, -al, 1 + al, -2 * c, 1 - al),
        "notch": (1, -2 * c, 1, 1 + al, -2 * c, 1 - al),
        "allpass": (1 - al, -2 * c, 1 + al, 1 + al, -2 * c, 1 - al),
        "peaking": (1 + al * a, -2 * c, 1 - al * a, 1 + al / a, -2 * c, 1 - al / a),
        "lowshelf": (a * ((a + 1) - (a - 1) * c + sa), 2 * a * ((a - 1) - (a + 1) * c),
                     a * ((a + 1) - (a - 1) * c - sa), (a + 1) + (a - 1) * c + sa,
                     -2 * ((a - 1) + (a + 1) * c), (a + 1) + (a - 1) * c - sa),
        "highshelf": (a * ((a + 1) + (a - 1) * c + sa), -2 * a * ((a - 1) + (a + 1) * c),
                      a * ((a + 1) + (a - 1) * c - sa), (a + 1) - (a - 1) * c + sa,
                      2 * ((a - 1) - (a + 1) * c), (a + 1) - (a - 1) * c - sa),
    }[kind]
    b0, b1, b2, a0, a1, a2 = t
    return (b0 / a0, b1 / a0, b2 / a0), (a1 / a0, a2 / a0)


def resp(b, a, f, fs):
    z1 = cmath.exp(-1j * 2 * math.pi * f / fs)
    return (b[0] + b[1] * z1 + b[2] * z1 * z1) / (1 + a[0] * z1 + a[1] * z1 * z1)


def lr4(fs, fc, f):
    q = 1 / math.sqrt(2)
    lo = resp(*rbj("lowpass", fs, fc, q, 0), f, fs) ** 2
    hi = resp(*rbj("highpass", fs, fc, q, 0), f, fs) ** 2
    return lo, hi


def db(x):
    return 20 * math.log10(x)


def fmt(x):
    if x == float("-inf"):
        return "-inf"
    return repr(float(x)) if not float(x).is_integer() else str(int(x)) if abs(x) < 1e15 else repr(x)


def nums(xs):
    return " ".join(fmt(x) for x in xs)


def write(name, header, pairs):
    path = os.path.join(OUT, name)
    with open(path, "w", encoding="utf-8") as fh:
        for line in header.strip("\n").split("\n"):
            fh.write(("# " + line).rstrip() + "\n")
        fh.write("\n")
        for k, v in pairs:
            if k.startswith("#"):
                fh.write(k + "\n")
            else:
                fh.write(f"{k} = {v}\n")
    return path


# --- 1. biquads -----------------------------------------------------------

def biquads():
    fs = 48000
    q = 1 / math.sqrt(2)
    itu1_b = (1.53512485958697, -2.69169618940638, 1.19839281085285)
    itu1_a = (-1.69065929318241, 0.73248077421585)
    itu2_a = (-1.99004745483398, 0.99007225036621)
    common = [("source", f"{ITU} {PYLN}"), ("read", READ)]

    b, a = rbj("highshelf", fs, 1500, q, 4.0)
    worst = max(abs(x - y) for x, y in zip(b + a, itu1_b + itu1_a))
    write("kweight-stage1-highshelf.txt", f"""
ITU-R BS.1770-5 (11/2023) Annex 1 Table 1 prints the K-weighting's stage 1
(the head's shelving pre-filter) at 48 kHz. pyloudnorm (MIT) reproduces it with
the cookbook high shelf at fc = 1500 Hz, G = 4.0 dB, Q = 1/sqrt 2 (meter.py,
"high_shelf", read {READ}). The cookbook design at those parameters lands
within {worst:.2e} of every printed coefficient; the tolerance is 2e-4.
""", [("kind", "biquad_coefficients"), *common, ("type", "highshelf"), ("rate_hz", fs),
      ("f0_hz", 1500), ("q", repr(q)), ("gain_db", 4),
      ("expect_b", nums(itu1_b)), ("expect_a", nums(itu1_a)), ("tolerance", "2e-4")])

    b, a = rbj("highpass", fs, 38, 0.5, 0)
    worst = max(abs(x - y) for x, y in zip(a, itu2_a))
    write("kweight-stage2-highpass.txt", f"""
ITU-R BS.1770-5 Annex 1 Table 2: the K-weighting's stage 2 high-pass at 48 kHz,
printed as b = 1, -2, 1 (not normalised) and a1, a2. pyloudnorm (MIT) reproduces
it with the cookbook high-pass at fc = 38 Hz, Q = 0.5. The denominator lands
within {worst:.2e} of the printed one (tolerance 5e-5); the numerator, divided by
its b0, must be the printed 1, -2, 1 (tolerance 1e-12): the cookbook's
normalisation scales it by (1 + cos w0)/(2 a0), a passband gain of 1 rather
than the printed filter's 1.0050.
""", [("kind", "biquad_coefficients"), *common, ("type", "highpass"), ("rate_hz", fs),
      ("f0_hz", 38), ("q", 0.5), ("gain_db", 0), ("expect_b_ratio", "1 -2 1"),
      ("expect_a", nums(itu2_a)), ("tolerance", "5e-5"), ("tolerance_ratio", "1e-12")])

    freqs = [20, 100, 500, 1000, 1500, 2000, 5000, 10000, 20000]
    worst = max(abs(db(abs(resp(*rbj("highshelf", fs, 1500, q, 4.0), f, fs)))
                    - db(abs(resp(itu1_b, itu1_a, f, fs)))) for f in freqs)
    write("kweight-stage1-response.txt", f"""
The same stage 1, judged by what it does rather than by its coefficients: the
magnitude of the cookbook design and of the filter ITU-R BS.1770-5 Table 1
prints, compared at each frequency (largest difference here {worst:.4f} dB;
tolerance 0.01 dB). And the running f32 section's impulse response against the
printed filter's, run in f64 Direct Form I (tolerance 3e-4 per sample over
64 samples: the coefficient difference above, rounded to f32).
""", [("kind", "biquad_reference"), *common, ("type", "highshelf"), ("rate_hz", fs),
      ("f0_hz", 1500), ("q", repr(q)), ("gain_db", 4), ("ref_b", nums(itu1_b)),
      ("ref_a", nums(itu1_a)), ("freqs_hz", nums(freqs)), ("tolerance_db", "0.01"),
      ("impulse_frames", 64), ("tolerance_impulse", "3e-4")])

    rbj_common = [("source", RBJ), ("read", READ)]
    props = [
        ("rbj-lowpass.txt", "lowpass", 1000, 2.0, 0, [0, 1000, 24000],
         [0, db(2.0), float("-inf")],
         "LPF, H(s) = 1/(s^2 + s/Q + 1): |H| = Q at f0 (here Q = 2, +6.0206 dB), 1 at DC, and a\n"
         "double zero at Nyquist (the cookbook's b0 + b2 = b1 there)."),
        ("rbj-highpass.txt", "highpass", 1000, q, 0, [0, 1000, 24000],
         [float("-inf"), db(q), 0],
         "HPF, H(s) = s^2/(s^2 + s/Q + 1): |H| = Q at f0 (Q = 1/sqrt 2, -3.0103 dB), a double\n"
         "zero at DC, 1 at Nyquist."),
        ("rbj-bandpass.txt", "bandpass", 1000, 2.0, 0, [0, 1000, 24000],
         [float("-inf"), 0, float("-inf")],
         "BPF (constant 0 dB peak gain), H(s) = (s/Q)/(s^2 + s/Q + 1): 0 dB at f0, zeros at DC\n"
         "and Nyquist."),
        ("rbj-notch.txt", "notch", 1000, 2.0, 0, [0, 1000, 24000],
         [0, float("-inf"), 0],
         "notch, H(s) = (s^2 + 1)/(s^2 + s/Q + 1): a zero at f0, 1 at DC and Nyquist."),
        ("rbj-allpass.txt", "allpass", 1000, 2.0, 0, [0, 100, 1000, 10000, 24000],
         [0, 0, 0, 0, 0],
         "APF, H(s) = (s^2 - s/Q + 1)/(s^2 + s/Q + 1): unit magnitude everywhere."),
        ("rbj-peaking.txt", "peaking", 1000, 1.0, 6, [0, 1000, 24000],
         [0, 6, 0],
         "peakingEQ: dBgain at f0 (the analogue prototype's A^2 at s = j), 0 dB at DC and\n"
         "Nyquist."),
        ("rbj-peaking-cut.txt", "peaking", 250, 4.0, -12, [0, 250, 24000],
         [0, -12, 0],
         "peakingEQ, a cut (the room-correction case): -12 dB at f0, 0 dB at DC and Nyquist."),
        ("rbj-lowshelf.txt", "lowshelf", 200, q, 12, [0, 200, 24000],
         [12, 6, 0],
         "lowShelf: dBgain towards DC, and f0 is the \"shelf midpoint frequency\" with\n"
         "\"midpoint (dBgain/2)\" gain; 0 dB at Nyquist."),
        ("rbj-highshelf.txt", "highshelf", 5000, q, -9, [0, 5000, 24000],
         [0, -4.5, -9],
         "highShelf: 0 dB at DC, dBgain/2 at the midpoint f0, dBgain at Nyquist."),
    ]
    for name, kind, f0, qq, g, freqs, expect, text in props:
        write(name, f"The RBJ cookbook as the W3C note gives it. {text}\n"
              "-inf: the magnitude must be below -100 dB.",
              [("kind", "biquad_magnitude"), *rbj_common, ("type", kind), ("rate_hz", fs),
               ("f0_hz", f0), ("q", repr(qq)), ("gain_db", g), ("freqs_hz", nums(freqs)),
               ("expect_db", nums(expect)), ("tolerance_db", "1e-6")])


# --- 2. LR4 ---------------------------------------------------------------

def crossovers():
    for fs, fc in [(48000, 80), (48000, 2000), (96000, 120)]:
        freqs = [fc / 8, fc / 2, fc, fc * 2, fc * 8]
        worst = max(abs(db(abs(sum(lr4(fs, fc, f))))) for f in freqs)
        write(f"lr4-{fc}hz-{fs // 1000}k.txt", f"""
Linkwitz-Riley 4th order (RaneNote 160): two cascaded 2nd-order Butterworth
sections per branch; each branch -6 dB at the crossover (.707 x .707 = .5, so
20 log10 0.5 = -6.0206 dB); the branches "everywhere in phase"; "the summed
response is perfectly flat" (an all-pass). Checked on the designed response
(sum here within {worst:.1e} dB) and on the running f32 split: a sine at the
crossover, amplitude 1, comes out of each branch at amplitude 0.5 once settled
(sqrt(2 x mean square) over the last tenth, a whole number of cycles;
tolerance 2e-4).
""", [("kind", "lr4"), ("source", f"{RANE} https://www.linkwitzlab.com/filters.htm"),
          ("read", READ), ("rate_hz", fs), ("crossover_hz", fc),
          ("at_crossover_db", "-6.020599913279624"), ("tolerance_db", "1e-6"),
          ("freqs_hz", nums(freqs)), ("sum_tolerance_db", "1e-6"),
          ("phase_tolerance_deg", "1e-6"), ("sine_frames", 48000), ("sine_tolerance", "2e-4")])


# --- 3. delay -------------------------------------------------------------

def delays():
    write("delay-37.txt", """
A delay line of M samples: y(n) = x(n - M), starting from zeros (J. O. Smith
III, Physical Audio Signal Processing, Delay Lines). Exact. The fixed maximum
is 4800 frames (50 ms at 96 kHz): 4800 is taken, 4801 refused.
""", [("kind", "delay"), ("source", JOS), ("read", READ), ("delay_frames", 37),
          ("frames", 400), ("input", "ramp 0.001 + noise 5 0.25"), ("rate_hz", 48000),
          ("max_frames", 4800), ("refuse_frames", 4801)])
    write("delay-zero.txt", "A zero-sample delay is a wire: y(n) = x(n). Exact.",
          [("kind", "delay"), ("source", JOS), ("read", READ), ("delay_frames", 0),
           ("frames", 64), ("input", "noise 9 0.5"), ("rate_hz", 48000),
           ("max_frames", 4800), ("refuse_frames", 4801)])


# --- 4. limiter -----------------------------------------------------------

def limiters():
    write("limiter-ceiling.txt", """
A brickwall limiter: "the output level never exceeds a set limit" (Steinberg).
Two channels, linked; bursts four and eight times the ceiling; every output
sample of both channels must be within [-ceiling, ceiling], and once the
release has returned the gain to unity the output is the input delayed by the
look-ahead, bit for bit (checked over the last 4800 frames).
""", [("kind", "limiter"), ("source", STEIN), ("read", READ), ("rate_hz", 48000),
          ("ceiling", 0.5), ("lookahead_frames", 96), ("release_ms", 50), ("frames", 48000),
          ("input.0", "burst 1000 2 0 3000 + noise 7 0.1"),
          ("input.1", "burst 3000 4 1000 2000 + noise 8 0.1"),
          ("check", "ceiling"), ("unity_tail_frames", 4800)])
    write("limiter-unity.txt", """
Input that never exceeds the ceiling passes at unity gain: the output is the
input delayed by the look-ahead, bit for bit, on every frame.
""", [("kind", "limiter"), ("source", STEIN), ("read", READ), ("rate_hz", 48000),
          ("ceiling", 0.5), ("lookahead_frames", 96), ("release_ms", 100), ("frames", 9600),
          ("input.0", "noise 3 0.4 + sine 440 0.09"), ("input.1", "sine 97 0.49"),
          ("check", "unity"), ("unity_tail_frames", 9600)])
    write("limiter-ceiling-low.txt", """
The room's limit below unity (the chain's ceiling is min(1, limit gain)): a
ceiling of 0.1 against full-scale noise, one channel, a 1 ms look-ahead at
96 kHz. Every sample within the ceiling.
""", [("kind", "limiter"), ("source", STEIN), ("read", READ), ("rate_hz", 96000),
          ("ceiling", 0.1), ("lookahead_frames", 96), ("release_ms", 100), ("frames", 19200),
          ("input.0", "noise 21 1"), ("check", "ceiling"), ("unity_tail_frames", 0)])


# --- 5. compressor --------------------------------------------------------

def gc(x, t, r, w):
    if 2 * (x - t) < -w:
        return x
    if w > 0 and 2 * abs(x - t) <= w:
        return x + (1 / r - 1) * (x - t + w / 2) ** 2 / (2 * w)
    return t + (x - t) / r


def compressors():
    for name, t, r, w in [("compressor-static-soft.txt", -20, 4, 10),
                          ("compressor-static-hard.txt", -10, 2, 0),
                          ("compressor-static-night.txt", -24, 3, 12)]:
        xs = [-60, t - w / 2 - 1, t - w / 4, t, t + w / 4, t + w / 2 + 1, 0]
        write(name, f"""
The gain computer's static characteristic, Giannoulis, Massberg and Reiss
(JAES 2012), as MathWorks prints it citing them: y = x below the knee,
y = x + (1/R - 1)(x - T + W/2)^2/(2W) inside it, y = T + (x - T)/R above.
T = {t} dB, R = {r}, W = {w} dB. Expected values computed from that equation.
""", [("kind", "compressor_static"), ("source", f"{GMR} {MW}"), ("read", READ),
              ("threshold_db", t), ("ratio", r), ("knee_db", w), ("inputs_db", nums(xs)),
              ("expect_db", nums([gc(x, t, r, w) for x in xs])), ("tolerance_db", "1e-9")])
    for fs, at, rel in [(48000, 10, 100), (44100, 5, 200)]:
        write(f"compressor-timing-{fs}.txt", f"""
The smoothing's time constants as MathWorks defines them (citing Giannoulis et
al.): alpha = exp(-ln 9 / (Fs T)), so T is the 10 % to 90 % time of a step. A
step of the computed gain from 0 to -10 dB crosses -1 dB and -9 dB
attack_ms x rate / 1000 frames apart (here {at * fs / 1000:g}), and back up crosses
-9 and -1 dB release_ms x rate / 1000 apart ({rel * fs / 1000:g}), each within one frame.
""", [("kind", "compressor_timing"), ("source", MW), ("read", READ), ("rate_hz", fs),
              ("attack_ms", at), ("release_ms", rel), ("step_db", -10),
              ("tolerance_frames", 1)])


# --- 6. ISO 226 and loudness ------------------------------------------------

FREQS = [20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000,
         1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000, 12500]
AF = [0.532, 0.506, 0.480, 0.455, 0.432, 0.409, 0.387, 0.367, 0.349, 0.330, 0.315, 0.301,
      0.288, 0.276, 0.267, 0.259, 0.253, 0.250, 0.246, 0.244, 0.243, 0.243, 0.243, 0.242,
      0.242, 0.245, 0.254, 0.271, 0.301]
LU = [-31.6, -27.2, -23.0, -19.1, -15.9, -13.0, -10.3, -8.1, -6.2, -4.5, -3.1, -2.0, -1.1,
      -0.4, 0.0, 0.3, 0.5, 0.0, -2.7, -4.1, -1.0, 1.7, 2.5, 1.2, -2.1, -7.1, -11.2, -10.7, -3.1]
TF = [78.5, 68.7, 59.5, 51.1, 44.0, 37.5, 31.5, 26.5, 22.1, 17.9, 14.4, 11.4, 8.6, 6.2, 4.4,
      3.0, 2.2, 2.4, 3.5, 1.7, -1.3, -4.2, -6.0, -5.4, -1.5, 6.0, 12.6, 13.9, 12.3]


def spl(f, phon):
    i = FREQS.index(f)
    af = 4.47e-3 * (10 ** (0.025 * phon) - 1.15) + (0.4 * 10 ** ((TF[i] + LU[i]) / 10 - 9)) ** AF[i]
    return (10 / AF[i]) * math.log10(af) - LU[i] + 94


def loudness():
    rows = [20, 50, 100, 1000, 4000, 10000, 12500]
    write("iso226-table.txt", f"""
ISO 226:2003's parameter table (af, Lu, Tf), the rows this fixture names, as
reproduced at {ISO_TABLE} (read {READ}); the implementations' tables must equal
them (tolerance 1e-12). Then the standard's formula (the same page):
Af = 4.47e-3 (10^(0.025 Ln) - 1.15) + (0.4 10^((Tf + Lu)/10 - 9))^af,
Lp = (10/af) log10(Af) - Lu + 94, at each (frequency, phon) pair, computed here
from the cited table and formula.
""", [("kind", "iso226"), ("source", f"{ISO} {ISO_TABLE}"), ("read", READ),
          ("table_freqs_hz", nums(rows)),
          ("af", nums([AF[FREQS.index(f)] for f in rows])),
          ("lu", nums([LU[FREQS.index(f)] for f in rows])),
          ("tf", nums([TF[FREQS.index(f)] for f in rows])),
          ("freqs_hz", nums([20, 50, 100, 1000, 1000, 1000, 4000, 10000, 12500, 50])),
          ("phon", nums([40, 40, 60, 20, 40, 80, 60, 80, 40, 80])),
          ("expect_spl_db", nums([spl(f, p) for f, p in zip(
              [20, 50, 100, 1000, 1000, 1000, 4000, 10000, 12500, 50],
              [40, 40, 60, 20, 40, 80, 60, 80, 40, 80])])),
          ("tolerance_db", "1e-9")])

    def gains(att):
        if att <= 0:
            return 0.0, 0.0
        lvl = min(max(80 - att, 20), 80)
        def need(f):
            return (spl(f, lvl) - spl(1000, lvl)) - (spl(f, 80) - spl(1000, 80))
        return min(max(need(50), 0), 12), min(max(need(10000), 0), 6)

    atts = [0, 6, 10, 20, 30, 40, 60]
    lows, highs = zip(*[gains(a) for a in atts])
    room = [1, 0.5, 0.1, 0.01, 0.001, 0]
    def quant(g):
        att = -20 * math.log10(g) if g > 0 else 80
        att = min(max(att, 0), 60)
        return math.floor(att / 0.5 + 0.5) * 0.5
    write("loudness-compensation.txt", f"""
chorus's loudness compensation (docs/dsp.md; every constant ASSUMED there):
listening at L = 80 - att phon (held to 20..80), the boost a frequency needs is
(Lp(f, L) - Lp(1k, L)) - (Lp(f, 80) - Lp(1k, 80)), taken at 50 Hz for the low
shelf (capped at +12 dB) and at 10 kHz for the high shelf (capped at +6 dB),
never negative. Lp is ISO 226:2003 (the table and formula cited in
iso226-table.txt); the expected gains are computed here from them. The room
gain's attenuation is quantised to 0.5 dB steps (0 is the bottom, 60 dB).
""", [("kind", "loudness"), ("source", f"{ISO_TABLE} {DSPDOC}"), ("read", READ),
          ("attenuations_db", nums(atts)), ("expect_low_db", nums(lows)),
          ("expect_high_db", nums(highs)), ("tolerance_db", "1e-9"),
          ("room_gains", nums(room)), ("expect_attenuation_db", nums([quant(g) for g in room]))])


# --- 7-10. chains -----------------------------------------------------------

def peq_gain(fs, f):
    return abs(resp(*rbj("peaking", fs, 2000, 0.667, 4.0), f, fs))


def chains():
    fs = 48000
    sd = [("source", DSPDOC), ("read", READ)]
    common = [("rate_hz", fs), ("room_gain", 1), ("limit_gain", 1)]
    out = []

    out.append(write("chain-flat-stereo.txt", """
A flat chain (every setting at its default, not in a set, no two-way, no delays)
runs no filter: each output is its input channel times the room gain, delayed by
the limiter's look-ahead (96 frames at 48 kHz), bit for bit.
""", [("kind", "chain"), *sd, ("rate_hz", fs), ("channel_map", "1 2"), ("frames", 4800),
          ("room_gain", 0.5), ("limit_gain", 1), ("input.0", "noise 1 0.9"),
          ("input.1", "noise 2 0.9 + sine 1000 0.05"), ("out_channels", 2),
          ("latency_frames", 96), ("exact.0", 0), ("exact.1", 1)]))
    out.append(write("chain-flat-5p1.txt", """
A flat chain on 5.1 (FL FR FC LFE BL BR) at 44.1 kHz: six outputs, each its input
times the room gain, delayed by the look-ahead (88 frames: 2 ms rounded), bit for bit.
""", [("kind", "chain"), *sd, ("rate_hz", 44100), ("channel_map", "1 2 3 4 5 6"),
          ("frames", 2000), ("room_gain", 0.8125), ("limit_gain", 0.9),
          *[(f"input.{c}", f"noise {c + 11} 0.95") for c in range(6)],
          ("out_channels", 6), ("latency_frames", 88),
          *[(f"exact.{c}", c) for c in range(6)]]))

    g = peq_gain(fs, 2000)
    out.append(write("chain-speech-mid.txt", f"""
Speech enhancement on stereo: mid/side, the voice-band boost (2 kHz and Q 0.667
ASSUMED; +4 dB, Geiger et al. EUSIPCO 2015's 3.8 dB centre gain rounded) on the mid. A centred 2 kHz tone (L = R) is all mid, so both
outputs settle at amplitude 0.25 x |H(2 kHz)| = {0.25 * g:.9f} (the cookbook
peaking gain at f0: dBgain).
""", [("kind", "chain"), ("source", f"{GEIGER} {RBJ} {DSPDOC}"), ("read", READ), *common, ("channel_map", "1 2"), ("frames", 9600),
          ("speech", 1), ("input.0", "sine 2000 0.25"), ("input.1", "sine 2000 0.25"),
          ("out_channels", 2), ("amplitude.0", repr(0.25 * g)), ("amplitude.1", repr(0.25 * g)),
          ("amplitude_from", 4800), ("amplitude_tolerance", "2e-5")]))
    out.append(write("chain-speech-side.txt", """
Speech enhancement on stereo, a side-only signal (L = -R): the mid is exactly 0,
the boost of 0 is 0, and l = 0 + s, r = 0 - s give the input back, bit for bit.
""", [("kind", "chain"), *sd, *common, ("channel_map", "1 2"), ("frames", 4800),
          ("speech", 1), ("input.0", "sine 2000 0.25 + noise 4 0.2"),
          ("input.1", "sine 2000 -0.25 + noise 4 -0.2"), ("out_channels", 2),
          ("exact.0", 0), ("exact.1", 1)]))
    out.append(write("chain-speech-centre.txt", f"""
Speech enhancement on 5.1: the boost on FC alone. The FC output settles at
0.25 x |H(2 kHz)| = {0.25 * g:.9f}; every other output is its input, bit for bit.
""", [("kind", "chain"), *sd, *common, ("channel_map", "1 2 3 4 5 6"), ("frames", 9600),
          ("speech", 1), ("input.0", "noise 1 0.3"), ("input.1", "noise 2 0.3"),
          ("input.2", "sine 2000 0.25"), ("input.3", "noise 3 0.3"), ("input.4", "noise 5 0.3"),
          ("input.5", "noise 6 0.3"), ("out_channels", 6), ("amplitude.2", repr(0.25 * g)),
          ("amplitude_from", 4800), ("amplitude_tolerance", "2e-5"),
          ("exact.0", 0), ("exact.1", 1), ("exact.3", 3), ("exact.4", 4), ("exact.5", 5)]))

    lo40, hi40 = lr4(fs, 80, 40)
    out.append(write("chain-2p1-main.txt", f"""
Bass management, a main in a 2.1 set (FL FR LFE; role FL, a subwoofer present,
the 80 Hz default: "The most common crossover frequency recommended (and the
THX standard) is 80 Hz", {SVS}, read {READ}). The output is FL through the LR4
high branch: a 40 Hz tone of 0.5 settles at 0.5 |H_high(40)| = {0.5 * abs(hi40):.9f}
(the designed response, computed here); FR and LFE do not reach it.
""", [("kind", "chain"), *sd, *common, ("channel_map", "1 2 4"), ("frames", 48000),
          ("role", 1), ("sub_present", 1), ("crossover_hz", 80),
          ("input.0", "sine 40 0.5"), ("input.1", "sine 1000 0.5"), ("input.2", "sine 50 0.5"),
          ("out_channels", 1), ("amplitude.0", repr(0.5 * abs(hi40))), ("amplitude_from", 38400),
          ("amplitude_tolerance", "1e-4"), ("golden", 1)]))
    sub = 10 ** (-300 / 2000)
    out.append(write("chain-2p1-sub.txt", f"""
Bass management, the LFE member of a 2.1 set: the LR4 low branch of FL + FR, at
the subwoofer level (-3 dB), inverted. FL = FR = a 40 Hz tone of 0.2, so the
feed settles at 0.4 |H_low(40)| 10^(-3/20) = {0.4 * abs(lo40) * sub:.9f} (the
designed response in f64). The tolerance, 3e-4, is wider than the main's: the
low-pass sections at 80 Hz of 48 kHz have their poles close to z = 1, and
rounding their coefficients to f32 (BRIEF section 5.6) moves the 40 Hz gain by
-0.005 dB (computed here: 0.99945 of the f64 design).
""", [("kind", "chain"), *sd, *common, ("channel_map", "1 2 4"), ("frames", 48000),
          ("role", 4), ("sub_present", 1), ("crossover_hz", 80), ("sub_level_cdb", -300),
          ("sub_polarity_inverted", 1), ("input.0", "sine 40 0.2"), ("input.1", "sine 40 0.2"),
          ("input.2", "silence"), ("out_channels", 1),
          ("amplitude.0", repr(0.4 * abs(lo40) * sub)), ("amplitude_from", 38400),
          ("amplitude_tolerance", "3e-4"), ("golden", 1)]))
    out.append(write("chain-2p1-lfe.txt", f"""
The LFE channel into the subwoofer feed at +10 dB: ATSC A/52:2018 section 3,
the LFE channel "is intended to be reproduced at a level +10 dB with respect to
the fbw channels" ({ATSC}, read {READ}). Mains silent, LFE a 50 Hz tone of 0.1: the feed
has amplitude 0.1 x 10^(10/20) = {0.1 * 10 ** 0.5:.9f}.
""", [("kind", "chain"), ("source", f"{ATSC} {BASS}"), ("read", READ), *common,
          ("channel_map", "1 2 4"), ("frames", 9600), ("role", 4), ("sub_present", 1),
          ("input.0", "silence"), ("input.1", "silence"), ("input.2", "sine 50 0.1"),
          ("out_channels", 1), ("amplitude.0", repr(0.1 * 10 ** 0.5)), ("amplitude_from", 4800),
          ("amplitude_tolerance", "1e-5")]))

    tw = 10 ** (-300 / 2000)
    out.append(write("chain-two-way.txt", f"""
The two-way split (ASSUMED example crossover 2 kHz): an unbonded stereo
endpoint's downmix (L + R)/2 split by LR4 into woofer (output 0, trim 0) and
tweeter (output 1, trim -3 dB, delay 100 us = 5 frames, inverted). A 2 kHz tone
of 0.5 on both channels: each branch is -6.02 dB at the crossover, so the
woofer settles at 0.25 and the tweeter at 0.25 x 10^(-3/20) = {0.25 * tw:.9f}.
""", [("kind", "chain"), ("source", f"{RANE} {DSPDOC}"), ("read", READ), *common,
          ("channel_map", "1 2"), ("frames", 9600), ("two_way", 1), ("two_way_hz", 2000),
          ("woofer", "0 0 0"), ("tweeter", "-300 100 1"),
          ("input.0", "sine 2000 0.5"), ("input.1", "sine 2000 0.5"), ("out_channels", 2),
          ("amplitude.0", 0.25), ("amplitude.1", repr(0.25 * tw)), ("amplitude_from", 4800),
          ("amplitude_tolerance", "1e-4"), ("golden", 1)]))
    out.append(write("chain-two-way-sum.txt", """
The two-way split sums flat: woofer + tweeter (no trims, no delays) is the LR4
all-pass of the input, so a 3 kHz tone of 0.5 on a mono stream comes back out
of the sum at amplitude 0.5 (RaneNote 160: "The summed response is perfectly
flat").
""", [("kind", "chain"), ("source", RANE), ("read", READ), *common,
          ("channel_map", "0"), ("frames", 9600), ("two_way", 1), ("two_way_hz", 2000),
          ("woofer", "0 0 0"), ("tweeter", "0 0 0"), ("input.0", "sine 3000 0.5"),
          ("out_channels", 2), ("sum_amplitude", 0.5), ("amplitude_from", 4800),
          ("amplitude_tolerance", "1e-4")]))

    out.append(write("chain-everything-stereo.txt", """
Every stage at once on an unbonded stereo endpoint: room EQ (two filters),
bass +4, treble -3, loudness at a room gain of 0.3 (10.5 dB down), speech,
night, a per-output delay, and the limiter at the room's limit 0.25. The
samples are the Rust chain's output (a drift fixture: it holds the C chain to
the Rust chain; it is not a worked example), and no output sample exceeds the
ceiling.
""", [("kind", "chain"), *sd, ("rate_hz", fs), ("channel_map", "1 2"), ("frames", 9600),
          ("room_gain", 0.3), ("limit_gain", 0.25), ("bass_db", 4), ("treble_db", -3),
          ("loudness", 1), ("night", 1), ("speech", 1), ("room_eq_enabled", 1),
          ("room_eq", "60 -600 4000 250 200 1500"), ("output_delay_us", "0 250"),
          ("input.0", "noise 31 0.6 + sine 55 0.3"),
          ("input.1", "noise 32 0.6 + burst 2500 0.4 2000 6000"),
          ("out_channels", 2), ("ceiling_holds", 1), ("golden", 1)]))
    out.append(write("chain-everything-sub.txt", """
Every stage at once on the LFE member of a 5.1 set (FL FR FC LFE BL BR):
room EQ, tone, loudness, speech on FC, night (linked over all six channels),
the LFE feed (low branch of the five mains plus LFE at +10 dB) at +2 dB,
inverted, 100 Hz crossover. Samples from the Rust chain (a drift fixture).
""", [("kind", "chain"), *sd, ("rate_hz", fs), ("channel_map", "1 2 3 4 5 6"),
          ("frames", 9600), ("room_gain", 0.5), ("limit_gain", 1), ("bass_db", -2),
          ("treble_db", 5), ("loudness", 1), ("night", 1), ("speech", 1),
          ("room_eq_enabled", 1), ("room_eq", "45 -900 2500"), ("role", 4),
          ("sub_present", 1), ("crossover_hz", 100), ("sub_level_cdb", 200),
          ("sub_polarity_inverted", 1),
          *[(f"input.{c}", f"noise {40 + c} 0.3 + sine {30 + 7 * c} 0.1") for c in range(6)],
          ("out_channels", 1), ("ceiling_holds", 1), ("golden", 1)]))
    return out


def golden(paths):
    for path in paths:
        text = open(path, encoding="utf-8").read()
        if "\ngolden = 1\n" not in text:
            continue
        lines = subprocess.run(
            ["cargo", "run", "-q", "-p", "chorus-dsp", "--example", "chain_samples", "--", path],
            cwd=ROOT, check=True, capture_output=True, text=True).stdout
        with open(path, "a", encoding="utf-8") as fh:
            fh.write("# The Rust chain's output (the chain_samples example): a drift check.\n")
            fh.write(lines)


def main():
    os.makedirs(OUT, exist_ok=True)
    for name in os.listdir(OUT):
        if name.endswith(".txt"):
            os.remove(os.path.join(OUT, name))
    biquads()
    crossovers()
    delays()
    limiters()
    compressors()
    loudness()
    paths = chains()
    if "--no-golden" not in sys.argv:
        golden(paths)


if __name__ == "__main__":
    main()
