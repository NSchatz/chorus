# chorus

chorus is a from-scratch multiroom and surround audio system. One containerized server keeps a
single authoritative timeline and cuts audio into chunks stamped on it. A custom, encrypted
protocol carries the chunks to speakers that run chorus's own software: C firmware on ESP32-S3
smart speakers and a Rust client on Linux endpoints. Each speaker disciplines its own playout to
the server's timeline.

The aim is to replace a Sonos household at parity across software, hardware and Home Assistant
while owning every layer that defines the system: the sync engine, the protocol, the playback
engine, the DSP and the firmware. The headline target is sub-millisecond inter-device error on
wired speakers, measured with a rig rather than asserted, plus a lip-sync-grade TV path.
Snapcast, squeezelite, shairport-sync and Roc are studied as prior art under a clean-room rule.
None of them is a dependency.

> **Status, 2026-10-06.** chorus is built and tested in software end to end: the server,
> protocol v2, the sync engine, the Linux endpoint, the ESP32-S3 firmware, rooms and groups, DSP,
> the TV path, inputs (UPnP, Spotify Soloist, line-ins), firmware updates, the Home Assistant
> integration, voice and the installable app. It runs against simulators, fakes, ALSA's `null`
> device and an ESP32-S3 emulator. **Nothing has been heard, flashed or deployed.** Every success
> criterion that needs hardware is NOT PASSED, and no report in
> [`docs/measurements/`](docs/measurements/) is a hardware measurement. The bench that settles
> those criteria is the owner's to buy and run (see [Where it stands](#where-it-stands)).

## Contents

