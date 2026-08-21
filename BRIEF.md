# Chorus: From-Scratch Multiroom and Surround Audio System
## Project Brief and Design Guide v2 (hand-off document for Claude Code)

Owner: Noah
Date: 2026-08-21
Status: Guiding document, not a rigid specification. It defines goals, requirements, constraints, and reference knowledge, and it makes recommendations with rationale. Implementation decisions are made during the project, in collaboration with the owner, and recorded as they happen.
Codename: "chorus" (placeholder, rename freely).

---

## 0. How to use this document (instructions to Claude Code)

1. Read the whole document before writing code. Treat it as the project's shared context, not a checklist to execute blindly.
2. Most of this document is recommendation, not mandate. When you reach a decision point, weigh the options, propose an approach with reasoning, and let the owner confirm before committing to anything expensive to reverse. Cheap-to-reverse decisions can just be made and noted.
3. Section 12 lists the decisions that need to be made. Keep a lightweight decision log (one markdown file per significant decision in `docs/decisions/`) so the reasoning survives.
4. A small number of items are hard guardrails (Section 3.1) for safety, legal, and project-integrity reasons. Those are not negotiable. Everything else is.
5. Recommendations below are labeled as such. If measurement or experience during the build contradicts a recommendation, say so and propose the change; the recommendation loses.
6. Style note from the owner: no em dashes anywhere (code comments, docs, commits). Use commas, periods, parentheses, or hyphens. Direct, peer-level tone.

---

## 1. Vision

Replace the Sonos ecosystem with a system Noah builds and understands end to end:

- A centralized audio server running in a container on his Proxmox homelab (Dell R730xd).
- Self-developed software on every speaker endpoint: embedded firmware on microcontroller-class smart speakers, and a lightweight client on Linux-class endpoints.
- A custom network protocol with tightly synchronized multiroom playback.
- Eventually, a low-latency path for TV and surround audio within lip-sync limits.
- The usual product surround: discovery, control, grouping, volume, OTA updates, Home Assistant integration, observability.

The point is not just the product. It is owning the whole stack: the sync algorithm, the protocol, the playback engine, the DSP, the firmware. Existing projects (Snapcast, squeezelite, shairport-sync, Roc) are studied as prior art, not adopted as dependencies.

Environment facts to assume:

- Gigabit Cat6 to most rooms through conduit; TP-Link Omada PoE+ switch; UniFi Wi-Fi; VLANs.
- Proxmox host with Docker; Home Assistant and an MQTT broker already running.
- Endpoints will be mostly wired Ethernet or PoE. Wi-Fi endpoints are acceptable for casual music zones only.
- Owner's background: strong Node.js/Python/AWS/Terraform/Docker/CI-CD, ESP32/ESPHome experience, comfortable learning C and Rust, DIY speaker building and woodworking capability.

---

## 2. Requirements

These describe the destination. How to get there is open.

### 2.1 Functional

