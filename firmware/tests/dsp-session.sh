#!/usr/bin/env bash
# Goal 12, done-when line C on the C endpoint: bass management and the
# two-way crossover end to end on fakes, against the real server.
#
# Nothing here is a mock of chorus. The real `chorus-server` at HEAD serves a
# room from a real named pipe (the FIFO source, ADR 0050) with its control
# plane on; four real C endpoint processes (firmware/tests/main_dsp_session.c:
# the session supervisor, the playout path and its sound chain, as the board
# wires them) join it over loopback with protocol v2. Three are bonded as the
# living room's 2.1 set (FL, FR, LFE), wired; the fourth is a two-way speaker
# (endpoint.conf's `two_way*`, switched on with --two-way on) alone in a
# second room. What is fake is the I2S controller: each endpoint's writer
# thread plays the DMA on the monotonic clock and keeps every frame it wrote,
# and those captures are what is graded:
#
#   1. mains carry the LR4 high branch and the sub the LR4 low branch of
#      FL + FR at the set's crossover (80 Hz), each -6.02 dB at it (the sub's
#      0 dB there is -6.02 dB on the sum of two equal channels);
#   2. a `bass_management` crossover change (120 Hz) is followed;
#   3. the two-way's woofer and tweeter slots are the LR4 split at its 2 kHz
#      and sum flat;
#   4. a `sound` bass +10 dB and treble -6 dB change a main's captured levels by
#      what the RBJ cookbook shelves (100 Hz and 8 kHz, Q 1/sqrt 2; docs/dsp.md)
#      predict;
#   5. with the room's limit lowered to 0.250, every captured sample of the set
#      stays at or under 0.250 of full scale.
#
# The source is a known stereo multi-tone, left = right, every tone 0.1 of full
# scale and a whole number of cycles per tenth of a second, so each tone's level is
# one DFT bin over a tenth of a second (the median of five such windows), and
# from 1 kHz up one Hann-weighted bin over a fiftieth of a second (the median of
# twenty-five), which the sync loop's frame corrections cannot smear. Levels are
# arithmetic on captured samples: not timing evidence (BRIEF section 3.1 rule 3).
#
# Runs anywhere: no audio device, no privilege, loopback only, about 30 s.
#
#   bash firmware/tests/dsp-session.sh      # or: make -C firmware dsp-session

set -euo pipefail

FW="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../../tools/lib.sh
source "$FW/../tools/lib.sh"

BUILD="${BUILD:-$FW/build}"
ENDPOINT="$BUILD/chorus-endpoint-dsp-session"

build_once

if [ ! -x "$ENDPOINT" ]; then
    say "FAIL $ENDPOINT has not been built"
    exit 1
fi
SERVER="$BIN_DIR/chorus-server"
if [ ! -x "$SERVER" ]; then
    say "FAIL $SERVER has not been built"
    exit 1
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/chorus-dsp-session.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

CHORUS_SERVER="$SERVER" CHORUS_ENDPOINT="$ENDPOINT" CHORUS_WORK="$WORK" python3 - <<'PY'
import http.client, json, math, os, re, struct, subprocess, sys, threading, time

server_bin = os.environ["CHORUS_SERVER"]
endpoint_bin = os.environ["CHORUS_ENDPOINT"]
work = os.environ["CHORUS_WORK"]
RATE = 48000
TONES = [30, 80, 120, 300, 1000, 2000, 6000]
AMP = 0.1
IN_DB = 20 * math.log10(AMP)
failures = 0
checks = 0

def check(ok, what):
    global failures, checks
    checks += 1
    print(("pass " if ok else "FAIL ") + what, flush=True)
    if not ok:
        failures += 1

# --- the source: a FIFO fed with a known multi-tone ---------------------------
fifo = os.path.join(work, "pcm.fifo")
os.mkfifo(fifo)
second = bytearray()
for n in range(RATE):
    v = sum(AMP * math.sin(2 * math.pi * f * n / RATE) for f in TONES)
    s = int(round(v * 32767))
    second += struct.pack("<hh", s, s)
second = bytes(second)
feeding = True

def feed():
    with open(fifo, "wb", buffering=0) as w:
        while feeding:
            try:
                w.write(second)
            except BrokenPipeError:
                return

