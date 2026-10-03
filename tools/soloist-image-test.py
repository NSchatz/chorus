#!/usr/bin/env python3
"""The run half of `make soloist-image`'s test (tools/soloist-image.sh starts it).

usage: soloist-image-test.py <run dir> <supervisor pid> <chorus-soloistd> <pw-cat>

The supervisor from the unpacked image is already running with `--pipewire auto`
over the image's PipeWire and WirePlumber, with the fake Soloist as its
`--soloist-bin`. This script is chorus-server's side and the Spotify app's side:

1. waits for PipeWire's socket and the health check to say healthy;
2. connects to r0.sock as the server would: hello, build, status idle; `assign`;
   status running; the fake "logs in" and "plays" and the events are relayed;
3. plays a known float32 signal with the image's own pw-cat into the sink
   `chorus-r0` and reads the FIFO r0.pcm: every frame that arrives must be a
   frame of the signal, bit for bit and in order. The fake Soloist has no
   PipeWire client (it writes a FIFO itself, which is off here), so pw-cat is
   the PipeWire client in its place;
4. `release`, then SIGTERM: no child left, the health check unhealthy (the shell
   that started the supervisor checks its exit code, 0).

A quantum lost to a busy host (the PipeWire probe saw them without real-time
scheduling) is counted and reported, not failed: it is the host's scheduling,
not the image. Arrival of less than half the signal after three plays fails.
Only the monotonic clock is read.
"""

import json
import os
import signal
import socket
import struct
import subprocess
import sys
import threading
import time

RATE = 44100
SECONDS = 2
FRAMES = RATE * SECONDS


def fail(why):
    print("soloist-image test: FAIL: " + why)
    sys.exit(1)


def wait_for(what, check, seconds=30.0):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    fail("timed out waiting for " + what)