- Multiple named zones; zones can be grouped to play the same source in sync.
- At least one PCM source into the server (a FIFO or equivalent is fine to start); streaming-service integrations are out of scope for the codebase (whatever feeds PCM into the server is the owner's choice and lives outside the system boundary).
- Per-zone volume and mute, controllable from a UI and from Home Assistant.
- Endpoints self-discover the server (with a static-address fallback) and recover from server restarts, network blips, and their own crashes without human intervention.
- Fleet lifecycle: safe remote firmware updates, basic provisioning, and per-device health/sync telemetry.
- Later phase: a TV/surround path with lip-sync-grade latency on wired endpoints.

### 2.2 Non-functional targets (measured, not vibes)

| Scenario | Acceptable | Aspirational |
|---|---|---|
| Multiroom music, different rooms | < 5 ms inter-device error | < 1 ms |
| Stereo pair / same room | < 0.5 ms | < 0.2 ms |
| Surround inter-channel (later) | < 0.5 ms | < 0.2 ms |
| TV audio vs video (later) | within +/- 40 ms, audio never leading > 15 ms | within +/- 20 ms |
| Unattended stability | days | weeks |

Context for these numbers: sub-0.2 ms is what the best open-source reference (Snapcast) reports as typical; ~1 ms is where inter-speaker error starts to matter audibly in the same room; the lip-sync bounds come from ITU-R BT.1359-1 detectability thresholds (+45 ms audio-early to -125 ms audio-late) with a conservative margin.

### 2.3 Explicit non-goals

- No Dolby Atmos, TrueHD, or DTS decode (licensing makes legal open implementation impossible; set sources to PCM).
- No DRM streaming integrations in this codebase.
- No Sonos protocol compatibility.
- No mobile app for v1 (web UI and HA are enough).

---

## 3. Guiding principles

### 3.1 Hard guardrails (the only non-negotiables)

1. Clean-room rule: reference projects may be read for design understanding, but no code copied or closely paraphrased. Snapcast is GPL-3.0; this project must not become a derivative work.
2. Never enable Secure Boot, Flash Encryption, or anti-rollback eFuses on development hardware. These burns are irreversible. Production-only, and only on explicit owner instruction.
3. Timing and sync claims are backed by measurement with the harness (Section 10). "Sounds synced" is not evidence.
4. Monotonic clocks only in the audio/timestamp path. Wall-clock time is for logs. (Snapcast issue #522 documents exactly how wall clock in the timeline breaks playback when NTP steps.)
5. No em dashes in any written output.

### 3.2 Principles (strong defaults, open to argument)

- Build what defines the system; vendor what does not. The sync engine, protocol, jitter buffer, playback timing, DSP, and control plane are the project. The RTOS, TCP/IP stack, radio drivers, audio device drivers (ALSA, ESP-IDF I2S), and crypto primitives are platform, use them. The gray zone in between (JSON parsing, mDNS, WebSocket, MQTT, resamplers, codecs) is decided case by case: prefer building when it is small and instructive, vendoring when it is large, fiddly, and undifferentiated. Log each such call in the decision log.
- Fewer dependencies is a feature. Every library added should survive the question "would writing this teach us something or protect the timing path?" An audio codec: vendor if ever needed. A jitter buffer: never vendor.
- Riskiest unknown first. The single make-or-break question is whether this hardware and software can hold sub-millisecond sync. Retire that before building product features on top.
- Wired first. The Cat6 and PoE+ switch are the biggest advantages this project has. Anything with tight timing requirements goes on copper. Wi-Fi is a convenience tier with bigger buffers and looser expectations.
- Simple mechanisms that proven systems use beat clever mechanisms that might be better. Start with what Snapcast-class systems demonstrably achieve, then improve with data.

---

## 4. System architecture (high level)

```
  Sources (outside the boundary: FIFO writers, capture devices)
      |
      v
  chorus-server (container on Proxmox)
    - ingests PCM, cuts it into timestamped chunks on a single server timeline
    - manages streams, groups, zones, clients
    - streams audio to each endpoint (unicast)
    - answers time-sync requests as the master clock
    - exposes a control plane (UI, Home Assistant, metrics)
      |
      +--> wired ESP32-class smart speakers (jitter buffer, sync servo, DSP, I2S -> class-D amp)
      +--> Linux endpoints (same logic, ALSA output; also the likely home-theater brain)
      +--> (later) low-latency wired path for TV/surround
```

Shape of the system that is unlikely to change: one server as the timeline authority; timestamped chunks; clients that buffer, discipline their local playout to the server timeline, and correct drift continuously. Nearly everything else (languages, exact wire format, exact transport, buffer sizes, hardware) is a decision to make along the way, with recommendations below.

---

## 5. Key design areas: background, options, recommendations

Each area below gives enough background to reason from, the realistic options, a recommendation, and what remains open. Recommendations are starting positions, not rulings.

### 5.1 Server platform and language

Background: the server's job is soft-real-time. It must timestamp audio consistently and push chunks with low tail jitter. Garbage-collected runtimes can work if the hot path avoids the GC, but they add a failure mode the project exists to avoid; measured comparisons show GC languages missing modest tail-latency targets under memory pressure where Rust stays flat.

Options: Rust (no GC, deterministic, steeper learning curve), Go (fast to write, sub-ms GC that still needs care around the hot path), C/C++ (maximum control, memory-safety burden on a network-facing daemon), Node/Python (fine for control plane, unfit for the audio path).

Recommendation: Rust for the server and the Linux client, plain threads (an async runtime is unnecessary for tens of connections and adds opacity to the timing story). Go is the pragmatic fallback if Rust velocity becomes a problem, with audio buffers kept off the GC heap. Keep Node/Python for tooling and, if desired, UI glue.

Open: final language call; whether the control plane lives in the same process or a sidecar; threading topology details.

### 5.2 Transport and wire protocol

Background and the math that shapes it: uncompressed PCM is tiny on this LAN. 48 kHz/16-bit stereo is 1.536 Mbit/s (0.15% of gigabit); even 48 kHz/24-bit 5.1 is 6.9 Mbit/s. A dozen unicast zones round to nothing. Compression is therefore an optimization for Wi-Fi robustness or scale, not a requirement, and it adds latency and dependencies. Multicast looks attractive and is a trap on Wi-Fi (sent at low basic rates, unacknowledged, and power-save clients force AP buffering measured in hundreds of ms); on wired it mostly saves bandwidth this system does not need to save.

For the buffered music path, TCP is viable and simple: reliability for free, and a few-hundred-ms jitter buffer absorbs retransmits (Snapcast ships on TCP and reaches sub-0.2 ms sync). For a future low-latency TV path, TCP's head-of-line blocking is disqualifying; that path wants UDP with forward error correction (even simple XOR parity over small groups) because there is no time for a retransmit round trip at a ~10 ms buffer.

What the protocol needs to carry, however it is framed: a session hello/capabilities exchange; a stream format announcement; audio chunks bearing a sequence number and a presentation timestamp on the server timeline; a time-sync request/response with the four timestamps; control messages (volume, grouping, config); and client telemetry. Keeping the hot path binary and the control messages JSON is a reasonable split. One multiplexed connection per device keeps embedded firmware simple; separate connections are cleaner conceptually. Design for forward compatibility (unknown message types are skipped, not fatal) and consider cross-language conformance fixtures early, since the protocol will be implemented twice (Rust and C).

Recommendation: start with uncompressed PCM over TCP unicast, ~20 ms chunks, ~500 ms default buffer for music; design the header fields now with the TV path in mind (UDP + FEC later); one multiplexed TCP connection per device; golden test vectors shared between the Rust and C implementations from the first week.

Open: exact framing and field layout; port numbers; chunk and buffer sizes (tune with data); when and whether FLAC ever enters for Wi-Fi zones; single vs dual connection per device.

### 5.3 Clock synchronization (the heart of the project)

This is the area where understanding matters more than any specific choice, so the background here is deeper.

The problem decomposes into two loops:

1. Estimate the offset between the server clock and each client clock.
2. Discipline the client's actual DAC playout to the shared timeline, continuously, because crystals drift (typical +/- 20-50 ppm each; two devices can diverge ~6 ms/minute uncorrected).

Offset estimation, NTP-style (RFC 5905): client stamps t0 at send; server stamps t1 at receive and t2 at reply; client stamps t3 at receive. Then `rtt = (t3 - t0) - (t2 - t1)` and `offset = ((t1 - t0) + (t2 - t3)) / 2`, exact only for symmetric paths. Raw measurements are noisy (queuing), so they get filtered; the proven toolkit is: prefer the minimum-RTT sample in a sliding window (least-queued is most trustworthy), median filters for robustness, and light exponential smoothing on the result. Kalman filtering is the fancier alternative; it is probably unnecessary and can be revisited if measurements say so.

Closing the loop at the DAC is the crucial trick: ask the audio hardware how much is queued ahead of the DAC (ALSA `snd_pcm_delay()`; on a microcontroller, frames written minus frames the I2S DMA has consumed) so the servo's error signal reflects when a sample is actually audible, not when it was handed to a driver.

Correction mechanism: the standard, proven approach is single-sample insertion/deletion spread thinly (one 48 kHz sample is ~20.8 us; sprinkled at a few per second it is inaudible), driven by a small proportional-plus-integral law on the filtered error, clamped to a few hundred ppm. Continuous resampling is the smoother alternative at real complexity cost; earn it with evidence of audible artifacts first. Two tiers work well in practice: fine corrections when slightly off, and a hard resync (mute, realign, resume) when badly off.

Reference constants from the best-studied open implementation (Snapcast), useful as starting points and sanity checks, not as law: median filter windows of roughly 20/100/500 chunks; fine correction engaging around 0.1 ms of filtered error; hard resync around a few ms of sustained error; rate corrections clamped near +/- 500 ppm; time-sync exchanges about once per second with a burst on connect. Snapcast reports typical deviation below 0.2 ms with exactly this shape of design, on ordinary hardware, without PTP.

What accuracy to expect: wired Ethernet with software timestamps supports sub-millisecond playout sync comfortably (network jitter is microseconds on a quiet switched segment). Wi-Fi can reach low single-digit ms if and only if modem power save is disabled on the client; with power save on, jitter reaches tens to hundreds of ms and no servo can fix it. PTP-style hardware timestamping is overkill here, but its lesson (timestamp as close to the wire as possible) is worth stealing: stamp t1 immediately on socket read and t2 immediately before write.

Strong recommendation regardless of other choices: build a deterministic simulator for the sync engine (virtual clocks with configurable ppm skew, injectable jitter distributions) before touching hardware, so servo logic can be developed and regression-tested in CI.

Open: exact filter parameters and servo gains (tune against the simulator and the physical harness); correction mechanism refinements; how aggressively to adapt buffer sizes.

### 5.4 Endpoint platforms

Two tiers make sense, and both will likely exist in the final system:

ESP32-S3 class (microcontroller): lowest cost (~$15 board), sub-watt power, instant boot, and full control of the timing path. Constraints to design around: PSRAM is where the jitter buffer lives, but DMA and cache interactions mean the innermost I2S buffers stay in internal RAM; pin the network stack and the audio work to different cores; disable Wi-Fi modem power save or sync is hopeless; wired Ethernet via a W5500 over SPI is the easy, reliable option (an RMII PHY is faster but burns ~9 pins including a strapping pin); use the APLL clock source so audio sample rates are accurate. The ESP32-P4 is the newer part with more DSP headroom but needs a companion chip for radio; worth considering for heavier zones later.

Linux class (Pi Zero 2 W / CM5 / any mini PC): full OS, easy iteration, direct ALSA access with real `snd_pcm_delay`, real-time scheduling available, natural home for the home-theater brain and for early development before firmware exists. Costs more power and money per zone, boots slower.

Recommendation: develop the client logic on Linux first (fastest feedback loop, easiest measurement), then port the sync/protocol/DSP cores to ESP32-S3 as C components validated against shared fixtures. Deploy ESP32-S3 for distributed music speakers (wired or PoE where possible) and Linux for the theater front stage and any zone needing heavy DSP.

Open: final board selection; wired-vs-Wi-Fi per zone; whether ESP32-P4 earns a slot; whether Pi-class endpoints stay in the end state or remain development vehicles.

### 5.5 Amplification and audio hardware

Background: the standout part for a smart speaker is the TI TAS5825M (or its close sibling TAS5805M): I2S digital input, integrated DSP, up to 2x38 W at 24 V, one chip from bitstream to speaker terminals. TI gates its DSP configuration GUI behind approvals, but the community has documented bring-up over plain I2C well enough for DIY use, and if the on-chip DSP is bypassed (all DSP done on the host processor) the amp is just a transparent I2S power stage, which is also the simplest and most controllable design. Alternatives: MERUS MA12070 (efficient, up to ~2x80 W) for bigger zones; PCM5102A DAC plus any analog-input class-D board as the escape hatch if TAS sourcing or bring-up disappoints.

Practical TAS5825M bring-up notes worth keeping (verify all against the datasheet during bring-up; the register details below are community-verified starting points, not gospel): 7-bit I2C address typically 0x4C (ADR pin low); allow ~250 ms after DVDD before I2C; I2S clocks must be running before entering PLAY; a minimal init is page/book select (0x00=0x00, 0x7F=0x00), analog gain per PVDD (find the exact AGAIN register in the datasheet, do not guess), digital volume 0x4C (0x30 = 0 dB, -0.5 dB per step, 0xFF mute), state register 0x03 (0x02 Hi-Z, 0x03 PLAY), then clear faults (0x78=0x80). DIE_ID at 0x67 reads 0x95 on a live part. Monitor fault registers 0x70-0x73 and clock detect 0x39; go Hi-Z before any I2S clock change.

PoE powering: 802.3af yields ~13 W at the device (compact full-range endpoint), 802.3at ~25 W (proper bookshelf), 802.3bt 51-71 W (sub or big zone). A PoE splitter or PD module feeding the amp's DC input is the simple integration; mind the Omada switch's total power budget across the fleet.

Recommendation: ESP32-S3 + TAS5825M with the amp's DSP bypassed and all processing on the ESP32; W5500 wired or PoE via splitter; prototype on off-the-shelf breakouts and only consider a custom PCB after the design is proven and frozen.

Open: exact modules/boards; PVDD voltage per speaker class; PoE class per zone; whether any zone justifies MA12070 or multichannel amps; enclosure/driver designs (owner's department).

### 5.6 DSP

Background: everything needed is classical and well-documented. Biquad IIR filters from the RBJ Audio EQ Cookbook cover EQ, shelves, and the building blocks of crossovers; a Linkwitz-Riley 4th-order crossover (two cascaded Butterworth sections per branch) sums flat and is the standard for active two-way speakers, which is a genuinely exciting option here: a stereo TAS5825M can drive woofer on one channel and tweeter on the other, turning a chorus endpoint into a proper active loudspeaker. Add per-output delay for driver and room alignment, and a look-ahead limiter for protection. Float32 everywhere (the S3 has an FPU; fixed point is not worth the bookkeeping). Sample-rate conversion beyond drift correction (for example 44.1 k sources into a 48 k pipeline) is the one place where vendoring a quality resampler is defensible; decide if and when the need actually arises.

Recommendation: implement a small DSP library once as a reference (biquads, LR4 crossover, delay, limiter) with test fixtures, and port it to the firmware validated against the same fixtures. Run all DSP on the endpoint processor, not the amp chip.

Open: filter set scope for v1; per-zone DSP configuration format; whether room correction ever enters; SRC strategy for 44.1 k sources.

### 5.7 The TV and surround path (later phase, design-aware now)

Background: this is a different problem from multiroom music. Music tolerates half-second buffers; TV audio must land within lip-sync limits (practical target: within ~40 ms of video, audio never leading by more than ~15 ms). That budget decomposes roughly as: capture 2-10 ms, packetize <1 ms, wired network <1 ms, jitter buffer ~10 ms, DSP+DAC 2-10 ms, total ~15-35 ms, achievable only on wired endpoints with small buffers and FEC instead of retransmission. Capture options: HDMI eARC extractor into an S/PDIF or USB capture device on the server (TV set to PCM). Note S/PDIF cannot carry 6-channel LPCM, so true 5.1 capture eventually means an HDMI-side capture device; a stereo TV path is the sensible first version. TVs' own video processing delay is compensated with a signed global A/V trim, calibrated with a flash+beep clip and a high-frame-rate phone camera.

Recommendation: defer implementation until the music path is solid, but design the chunk header now so the same protocol family covers a 5 ms low-latency mode; plan on UDP + simple XOR parity FEC, wired only, stereo first.

Open: everything about implementation timing; capture hardware choice; whether the theater front stage is a chorus endpoint or the owner's existing AVR fed by one.

### 5.8 Control plane, discovery, and integrations

Background: the durable pattern is a server-authoritative state model (streams, groups, zones, clients, volumes) with a JSON message catalog: state snapshot, mutations (volume, mute, grouping, per-zone latency trim, DSP config), events fanned out to subscribers, and client telemetry flowing back. Discovery wants mDNS (`_chorus._tcp` or similar) with a static-address fallback; note that mDNS across VLANs needs a reflector or shared L2, and that a containerized server should use host networking for multicast to work at all. Home Assistant integration is cheapest via MQTT discovery (retained config topics make zones appear as entities with no custom HA code). A web UI can stay minimal (single page, no framework) for a long time.

Recommendation: JSON control plane with a small, versioned message catalog; WebSocket for UIs; mDNS with fallback; MQTT/HA in a later phase. Whether WebSocket/mDNS/MQTT are hand-written (each is small and instructive) or minimally vendored is a per-item call for the decision log; hand-writing them fits the project's spirit and none is large.

Open: message catalog details; UI scope; hand-write vs vendor for each small protocol; auth story if the control plane ever leaves the trusted VLAN.

### 5.9 Fleet: OTA, provisioning, observability

Background: OTA is the feature that keeps wall-mounted speakers from becoming a maintenance nightmare, and it must exist before the fleet grows past two devices. ESP-IDF's A/B partition scheme with rollback (new image to inactive slot, boot, self-test, mark valid, else automatic revert) is the right shape; demonstrating a deliberate bad-image rollback is the test that matters. Linux endpoints can start as a binary plus systemd unit deployed with the owner's normal CI muscle. Provisioning can be a serial console command writing NVS during development, with fancier onboarding only if ever needed. Observability: per-client telemetry (buffer fill, filtered sync error, correction rate, resync/underrun counters, RSSI, heap, firmware version) aggregated at the server and exposed for Prometheus/Grafana; this is where the owner's DevOps instincts pay off directly.

Recommendation: build OTA with rollback early (before device #3 exists); keep provisioning primitive until it hurts; emit metrics from day one because the sync work needs them anyway.

Open: update distribution details; image signing timeline (production only, per guardrail 2); dashboard scope.

---

## 6. Reference knowledge: numbers worth keeping at hand

Bandwidth (raw PCM): 48k/16/stereo = 1.536 Mbit/s; 48k/24/stereo = 2.304; 48k/24/5.1 = 6.912; all under 0.7% of gigabit.

Sync expectations by transport: wired + software timestamps supports ~0.1-0.2 ms typical playout sync (Snapcast-class); Wi-Fi with power save off, low single-digit ms with excursions; Wi-Fi with power save on, unusable for sync.

Crystal drift: +/- 20-50 ppm per device; ~100 ppm relative worst case = ~6 ms/minute uncorrected.

Sample durations: one frame at 48 kHz = 20.83 us; 1 ms of sound travels 34.3 cm (per-channel delay alignment math).

Lip sync (ITU-R BT.1359-1): detectability +45 ms (early) to -125 ms (late); practical design target within +/- 40 ms, never leading by more than 15 ms.

Docker for real-time audio: host network mode (multicast/mDNS and latency), `cap_add: SYS_NICE`, `ulimits: rtprio` and unlimited `memlock`, request SCHED_FIFO around priority 50 (never the max), verify with cyclictest; CLOCK_MONOTONIC/RAW pass through the container boundary unchanged on a native Linux host.

ESP32-S3 pitfalls list: octal-PSRAM modules reserve GPIO 35-37; strapping pins 0/3/45/46, USB 19/20, UART0 43/44 are off-limits or risky; APLL for audio-accurate clocks; mclk multiple must be a multiple of 3 for 24-bit slots; DMA from internal RAM, jitter buffer in PSRAM; `esp_wifi_set_ps(WIFI_PS_NONE)` is mandatory on Wi-Fi.

---

## 7. Suggested repository shape

Not prescriptive; adjust as the code wants. The properties that matter: the sync, protocol, and DSP cores live as pure, heavily tested libraries with no I/O; the firmware mirrors them as C components validated against shared fixture files; tools and measurement scripts live beside the code; decisions get logged.

```
chorus/
  CLAUDE.md                # working agreement (Appendix A)
  BRIEF.md                 # this document
  docs/decisions/          # decision log
  docs/measurements/       # saved harness reports
  crates/                  # e.g. protocol / sync / dsp / server / client-linux
  firmware/esp32s3/        # ESP-IDF project with mirrored components
  fixtures/                # protocol vectors, dsp vectors, sync traces
  tools/measure/           # harness scripts
  deploy/                  # Dockerfile, compose, systemd units
```

---

## 8. Suggested roadmap

Ordered to retire risk, with "success looks like" instead of rigid gates. Reorder with reason if the work argues for it.

1. Foundation. Repo, CI, the protocol core with cross-language fixtures, and the sync simulator. Success: servo logic converging in simulation under realistic jitter and skew models before any hardware exists.
2. First sound. Minimal server (FIFO in, chunks out) and a minimal Linux client playing timestamped PCM via ALSA, no servo yet. Success: clean audio, plausible reported DAC delay, stable buffer.
3. Measurement rig. Dual-input capture + cross-correlation tooling, and a free-run drift baseline between two clients. Success: the rig produces trustworthy numbers; drift matches crystal expectations.
4. Synchronization. Full sync engine on two wired Linux clients, tuned against the rig. Success: sustained sub-millisecond error, zero hard resyncs at steady state. This is the milestone the whole project hinges on; budget the most time here.
5. Embedded bring-up. ESP32-S3 firmware: I2S out, TAS5825M alive, network playback, sync core ported and measured against a Linux client. Success: the embedded endpoint syncs comparably to Linux and survives disconnect/reconnect abuse unattended.
6. Product hardening. Groups, volume, control plane, minimal UI, discovery, reconnect storms, multi-day soak.
7. Wi-Fi tier. One wireless endpoint characterized honestly (with and without power save, documented), held to the looser targets.
8. DSP. The filter library, fixture-validated on both platforms; the active two-way speaker demo.
9. TV/surround path. Wired, small-buffer, FEC, stereo first, A/V calibrated.
10. Fleet. OTA with demonstrated rollback, MQTT/HA, dashboards, provisioning polish, whole-home rollout and a long soak.

---

## 9. Risks and honest failure modes

| Risk | Early signal | Response |
|---|---|---|
| Cannot hold sub-ms sync on real hardware | phase 4 measurements | stop feature work; audit clock sources, DAC-delay accounting, timestamp placement; nothing else matters until this holds |
| Wi-Fi jitter defeats the servo | phase 7 histograms | bigger buffers or wired-only for that zone; never chase Wi-Fi with servo aggression |
| TAS5825M sourcing/bring-up trouble | phase 5 | TAS5805M sibling, or DAC + analog class-D escape hatch |
| TV path misses lip sync | phase 9 latency decomposition | shrink buffers on a measured-clean wired link; worst case, theater front stage stays on the existing AVR fed by one wired endpoint |
| Scope creep (codecs, streaming services, apps) | any time | out of scope per Section 2.3 until the owner says otherwise |
| Project fatigue | honest self-assessment | the phased order front-loads the intellectually rewarding part (sync) and defers grind (fleet polish); keep phases small and demonstrable |

---

## 10. Measurement methodology (summary; build early)

- Primary: two endpoints' line outputs into the L/R of one USB audio interface; play a chirp; cross-correlate sliding windows; report median/p95/max lag. ~10 us resolution at 96 kHz capture. Save every report to docs/measurements/.
- Digital cross-check: a marker pattern in the stream toggles a GPIO on each endpoint at the moment of I2S write; logic analyzer measures the edge delta, isolating the servo from analog path differences.
- Latency: injected click timestamped at ingest, detected at the speaker; for A/V, flash+beep clip filmed at 240 fps.
- Jitter: clients log inter-arrival and RTT histograms continuously; compare wired vs Wi-Fi vs Wi-Fi-with-power-save once, for the record.

---

## 11. Reference material (study, not adopt)

- Snapcast (GPL-3.0, clean-room rule): client stream/sync code, time provider, binary protocol doc, and issue #522 (the monotonic clock lesson). https://github.com/badaix/snapcast
- ESP32 snapclient: task split, PSRAM buffering, single-sample correction on a microcontroller. https://github.com/CarlosDerSeher/snapclient
- squeezelite: SlimProto timing, DAC-delay accounting. https://github.com/ralph-irving/squeezelite
- shairport-sync + NQPTP: sync engine and PTP-ish timing in software. https://github.com/mikebrady/shairport-sync
- Roc Toolkit: FEC and adaptive latency tuner design. https://github.com/roc-streaming/roc-toolkit
- RBJ Audio EQ Cookbook: https://webaudio.github.io/Audio-EQ-Cookbook/audio-eq-cookbook.html
- RFC 5905 (NTP), RFC 3550 (RTP/RTCP), RFC 6455 (WebSocket), IEEE 1588 (PTP concepts), ITU-R BT.1359-1 (lip sync).
- TAS5825M datasheet (register truth): https://www.ti.com/lit/ds/symlink/tas5825m.pdf
- ESP-IDF programming guides (I2S, Ethernet, OTA, mDNS): https://docs.espressif.com/projects/esp-idf/
- ALSA PCM API: https://www.alsa-project.org/alsa-doc/alsa-lib/group___p_c_m.html
- Docker real-time reference: https://github.com/2b-t/linux-realtime

Two earlier research reports (a landscape survey and a detailed from-scratch blueprint) exist in the owner's records with deeper source-level detail; consult them when a section here feels thin.

---

## 12. Decisions to make (the intentional ambiguity, tracked)

Log each of these in docs/decisions/ when made, with a sentence of reasoning. Rough order of arrival:

1. Server/client language (recommendation: Rust, std threads).
2. Repo and workspace layout; CI shape.
3. Wire protocol framing, field layout, ports, connection model.
4. Chunk size and default buffer target (starting points: 20 ms / 500 ms).
5. Sync filter parameters and servo gains (start from the reference constants in 5.3, tune with the rig).
6. Hand-write vs vendor: JSON handling, mDNS, WebSocket, MQTT (case-by-case per 3.2).
7. Dev bench hardware: exact ESP32-S3 board, TAS5825M module, wired interface (W5500 vs RMII), measurement interface.
8. Pin map for the bench build (respect the pitfalls list in Section 6).
9. Zone/group/config data model and file format.
10. Volume taper and mute behavior.
11. DSP v1 scope and config schema.
12. Wi-Fi tier policy: which zones, what buffer, what targets.
13. TV path capture hardware and phase timing.
14. OTA distribution details and (much later, production only) signing.
15. HA/MQTT entity model.
16. Whether any of the recommendations above deserve overturning as evidence arrives.

---

## Appendix A: CLAUDE.md starter (copy to repo root)

```markdown
# chorus

From-scratch multiroom + surround audio: containerized server, custom sync protocol,
embedded speaker firmware. BRIEF.md is the guiding document: goals, constraints,
recommendations, and open decisions. It recommends; it does not dictate.

## Working agreement

1. Propose before committing to anything expensive to reverse. Cheap decisions:
   make them, note them in docs/decisions/.
2. Hard guardrails (BRIEF.md 3.1): clean-room vs GPL references; no eFuse burns on
   dev hardware; measurement-backed timing claims; monotonic clocks in the audio
   path; no em dashes anywhere.
3. Prefer building over vendoring when small and instructive; vendor the large and
   undifferentiated; log gray-zone calls.
4. The sync engine is the project. Simulator first, hardware second, measurement
   always. Reports go in docs/measurements/.
5. Protocol, sync, and DSP cores are pure libraries with shared fixtures so the
   Rust and C implementations cannot drift apart.
6. Items marked "verify against the datasheet" are starting points, not truth.
7. Finish a phase's "success looks like" before moving on, or say why not.
```