# --- the real server ------------------------------------------------------------
server = subprocess.Popen(
    [server_bin, "--listen", "127.0.0.1:0", "--control-listen", "127.0.0.1:0",
     "--allow-non-realtime", "--allow-unlocked-memory", "--ephemeral-identity",
     "--source", "fifo:" + fifo, "--rate", "48000", "--channels", "2",
     "--format", "pcm_s16le", "--serve-forever", "--max-clients", "4",
     "--zone", "living", "--zone", "study"],
    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
seen = []
audio = control = None
ready = threading.Event()

def read_server():
    global audio, control
    for line in server.stdout:
        seen.append(line.rstrip())
        m = re.search(r"chorus-server: listening on=(\S+)", line)
        if m:
            audio = m.group(1)
        m = re.search(r"control listening on=(\S+)", line)
        if m:
            control = m.group(1)
        if audio and control:
            ready.set()

threading.Thread(target=read_server, daemon=True).start()
threading.Thread(target=feed, daemon=True).start()
if not ready.wait(15):
    print("FAIL the server never listened:\n" + "\n".join(seen))
    server.kill()
    sys.exit(1)

def command(body):
    host, port = control.rsplit(":", 1)
    c = http.client.HTTPConnection(host, int(port), timeout=5)
    c.request("POST", "/api/command", json.dumps(body), {"Content-Type": "application/json"})
    r = c.getresponse()
    answer = r.read().decode()
    ok = r.status == 200
    check(ok, "the control plane applied %s (%d)" % (body["t"], r.status))
    if not ok:
        print("  " + answer)
    return ok

suffix = str(os.getpid())
ids = {k: "dsp-%s-%s" % (k, suffix) for k in ("fl", "fr", "sub", "tw")}
for zone, key in (("living", "fl"), ("living", "fr"), ("living", "sub"), ("study", "tw")):
    command({"v": 2, "t": "attach", "zone": zone, "endpoint": ids[key], "link": "wired"})
for zone in ("living", "study"):
    command({"v": 2, "t": "volume", "zone": zone, "volume": 1.0})
    command({"v": 2, "t": "sound", "zone": zone, "loudness": False})
command({"v": 2, "t": "bond", "zone": "living", "members": [
    {"endpoint": ids["fl"], "role": "FL"}, {"endpoint": ids["fr"], "role": "FR"},
    {"endpoint": ids["sub"], "role": "LFE"}]})

# --- the four C endpoints --------------------------------------------------------
RUN = 22
procs = {}
for key in ids:
    args = [endpoint_bin, "--server", audio, "--endpoint-id", ids[key],
            "--run-seconds", str(RUN), "--capture", os.path.join(work, key + ".raw")]
    if key == "tw":
        args += ["--two-way", "on"]
    procs[key] = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                  text=True)

marks = {}
time.sleep(8.0)              # joined, acquired, settled
marks["a"] = time.monotonic()  # 2.1 at 80 Hz, the two-way at 2 kHz
time.sleep(2.0)
command({"v": 2, "t": "bass_management", "zone": "living", "crossover_hz": 120})
marks["b"] = time.monotonic()
time.sleep(3.0)
command({"v": 2, "t": "sound", "zone": "living", "bass": 10, "treble": -6})
marks["c"] = time.monotonic()
time.sleep(3.0)
command({"v": 2, "t": "limit", "zone": "living", "limit": 0.25})
marks["d"] = time.monotonic()

summaries = {}
for key, p in procs.items():
    out, _ = p.communicate(timeout=RUN + 30)
    summaries[key] = out
    check(p.returncode == 0, "endpoint %s ran its time (exit %d)" % (key, p.returncode))
feeding = False
server.kill()
server.wait()

def field(key, name):
    m = re.findall(r"\b%s=(\S+)" % name, summaries[key])
    return m[-1] if m else None

for key in procs:
    print("  %s: %s" % (key, " | ".join(l for l in summaries[key].splitlines()
                                          if l.startswith("chorus-endpoint-dsp-session"))))
    check(field(key, "dsp_engaged") == "1" and field(key, "dsp_latency_frames") == "96"
          and field(key, "dsp_refusals") == "0",
          "%s: the chain is in the path, 96 frames (2 ms) of latency, nothing refused" % key)

# --- the captures ------------------------------------------------------------------
def load(key):
    with open(os.path.join(work, key + ".raw"), "rb") as f:
        return f.read()