- [What chorus is](#what-chorus-is)
- [How it works](#how-it-works)
- [What it does](#what-it-does)
- [Where it stands](#where-it-stands)
- [The repository](#the-repository)
- [Building and testing](#building-and-testing)
- [Running it](#running-it)
- [Deploying](#deploying)
- [Contributing](#contributing)
- [How the project is run](#how-the-project-is-run)
- [Documentation map](#documentation-map)
- [Licence](#licence)

## What chorus is

chorus is one server and many speakers on a home network:

- **chorus-server** runs in a Docker Compose container with host networking on a bare-metal
  Debian 13 homelab host. It is the timeline authority, the time-sync master, the room model and
  the control plane.
- **Speakers run chorus's own code.** ESP32-S3 smart speakers drive a TAS5825M class-D amplifier
  over I2S. They connect through a W5500 Ethernet module with PoE, or over Wi-Fi in casual rooms.
  Linux endpoints (Raspberry Pi class) cover multichannel outputs and the TV hub.
- **Content arrives only as inputs:**
  - every room and group is a UPnP AV and OpenHome renderer;
  - official Spotify Soloist receivers run unmodified, as separate processes;
  - line-ins, streamers and the TV come in through a Linux endpoint;
  - alarms can play stored stream URLs;
  - Home Assistant can make announcements.

  chorus keeps no library, radio directory or podcasts, and uses no Music Assistant or Sendspin.
- **chorus owns everything between the input and the speaker:** rooms, groups, sync, sound, the
  app and the Home Assistant integration.

### Targets

These come from BRIEF.md section 2.2. Each one is shown by measurement with the rig, not by ear
(guardrail 3).

| Scenario | Acceptable | Aspirational |
|---|---|---|
| Multiroom music, different rooms | < 5 ms inter-device error | < 1 ms |
| Stereo pair, same room | < 0.5 ms | < 0.2 ms |
| Surround inter-channel | < 0.5 ms | < 0.2 ms |
| TV audio against video | within +/- 40 ms, audio never leading by more than 15 ms | within +/- 20 ms |
| Unattended stability | days | weeks |

### What chorus will not do

These come from BRIEF.md section 2.3:

- No Dolby Atmos, TrueHD or DTS decode. Licensing rules out an open implementation, so sources
  are set to PCM.
- No DRM or service-authentication code in chorus. Official receivers run as separate, unmodified
  processes that feed a chorus sink.
- No reverse-engineered receivers (librespot, AirPlay 2, Google Cast), and no Sonos protocol
  compatibility.
- No portable or battery speakers, and no Bluetooth input.
- No app-store app. The app is an installable web app (PWA).

## How it works

```
   UPnP AV / OpenHome   Spotify Soloist   stored URLs, chimes   line-in, streamer, TV
           |                  |                   |             (sent up by a Linux
           v                  v                   v              endpoint's source role)
  +------------------------------------------------------------------------+    |
  | chorus-server  (one container, host networking)                        |<---+
  |   decode -> one stream slot per playing group -> 20 ms chunks stamped  |
  |   on ONE monotonic server timeline; time-sync master (RFC 5905);       |
  |   room model: rooms, bonded sets, groups, volume, limits, schedule     |
  +-----------+-----------------------------------------------+------------+
              | audio: TCP 4010, Noise-encrypted              | control plane:
              | (the TV path: UDP with FEC)                   | HTTP + SSE, TCP 4020
              v                                               v
   ESP32-S3 speakers        Linux endpoints          the app (/app/), control page (/),
   W5500 or Wi-Fi,          ALSA, multichannel,      chorusctl, Home Assistant,
   TAS5825M amp, DSP        line-in, CEC TV hub      /metrics, MQTT (opt-in)
```

### One timeline, measured at the DAC

The server stamps every chunk with the presentation time of its first sample. That time is in
nanoseconds on one monotonic timeline (`CLOCK_MONOTONIC`, through `std::time::Instant`).
Wall-clock time never enters the audio path (guardrail 4); it is only for logs. Chunks are 20 ms
long (960 frames at 48 kHz). By default a wired endpoint plays 180 ms behind the stamp and a
Wi-Fi endpoint 500 ms behind it. That delay is the buffer that absorbs network jitter and TCP
retransmits.

Each endpoint closes two loops (BRIEF.md section 5.3):

1. **Offset.** An RFC 5905 four-timestamp exchange runs every 500 ms. The client keeps the sample
   with the smallest round trip in a window of 64, projects it forward by the drift the window
   shows, and smooths it lightly.
2. **Playout.** The error is taken where a sample becomes audible, not where it is handed to a
   driver:
   - the Linux client asks ALSA for `snd_pcm_delay`;
   - the ESP32-S3 counts the frames its I2S DMA has consumed, stamped in the interrupt.

   A PI law clamped to +/- 300 ppm inserts or drops single frames, spread thinly enough to be
   inaudible. An error of 2 ms or more is a hard resync: a step under a 20 ms mute.

Every constant is in [`config/sync.conf`](config/sync.conf), with the reasoning in
[ADR 0014](docs/decisions/0014-the-sync-loop-on-the-real-path.md).

The servo was developed against a deterministic, seeded simulator
([`crates/sync`](crates/sync)). It models virtual crystals with ppm skew and wander; uniform,
exponential and bursty jitter; and asymmetric paths. On the same scenarios, the C port in the
firmware reproduces the Rust servo exchange by exchange. At house scale (8 rooms, 20 endpoints)
the simulator puts the worst stereo pair at 101.7 us and the worst wired pair at 138.5 us. Those
figures come from
[a simulation and are not timing evidence](docs/measurements/sim-house-8-rooms.md).

### The audio protocol

Each endpoint opens one TCP connection to the server, on port 4010. That connection carries
everything: the handshake, time sync, audio, telemetry and role messages. The contract is
[`docs/protocol.md`](docs/protocol.md) plus the vectors in
[`fixtures/protocol/`](fixtures/protocol/). The Rust implementation (`crates/protocol`) and the C
one (`firmware/src/protocol*.c`) are both held to those bytes.

- **Framing.** Every frame starts with a 3-byte header: the type and the length. A peer skips an
  unknown type by its length, so old peers survive new messages. The audio chunk header is 32
  bytes. It carries a sequence number, the timestamp, the rate, the channel count and the sample
  format, and keeps 14 bytes reserved for the TV path.
- **Encryption and trust.** Protocol v2 sessions are `Noise_XX_25519_ChaChaPoly_SHA256`. Keys are
  trusted on first use: the server pins each endpoint's key when it adopts it, and the endpoint
  pins the server's. A changed key is refused and shown in the app and in Home Assistant. Only
  the owner's `speaker_forget` removes a pin. A protocol v1 peer is refused by name.
- **Capabilities.** An endpoint declares its codecs, formats, channels and rates. The server picks
  the codec from a per-link preference: PCM is mandatory, FLAC and Opus are optional. Channel
  order is always explicit, never implied.
- **Roles.** A session can take roles beyond playing:
  - **metadata:** now playing and artwork;
  - **controller:** the speaker's buttons;
  - **visualizer:** levels and colour for an LED;
  - **source:** an endpoint's line-in, optical or ARC input, sent upstream stamped at the instant
    it was captured;
  - **voice:** 16 kHz microphone audio, only while the hardware mute is off and voice is on for
    the room.
- **Room state on the wire.** `room_volume` and `sound` reach every endpoint of a room. Each
  endpoint enforces, at every frame, the smallest of the ramped gain, the room's limit and its own
  ceiling.

### The TV path

The TV path is a separate low-latency mode for one wired room:

1. A Linux hub captures stereo LPCM from the TV's optical or HDMI ARC output.
2. The hub rate-matches it onto the server timeline and sends it up. A signal that is not PCM, or
   that drifts beyond +/- 1500 ppm, is refused by name.
3. The server relays it to the room's players as UDP datagrams. They are encrypted under a
   per-offer ChaCha20-Poly1305 key and sent in 2.5 ms chunks with one XOR parity chunk per four.
   The default target latency is 25 ms.

The hub is also the TV's Audio System over HDMI-CEC. It uses the kernel's `/dev/cecN`, not
libcec. So the TV remote drives the room's volume, and the TV's power starts and stops the room.
Each room has a signed A/V trim. See [`docs/cec.md`](docs/cec.md) and ADRs 0087 to 0094.

### The control plane

The control plane runs on `--control-listen` (TCP 4020 by convention), separate from the audio
connection. It is HTTP/1.1 with server-sent events. Its contract is
[`docs/control-plane.md`](docs/control-plane.md), with vectors in
[`fixtures/control/`](fixtures/control/). The catalog is versioned and made of canonical JSON
messages; an unknown field or command is rejected.

The server's state is authoritative and persisted in `--state-file`. Every client reads it from
`GET /api/state` and `GET /api/events`, and changes it through `POST /api/command`. Rooms are
declared when the server starts (`--zone`); no command creates one.

The control plane has no authentication of its own. A deployment reaches it through the
household's reverse proxy: HTTPS, local network only, behind the household login. The server
itself refuses any `POST` that is not JSON or that names another origin.

Discovery is hand-written multicast DNS and DNS-SD (`_chorus-audio._tcp` and `_chorus-ctl._tcp`),
with a static address as the fallback. An endpoint that has neither exits and says which one it
lacked.

### The endpoints

| | ESP32-S3 speaker ([`firmware/`](firmware/)) | Linux endpoint ([`crates/client-linux`](crates/client-linux)) |
|---|---|---|
| Platform | C on ESP-IDF v6.1 | Rust with plain threads; ALSA loaded at run time (`dlopen`) |
| Link | W5500 over SPI (wired, PoE through an 802.3at splitter), or Wi-Fi with power save off | Ethernet or Wi-Fi |
| Device delay | frames the I2S DMA consumed, counted and stamped in the interrupt | `snd_pcm_delay` |
| Output | TAS5825M over I2S, 24-bit samples in 32-bit slots, with a register map read from TI's datasheet (page cited on every line) | any ALSA device; an output map drives N channels (the assumed multichannel board is a Pi 5 with an 8-channel DAC HAT) |
| Extras | buttons and a status LED, a microphone gate (no microphone part chosen yet), Wi-Fi setup from a phone over the speaker's own access point, A/B OTA with rollback, a serial console | a front panel through evdev and the LED class, a line-in as a source, the TV hub (capture, CEC, the low-latency path) |
| Variants | board profiles `brick-s3-wired` (the default), `compact-s3-wifi`, `devkitc-s3-louderhat-wired` (the compact on bought modules) and `qemu-s3-openeth` (the emulator) | the `chorus-endpoint` `.deb` for arm64 and amd64, with a hardened systemd unit |

Both kinds of endpoint speak the same protocol and run the same DSP chain on every frame. The C
cores are held to the Rust cores' fixtures.

## What it does

Every item here is built and tested in software. What has and has not run on hardware is in
[Where it stands](#where-it-stands).

### Rooms and groups

See [`docs/control-plane.md`](docs/control-plane.md) and
[ADR 0075](docs/decisions/0075-control-catalog-v2.md).

- Named rooms, and bonded sets within a room (wired endpoints only): a stereo pair (with an
  optional sub), a left-centre-right set, and a 5.1 theater set.
- Saved groups that persist, and live groups formed on the fly. "Take the room" moves a room to
  a new source.
- Sonos-style group volume: the group slider scales every room in proportion, and each room is
  clamped to its own limit.
- Per-room volume limits and quiet hours: up to eight weekly windows, each with its own cap,
  switched on and off per room.
- Alarms with ramps and four sources: a generated chime, a line-in, a stored stream URL or a
  stored Spotify URI. An alarm falls back to a bell when its source cannot play.
- Sleep timers with a fade.
- Autoplay rules that start a room when a line-in gets signal or the TV turns on.

### Sound

See [`docs/dsp.md`](docs/dsp.md) and [`docs/room-correction.md`](docs/room-correction.md).

- **One DSP library in two languages:** `crates/dsp` and `firmware/src/dsp.c`, held to shared
  fixtures whose expected values come from cited worked examples. It has RBJ biquads,
  Linkwitz-Riley crossovers, delay, and a look-ahead limiter at the room's limit.
- **Per-room settings:** bass, treble, loudness (ISO 226), night mode, speech enhancement, bass
  management, an active two-way crossover, theater up- and downmix maps, and a signed A/V trim.
- **Room correction:**
  1. The app plays a sweep and records it with the phone's microphone.
  2. The server fits at most eight peaking filters. It prefers cuts, never boosts more than
     +3 dB, and leaves nulls alone.
  3. A fitted correction has one undo.
- **Announcements:** the music ducks 20 dB in 200 ms, the clip plays, and the music restores in
  500 ms. Ducking happens per room, so one room inside a playing group can duck alone.

### Inputs

See [`docs/inputs.md`](docs/inputs.md), [`docs/upnp.md`](docs/upnp.md),
[`docs/soloist.md`](docs/soloist.md), [`docs/streams.md`](docs/streams.md) and
[`docs/decoders.md`](docs/decoders.md).

- **UPnP** (`--upnp`, off by default): every room, saved group and live group is a UPnP AV media
  renderer. It also offers OpenHome Playlist, so a queue plays gaplessly with no phone connected.
- **Spotify** (off by default): official Soloist receivers, one per room and group, supervised by
  `chorus-soloistd`. chorus never downloads, ships or runs Soloist; the owner supplies it.
- **Line-ins:** one line-in can be shared into any number of groups at one latency. A line-in
  labelled as a streamer plays into its own room when signal appears.
- **Decoding:** the server decodes MP3, FLAC, Vorbis, ALAC and WAV through Symphonia, and Opus
  through libopus. AAC is never compiled.
- **Fetching:** media is fetched by a hand-written HTTP/1.1 client under an address policy. No
  command plays an arbitrary URL.

### Ways to control it

- **The app** ([`docs/app.md`](docs/app.md), [`web/`](web/)): Lit 3 elements bundled by esbuild,
  compiled into `chorus-server` and served under `/app/`. It covers:
  - rooms, groups, inputs, and now playing with artwork;
  - each room's sound, limits, theater settings and correction;
  - autoplay, alarms and sleep timers;
  - speakers: naming, firmware updates and a Wi-Fi setup walk-through.

  It has layouts for a phone, a desktop and a wall tablet in kiosk mode, and installs behind the
  household login.
- **The control page** ([`docs/control-page.md`](docs/control-page.md)): the original single page
  at `/`, one card per room.
- **`chorusctl`** ([`docs/chorusctl.md`](docs/chorusctl.md)): a command line over the control API,
  with `--json` and documented exit codes.
- **Home Assistant** ([`docs/home-assistant.md`](docs/home-assistant.md),
  [`integrations/homeassistant/`](integrations/homeassistant/)): a custom integration written to
  every rule of Home Assistant's quality scale up to Platinum (self-certified). It provides:
  - a media player per room and per saved group, and sound controls;
  - a device per adopted speaker, with a firmware update entity, diagnostics and button events;
  - a visualizer sensor per room;
  - an Assist satellite per voice room;
  - a dashboard built from stock cards.

  It is installed from a pinned export, never through HACS.
- **Prometheus** (`GET /metrics`, [`docs/telemetry.md`](docs/telemetry.md)) and an opt-in,
  read-only **MQTT** publisher ([`docs/mqtt.md`](docs/mqtt.md)).

### Voice

See ADRs 0166 to 0179.

- A room with a microphone is an Assist satellite in Home Assistant.
- The wake word ("Okay Nabu") runs on the server: microWakeWord's Apache-2.0 model, in chorus's
  own integer interpreter.
- A speaker streams microphone audio only while its hardware mute is off and voice is on for the
  room. That audio is never a source anyone can play.

### Fleet

See [`docs/firmware-updates.md`](docs/firmware-updates.md).

- **Adoption:** a new speaker on the audio network is adopted automatically, with its key pinned,
  and is named and assigned a room afterwards.
- **Firmware updates:** the server stages and verifies images and shows "update available". A
  speaker installs one only on an explicit install command, into its inactive A/B slot. If the
  new image does not rejoin the server in time, the speaker rolls back by itself.
- **Telemetry:** about once a second, each speaker reports its buffer fill, sync error,
  correction rate, resyncs, underruns, link, RSSI and heap.

## Where it stands

chorus is built in phases:

- BRIEF.md section 8 gives the ten engineering phases, each with a "success looks like".
- Section 8.1 extends them into the program's phases toward Sonos parity.
- The per-criterion record for phases 2 to 7 is
  [`docs/verification-record.md`](docs/verification-record.md).
- The cold audit of phases 1 to 10, from the start of the program, is
  [`docs/audit/2026-09-audit.md`](docs/audit/2026-09-audit.md).
- Every item of section 8 and of the owner's parity decisions, with its state and evidence, is
  [`docs/parity.md`](docs/parity.md).

**One rule is behind every row: hardware criteria are graded only on hardware.** Simulations,
host runs on ALSA `null`, QEMU boots and fakes prove the logic. They never pass a timing
criterion. Every report in `docs/measurements/` says which kind it is: `host`, `simulation` or
`synthetic`. None is `hardware` yet.

| BRIEF.md phase | In software | Hardware criteria |
|---|---|---|
| 1. Foundation | met in simulation: the Rust and C cores read the same fixtures and agree exchange by exchange | none (a simulation phase) |
| 2. First sound | server, Linux client and FIFO input built; the continuity half of the ten-minute run passes on ALSA `null` | NOT PASSED: nothing has been heard |
| 3. Measurement rig | lag and drift analysis checked against synthetic captures | NOT PASSED: no capture from real devices |
| 4. Synchronization | the servo is closed on the real path; a modelled hour stays inside 0.2 ms (a model, not the criterion) | NOT PASSED: the one-hour two-endpoint run needs the rig |
| 5. Embedded bring-up | the gate compiles an image for every board profile; the image boots under QEMU and a real server adopts it; the playout path is graded on a host against a fake DMA | NOT PASSED: never flashed, never driven a pin |
| 6. Product hardening | rooms, groups, volume, control, discovery and restart storms built; a one-hour, 8-room house soak on ALSA `null` passes | NOT PASSED: the three-day soak |
| 7. Wi-Fi tier | expectations published; jitter analysis run over modelled series | NOT PASSED: no radio characterized |
| 8. DSP | the library on both platforms from shared fixtures; room-correction fitting | not run: the chain's cost on the chip and the two-way speaker are bench steps |
| 9. TV / surround | capture rate matching, the UDP relay with FEC, CEC on a fake bus, A/V trim and theater maps built; a simulated latency budget | not run: no TV, no capture device |
| 10. Fleet | OTA under QEMU (a good image confirmed, a bad one rolled back), adoption, provisioning, telemetry, MQTT and `chorusctl` | not run on a board; not deployed |

The program's phases (BRIEF.md section 8.1):

| Program phase (goals) | State |
|---|---|
| Start-up, audit, fixes, conventions, first release (1 to 4) | done: the audit and its fixes, `make gate`, chorus's own conventions, MIT OR Apache-2.0, release v0.1.0 (2026-09-30), and the homelab deploy PR opened for the owner |
| Protocol v2 (5, 6) | built |
| Sound, rig and sync in software; bench packets (7) | built: the 8-room simulation and the owner's bench packet |
| The embedded platform (8, 9) | built, on a host and under QEMU |
| The Linux endpoint tier (10) | built and packaged as a `.deb` |
| Rooms and groups (11) | built |
| DSP (12) | built |
| The TV path (13) | built, on fakes |
| Fleet (14, 15) | built |
| Inputs (16, 17) | built; Soloist itself has never been downloaded or run here (a fake receiver stands in) |
| The Home Assistant integration (18, 19) | built |
| Voice and announcements (20) | built in software; no microphone has been chosen or bought |
| The app (21, 22) | built, both parts; never opened on a phone |
| Acoustic design tools (23) | belongs to the owner's shared Python library, not this repository |
| The speaker designs (24 to 26) | designed: the compact speaker, the two-way, the subwoofer and the LCR set, each landed in the owner's devices repo ([`docs/hardware/`](docs/hardware/), decisions 0233 and 0234); no rack amp and no soundbar ([P13](docs/proposals/P13-rack-amp-zones.md), decision 0231); nothing built or bought |
| Finale (27) | under way: the docs, the pins ([`docs/pins.md`](docs/pins.md)) and the parity checklist; release v0.2.0 |

Since 2026-10-04 the remaining work arrives as tasks from the owner's agent harness rather than as
program goals.

**Every hardware step waits on the owner** (K4, K28, K93):

- buying the bench on the owner's final list in
  [`docs/proposals/P4-bench-purchase.md`](docs/proposals/P4-bench-purchase.md);
- running its sessions S0 to S12 ([`docs/bench-packet.md`](docs/bench-packet.md));
- flashing;
- the app's phone checks;
- any OTA install on an installed speaker;
- applying the homelab deploy.

Hardware results come back as schema-checked reports on `bench/*` branches
([`docs/bench.md`](docs/bench.md)).

## The repository

```
BRIEF.md                     the guiding document: goals, targets, guardrails, design areas, roadmap
CLAUDE.md                    the must-knows for the agents that build chorus (in full: docs/working-agreement.md)
crates/                      26 Rust crates: the server, the Linux client, chorusctl, the pure cores
firmware/                    the ESP32-S3 endpoint in C (ESP-IDF v6.1) and its host test suites
web/                         the app (Lit 3, esbuild); its build output, dist/, is committed
integrations/homeassistant/  the Home Assistant custom integration and its test harness
fixtures/                    shared vectors and scenarios that both implementations are held to
config/                      the constants that timing and measurement claims rest on
audio-path.conf              every function on the audio and timestamp path, enumerated and checked
real-time-acquisitions.conf  every real-time thread acquisition, checked
tools/                       the gate, conventions checks, verifications, bench, QEMU, packaging, release
deploy/                      the server image, compose files, the Linux endpoint's unit, Soloist receivers
third_party/                 vendored libopus 1.6.1, dr_flac 0.13.3 and the microWakeWord model
docs/                        contracts, guides, decisions, measurements, proposals, research, audits
```

### Crates

| Crate | What it is |
|---|---|
| **Programs** | |
| `server` | `chorus-server`: ingest, stream slots, the room model, the schedule, the control plane, the app, UPnP, Soloist, MQTT, metrics, firmware staging |
| `client-linux` | `chorus-client`: the Linux endpoint (playout, the sync loop, the output map, line-in, front panel, the CEC hub) |
| `ctl` | `chorusctl`, a std-only client of the control API |
| `soloistd` | `chorus-soloistd`, the supervisor of one Spotify Soloist receiver |
| `measure` | `chorus-measure` and `chorus-measure-capture`, the measurement rig (lag, drift, jitter, rate) |
| **Pure cores** | |
| `protocol` | the audio wire protocol: framing, v2 sessions, the Noise handshake, negotiation, the low-latency datagrams |
| `sync` | virtual clocks, jitter models, the servo and the deterministic simulator (`chorus-sim-house` and others) |
| `control` | the control catalog: versioned JSON, the server-authoritative room state, its persisted form, the bounded fanout |
| `discovery` | multicast DNS and DNS-SD, hand-written |
| `dsp` | filters, crossovers, delay, limiter, compressor, loudness, speech, bass management, room-correction fitting, the visualizer, the announcement mixer |
| `schedule` | civil time from TZif, weekly windows, alarms, sleep timers, ramps and generated chimes; reads no clock |
| `controls` | one model of a speaker's buttons, LED and microphone gate (the twin of `firmware/src/controls.c`) |
| `upnp` | the UPnP AV and OpenHome renderer protocol, written from the specifications |
| `soloist` | the Soloist receiver protocol, API model and pool |
| `mqtt` | the pure half of the publish-only MQTT 3.1.1 client |
| `wakeword` | the wake-word detector: the microWakeWord frontend and an integer interpreter for its models |
| `decode` | the server's decoders, resampler and remix |
| `audio-path` | the checks that hold `audio-path.conf` to the code |
| **Platform and host** | |
| `audio` | PCM ingest, chunking and the monotonic server timeline |
| `alsa` | ALSA playback and capture through `libasound.so.2`, loaded at run time |
| `cec` | HDMI-CEC for the Linux hub, over the kernel's API |
| `fetch` | a blocking HTTP/1.1 client with rustls, the fetch policy, ICY metadata and the packed-MP3 subset of HLS |
| `hostctl` | the container's scheduling contract: rtprio ceiling, CPU-time bound, locked memory |
| `hostprobe` | host evidence: wakeup jitter, kernel receive timestamps, UDP loss |
| `opus-sys` | libopus 1.6.1 from `third_party/opus`, fixed point, decoder only |
| **Tests only** | |
| `soloist-fake` | a fake Soloist receiver; no image or release can carry it |

### Firmware

`firmware/src/` is the endpoint itself. The host build and the ESP-IDF build share it byte for
byte. It contains:

- the protocol and the Noise session, over the PSA Crypto API;
- the sync servo, the jitter buffer and I2S playout;
- the W5500 link, Wi-Fi, and Wi-Fi provisioning over the speaker's own access point;
- the amplifier sequencer, the controls and the DSP chain;
- FLAC and Opus decoding (vendored dr_flac and libopus);
- the low-latency receiver and OTA;
- identity and storage, discovery, telemetry and a serial console.

The rest of the firmware tree:

- `firmware/main/` binds the endpoint to ESP-IDF.
- `firmware/boards/` holds the board profiles, laid over
  [`firmware/config/endpoint.conf`](firmware/config/endpoint.conf). That file holds every value
  the endpoint uses, each marked when it is assumed rather than read from a datasheet.
- `firmware/tests/` runs everything on a host against fakes of the amplifier, flash, radio and
  store.
- `firmware/check/` holds the safety scans. One fails on any configuration that could burn an
  eFuse.

## Building and testing

What you need:

- **Rust 1.98.1**, pinned in `rust-toolchain.toml`; rustup installs it on first use. No crate needs
  a system library to build. `libasound.so.2` is needed only to play audio.
- **A C compiler** for the firmware's host checks, and the source tree of ESP-IDF v6.1 at the
  commit pinned in `firmware/config/endpoint.conf`. The host build compiles its crypto library
  from that tree (set `CHORUS_IDF_V61_DIR`). Building an image needs the full ESP-IDF toolchain.
- **The gate's other tools.** They are pinned with digests in `mise.toml` and installed rootless
  with `mise install`:
  - gitleaks, cargo-deny, shellcheck, actionlint, yamllint, clang-format and cppcheck;
  - zig and cargo-zigbuild, for the cross builds;
  - cargo-nextest and promtool;
  - uv, for the Home Assistant harness;
  - node and pnpm, for the app.

Then:

```sh
cargo build --release -p chorus-server -p chorus-client-linux  # the two programs
cargo test -p chorus-protocol                                  # one crate's tests
cargo test -p chorus-client-linux --test zone_apply            # one test target
make verify          # quick no-device checks: the refusal paths, and that unrun checks say so
make firmware-check  # the ESP32-S3 endpoint on a host: no board, no ESP-IDF toolchain
make web-test        # the app's unit tests (node, no browser)
make ha-test         # the Home Assistant integration under its pinned harness
make gate-fast       # the conventions checks alone
make gate-changed    # what a pull request runs: only what the change touches
make gate            # everything, nightly on main
```

**The gate.** `make gate` (`tools/gate.sh`) runs every check a change is held to, and times each
step:

- formatting, and clippy with warnings denied;
- the workspace tests under nextest;
- the conventions checks;
- the firmware's host suites, an image for each board profile, and the QEMU boot and OTA runs;
- the server and Soloist images;
- the app's tests, its build and its browser smoke test;
- the Home Assistant steps.

A pull request runs `make gate-changed` ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)):
the conventions checks, then fmt, clippy and the tests of only the crates the change touches and
the crates that depend on them, plus the app, Home Assistant and firmware host checks only when
those trees changed. A docs-only change builds nothing. Its target is two minutes. The full gate
runs nightly on main ([`.github/workflows/nightly.yml`](.github/workflows/nightly.yml)). A pull request merges when an independent reviewer passes it,
without waiting for CI, and a red main gets a fix-forward task
([ADR 0140](docs/decisions/0140-ci-is-the-gate.md)). On a development host, local runs stay
narrow: one crate's focused test, or one conventions check.

**Tests need no device.** Nothing in the suite needs a sound card, a board or any network beyond
loopback. Each check that needs a real environment is a script in `tools/`. Where its
prerequisite is missing, the script refuses: it exits non-zero, naming what is missing and the
criterion it would have verified, rather than reporting green.

```
./tools/ten-minute-run.sh                 # needs a real audio device
./tools/spin-test.sh                      # needs a granted rtprio ceiling above zero
./tools/host-contract.sh                  # needs a granted rtprio ceiling above zero
./tools/device-loss-run.sh                # needs a device you can remove mid-run
./tools/control-plane-run.sh              # needs a playback device that opens
./tools/restart-storm-run.sh              # needs a playback device that opens
./tools/discovery-fallback-run.sh         # needs a playback device that opens
./tools/mdns-live-run.sh                  # needs a link that carries multicast DNS
./tools/measure/capture-run.sh            # needs two endpoints and an audio interface
./tools/sync-hour-run.sh                  # needs two wired endpoints and the rig
./tools/firmware-image.sh                 # needs ESP-IDF at the declared version
./tools/endpoint-rig-run.sh               # needs an ESP32-S3, an amplifier and the rig
./tools/wireless-characterization-run.sh  # needs a wireless ESP32-S3 in another room and the rig
./tools/soak-run.sh                       # needs three days of wall clock and the rig
```

[`docs/verification-record.md`](docs/verification-record.md) records, per criterion, which of
these ran where chorus was built and which did not, with the refusal quoted.

## Running it

### On one machine

This runs a server playing a test tone, and one Linux endpoint playing it on the default ALSA
device:

```sh
cargo build --release -p chorus-server -p chorus-client-linux

./target/release/chorus-server --source tone --serve-forever --ephemeral-identity \
    --allow-non-realtime --allow-unlocked-memory

./target/release/chorus-client --server 127.0.0.1:4010 --device default --ephemeral-identity
```

- `--source` also takes a file, `chirp` (the rig's test signal) or `fifo:<path>`. A FIFO is a
  named pipe the server holds open, so a player can write PCM into it. An underrun plays as
  silence.
- `--allow-non-realtime --allow-unlocked-memory` are for a development machine. Without them the
  server refuses to start unless the host grants it a real-time priority and lets it lock its
  memory. Every status line of a run that uses them says what is missing.
- `--ephemeral-identity` gives each process a throwaway key. A real server keeps its key and its
  adopted speakers in `--identity-dir`, or beside `--state-file`. A real endpoint needs
  `--identity-dir` and a stable `--endpoint-id`.
- `--device` takes an ALSA PCM name: `default`, `hw:Loopback,0` for a loopback, or `null` to open
  the device and discard the audio.
- `--control-listen 127.0.0.1:4020 --zone <id>` on the server turns on the control plane:
  - the app, at `http://127.0.0.1:4020/app/`;
  - the control page, at `/`;
  - the API that `chorusctl --server 127.0.0.1:4020 rooms list` talks to.

  The client's `--control` and `--zone` attach it to that room.

[`docs/sound-2.md`](docs/sound-2.md) is the guide to this basic path: every number it chose, and
the checks behind each. [`docs/linux-endpoint.md`](docs/linux-endpoint.md) covers the endpoint's
flags, multichannel output maps, the front panel, the line-in and the TV hub.

### Optional services

Each of these is off unless its flag is given:

| Flag | Turns on | Guide |
|---|---|---|
| `--slots <n>`, `--players <n>` | one stream per playing group, and network media players | [`docs/control-plane.md`](docs/control-plane.md) |
| `--upnp` | a UPnP AV and OpenHome renderer per room and group (HTTP on 4030, SSDP on UDP 1900) | [`docs/upnp.md`](docs/upnp.md) |
| `--soloist-receivers <n>`, `--soloist-dir <dir>` | Spotify Connect through the owner's Soloist receivers | [`docs/soloist.md`](docs/soloist.md) |
| `--mqtt-broker <host:port>` | the read-only MQTT publisher | [`docs/mqtt.md`](docs/mqtt.md) |
| `--firmware-dir <dir>` | firmware staging, and installs on explicit command | [`docs/firmware-updates.md`](docs/firmware-updates.md) |
| `--announce-origin <origin>` | announcements, from that origin only | [`deploy/README.md`](deploy/README.md) |
| `--advertise` | the multicast DNS advertisement | [`docs/control-plane.md`](docs/control-plane.md) |

`chorus-server --help` lists every flag.

## Deploying

Deploying is the owner's action. Nothing in this repository deploys, flashes or installs anything
on a real device.

- **The server** ships as a daemonless OCI image. `make image` builds a static musl
  `chorus-server` on a digest-pinned distroless base. [`deploy/compose.yaml`](deploy/compose.yaml)
  runs it with:
  - host networking, for multicast DNS and latency;
  - an rtprio ceiling of 20, with a CPU-time bound on every real-time thread;
  - 64 MiB of locked memory;
  - every capability dropped;
  - a health check on the control plane.

  The homelab deploy is a PR in the owner's homelab repository, which the owner merges and
  applies. [`deploy/README.md`](deploy/README.md) has the files, the identity directory and the
  log lines.
- **Linux endpoints** install the `chorus-endpoint` `.deb`, built by `make endpoint-packages`
  (cross-built for arm64 and amd64 at glibc 2.36). It runs `chorus-client` under a hardened
  systemd unit ([`docs/linux-endpoint.md`](docs/linux-endpoint.md)).
- **ESP32-S3 speakers** are flashed only through `tools/firmware-flash.sh`. It refuses unless the
  owner, at the bench, enables it on their own command line. No build ever enables Secure Boot,
  flash encryption or anti-rollback eFuses (guardrail 2), and the gate scans for them. After the
  first flash, updates go over the network as described under [Fleet](#fleet).
- **Soloist receivers** run as their own containers, from an image that contains no Soloist file
  ([`deploy/soloist/`](deploy/soloist/), [`docs/soloist.md`](docs/soloist.md)).
- **Releases** are cut by CI from a `v<x.y.z>` tag on main
  ([`.github/workflows/release.yml`](.github/workflows/release.yml), which runs
  `make release`), and the same tag publishes the images to ghcr.io
  ([`docs/release.md`](docs/release.md)). The releases so far are v0.1.0 and v0.2.0.

## Contributing

chorus is public, and changes come as pull requests against `main`. What a change is held to:

- **Build and test it narrowly.** Set up the toolchains under
  [Building and testing](#building-and-testing): `mise install` for the gate's tools, rustup
  for the pinned Rust. Then run the focused tests of what you touched: one crate's
  `cargo test -p <crate>`, `make firmware-check` for `firmware/`, `make web-test` for `web/`,
  `make ha-test` for the Home Assistant integration. `make gate-fast` runs the conventions checks
  alone, with no build.
- **CI is the gate.** Every pull request runs `make gate-changed`: the conventions checks, then
  only the crates and trees the change touches and their dependants. The full `make gate` runs
  nightly on main ([ADR 0140](docs/decisions/0140-ci-is-the-gate.md)). A change merges as one
  squashed commit once an independent review passes it. A red main is fixed forward, never by
  rewriting history.
- **Follow the conventions.** [`docs/conventions.md`](docs/conventions.md) lists every rule and
  the check that holds it. The ones a newcomer meets first:
  - a commit subject is `<area>: <summary>`, with a lower-case area such as a crate name,
    `firmware`, `web`, `docs`, `tools`, `deploy` or `ci`, in at most 100 characters (rule 21);
  - no em dash anywhere: code, docs, commit messages or pull request bodies (rule 18);
  - no personal name, address, LAN address, hostname or secret in a tracked file (rule 19);
  - protocol, sync and DSP behaviour is specified by files in [`fixtures/`](fixtures/) that the
    Rust and the C implementations both read, so a behaviour change adds or changes a fixture,
    never a constant in one language only (rule 9);
  - every timing claim cites a report in [`docs/measurements/`](docs/measurements/), labelled
    with its source (rule 11), and only monotonic clocks enter the audio path (BRIEF.md
    section 3.1);
  - a new dependency, tool or version is pinned exactly ([`docs/pins.md`](docs/pins.md)).
- **Keep the clean room.** Never open the source of a GPL project, or the design files of
  reciprocally licensed hardware. Their docs, issues and specifications are fine, and so is
  permissive source, cited. [`docs/clean-room.md`](docs/clean-room.md) lists the projects and
  says how a reading is recorded.
- **Never write code that can burn an eFuse or flash a board by itself.** The gate scans for
  both (rule 20).
- **Proposing a change.**
  - A fix or a small feature is a pull request with its tests.
  - A choice that is cheap to reverse is made in the pull request and recorded as a decision in
    [`docs/decisions/`](docs/decisions/): draft it as `0000-<slug>.md`, rename it to the pull
    request's number once that exists, and add it to the index (rule 15).
  - A choice that is expensive to reverse, such as a toolchain upgrade, a new platform or a
    change to a guardrail-adjacent rule, starts as a proposal in
    [`docs/proposals/`](docs/proposals/), and the owner decides it.
  - When a measurement contradicts BRIEF.md, the same change records it as a decision and
    corrects the brief. The five guardrails of BRIEF.md section 3.1 may be tightened, never
    relaxed.
- **Contributions are licensed** under MIT OR Apache-2.0, like the rest of chorus (see
  [Licence](#licence)).

## How the project is run

- **BRIEF.md is the guiding document.** It recommends rather than dictates, and the work keeps it
  current: verified corrections and the owner's decisions are written into it, dated, with their
  decision IDs (the K-numbers).
- **There are five hard guardrails** (BRIEF.md section 3.1). They may be tightened but never
  relaxed:
  1. **Clean room.** No code is copied or closely paraphrased from reference projects:
     - of GPL projects, only docs, issues, papers and protocol specs are read, never source;
     - the design files of reciprocally licensed hardware are never opened;
     - permissive source may be read and cited.

     See [`docs/clean-room.md`](docs/clean-room.md). Every proposal, research note and decision
     since ADR 0032 records what was read.
  2. No Secure Boot, flash encryption or anti-rollback eFuses on development hardware.
  3. Timing claims are backed by measurement with the harness. "Sounds synced" is not evidence.
  4. Only monotonic clocks are used in the audio and timestamp path.
  5. No em dashes in any written output.
- **Decisions** are one file each in [`docs/decisions/`](docs/decisions/), with an
  [index](docs/decisions/README.md). From 0032 on, a record's number is the number of the pull
  request that added it. Bigger choices start as proposals in
  [`docs/proposals/`](docs/proposals/), and the owner decides them.
- **Measurements** go in [`docs/measurements/`](docs/measurements/). Each one is labelled with its
  source (`hardware`, `host`, `simulation` or `synthetic`) and the commit it measured.
- **Every rule has a check.** [`docs/conventions.md`](docs/conventions.md) lists chorus's rules
  and the script or gate step that enforces each one. They cover formatting and lints, shared
  fixtures, pins, licences and dependencies, provenance, identity and secrets, the flash guard,
  commit messages and more.
- **Build what defines the system, vendor what does not.** These are written here:
  - the sync engine, the protocol, the jitter buffer, the DSP and the control plane;
  - small protocols such as JSON, multicast DNS, HTTP/1.1 and MQTT, because each is small and
    instructive.

  The RTOS, network stacks, drivers and crypto primitives are platform. Vendored and pinned code
  is listed in [`third_party/README.md`](third_party/README.md) and `deny.toml`.
- **Work arrives as tasks** from the owner's agent harness, each with a finish line written as
  runnable checks. [`CLAUDE.md`](CLAUDE.md) holds the must-knows for the agents doing it, and
  [`docs/working-agreement.md`](docs/working-agreement.md) the full working agreement. The
  original program (its brief, goal files and ledgers) is pinned at
  [`.claude/goals`](https://github.com/NSchatz/chorus/tree/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals).

## Documentation map

**Contracts** (what a second implementation is held to)

| Doc | What it covers |
|---|---|
| [`docs/protocol.md`](docs/protocol.md) | the audio wire format: frames, v2 sessions, roles, the low-latency datagrams |
| [`docs/control-plane.md`](docs/control-plane.md) | the control catalog, its routes and the state message |
| [`fixtures/README.md`](fixtures/README.md) | the shared fixtures, their formats and who reads them |

**Guides**

| Doc | What it covers |
|---|---|
| [`docs/sound-2.md`](docs/sound-2.md) | the basic audio path: how to run it, and every number it chose |
| [`docs/linux-endpoint.md`](docs/linux-endpoint.md) | the Linux endpoint: package, flags, multichannel, front panel, TV hub |
| [`firmware/README.md`](firmware/README.md) | the ESP32-S3 endpoint and its host checks |
| [`docs/wireless-expectations.md`](docs/wireless-expectations.md) | what the Wi-Fi tier promises, and why |
| [`docs/dsp.md`](docs/dsp.md), [`docs/room-correction.md`](docs/room-correction.md) | the DSP chain, per-room sound, room correction |
| [`docs/visualizer.md`](docs/visualizer.md), [`docs/chimes.md`](docs/chimes.md) | the visualizer stream, the generated chimes |
| [`docs/inputs.md`](docs/inputs.md), [`docs/streams.md`](docs/streams.md), [`docs/decoders.md`](docs/decoders.md) | alarm sources, line-in sharing, streamers; what URLs play; the decoders |
| [`docs/upnp.md`](docs/upnp.md), [`docs/soloist.md`](docs/soloist.md) | the UPnP AV and OpenHome renderers; Spotify through Soloist |
| [`docs/cec.md`](docs/cec.md) | the TV hub as the TV's Audio System |
| [`docs/app.md`](docs/app.md), [`docs/control-page.md`](docs/control-page.md), [`docs/chorusctl.md`](docs/chorusctl.md) | the app, the control page, the command line |
| [`docs/home-assistant.md`](docs/home-assistant.md) | the Home Assistant integration (installing it: [`integrations/homeassistant/README.md`](integrations/homeassistant/README.md)) |
| [`docs/mqtt.md`](docs/mqtt.md), [`docs/telemetry.md`](docs/telemetry.md) | the MQTT publisher; `/metrics` and what each speaker reports |
| [`docs/firmware-updates.md`](docs/firmware-updates.md) | staging, installs, A/B slots and rollback |
| [`docs/house-soak.md`](docs/house-soak.md) | the one-hour, 8-room software soak |
| [`deploy/README.md`](deploy/README.md), [`docs/release.md`](docs/release.md) | deploying the server; cutting a release |

**Hardware and the bench**

| Doc | What it covers |
|---|---|
| [`docs/bench-packet.md`](docs/bench-packet.md) | the owner's buy list, wiring and sessions S0 to S12 |
| [`docs/bench.md`](docs/bench.md) | how hardware results come back into the repository |
| [`docs/hardware/`](docs/hardware/) | the speaker designs (compact, two-way, subwoofer, LCR set), controls per speaker class, Linux multichannel, the voice-room microphone |

**Records**

| Doc | What it covers |
|---|---|
| [`docs/verification-record.md`](docs/verification-record.md) | each phase's criteria: what ran, where, and what did not |
| [`docs/parity.md`](docs/parity.md) | every parity item with its state and evidence |
| [`docs/pins.md`](docs/pins.md) | every pin, whether it is current, and why any is held back |
| [`docs/audit/`](docs/audit/) | the cold audit of every phase, and the Home Assistant mutation audit |
| [`docs/decisions/`](docs/decisions/) | one record per significant decision |
| [`docs/measurements/`](docs/measurements/) | measurement reports, each labelled with its source |
| [`docs/proposals/`](docs/proposals/), [`docs/research/`](docs/research/) | proposals put to the owner, and the research behind them |
| [`docs/conventions.md`](docs/conventions.md), [`docs/clean-room.md`](docs/clean-room.md) | the rules and their checks; the clean-room record |

## Licence

chorus is licensed under either the [MIT licence](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option. A contribution is licensed the same way unless it says otherwise.
Vendored code keeps its own licence ([`third_party/README.md`](third_party/README.md)). The images list theirs in
`deploy/THIRD-PARTY-NOTICES.md` and `deploy/soloist/THIRD-PARTY-NOTICES.md`.
