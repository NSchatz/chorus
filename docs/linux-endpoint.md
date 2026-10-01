# The Linux endpoint: install, configure, check

A Linux machine with an ALSA playback device (a Raspberry Pi with a DAC board, a small x86
box with a USB or HDMI output) becomes a chorus endpoint by installing one Debian package,
`chorus-endpoint`. This page is the owner's: installing it on a speaker, enabling the service
and running the host probes on it are the owner's acts. Nothing in the repository installs,
deploys or runs it anywhere. The decisions behind the package, with their sources, are in
[ADR 0069](decisions/0069-the-linux-endpoint-package.md).

## Which package

| Machine | OS | Package |
|---|---|---|
| Raspberry Pi 3, 4 or 5 running a 64-bit OS (the device classes Raspberry Pi Imager lists for both 64-bit images: `pi3-64bit`, `pi4-64bit`, `pi5-64bit`, https://downloads.raspberrypi.com/os_list_imagingutility_v4.json, read 2026-09-30) | Raspberry Pi OS (64-bit) or Raspberry Pi OS (Legacy, 64-bit) | `chorus-endpoint_<ver>_arm64.deb` |
| Any 64-bit ARM board | Debian 12 or 13 (arm64) | `chorus-endpoint_<ver>_arm64.deb` |
| Any x86_64 machine | Debian 12 or 13 (amd64) | `chorus-endpoint_<ver>_amd64.deb` |

Both need glibc 2.36 or later (Debian 12 "bookworm" and Raspberry Pi OS Legacy ship 2.36,
Debian 13 "trixie" and Raspberry Pi OS ship 2.41; sources in ADR 0069). A 32-bit Raspberry Pi OS (`armhf`) is not
built. The packages are attached to each chorus release with their sha256 in `SHA256SUMS`
(docs/release.md), or built from a checkout with `make endpoint-packages` (both land in
`target/endpoint-packages/`).

## Install

```
sha256sum -c --ignore-missing SHA256SUMS
sudo apt install ./chorus-endpoint_<ver>_arm64.deb
```

`apt install ./<file>` pulls in the two dependencies: `libc6 (>= 2.36)` and the ALSA library
(`libasound2t64` on trixie, `libasound2` on bookworm; chorus-client loads it at run time).
`python3` is recommended, for `chorus-verify-host` below. Installing does not start or enable
the service.

What it installs:

| Path | What |
|---|---|
| `/usr/bin/chorus-client` | the endpoint |
| `/usr/lib/systemd/system/chorus-client.service` | the unit that runs it |
| `/etc/chorus/client.conf` | its arguments (a conffile: upgrades keep your edits) |
| `/usr/bin/chorus-verify-host` | the host contract and the spin test (below) |
| `/usr/lib/chorus/probe/`, `/usr/lib/chorus/verify-host/` | what `chorus-verify-host` runs |
| `/usr/bin/chorus-wakeup-probe` | the wakeup-jitter probe (ADR 0047) |
| `/usr/lib/udev/rules.d/70-chorus-leds.rules` | makes a front-panel LED named `chorus-*` writable by the service |
| `/usr/share/doc/chorus-endpoint/` | a README, the licences of everything linked in, and `examples/` (the front-panel drop-in and an example panel) |

## Configure

Edit `/etc/chorus/client.conf`. It holds one line, `CHORUS_CLIENT_ARGS=...`, unquoted; the
file's own comments list every argument worth setting. At least:

- `--endpoint <name>`: this speaker's name. The server pins this endpoint's key to it (or to
  `--endpoint-id`), so keep it stable.
- `--zone <zone>`: the room it plays.
- `--discover` and/or `--server <host:4010>`: where the server is. With `--discover` the
  endpoint looks for the server by multicast DNS first and falls back to the static address.
- `--control <host:4020>`: the server's control plane (volume, mute, grouping).
- `--device <name>`: the ALSA device; `aplay -L` (package `alsa-utils`) lists them.
- `--max-volume <0.000-1.000>`: this speaker's own volume ceiling (default 1.000). Nothing the
  server or the control plane sends plays above it, and until the server's first `room_volume`
  the speaker plays at it (`docs/decisions/0074-room-volume-on-the-audio-wire.md`).
- `--visualizer-bands <1-64>`: declare the visualizer role and ask the server for that many
  bands per frame; every beat and colour received is logged (`docs/visualizer.md`). A front
  panel whose light follows the visualizer needs no flag.
- `--two-way crossover-hz=<Hz>,woofer=<output>,tweeter=<output>` (optional): this speaker drives a
  woofer and a tweeter through an LR4 crossover (`docs/dsp.md`); each key may be left out and
  takes the ASSUMED example (2000 Hz, woofer on output 0, tweeter on output 1). The device is
  opened with enough channels to reach both drivers. It names the device outputs itself, so it
  is refused beside `--output-channels`/`--output`.

- `--cec <device>` (a hub only): be the TV's Audio System over HDMI-CEC on `/dev/cecN`, so the
  TV's remote drives the room's volume and the TV's power starts and stops the TV input
  (`docs/cec.md`, which also gives the drop-in the service needs for the device). With it:
  `--cec-arc` (off, ASSUMED), `--cec-autoplay-on-power on|off` (on) and `--cec-osd-name <name>`
  (`chorus`, ASSUMED; 1 to 14 printable ASCII bytes). An adapter that cannot be opened or
  claimed is logged (`cec unusable`) and retried; the endpoint plays on.

### A TV input (optical, HDMI ARC)

`--line-in <device> --line-in-kind optical` (or `hdmi_arc`) makes this endpoint the TV's hub: it
offers the TV as a source any room can play (`--line-in-name` names it). A TV input is clocked
by the TV, which may run up to +/-1000 ppm off its nominal rate (IEC 60958-3 Level II), so the
hub does not send it as captured (ADR 0090):

- **Rate matching.** It reads the device in 5 ms periods (240 frames at 48 kHz), filters each
  period's timestamp through a delay-locked loop (Adriaensen 2005) to estimate the TV's true
  rate, and resamples to exactly the nominal rate on the server's timeline with a ratio loop
  (Adriaensen 2012, 0.05 Hz after a 4 s start-up). Every chunk upstream is stamped with the
  capture instant of its first frame, and consecutive chunks are exactly one chunk apart. The
  first 0.5 s after the TV appears is the loop's warm-up: nothing is sent yet.
- **Refusals.** The hub forwards nothing and offers the input with no signal when:
  - `non-pcm`: the TV sends a compressed format (IEC 61937: AC-3, DTS). Set the TV's digital
    audio output to PCM (often "PCM" or "Stereo" rather than "Auto" or "Bitstream"). It is
    taken back 250 ms after the stream is PCM again.
  - `no-lock`: no frames for 100 ms (the TV is off or unplugged) or the device fails. Taken
    back 0.5 s after frames return.
  - `rate-out-of-range`: the TV runs outside +/-1500 ppm of `--line-in-rate-hz` for 2 s (usually
    the wrong rate configured: a TV at 44.1 kHz on a 48 kHz setting). Taken back when a fresh
    measurement is inside.

  Each refusal is a journal line, `source-tv-refused source_id=1 reason=<reason> detail=<what to
  do>`, and the offer that follows says `signal=0 reason=<reason>`. At the end of a session the
  hub prints `source-tv ppm=<the TV's measured rate error> relocks=... ring_overflows=...
  ring_underflows=... non_pcm_periods=... refused_non_pcm=... refused_no_lock=... refused_rate=...
  frames_dropped=...`; a relock (`source-tv-relock reason=...`) is a capture overrun or a jump in
  the timeline, after which the hub starts the loop again.

A `line_in` (analogue) input is captured and stamped as before, without rate matching.

### The TV path's low-latency stream (`--low-latency on|off`)

A TV played into one wired room is not played at the slot path's 180 ms: the server relays it
over UDP, each 2.5 ms chunk stamped to be heard `L_tv` (20 ms by default) after the TV made it
(`docs/protocol.md` "Low-latency path", ADR 0091). A wired endpoint takes part by default;
`--low-latency off` keeps it on the slot path (it then never advertises the feature). An endpoint
with `--transport wireless` never advertises it either (the path is wired only).

- **As a player** (any wired endpoint): the server's `low_latency_offer` (0x16) is answered with
  `low_latency_accept` (0x17) carrying a fresh UDP port, or refused by status (`refused_wireless`,
  `refused_fec` for an FEC shape or chunk this stream cannot take, `refused_no_socket`).
  Datagrams are taken from the server's address only, opened, repaired from their group's parity,
  and held in a small jitter buffer by stamp. The device is then paced to a 4 ms delay (ASSUMED:
  the budget's endpoint output path), after letting the slot path's deeper buffer play out, and
  the chunk stamped at the instant the next written frame is heard is written. A chunk that
  arrives after its instant is dropped (`late`); a missing one is concealed (the last frame held
  and faded to silence over one chunk, the next chunk faded in over the same; ASSUMED one chunk,
  2.5 ms), never a hard cut. If the play position drifts more than 0.5 ms (ASSUMED) from the
  heard instant it is moved back (`resnaps`). The room's volume and sound apply as on the slot
  path. Journal lines: `low-latency start stream_tag=... udp_port=...`, `low-latency end
  reason=...`, and at the end of a session `low-latency stream_tag=... opened=... auth_failed=...
  replayed=... delivered=... recovered=... unrecoverable=... late=... played=... concealed=...
  rejected=... no_offset=... resnaps=... superseded=...` (`superseded`: slot-path chunks not
  played while the stream played; the delay log's events `low-latency enter`/`output`/`leave`
  mark the switch, and its samples during the stream are ungraded).
- **As a TV hub** (a TV input, direction `from_endpoint`): after accepting, the rate-matched
  output is cut in the offer's chunk size (120 frames) and every chunk leaves as datagrams to the
  server's port (XOR parity per the offer's `fec_k`) instead of up the session, with the same
  sequence and stamp; an `end` goes back to the session and 20 ms chunks. Journal lines:
  `source-low-latency start stream_tag=... chunk_frames=... fec_k=...`, `source-low-latency end
  ...`, and at the end of a session `source-low-latency streams=... chunks_sent=...
  datagrams_sent=... send_failures=...`. A `line_in` refuses the offer.

The 4 ms device target and the 1 ms top-up interval are for a modelled device; the ALSA period
size a real DAC needs to hold 4 ms without underruns is a bench question (session S8).
`--low-latency-target-us <us>` sets the target (above the sound chain's own fixed latency, its
limiter's 2 ms look-ahead); a larger one needs a room `L_tv` that leaves room for it (the
server's `--tv-latency-ms`), or every chunk arrives late.

The room's sound (bass, treble, loudness, night, speech, room correction) and, in a bonded set,
bass management come from the server's `sound` message and need no setting here: the client runs
the sound chain from the first `sound` it receives (`dsp engaged ...` on its output) and prints
`dsp engaged=... sounds_applied=... refusals=... clipped_samples=...` at the end of a session. A
member of a bonded set plays its own role; with no output map that role's feed goes to every
device channel, and an output map (`--output 0=LFE` on a subwoofer) is resolved against the
role, so a stereo stream's subwoofer feed reaches the output that names `LFE`. A surround
stream (a 5.1 film) on a two-channel device with no output map and no `--two-way` is folded to
stereo by ITU-R BS.775-4's 2/0 downmix, centre and surrounds in, LFE dropped (`dsp
stereo-downmix ...` on its output); a device that refuses six channels is opened with two for it
(`device-channels ... reason=stream-count-refused`). An output map, if given, says what each
output plays instead.

The unit adds `--rejoin --identity-dir /var/lib/chorus-client` itself: the endpoint's key and
its server pins live in `/var/lib/chorus-client` and survive restarts and upgrades.

## Enable and watch

```
sudo systemctl enable --now chorus-client
journalctl -u chorus-client -f
systemctl status chorus-client
```

The status lines (`chorus-client: identity ...`, `starting ...`, `session ...`) go to the
journal. The service runs as a transient unprivileged user in group `audio`, restarts two
seconds after any exit, and stays stopped after exit 2 (a refused configuration): read the
journal, fix `client.conf`, and `sudo systemctl restart chorus-client`.

The first time the endpoint connects, the server has to adopt it (protocol v2 pins its key);
the server's own documentation (deploy/README.md) says how.

## The host probes (`make verify-host` on the endpoint)

`chorus-verify-host` runs the two checks `make verify-host` runs in the repository, the same
scripts, against the probes the package installs:

- the host contract: a real-time contract taken on this kernel and graded against `/proc`
  (the priority inside the granted ceiling, every real-time thread bounded and reported);
- the spin test: a thread under `SCHED_FIFO` spins past its CPU-time bound, which must fire
  within a second while a normal-priority heartbeat keeps going.

Both need a real-time priority ceiling, which a login shell does not have, so run it under the
service's own limits:

```
sudo systemd-run --pty --wait --collect -p DynamicUser=yes \
    -p LimitRTPRIO=20 -p LimitRTTIME=200ms -p LimitMEMLOCK=64M \
    /usr/bin/chorus-verify-host
```

Without a ceiling each check exits 3 with `MISSING PREREQUISITE` and the criterion it could
not verify; that is a refusal, never a pass. The results are evidence about this machine only
when they are saved as a bench report (docs/bench.md); nothing here is timing evidence until
then.

## Real-time playout (off by default)

Once `chorus-verify-host` passes, add `--rt-priority 20 --rttime-us 200000` to
`CHORUS_CLIENT_ARGS` and restart. The playout thread then runs under `SCHED_FIFO` at priority
20 (clamped to the unit's `LimitRTPRIO=20`), with a 200 ms real-time CPU bound applied first
(clamped to `LimitRTTIME=200ms`), and the journal says
`real-time thread=playout policy=SCHED_FIFO priority=20 ...` at every session. On a host that
grants no priority the client refuses at start (`stopped reason=real-time-refused`, exit 2).
The values are the server's contract (`config/verification.conf`); what an endpoint needs is
not measured yet (ASSUMED).

## A front panel

An endpoint with buttons and a status light (the rack amp, ADR 0067) runs them as the controller
role with `--front-panel <file>`. The board's device-tree overlays turn the buttons into a
gpio-keys input device and the light into an LED class device (the example file names the
overlay lines; all of it ASSUMED until the rack amp's board is designed). Then:

1. Copy the example and edit it for the board:
   `sudo cp /usr/share/doc/chorus-endpoint/examples/front-panel-rack-amp.conf /etc/chorus/front-panel.conf`
2. Give the light a name starting with `chorus-` (the overlay's `label=`, e.g.
   `label=chorus-status`). The package's udev rule makes that LED's `brightness` (and
   `multi_intensity`, if it has one) group `audio` and group-writable, which the service is in;
   no other LED is touched. Apply it once with `sudo udevadm trigger --subsystem-match=leds`
   or a reboot.
3. Let the service read the buttons, by installing the drop-in the package ships (it adds group
   `input` and opens the input device class read-only; not in the unit by default because it
   lets the service read every input device, keyboards included):

   ```
   sudo install -D -m 0644 /usr/share/doc/chorus-endpoint/examples/front-panel.conf \
       /etc/systemd/system/chorus-client.service.d/front-panel.conf
   sudo systemctl daemon-reload
   ```

4. Add `--front-panel /etc/chorus/front-panel.conf` to `CHORUS_CLIENT_ARGS` and restart. A panel
   that cannot start (a device it cannot open, a key the class does not have) stops the client
   with `stopped reason=front-panel-refused` (exit 2), and the unit stays stopped.

## Changing the unit

Never edit `/usr/lib/systemd/system/chorus-client.service` (an upgrade replaces it). A drop-in
survives upgrades:

```
sudo systemctl edit chorus-client
```

The front-panel drop-in above is the pattern for any other device a later feature needs.

## Remove

```
sudo systemctl disable --now chorus-client
sudo apt remove chorus-endpoint          # keeps /etc/chorus/client.conf
sudo apt purge chorus-endpoint           # removes it too
```

The endpoint's key stays in `/var/lib/private/chorus-client` until removed by hand; removing
it means the server sees a new key under the same id and refuses it until re-adopted.