caps = {k: load(k) for k in procs}
# A level is the median over SPANS windows of a tenth of a second (every tone
# is a whole number of cycles in one): the sync loop inserts or drops a frame
# now and then (that is its job), and a window holding one is the outlier the
# median sets aside rather than a smeared bin.
SPAN = RATE // 10
SPANS = 5
WIN = SPAN * SPANS

def window(key, at_s):
    """Slots 0 and 1, full-scale floats, for the half second whose first frame
    reached the pins `at_s` monotonic seconds."""
    start = int(field(key, "capture_start_ns")) / 1e9
    first = int((at_s - start) * RATE)
    data = caps[key]
    b = int(field(key, "slot_bytes"))
    out = ([], [])
    for i in range(first, first + WIN):
        at = i * 2 * b
        for s in range(2):
            raw = data[at + s * b: at + (s + 1) * b]
            if len(raw) < b:
                return None
            out[s].append(int.from_bytes(raw, "little", signed=True) / float(1 << (8 * b - 1)))
    return out

tables = {}
for f in TONES:
    tables[f] = ([math.cos(2 * math.pi * f * i / RATE) for i in range(SPAN)],
                 [math.sin(2 * math.pi * f * i / RATE) for i in range(SPAN)])

# From FINE_HZ up a tone is measured on PIECE frames at a time instead. One
# inserted or dropped frame turns a 6 kHz tone 45 degrees, and a tenth of a
# second holding one reads up to 0.69 dB low; on a loaded host the servo slips
# frames at up to max_correction_ppm (config/sync.conf: 300, a frame every
# 69 ms), so most spans hold one and their median is smeared with them (CI's
# runner: FL at 6000 Hz -0.55 dB with FR beside it at -0.00; a run later, on
# the spans still, the two-way's tweeter at 6000 Hz 0.60 dB under its LR4 and
# woofer + tweeter at -0.624 dB, every other tone in place). A fiftieth of a
# second holds one less than a third of the time at that rate, so the median
# piece holds none. The Hann weights keep the low tones, which are not whole
# cycles in a piece, out of the bin: the nearest is 14 bins off, under -70 dB.
# Under FINE_HZ a slip costs under 0.1 dB and the spans stand.
FINE_HZ = 1000
PIECE = RATE // 50
PIECES = WIN // PIECE
fine = {}
for f in TONES:
    if f >= FINE_HZ:
        hann = [0.5 - 0.5 * math.cos(2 * math.pi * i / PIECE) for i in range(PIECE)]
        fine[f] = ([h * math.cos(2 * math.pi * f * i / RATE) for i, h in enumerate(hann)],
                   [h * math.sin(2 * math.pi * f * i / RATE) for i, h in enumerate(hann)])