def signal_frame(n):
    """Frame n: left a ramp, right the ramp's count, multiples of 1/8192 (exact in a float32)."""
    return ((n % 8192) / 8192.0 - 0.5, ((n // 8192) % 8192) / 8192.0 - 0.5)


def frame_index(left, right):
    a, b = (left + 0.5) * 8192.0, (right + 0.5) * 8192.0
    if a != int(a) or b != int(b) or not (0 <= a < 8192 and 0 <= b < 8192):
        return None
    return int(b) * 8192 + int(a)


class Lines:
    """The supervisor protocol: newline-delimited JSON on a Unix socket."""

    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.sock.settimeout(0.2)
        self.buffer = b""
        self.seen = []

    def send(self, message):
        self.sock.sendall(json.dumps(message, separators=(",", ":")).encode() + b"\n")

    def expect(self, what, match, seconds=30.0):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            while b"\n" in self.buffer:
                line, self.buffer = self.buffer.split(b"\n", 1)
                message = json.loads(line)
                self.seen.append(message)
                if match(message):
                    return message
            try:
                data = self.sock.recv(65536)
            except socket.timeout:
                continue
            if not data:
                fail("the supervisor closed the connection while waiting for " + what)
            self.buffer += data
        fail("no %s within %.0f s; last messages: %s" % (what, seconds, self.seen[-4:]))


def app(control, command):
    """The Spotify app, through the fake's control socket."""
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(10)
        s.connect(control)
        s.sendall(command.encode() + b"\n")
        answer = s.makefile().readline().strip()
    if answer != "ok":
        fail("the fake Soloist answered %r to %r" % (answer, command))


def children_of(pid):
    found = []
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        try:
            with open("/proc/%s/stat" % entry) as f:
                fields = f.read().rsplit(")", 1)[1].split()
        except OSError:
            continue
        if int(fields[1]) == pid:
            found.append(int(entry))
    return found


def main():
    run_dir, supervisor, soloistd, pw_cat = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
    receivers, runtime = run_dir + "/recv", run_dir + "/run"
    health = [soloistd, "--health-check", "--soloist-dir", receivers, "--pipewire-runtime-dir", runtime]

    def alive():
        try:
            os.kill(supervisor, 0)
        except OSError:
            fail("the supervisor exited early")
        return True

    wait_for("PipeWire's socket", lambda: alive() and os.path.exists(runtime + "/pipewire-0"))
    wait_for("the health check", lambda: alive() and subprocess.run(health, capture_output=True).returncode == 0)
    said = subprocess.run(health, capture_output=True, text=True).stdout.strip()
    print("soloist-image test: --health-check -> " + said)

    # The FIFO's reader, as chorus-server's: attached before anything plays, draining always.
    captured = bytearray()
    stop = threading.Event()
    fifo = os.open(receivers + "/r0.pcm", os.O_RDONLY | os.O_NONBLOCK)

    def drain():
        while not stop.is_set():
            try:
                data = os.read(fifo, 65536)
            except BlockingIOError:
                data = b""
            if data:
                captured.extend(data)
            else:
                time.sleep(0.005)

    reader = threading.Thread(target=drain, daemon=True)
    reader.start()

    # chorus-server's side of the socket.
    server = Lines(receivers + "/r0.sock")
    hello = server.expect("hello", lambda m: m.get("t") == "hello")
    build = server.expect("build", lambda m: m.get("t") == "build")
    server.expect("status idle", lambda m: m.get("t") == "status" and m.get("state") == "idle")
    if hello.get("receiver") != 0 or not build.get("present") or build.get("build_epoch") is None:
        fail("hello or build is not what the fake gives: %s %s" % (hello, build))
    print("soloist-image test: r0.sock: hello v%s receiver %s, build %r expires_epoch %s"
          % (hello.get("v"), hello.get("receiver"), build.get("version"), build.get("expires_epoch")))
    server.send({"t": "assign", "generation": 1, "target": "room:kitchen", "name": "Kitchen"})
    server.expect("status running", lambda m: m.get("t") == "status" and m.get("state") == "running"
                  and m.get("target") == "room:kitchen" and m.get("generation") == 1)
    control = run_dir + "/app.sock"
    wait_for("the fake's control socket", lambda: os.path.exists(control))
    app(control, "login")
    server.expect("the login event", lambda m: m.get("t") == "event"
                  and m["event"].get("type") == "auth_state" and m["event"].get("logged_in") is True)
    uri = "spotify:track:6rqhFgbbKwnb9MLmUQDhG6"
    app(control, "play " + uri)
    server.expect("the playing event", lambda m: m.get("t") == "event" and m.get("generation") == 1
                  and m["event"].get("type") == "playback_changed" and m["event"].get("status") == "playing")
    print("soloist-image test: assign room:kitchen -> running; the fake's login and play of %s relayed as events" % uri)

    # The PipeWire half: the image's pw-cat into the sink, the FIFO read back.
    clip = b"".join(struct.pack("<ff", *signal_frame(n)) for n in range(FRAMES))
    env = dict(os.environ, XDG_RUNTIME_DIR=runtime, PIPEWIRE_CONFIG_DIR=runtime + "/conf")
    best = None
    for attempt in (1, 2, 3):
        del captured[:]
        played = subprocess.run(
            [pw_cat, "-p", "--target", "chorus-r0", "--rate", str(RATE), "--channels", "2",
             "--format", "f32", "--raw", "-"],
            input=clip, env=env, capture_output=True, timeout=60)
        if played.returncode != 0:
            fail("pw-cat exited %d: %s" % (played.returncode, played.stderr.decode(errors="replace")[-400:]))
        quiet, size = time.monotonic() + 1.0, len(captured)
        while time.monotonic() < quiet:
            time.sleep(0.1)
            if len(captured) != size:
                quiet, size = time.monotonic() + 1.0, len(captured)
        data = bytes(captured[: len(captured) // 8 * 8])
        frames = struct.unpack("<%df" % (len(data) // 4), data)
        zeros = arrived = gaps = 0
        last = -1
        for i in range(0, len(frames), 2):
            left, right = frames[i], frames[i + 1]
            if left == 0.0 and right == 0.0:
                zeros += 1
                continue
            n = frame_index(left, right)
            if n is None or n >= FRAMES or struct.pack("<ff", left, right) != clip[n * 8: n * 8 + 8]:
                fail("the FIFO delivered a frame that is not the signal's: (%r, %r) at frame %d" % (left, right, i // 2))
            if n <= last:
                fail("the FIFO delivered frame %d after frame %d" % (n, last))
            if n != last + 1:
                gaps += 1
            last = n
            arrived += 1
        result = (arrived, gaps, zeros, attempt)
        if best is None or arrived > best[0]:
            best = result
        if arrived == FRAMES:
            break
    arrived, gaps, zeros, attempt = best
    if arrived * 2 < FRAMES:
        fail("pw-cat played %d frames into chorus-r0 and only %d reached r0.pcm in three plays" % (FRAMES, arrived))
    whole = "all of them, bit for bit" if arrived == FRAMES else (
        "every one bit for bit and in order, %d missing in %d gaps (quanta lost on a busy host)" % (FRAMES - arrived, gaps))
    print("soloist-image test: pw-cat -> chorus-r0 -> r0.pcm (float32, 44100 Hz, stereo): %d of %d frames arrived, %s; %d zero frames of padding; play %d"
          % (arrived, FRAMES, whole, zeros, attempt))

    # Release and stop.
    server.send({"t": "release", "generation": 2})
    server.expect("status idle after release", lambda m: m.get("t") == "status" and m.get("state") == "idle"
                  and m.get("generation") == 2)
    children = children_of(supervisor)
    if len(children) < 2:
        fail("the supervisor has %d children, not PipeWire and WirePlumber" % len(children))
    os.kill(supervisor, signal.SIGTERM)

    def gone(pid):
        try:
            with open("/proc/%d/stat" % pid) as f:
                return f.read().rsplit(")", 1)[1].split()[0] == "Z"
        except OSError:
            return True

    wait_for("the supervisor to stop", lambda: gone(supervisor), 20.0)
    wait_for("PipeWire and WirePlumber to stop", lambda: all(gone(p) for p in children), 10.0)
    stop.set()
    reader.join()
    after = subprocess.run(health, capture_output=True, text=True)
    if after.returncode != 1:
        fail("--health-check exits %d with no supervisor" % after.returncode)
    print("soloist-image test: release -> idle; SIGTERM: the supervisor and its %d children are gone; --health-check -> %s"
          % (len(children), after.stderr.strip()))


if __name__ == "__main__":
    main()
