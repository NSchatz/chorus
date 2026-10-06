# Chorus: From-Scratch Multiroom and Surround Audio System
## Project Brief and Design Guide v2 (hand-off document for Claude Code)

Owner: the owner (identity scrub, decided 2026-09-29 by the owner, K27)
Date: 2026-08-21
Status: Guiding document, not a rigid specification. It defines goals, requirements, constraints, and reference knowledge, and it makes recommendations with rationale. Implementation decisions are made during the project, in collaboration with the owner, and recorded as they happen.
Codename: "chorus" (placeholder, rename freely).
Amended: 2026-09-30 (chorus goal 4) with the owner's decisions of 2026-09-29 (K-numbers in [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) §1); each changed passage carries its reversal number R<n>, the words "decided 2026-09-29 by the owner" and its decision IDs. The log is at the end of this document ("Amendments").

---

## 0. How to use this document (instructions to Claude Code)

1. Read the whole document before writing code. Treat it as the project's shared context, not a checklist to execute blindly.
2. Most of this document is recommendation, not mandate. When you reach a decision point, weigh the options, propose an approach with reasoning, and let the owner confirm before committing to anything expensive to reverse. Cheap-to-reverse decisions can just be made and noted. This document itself is kept current by the program (verified corrections and the owner's decisions are written into it, dated); that editing needs no separate confirmation, the confirm-before-expensive rule above stays, and the Section 3.1 guardrails are never relaxed or removed by the program, only tightened (R16, decided 2026-09-29 by the owner, K47; limit I2).
3. Section 12 lists the decisions that need to be made. Keep a lightweight decision log (one markdown file per significant decision in `docs/decisions/`) so the reasoning survives.
4. A small number of items are hard guardrails (Section 3.1) for safety, legal, and project-integrity reasons. Those are not negotiable. Everything else is.
5. Recommendations below are labeled as such. If measurement or experience during the build contradicts a recommendation, say so and propose the change; the recommendation loses, and the program corrects this document in the same change (R16, decided 2026-09-29 by the owner, K47).
6. Style note from the owner: no em dashes anywhere (code comments, docs, commits). Use commas, periods, parentheses, or hyphens. Direct, peer-level tone.

---

## 1. Vision

Replace the Sonos ecosystem with a system the owner builds and understands end to end, at Sonos parity across software, hardware and Home Assistant: "Sonos parity. Software, hardware, home assistant integration, all of the above. FIRST CLASS." (the owner). chorus is its own complete system: music arrives as inputs (UPnP AV casting to every room and group, official Spotify Soloist receivers, line-ins, the TV); chorus owns rooms, groups (which rooms play together), sync, sound, the app and the Home Assistant integration; no Music Assistant and no Sendspin, in any form (R6, decided 2026-09-29 by the owner, K13, K54, K56, K57, K64, K66).

- A centralized audio server running in a container (Docker Compose, host networking) on the owner's bare-metal Debian 13 homelab host (a Dell R730xd); there is no Proxmox (R5, decided 2026-09-29 by the owner, K24, K34).
- Self-developed software on every speaker endpoint: embedded firmware on microcontroller-class smart speakers, and a lightweight client on Linux-class endpoints.
- A custom network protocol with tightly synchronized multiroom playback.
- Eventually, a low-latency path for TV and surround audio within lip-sync limits.
- The usual product surround: discovery, control, grouping, volume, OTA updates, Home Assistant integration, observability.

The point is not just the product. It is owning the whole stack: the sync algorithm, the protocol, the playback engine, the DSP, the firmware. Existing projects (Snapcast, squeezelite, shairport-sync, Roc) are studied as prior art, not adopted as dependencies.

Environment facts to assume:

- Gigabit Cat6 to most rooms through conduit; TP-Link Omada PoE+ switch; UniFi Wi-Fi; VLANs.
- A bare-metal Debian 13 host running Docker Compose stacks (no Proxmox), deployed through PRs in the owner's homelab repo that the owner merges and applies; Home Assistant and an MQTT broker already running (R5, decided 2026-09-29 by the owner, K24, K28, K34).
- Endpoints will be mostly wired Ethernet or PoE. Wi-Fi endpoints are acceptable for casual music zones only.
- Owner's background: strong Node.js/Python/AWS/Terraform/Docker/CI-CD, ESP32/ESPHome experience, comfortable learning C and Rust, DIY speaker building and woodworking capability.

---

## 2. Requirements

These describe the destination. How to get there is open.

### 2.1 Functional

- Multiple named zones (rooms); chorus groups rooms to play the same source in sync, in saved named groups and live ad-hoc groups (R6, decided 2026-09-29 by the owner, K54).
- Content arrives only as inputs: every room and group is a UPnP AV (DLNA) renderer; official Spotify Soloist receivers run one per room and group as separate processes; line-ins and the TV path; a FIFO for development. chorus holds no content of its own (no radio directory, library or podcasts) and uses no Music Assistant or Sendspin (R6, decided 2026-09-29 by the owner, K56, K57, K64, K66).
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
- No DRM streaming integrations in this codebase: official receivers (Spotify Soloist, per the approved casting proposals) run unmodified as separate processes feeding a chorus sink, and no DRM or service-authentication code enters chorus (R7, decided 2026-09-29 by the owner, K60, K66).
- No reverse-engineered receivers (librespot Spotify Connect, AirPlay 2 receivers, Google Cast) (R7, decided 2026-09-29 by the owner, K60).
- No Sonos protocol compatibility.
- No portable or battery speakers, and no Bluetooth input on endpoints (R7, decided 2026-09-29 by the owner, K53).
- No mobile app for v1 (web UI and HA are enough); the web UI grows into an app-grade installable PWA, with no app stores (R7, decided 2026-09-29 by the owner, K16).

---

## 3. Guiding principles

### 3.1 Hard guardrails (the only non-negotiables)

1. Clean-room rule: no code copied or closely paraphrased from reference projects. Snapcast is GPL-3.0; this project must not become a derivative work. Tightened (R17, decided 2026-09-29 by the owner, K33, K39): of GPL projects only the docs, issues, papers and protocol specs are read, never their source files; the design files of reciprocally licensed hardware (CERN-OHL-S, GPL) are never opened; permissive (MIT, Apache-2.0, BSD) source may be read and cited. `docs/clean-room.md` lists the projects and the record kept.
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
  Inputs (R6): UPnP AV casts, Soloist receivers, line-ins, the TV, FIFOs
      |
      v
  chorus-server (container, host networking, bare-metal Debian 13 host; R5)
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

ESP32-S3 class (microcontroller): lowest cost (~$15 board), sub-watt power, instant boot, and full control of the timing path. Constraints to design around: PSRAM is where the jitter buffer lives, but DMA and cache interactions mean the innermost I2S buffers stay in internal RAM; pin the network stack and the audio work to different cores; disable Wi-Fi modem power save or sync is hopeless; wired Ethernet via a W5500 over SPI is the easy, reliable option (an RMII PHY is faster but burns ~9 pins including a strapping pin); use the APLL clock source so audio sample rates are accurate. Corrected (R8, decided 2026-09-29 by the owner, K25, K37): the ESP32-S3 has no APLL and no Ethernet MAC, so neither "use the APLL" nor RMII applies to it. Its I2S clock sources are PLL_F160M (default), PLL_D2, XTAL and an external MCLK input (https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/clk_tree.html, read 2026-09-30), and its wired Ethernet is external SPI modules only (https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/network/esp_eth.html, read 2026-09-30). The platform and link are proposal P1 as approved at Checkpoint K (2026-09-30): ESP32-S3 everywhere on ESP-IDF v6.1.x, a W5500 over SPI with its interrupt line, PoE+ through a bought 802.3at splitter, native Wi-Fi for the Wi-Fi tier (`docs/proposals/P1-embedded-platform.md`). The ESP32-P4 is the newer part with more DSP headroom but needs a companion chip for radio; worth considering for heavier zones later.

Linux class (Pi Zero 2 W / CM5 / any mini PC): full OS, easy iteration, direct ALSA access with real `snd_pcm_delay`, real-time scheduling available, natural home for the home-theater brain and for early development before firmware exists. Costs more power and money per zone, boots slower.

Recommendation: develop the client logic on Linux first (fastest feedback loop, easiest measurement), then port the sync/protocol/DSP cores to ESP32-S3 as C components validated against shared fixtures. Deploy ESP32-S3 for distributed music speakers (wired or PoE where possible) and Linux for the theater front stage and any zone needing heavy DSP.

Open: final board selection; wired-vs-Wi-Fi per zone; whether ESP32-P4 earns a slot; whether Pi-class endpoints stay in the end state or remain development vehicles.

### 5.5 Amplification and audio hardware

Background: the standout part for a smart speaker is the TI TAS5825M (or its close sibling TAS5805M): I2S digital input, integrated DSP, up to 2x38 W at 24 V, one chip from bitstream to speaker terminals. TI gates its DSP configuration GUI behind approvals, but the community has documented bring-up over plain I2C well enough for DIY use, and if the on-chip DSP is bypassed (all DSP done on the host processor) the amp is just a transparent I2S power stage, which is also the simplest and most controllable design. Alternatives: MERUS MA12070 (efficient, up to ~2x80 W) for bigger zones; PCM5102A DAC plus any analog-input class-D board as the escape hatch if TAS sourcing or bring-up disappoints.

Corrected (R9, decided 2026-09-29 by the owner, K47): the TAS5805M's 7-bit I2C addresses are 0x2C-0x2F (ADR resistor to DVDD); 0x4C-0x4F (ADR resistor to GND) is the TAS5825M. "2x38 W at 24 V" is TI's 10% THD+N instantaneous rating; the TAS5825M gives 2x30 W continuous at 1% THD+N (8 ohm, 24 V). The TAS5805M is 2x23 W at 21 V (8 ohm, 1% THD+N) and accepts 32-96 kHz only, so it is not a power-equivalent sibling. Sources: TAS5825M datasheet SLASEH7H section 9.5.2 and Table 9-5, and TAS5805M datasheet SLASEH5D section 7.5.2 and Table 7-5 (https://www.ti.com/lit/ds/symlink/tas5825m.pdf and https://www.ti.com/lit/ds/symlink/tas5805m.pdf, read 2026-09-29); the power and sample-rate lines re-read on https://www.ti.com/product/TAS5825M and https://www.ti.com/product/TAS5805M, 2026-09-30.

Practical TAS5825M bring-up notes worth keeping (verify all against the datasheet during bring-up; the register details below are community-verified starting points, not gospel): 7-bit I2C address typically 0x4C (ADR pin low); allow ~250 ms after DVDD before I2C; I2S clocks must be running before entering PLAY; a minimal init is page/book select (0x00=0x00, 0x7F=0x00), analog gain per PVDD (find the exact AGAIN register in the datasheet, do not guess), digital volume 0x4C (0x30 = 0 dB, -0.5 dB per step, 0xFF mute), state register 0x03 (0x02 Hi-Z, 0x03 PLAY), then clear faults (0x78=0x80). DIE_ID at 0x67 reads 0x95 on a live part. Monitor fault registers 0x70-0x73 and clock detect 0x39; go Hi-Z before any I2S clock change.

Verified (chorus goal 9, 2026-09-30, K47; the reading is `docs/research/tas5825m-register-map.md`, the decision `docs/decisions/0064-the-tas5825m-register-map-and-bring-up.md`): against TI's datasheet SLASEH7H revision H, the notes above hold for the address table (p. 39), book and page select (p. 41), AGAIN at 0x54 with 0x1F the lowest setting, -15.5 dB (p. 63), DIG_VOL 0x4C (p. 57), DEVICE_CTRL2 0x03 (p. 48), FAULT_CLEAR 0x78 = 0x80 (p. 82) and DIE_ID 0x67 = 0x95 (p. 72). Three corrections: the datasheet's own startup wait is at least 5 ms after PDN goes high before the clocks, and at least 5 ms in Hi-Z with the DSP enabled before Play (p. 42), where "~250 ms after DVDD" is not a datasheet figure; the fault registers are 0x70-0x72, and 0x73 holds warnings (p. 79); and in I2S format the part accepts only 32 or 64 bit clocks per frame (pp. 7, 29), so a 24-bit sample travels in a 32-bit slot. The DSP is not bypassed by any register: the part runs its ROM process flow with the digital volume at 0 dB.

PoE powering: 802.3af yields ~13 W at the device (compact full-range endpoint), 802.3at ~25 W (proper bookshelf), 802.3bt 51-71 W (sub or big zone). A PoE splitter or PD module feeding the amp's DC input is the simple integration; mind the Omada switch's total power budget across the fleet.

Recommendation: ESP32-S3 + TAS5825M with the amp's DSP bypassed and all processing on the ESP32; W5500 wired or PoE via splitter; prototype on off-the-shelf breakouts and only consider a custom PCB after the design is proven and frozen.

Open: exact modules/boards; PVDD voltage per speaker class; PoE class per zone; whether any zone justifies MA12070 or multichannel amps; enclosure/driver designs (owner's department). Enclosure material per class is proposal P12 (`docs/proposals/P12-enclosures.md`, PROPOSED 2026-10-06, K88).

### 5.6 DSP

Background: everything needed is classical and well-documented. Biquad IIR filters from the RBJ Audio EQ Cookbook cover EQ, shelves, and the building blocks of crossovers; a Linkwitz-Riley 4th-order crossover (two cascaded Butterworth sections per branch) sums flat and is the standard for active two-way speakers, which is a genuinely exciting option here: a stereo TAS5825M can drive woofer on one channel and tweeter on the other, turning a chorus endpoint into a proper active loudspeaker. Add per-output delay for driver and room alignment, and a look-ahead limiter for protection. Float32 everywhere (the S3 has an FPU; fixed point is not worth the bookkeeping). Sample-rate conversion beyond drift correction (for example 44.1 k sources into a 48 k pipeline) is the one place where vendoring a quality resampler is defensible; decide if and when the need actually arises.

Recommendation: implement a small DSP library once as a reference (biquads, LR4 crossover, delay, limiter) with test fixtures, and port it to the firmware validated against the same fixtures. Run all DSP on the endpoint processor, not the amp chip.

Open: filter set scope for v1; per-zone DSP configuration format; whether room correction ever enters (settled 2026-10-05, K31, K87: it entered; the server fits a room's `room_eq` filters from a recording of a sweep, and the app records the sweep with a phone's microphone, `docs/room-correction.md`; no real room or phone has been measured with it yet); SRC strategy for 44.1 k sources.

### 5.7 The TV and surround path (later phase, design-aware now)

Background: this is a different problem from multiroom music. Music tolerates half-second buffers; TV audio must land within lip-sync limits (practical target: within ~40 ms of video, audio never leading by more than ~15 ms). That budget decomposes roughly as: capture 2-10 ms, packetize <1 ms, wired network <1 ms, jitter buffer ~10 ms, DSP+DAC 2-10 ms, total ~15-35 ms, achievable only on wired endpoints with small buffers and FEC instead of retransmission. Capture options: HDMI eARC extractor into an S/PDIF or USB capture device on the server (TV set to PCM). Note S/PDIF cannot carry 6-channel LPCM, so true 5.1 capture eventually means an HDMI-side capture device; a stereo TV path is the sensible first version. TVs' own video processing delay is compensated with a signed global A/V trim, calibrated with a flash+beep clip and a high-frame-rate phone camera.

Recommendation: defer implementation until the music path is solid, but design the chunk header now so the same protocol family covers a 5 ms low-latency mode; plan on UDP + simple XOR parity FEC, wired only, stereo first.

Open: everything about implementation timing; capture hardware choice; whether the theater front stage is a chorus endpoint or the owner's existing AVR fed by one.

### 5.8 Control plane, discovery, and integrations

Background: the durable pattern is a server-authoritative state model (streams, groups, zones, clients, volumes) with a JSON message catalog: state snapshot, mutations (volume, mute, grouping, per-zone latency trim, DSP config), events fanned out to subscribers, and client telemetry flowing back. Discovery wants mDNS (`_chorus._tcp` or similar) with a static-address fallback; note that mDNS across VLANs needs a reflector or shared L2, and that a containerized server should use host networking for multicast to work at all. Home Assistant integration is cheapest via MQTT discovery (retained config topics make zones appear as entities with no custom HA code). A web UI can stay minimal (single page, no framework) for a long time.

Recommendation: JSON control plane with a small, versioned message catalog; WebSocket for UIs; mDNS with fallback; MQTT/HA in a later phase. Superseded (R10, decided 2026-09-29 by the owner, K43, K46, K61): UIs use HTTP plus Server-Sent Events, as built (`docs/decisions/0026-*`); mDNS is hand-written (`docs/decisions/0025-*`); the app is an installable PWA on the stack of proposal P5 as approved (Lit 3 and esbuild), not "single page, no framework"; Home Assistant gets chorus's own integration (a media_player per room and saved group, sound controls, diagnostics), with MQTT only for the opt-in extras of proposal P10. As built (2026-10-05, K16, K40, K43, K86; `docs/app.md`, records `docs/decisions/0181-*` to `0191-*`): the app is Lit 3 elements bundled by esbuild, installed by pnpm with install scripts off and exact pins, unit-tested with node's own runner over happy-dom and live against a real server with no browser; its committed build output is compiled into chorus-server by a std-only build script and served under `/app/`, a permanent path, beside the control page at `/`; it installs behind the household login with a hand-written service worker that never answers or caches `/api/`, says "Signed out" when the login has lapsed, and lays itself out for a phone, a desktop and a wall tablet's kiosk (`/app/?kiosk`); the gate holds exactly one browser smoke test (Playwright's headless Chromium through a fake login, `make web-smoke`). Rooms, groups, inputs and now playing are built (part 1). Part 2 as built (2026-10-05, K31, K77, K80, K87, K91, K92, K93; `docs/app.md`, "Part 2 as built, and what was never run on real hardware"; records `docs/decisions/0197-*` and `0207-*`): every further screen has an address of its own in the fragment, and they are a room's sound, its volume limit and quiet hours, its theater settings (A/V trim, TV autoplay, TV upmix, bass management) and its room correction, measured with the microphone of the phone the screen is open on and fitted by the server; the house's autoplay rules; alarms with all four sources, stored sources and sleep timers; and the speakers (new, name, room, forget, a refused changed key), their firmware with "update available" and an install that only an explicit press starts, and a walk-through for a compact Wi-Fi speaker that never asks for the network's passphrase. The one browser smoke test walks those screens, sets a sound setting on a real server and records the server's sweep with the browser's fake audio device. Corrected by this work: this paragraph's earlier "sound, alarms, room correction and setup screens are not yet" no longer holds, and section 5.6's open question "whether room correction ever enters" is settled, it entered. Unverified, because no agent has a phone, a speaker or the bench: install and login on real phones (the owner's phone check, `docs/app.md`, "Phone check"), and for part 2 any room measured with a real microphone, whether a correction sounds better, the Wi-Fi walk-through against a real speaker and an install from the app that writes a real board (the owner's, `docs/app.md`, "Phone check, part 2"). Nothing about the app is a measurement. Whether WebSocket/mDNS/MQTT are hand-written (each is small and instructive) or minimally vendored is a per-item call for the decision log; hand-writing them fits the project's spirit and none is large.

Open: message catalog details; UI scope; hand-write vs vendor for each small protocol; auth story if the control plane ever leaves the trusted VLAN.

### 5.9 Fleet: OTA, provisioning, observability

Background: OTA is the feature that keeps wall-mounted speakers from becoming a maintenance nightmare, and it must exist before the fleet grows past two devices. ESP-IDF's A/B partition scheme with rollback (new image to inactive slot, boot, self-test, mark valid, else automatic revert) is the right shape; demonstrating a deliberate bad-image rollback is the test that matters. Linux endpoints can start as a binary plus systemd unit deployed with the owner's normal CI muscle. Provisioning can be a serial console command writing NVS during development, with fancier onboarding only if ever needed. Superseded (R11, decided 2026-09-29 by the owner, K92, K93): a chorus speaker on the audio network is adopted automatically and named afterwards in the app, with its session key pinned at adoption (trust on first use; a changed key is refused and surfaced in the app and Home Assistant); OTA images are staged by the server and install only on the owner's explicit install action, A/B with rollback. Observability: per-client telemetry (buffer fill, filtered sync error, correction rate, resync/underrun counters, RSSI, heap, firmware version) aggregated at the server and exposed for Prometheus/Grafana; this is where the owner's DevOps instincts pay off directly.

Recommendation: build OTA with rollback early (before device #3 exists); keep provisioning primitive until it hurts; emit metrics from day one because the sync work needs them anyway.

Open: update distribution details; image signing timeline (production only, per guardrail 2); dashboard scope.

---

## 6. Reference knowledge: numbers worth keeping at hand

Bandwidth (raw PCM): 48k/16/stereo = 1.536 Mbit/s; 48k/24/stereo = 2.304; 48k/24/5.1 = 6.912; all under 0.7% of gigabit.

Sync expectations by transport: wired + software timestamps supports ~0.1-0.2 ms typical playout sync (Snapcast-class); Wi-Fi with power save off, low single-digit ms with excursions; Wi-Fi with power save on, unusable for sync.

Crystal drift: +/- 20-50 ppm per device; ~100 ppm relative worst case = ~6 ms/minute uncorrected.

Sample durations: one frame at 48 kHz = 20.83 us; 1 ms of sound travels 34.3 cm (per-channel delay alignment math).

Lip sync (ITU-R BT.1359-1): detectability +45 ms (early) to -125 ms (late); practical design target within +/- 40 ms, never leading by more than 15 ms.

Docker for real-time audio (on the bare-metal Debian 13 host with Docker Compose, where chorus-server joins the homelab's host-network exception list; R5, decided 2026-09-29 by the owner, K24, K34): host network mode (multicast/mDNS and latency), `cap_add: SYS_NICE`, `ulimits: rtprio` and unlimited `memlock`, request SCHED_FIFO around priority 50 (never the max), verify with cyclictest; CLOCK_MONOTONIC/RAW pass through the container boundary unchanged on a native Linux host.

ESP32-S3 pitfalls list: octal-PSRAM modules reserve GPIO 35-37; strapping pins 0/3/45/46, USB 19/20, UART0 43/44 are off-limits or risky; APLL for audio-accurate clocks (the S3 has none, see 5.4, R8); mclk multiple must be a multiple of 3 for 24-bit slots; DMA from internal RAM, jitter buffer in PSRAM; `esp_wifi_set_ps(WIFI_PS_NONE)` is mandatory on Wi-Fi.

---

## 7. Suggested repository shape

Not prescriptive; adjust as the code wants. The properties that matter: the sync, protocol, and DSP cores live as pure, heavily tested libraries with no I/O; the firmware mirrors them as C components validated against shared fixture files; tools and measurement scripts live beside the code; decisions get logged.

```
chorus/
  CLAUDE.md                # working agreement (Appendix A removed, R17)
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

### 8.1 The program's phases (added 2026-09-30, chorus goal 4; K12, K13, K32, K47, decided 2026-09-29 by the owner)

The /goal program in [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) was the plan of record until 2026-10-04; its remaining goals are now tasks from the owner's agent harness. Its phases run the ten above in order and then the Sonos-parity work, foundations first (K32); one deliberate reorder puts rooms and groups (phase 11 below) before Fleet, because DSP and the TV path need them (I19). Each phase's acceptance is the "success looks like" line above (phases 1-10) or the line below, refined by that goal's done-when lines; real-hardware steps end as ready-to-run packets for the owner, never as success criteria (K7, K50).

| Program phase | Goals | Serves | Success looks like |
|---|---|---|---|
| Start-up, audit and proposals | 1 | all | a cold audit of every phase and the research proposals, decided at Checkpoint K |
| Fixes and the gate | 2 | 1 | the audit's fixes; `make gate` is the one merge check |
| Conventions, licence and identity | 3 | 1 | chorus's own conventions, each enforced by a check; MIT OR Apache-2.0; the identity scan |
| Reversals, loose ends, first release, deploy PR | 4 | 1, 6 | this amendment; release v0.1.0; the homelab deploy PR opened for the owner |
| Protocol v2 | 5, 6 | 1, 5 | capabilities, channel maps, PCM/FLAC/Opus, encrypted sessions with trust-on-first-use adoption, the metadata, controller, visualizer and source roles; Rust and C agree on shared fixtures |
| Sound, rig and sync in software; bench packets | 7 | 2, 3, 4 | the sync engine held to its targets in simulation at 8 rooms; bench packets for phases 2-4 |
| The embedded platform | 8, 9 | 5 | ESP32-S3 on ESP-IDF v6.1 with the W5500 link, the I2S playout path, the amp and controls; a flashing packet behind the owner-at-bench guard |
| The Linux endpoint tier | 10 | 5, 9 | multichannel Linux endpoints for the theater hub (no rack amp: the owner's house plan of 2026-10-04 dropped it; P13, decision 0231) |
| Rooms and groups | 11 | 6 | bonded sets, saved and live groups, Sonos-style group volume, limits, quiet hours, alarms, sleep, autoplay |
| DSP | 12 | 8 | the filter library on both platforms from shared fixtures; tone, loudness, night and speech modes; room-correction fitting |
| The TV path | 13 | 9 | stereo LPCM from optical and ARC, CEC on a Linux theater hub, theater bonding, A/V trim |
| Fleet | 14, 15 | 10 | OTA A/B with rollback on the owner's install action, adoption, Wi-Fi provisioning, telemetry, MQTT extras, `chorusctl`, dashboards |
| Inputs | 16, 17 | parity | decoders, a UPnP AV renderer per room and group, Soloist receivers per room and group, alarm sources, line-in sharing |
| The Home Assistant integration | 18, 19 | parity | a core-quality integration: media players per room and saved group, sound controls, diagnostics, events, update entities |
| Voice and announcements | 20 | parity | rooms as Assist satellites, the wake word on the server, announcements with ducking |
| The app | 21, 22 | parity | the installable PWA (Lit 3 and esbuild): rooms, groups, inputs, sound, setup and kiosk mode |
| Acoustic design tools | 23 | hardware | an acoustics package in the owner's shared Python library (box, port, baffle step, crossover), checked against published worked examples |
| The speaker designs | 24, 25, 26 | hardware | design packages for the compact speaker, the two-way, the subwoofer and the theater front's LCR set (corrected 2026-10-06, P13 and decision 0231: the owner's house plan of 2026-10-04 wires no room back to the rack and puts a soundbar in no room, so the rack amp of K70 and K74 and K72's soundbar are not designed; the LCR set of K72 is the front for both TV rooms) |
| Finale | 27 | all | a tagged release with every chosen parity feature built and tested on fakes and the simulator, the docs, the program report |

Voice path status (written 2026-10-05; K71, K73, P8 Option A): the voice path is proposal P8's Option A, approved by the owner under K71: each room with a microphone is an `assist_satellite` entity of chorus's own Home Assistant integration, and microphone audio goes speaker to chorus-server to Home Assistant's Assist pipeline. The wake word runs on the server (K73): speakers stream microphone audio only while the hardware mute is off and voice is enabled for the room, and the server runs permissively licensed wake-word models only (`docs/decisions/0166-*`, `0167-*`, `0169-*`, `0172-*`). Decided 2026-10-04 by the owner on P8's open inputs: the microphone route is run-scoped (audio only for an active run, to a per-run identifier, from the integration's registered address, with a hard time limit); chorus defines no voice intent that targets a Soloist source, and the Spotify policy reading is decided before voice rooms go live; the microphone purchase is deferred until the buy list `docs/hardware/voice-mic.md` exists, then one microphone for bench use. Nothing is ordered; the purchase is the owner's.

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

- Snapcast (GPL-3.0, clean-room rule): the binary protocol doc, the docs, and issue #522 (the monotonic clock lesson); never its source files (R17, decided 2026-09-29 by the owner, K33). https://github.com/badaix/snapcast
- ESP32 snapclient: task split, PSRAM buffering, single-sample correction on a microcontroller (from its docs and issues only; GPL, R17). https://github.com/CarlosDerSeher/snapclient
- squeezelite: SlimProto timing, DAC-delay accounting (docs and protocol descriptions only; GPL, R17). https://github.com/ralph-irving/squeezelite
- shairport-sync + NQPTP: sync engine and PTP-ish timing in software (docs only; GPL, R17). https://github.com/mikebrady/shairport-sync
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
6. Hand-write vs vendor: JSON handling, mDNS, WebSocket, MQTT (case-by-case per 3.2). MQTT: hand-written, a publish-only MQTT 3.1.1 client (decided 2026-10-03 in goal 15, K46, P10; `docs/decisions/0116-an-opt-in-read-only-mqtt-publisher.md`).
7. Dev bench hardware: exact ESP32-S3 board, TAS5825M module, wired interface (W5500 vs RMII; decided by P1: W5500, R8), measurement interface.
8. Pin map for the bench build (respect the pitfalls list in Section 6).
9. Zone/group/config data model and file format.
10. Volume taper and mute behavior.
11. DSP v1 scope and config schema.
12. Wi-Fi tier policy: which zones, what buffer, what targets.
13. TV path capture hardware and phase timing.
14. OTA distribution details and (much later, production only) signing.
15. HA/MQTT entity model. The MQTT half is decided (P10, approved by the owner at Checkpoint K; K46, K61): MQTT carries no entities and no Home Assistant discovery, only opt-in, read-only state and event topics (`docs/mqtt.md`); Home Assistant's entities come from chorus's own integration.
16. Whether any of the recommendations above deserve overturning as evidence arrives.

---

## Appendix A: CLAUDE.md starter (removed)

Removed (R17, decided 2026-09-29 by the owner, K33, K39, with R1-R4 of the same date): the starter no longer matched the repository's working agreement. `CLAUDE.md` in the repository root is the working agreement; it carries the clean-room tightening (R3), the plan of record and the owner-actions line (R1), BRIEF.md kept current by the program (R2), and rule 8 without its old citation (R4). Since 2026-10-06 (harness task 10, goals DECISIONS.md 13) the full text of all four lives in `docs/working-agreement.md`, moved word for word, and `CLAUDE.md` keeps their must-knows and points there.

---

## Amendments

Each reversal is written where its rule lives; this list says where. All were decided 2026-09-29 by the owner (R1-R4 changed `CLAUDE.md`; R15, the identity scrub, was written by goal 1; R12-R14 changed `docs/` and `deploy/`).

| # | Where | Change | Decisions |
|---|---|---|---|
| R5 | Sections 1, 4, 6 | Bare-metal Debian 13 host running Docker Compose, not Proxmox; chorus-server uses host networking | K24, K34 |
| R6 | Sections 1, 2.1, 4 | Sonos parity across software, hardware and HA; inputs-only content; no Music Assistant or Sendspin; chorus groups rooms | K13, K54, K56, K57, K64, K66 |
| R7 | Section 2.3 | Adds portable and battery speakers, Bluetooth input, reverse-engineered receivers; the app-grade PWA; official receivers as separate processes | K16, K53, K60, K66 |
| R8 | Sections 5.4, 6, 12 | The ESP32-S3 has no APLL and no Ethernet MAC; platform and link per P1 | K25, K37 |
| R9 | Section 5.5 | TAS5805M addresses, TAS5825M power ratings, the TAS5805M not a power equivalent | K47 |
| R10 | Section 5.8 | HTTP + SSE; the app stack per P5; chorus's own HA integration plus MQTT extras. As built 2026-10-05: Lit 3 and esbuild, served under `/app/`, one browser smoke test in the gate (`docs/app.md`); part 2 (sound, limits, theater, room correction, autoplay, alarms, speakers with firmware and Wi-Fi setup) built 2026-10-05, its phone check the owner's | K16, K40, K43, K46, K61, K86, K31, K77, K80, K87, K91, K92, K93 |
| R11 | Section 5.9 | Auto-adoption with trust-on-first-use keys; OTA installs only on the owner's command | K92, K93 |
| R16 | Section 0 items 2 and 5 | The program keeps this document current; confirm-before-expensive stays; Section 3.1 never relaxed | K47 |
| R17 | Sections 3.1, 11, 7, Appendix A | Clean-room tightened to docs, issues and specs of GPL projects; reciprocal hardware design files never opened; Appendix A removed | K33, K39 |