def level(x, f):
    if f in fine:
        c, s = fine[f]
        pieces = []
        for k in range(PIECES):
            part = x[k * PIECE:(k + 1) * PIECE]
            re_ = sum(a * b for a, b in zip(part, c))
            im_ = sum(a * b for a, b in zip(part, s))
            # The Hann weights sum to PIECE / 2: 4, where the plain bin has 2.
            pieces.append(20 * math.log10(4 * math.hypot(re_, im_) / PIECE + 1e-12) - IN_DB)
        return sorted(pieces)[PIECES // 2]
    c, s = tables[f]
    spans = []
    for k in range(SPANS):
        part = x[k * SPAN:(k + 1) * SPAN]
        re_ = sum(a * b for a, b in zip(part, c))
        im_ = sum(a * b for a, b in zip(part, s))
        spans.append(20 * math.log10(2 * math.hypot(re_, im_) / SPAN + 1e-12) - IN_DB)
    return sorted(spans)[SPANS // 2]

def lr4(fc, f, high):
    r = (f / fc) ** 4
    return 20 * math.log10((r if high else 1.0) / (1 + r))

def judge(what, got, want, tol):
    # A deep stopband is only held to being deep: below -30 dB the capture is
    # at the level of the other tones' leakage and the sync loop's work.
    if want < -30:
        check(got < -25, "%s: %.2f dB (LR4 %.2f, below -25)" % (what, got, want))
    else:
        check(abs(got - want) <= tol, "%s: %.2f dB (LR4 %.2f, +-%.1f)" % (what, got, want, tol))

SETTLE = 1.0  # past the playout latency, the DMA queue and the chain's reset
for phase, fc, at in (("80 Hz", 80.0, marks["a"]), ("120 Hz", 120.0, marks["b"] + SETTLE)):
    for key, role in (("fl", "FL"), ("fr", "FR"), ("sub", "LFE")):
        w = window(key, at)
        if w is None:
            check(False, "%s has a window at crossover %s" % (key, phase))
            continue
        for f in TONES:
            got = level(w[0], f)
            if key == "sub":
                want = lr4(fc, f, False) + 20 * math.log10(2)  # FL + FR, equal
            else:
                want = lr4(fc, f, True)
            judge("crossover %s, %s at %d Hz" % (phase, role, f), got, want, 0.5)
        check(max(abs(a - b) for a, b in zip(w[0], w[1])) < 1e-6,
              "crossover %s, %s plays one feed on both slots" % (phase, role))

w = window("tw", marks["a"])
for f in TONES:
    woofer, tweeter = level(w[0], f), level(w[1], f)
    total = level([a + b for a, b in zip(w[0], w[1])], f)
    judge("two-way woofer (slot 0) at %d Hz" % f, woofer, lr4(2000.0, f, False), 0.5)
    judge("two-way tweeter (slot 1) at %d Hz" % f, tweeter, lr4(2000.0, f, True), 0.5)
    check(abs(total) <= 0.2, "two-way woofer + tweeter at %d Hz: %.3f dB (flat, +-0.2)"
          % (f, total))

# The RBJ cookbook's shelves (docs/dsp.md: bass at 100 Hz, treble at 8 kHz,
# Q 1/sqrt 2, 1 dB per step), evaluated on the unit circle.
def shelf_db(kind, f0, gain_db, f):
    A = 10 ** (gain_db / 40)
    w0 = 2 * math.pi * f0 / RATE
    alpha = math.sin(w0) / 2 * math.sqrt(2)  # Q = 1/sqrt 2
    c = math.cos(w0)
    sa = 2 * math.sqrt(A) * alpha
    if kind == "low":
        b = (A * ((A + 1) - (A - 1) * c + sa), 2 * A * ((A - 1) - (A + 1) * c),
             A * ((A + 1) - (A - 1) * c - sa))
        a = ((A + 1) + (A - 1) * c + sa, -2 * ((A - 1) + (A + 1) * c), (A + 1) + (A - 1) * c - sa)
    else:
        b = (A * ((A + 1) + (A - 1) * c + sa), -2 * A * ((A - 1) + (A + 1) * c),
             A * ((A + 1) + (A - 1) * c - sa))
        a = ((A + 1) - (A - 1) * c + sa, 2 * ((A - 1) - (A + 1) * c), (A + 1) - (A - 1) * c - sa)
    z = complex(math.cos(2 * math.pi * f / RATE), -math.sin(2 * math.pi * f / RATE))
    h = (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
    return 20 * math.log10(abs(h))

before = window("fl", marks["c"] - 1.0)
# The sound change is measured two seconds after its command, not SETTLE:
# on a loaded host it reached the pins up to a quarter second past SETTLE and
# the median of the window then caught both levels (goal 13, PR #93's third
# red round). The limit lands at marks["d"], a second past this window's end.
after = window("fl", marks["c"] + 2.0)
for f in TONES:
    if lr4(120.0, f, True) < -30:
        continue
    delta = level(after[0], f) - level(before[0], f)
    want = shelf_db("low", 100.0, 10, f) + shelf_db("high", 8000.0, -6, f)
    check(abs(delta - want) <= 0.3,
          "sound bass +10 treble -6, FL at %d Hz moved %.2f dB (the shelves predict %.2f)"
          % (f, delta, want))

LIMIT = 0.25
for key in ("fl", "fr", "sub"):
    start = int(field(key, "capture_start_ns")) / 1e9
    b = int(field(key, "slot_bytes"))
    first = int((marks["d"] + 0.5 - start) * RATE)
    data = caps[key][first * 2 * b:]
    full = float(1 << (8 * b - 1))
    peak = 0.0
    for i in range(0, len(data) - b + 1, b):
        v = abs(int.from_bytes(data[i:i + b], "little", signed=True)) / full
        if v > peak:
            peak = v
    check(len(data) > RATE * 2 * b and peak <= LIMIT,
          "limit 0.250: every one of %d samples %s wrote after it is at or under it "
          "(peak %.4f)" % (len(data) // b, key, peak))

print("\ndsp-session: %d checks, %d failed" % (checks, failures))
sys.exit(1 if failures else 0)
PY
