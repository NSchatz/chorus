# Research: home theater path (decision K17)

Program: chorus /goal plan, 2026-09. Written 2026-09-29 for Checkpoint K.
Scope: what TVs emit over ARC/eARC/optical, capture hardware, codec legal status,
lip-sync budget, bass management and channel mapping, and a labelled
recommendation for K17 (theater scope).

Rules followed: every factual claim cites URL + "read 2026-09-29". "(snippet)"
means the claim came from a search-engine result summary, not a full page read.
Memory-only claims are marked ASSUMED. K33: no GPL (or LGPL) source file was
opened. Kernel `sound/` source pages turned up in search results and were
deliberately not opened. The one permissive source repo looked at was
oxideav-ac3 (MIT), and only its README.

---

## 0. Bottom line

- The only audio path that exists on **every** Roku TV is 2-channel LPCM, sent
  over optical (S/PDIF) or HDMI ARC. Over those same links, 5.1 exists only as a
  compressed bitstream: Dolby Digital (AC-3), DD+ or DTS. Uncompressed 5.1/7.1
  LPCM requires **eARC**, and Roku has eARC only on "select" models.
- AC-3 US patents expired in March 2017. The spec (ATSC A/52:2018) is a free
  PDF. That makes a clean-room AC-3 decoder in MIT/Apache Rust legally plausible.
  The last E-AC-3 patent reportedly expired on 2026-01-30. DTS core status is
  disputed. TrueHD, Atmos and MAT stay out.
- The AC-3 path costs at least one 32 ms frame of buffering, plus the TV's own
  encode delay, against a budget of 15 ms audio-early and 45 ms audio-late
  (ATSC IS-191). The PCM paths are therefore the lip-sync-friendly ones.
- **Recommendation (K17, labelled RECOMMENDED):** T1 stereo LPCM (optical first,
  then ARC with CEC for TV-remote volume and power), then T2 5.1 LPCM over eARC
  via an eARC-RX chip feeding TDM into a chorus capture endpoint. T2 depends on
  one of the owner's TVs having eARC and actually emitting multichannel PCM. T3 adds a
  clean-room AC-3 decoder for 5.1 over ARC/optical, only if T2 is blocked on that
  TV and the owner accepts the added latency. Details in section 7.

---

## 1. What TVs emit

### 1.1 ARC vs eARC capacity

| Link | LPCM | Compressed | Source |
|---|---|---|---|
| Optical S/PDIF | 2 ch only | 5.1 compressed (DD, DTS) | Roku: "limited to 2 channels of uncompressed stereo sound (PCM), or 5.1 channels of compressed surround sound" [R1] |
| HDMI ARC (HDMI 1.4) | 2 ch only | DD 5.1, DD+ 5.1, DTS 5.1 | [W1] (snippet), [W2] (snippet); about 1 Mbit/s [W1] (snippet) |
| HDMI eARC (HDMI 2.1) | up to 8 ch at 192 kHz/24-bit, "uncompressed 5.1 and 7.1, and 32-channel uncompressed audio" | TrueHD, Atmos, DTS-HD MA, DTS:X | HDMI LA [H1]; up to 37 Mbit/s [W1] (snippet) |

- In ARC mode, discovery and SAD (short audio descriptor) exchange run over CEC.
  eARC has its own discovery and capability channel, and CEC is optional, but
  some manufacturers still gate eARC on CEC being enabled [A1] (snippet, AVS
  threads and flatpanelshd).
- ASSUMED (ARC/CEC spec knowledge, not read): the TV chooses what to send from
  the sink's declared SADs. A sink that declares only LPCM 2ch gets stereo PCM,
  and a sink that declares AC-3 gets DD 5.1 for 5.1 content. Over eARC the same
  role is played by the eARC capabilities data structure, which is 256 bytes in
  ITE's IT6620 [I1].

### 1.2 Roku TV audio settings

- Location: **Settings > System > Audio > S/PDIF and ARC** [R1]. Options: Auto
  detect, Auto passthrough ("unmodified Dolby Digital, Dolby Digital Plus, or DTS
  audio when available"), or a specific format [R1]. A search summary lists the
  specific formats as PCM-Stereo, Dolby Digital and Dolby Digital Plus, and says
  passthrough on eARC can carry Dolby MAT and DD+ 7.1 [R2] (snippet).
- "PCM-Stereo" means the TV always outputs 2-channel PCM, so 5.1 sources are
  downmixed by the TV [R2] (snippet). The "Dolby Digital" setting converts all
  inputs to DD [R2] (snippet).
- eARC: "HDMI eARC is only available on select Roku TV models" [R1]. Roku's own
  spec table: Select Series HD has HDMI 1.4 (no eARC listed); Select 4K/QLED has
  eARC on HDMI 1; Plus Series and Pro Series have eARC on HDMI 4; Pro OLED has
  eARC on HDMI 1 [R3]. Some 2019 TCL 8/6/5-series gained eARC through Roku OS 9.4
  [R4] (snippet).
- Multichannel PCM out of a Roku TV is **unverified**. Sonos community reports
  describe TCL Roku TVs on eARC delivering only "stereo PCM" from built-in apps
  (for example Apple Music on the Roku TV), and a TCL 65R635 on passthrough
  showing only Atmos and DTS as detected formats [S2] (snippet). Treat it as a
  the Needs list measurement.
- Lip-sync controls: Roku's own "Adjust Audio Delay" (in the mobile app) applies
  only to Roku Streambars and Roku speakers [R5]. For third-party sinks, Roku's
  advice is to set the TV output to PCM [R5]. TCL's page only says to use Auto
  Passthrough and power-cycle [R6]. No TV-side delay knob was found for a
  third-party ARC sink, so chorus must carry its own signed A/V trim (BRIEF §5.7
  already plans this).

### 1.3 Sonos Arc as context

- Arc, Arc Ultra, Beam gen 2 and Beam Ultra support Stereo PCM, DD, DTS (core),
  DD+, Atmos (DD+), Atmos, TrueHD, Atmos (TrueHD), Multichannel PCM and Dolby
  Multichannel PCM. The advanced formats "Requires an eARC connection". Apple TV
  sends Atmos and Dolby MCPCM inside Dolby MAT [S1].
- Arc gained multichannel LPCM over eARC in software 12.2 [S3] (snippet).
- Sonos does not decode DTS-HD MA or DTS:X [S1]. Sonos has a user "TV Dialog
  Sync" (audio delay) slider [S4] (snippet). A community figure puts Sonos home
  theater buffering at about 30 ms [S5] (snippet, community, unverified).
- Parity target for chorus, then: accept 2ch PCM plus whatever multichannel form
  the TV can send; expose a lip-sync slider; control volume and power from the
  TV remote over CEC.

---

## 2. Capture hardware

Physical constraint: the server is a bare-metal Debian 13 x86 host, not beside
the TVs. Capture therefore happens in a **theater capture endpoint** at the TV
(ESP32-P4/S3 or a Linux board). It timestamps the samples onto the server
timeline and forwards them over wired Ethernet.

ASSUMED design note: the TV's S/PDIF or eARC clock is a foreign clock, so ingest
must rate-match (measure source rate against the timeline, then resample or
insert/drop samples). This is new work for the sync engine, and the simulator
can model it before hardware exists.

### 2.1 Options

| # | Path | Channels | Parts and cost | Availability (US) | Driver / firmware status |
|---|---|---|---|---|---|
| C1 | TV optical to S/PDIF receiver chip, I2S into endpoint | 2ch LPCM (or AC-3 bitstream as IEC 61937) | DIR9001 module (24-bit/96 kHz, optical+coax in, I2S out, 5 V) sold on Amazon/eBay [D1] (snippet; price not read, ASSUMED US$10-20) | Amazon US listings exist [D1] | Endpoint I2S RX in slave mode; no driver needed on ESP32 (ESP-IDF I2S) |
| C1b | Same, on a Pi | 2ch | HiFiBerry Digi+ I/O (WM8804, optical+coax in/out, 192/24): US$54.90 HiFiBerry, **US$44.75 PiShop.us** [D2] (snippet) | PiShop.us (US) [D2] | Mainline Pi overlay (ASSUMED) |
| C1c | USB S/PDIF capture on a Linux box | 2ch | Class-compliant USB S/PDIF input (for example miniDSP MCHStreamer: UAC2, optical/coax/I2S/TDM in, Linux OK [D3] (snippet); price page 403, ASSUMED ~US$100+) | miniDSP ships worldwide (ASSUMED) | snd-usb-audio, UAC2 [D3] (snippet) |
| C2 | HDMI ARC via an extractor box, optical out, then C1 | 2ch LPCM or DD/DTS 5.1 | OREI BK-931: US$109.99, accepts audio from "ARC/eARC-enabled TVs", optical = "PCM 2.0CH/Dolby/DTS 5.1CH", 3.5 mm = PCM 2.0, CEC passthrough [E1] | OREI, Skokie IL [E1] | Box handles the CEC/ARC handshake. chorus gets no CEC volume unless it also sits on CEC |
| C3 | eARC RX chip, I2S/TDM, into endpoint | up to 8ch LPCM (TDM-8 on one line, or 4 I2S lanes) | **Lattice SiI9437** eARC RX (8-ch I2S @ 192k, SPDIF, TDM, 32-QFN 4x4) [L1]; **US$4.10 at DigiKey** (qty 1) [L2] (snippet); eval kit **CP9437 obsolete** [L2] (snippet) | DigiKey (US) [L2] | Needs a host (ESP32/Linux) driving it over I2C. Datasheet reported public (SiI-DB-02013) [L1]; verify |
| C3b | ITE IT6620 eARC/ARC RX | 8 I2S lines (16ch), TDM, SPDIF; embedded CEC PHY and MCU | [I1]; price not found | Datasheet "requires member login" [I1] | NDA-ish; worse than SiI9437 for an open project |
| C3c | ADI ADV7672 (2x HDMI RX/TX crosspoint with eARC TX-or-RX; 8ch 192 kHz PCM + HBR) | 8ch | 108-lead LFCSP; EVAL-ADV7672EBZ exists [A2] (snippet) | ADI | Overkill (an 8K video crosspoint) for audio capture |
| C3d | Circal eARC/ARC receiver module | 8ch LPCM via 4-lane I2S in eARC mode, SPDIF in ARC mode, SPI control [E2] | Price on request [E2] | Contact vendor [E2] | Unknown chip; treat as a black box |
| C4 | Consumer eARC-to-USB capture device | n/a | **None found** in 2026 searches [U1] | n/a | Gap: no USB eARC capture product surfaced |
| C5 | "AVR position": chorus receives HDMI from sources and passes video to the TV | up to 8ch | HDMI RX chips with Linux CEC drivers exist (adv7604/11/12, adv7842, tc358743 [K1]) | n/a | Does not capture a Roku TV's **built-in** apps, which leave only via ARC/eARC/optical; out of scope for the owner's TVs |

Endpoint side of C3:
- ESP32-S3 TDM supports 16 slots, but "only up to 4 slots are supported while
  the slot is set to 32 bit-width, and 8 slots for 16 bit-width" [X1]. So 8ch at
  24/32-bit slots does not fit on one S3 I2S port. 8ch at 16-bit slots does,
  which loses the eARC 24-bit depth.
- A forum report says the P4 has no such limit (TDM WS width 0x1ff vs 0x7f)
  [X2] (snippet, forum; verify against the P4 TRM). This is a concrete input to
  K37 (S3 vs P4).

### 2.2 CEC (TV-remote volume, power, ARC initiation)

- **libcec**: "dual licensed under GPLv2/Commercial" [C1] (snippet). Linking it
  would make the chorus server binary GPL, so do not link it.
- **Linux kernel CEC framework**: `/dev/cecX`, managed from userspace via
  v4l-utils `cec-ctl`, `cec-compliance` and `cec-follower`. USB dongles
  supported: Pulse-Eight, RainShadow Tech, Extron. Dongles appear as
  `/dev/ttyACMX` and need `inputattach` to create `/dev/cecX` [K1].
  - The kernel doc does not mention ARC or System Audio Control [K1]. chorus
    would implement the audio-system role (System Audio Mode, Report Audio
    Status, volume keys) itself, over the kernel ioctl API, from Rust.
  - ASSUMED: using uAPI ioctls does not create a GPL derivative (syscall
    exception).
- **ESP32 CEC**: every library found is GPL: esp-cec GPL-2.0, CEClient GPL-3.0,
  floe/CEC GPL-2.0 [C2] (snippet). ESPHome's CEC component was not opened
  (ASSUMED GPL runtime). A chorus CEC stack on an MCU must be clean-room from the
  spec. IT6620 embeds a CEC PHY [I1].
- Pulse-Eight USB-CEC adapter price: not read (ASSUMED ~US$40).

---

## 3. Codec legal status (not legal advice)

| Codec | Patent status (US) | Spec | Open implementations and licence | Can chorus (MIT OR Apache-2.0) do it? |
|---|---|---|---|---|
| LPCM | none | HDMI/IEC 60958 | n/a | Yes |
| AC-3 (Dolby Digital) | Last patents expired March 2017 (US6449368 2017-03-14, US5890106 2017-03-19; "last patent ... expired March 20, 2017") [P1] (snippet), [P2] (snippet) | **ATSC A/52:2018 free PDF** [P3] (snippet); LOC: spec "fully documented and publicly available", 6 blocks x 256 samples per frame, max 640 kbit/s [P4] | liba52 **GPL** [F2] (snippet); FFmpeg ac3 decoder under FFmpeg's **LGPL-2.1+** [F1]; **oxideav-ac3: MIT, pure Rust AC-3 + E-AC-3 decoder/encoder, states it is derived from A/52:2018 (= ETSI TS 102 366), conformance corpus + 6 fuzz harnesses, 0 stars, 199 commits** [O1]; v0.0.10 dated 2026-07-03 [O2] (snippet) | **Yes**, clean-room from A/52 in Rust + C with shared fixtures, or vendor oxideav-ac3 after a provenance/quality audit (permissive, readable under K33) |
| E-AC-3 (DD+) | "The last Dolby Digital Plus (E-AC-3) patent US7516064 expired" 2026-01-30; Fedora thread says it "will need to be vetted by Fedora Legal", no outcome shown [P5] | A/52:2018 Annex E [P3] (snippet) | oxideav-ac3 includes E-AC-3 [O1] | **Likely yes** after a check; weaker evidence than AC-3 (one community claim, no legal confirmation found) |
| DTS core (Coherent Acoustics) | Disputed: a doom9 poster says "The last US patent expired on 2020-05-10", while US 7,548,853 is listed with "adjusted expiration: 2027-02-06" [P6] | ETSI TS 102 114 public [P7] (snippet) | libdca **GPL** [P7] (snippet) | **Not now**. Keep BRIEF §2.3; revisit after 2027-02-06 or a patent search |
| TrueHD / Atmos / MAT / DTS-HD / DTS:X | Proprietary; Atmos object metadata not public (ASSUMED) | Not public (ASSUMED) | n/a | **No**. BRIEF §2.3 non-goal stands. chorus must never advertise MAT/TrueHD/DTS-HD in its eARC capabilities or SADs |

Trademark: "Dolby" marks and logos need a Trademark and Standardization
Agreement for third-party use. Referring to the format in text is allowed only
if truthful and not misleading, and never with the logo [P8] (snippet, Dolby
terms and forum). chorus docs and UI should say "AC-3 (ATSC A/52)" and "E-AC-3",
and never claim "Dolby Digital" support or use the logo.

LGPL in a Rust server: FFmpeg's own checklist requires building without
`--enable-gpl` and `--enable-nonfree`, dynamic linking, distributing FFmpeg's
source, and attribution [F1]. Static linking under LGPL-2.1 §6 requires shipping
relinkable object files, which is awkward with cargo [F3] (snippet).

K33 note: FFmpeg is LGPL (GPL family). This report treats it as off-limits for
reading, same as GPL.

**Verdict:** do not link libavcodec into chorus-server. If AC-3 is ever needed,
build a clean-room decoder from A/52:2018 (a pure library with fixtures, like
sync and DSP), or vendor oxideav-ac3 after auditing it.

---

## 4. Latency budget for lip sync

- ITU-R BT.1359-1: detectability about +45 ms (audio early) to -125 ms (audio
  late); acceptability about +90 to -185 ms [T1] (snippet), [T2] (not parsed,
  PDF).
- ATSC IS-191: sound "should never lead the video program by more than 15
  milliseconds, and should never lag ... by more than 45 milliseconds" [T1]
  (snippet).
- EBU R37: each stage within 5 ms early to 15 ms late [T1] (snippet).
- BRIEF §2.2's target (±40 ms, audio never leading by more than 15 ms) sits
  inside these bounds.
- eARC: HDMI 2.1 adds mandatory lip-sync correction, in which the TV (eARC TX)
  can request the receiver's audio latency and the receiver reports it back
  [T3] (snippet, secondary sources; verify against the HDMI 2.1 spec).
  - RTINGS measured about 500 latency cases. Their finding is that a sink at the
    end of the chain via eARC often lacks latency headroom, and an HDMI-in (AVR
    position) connection gives the sink more time [T4] (snippet via an AVForums
    thread; the RTINGS page itself did not render).
  - A forum figure puts DD bitstream processing in soundbars at about 80 ms
    (Atmos about 140 ms) [T4] (forum, unverified).
- **AC-3 frame cost:** a sync frame is 6 x 256 = 1536 samples [P4], which is
  **32 ms at 48 kHz** (computed). A frame-based decoder buffers at least one
  frame. The TV's own DD encoder (used when the TV transcodes app audio to DD)
  adds at least another frame (ASSUMED). So the AC-3 path costs roughly 32-70 ms
  before chorus's own network and jitter budget.
- chorus budget, PCM path (BRIEF §5.7 decomposition; all ASSUMED until measured):
  - S/PDIF or eARC receiver: < 1 ms
  - capture chunk: 5 ms
  - wired hops (endpoint to server to endpoints): < 1 ms each
  - jitter buffer: about 10 ms
  - DSP + DAC: 2-10 ms
  - total: about 20-30 ms audio-late.
  - ASSUMED: ARC audio from a TV usually leaves the TV early relative to its
    delayed picture, which partly cancels chorus's lag. Hence the signed A/V
    trim, calibrated with a flash+beep clip filmed at 240 fps (BRIEF §5.7, §10).
    Sonos exposes the same knob as "TV Dialog Sync" [S4] (snippet).
- Calibration: 240 fps camera method (BRIEF §10). Later, if eARC latency
  reporting is implemented on the RX chip, report chorus's measured pipeline
  latency to the TV.

---

## 5. Bass management and channel mapping

### 5.1 Channel order

5.1 channel order differs by transport. Every edge must remap explicitly.

| Transport | 5.1 channel order | Source |
|---|---|---|
| WAV WAVE_FORMAT_EXTENSIBLE | FL, FR, FC, LFE, BL, BR (master order FL FR FC LF BL BR FLC FRC BC SL SR TC ...) | [M1] |
| ALSA default (surround51) | FL, FR, RL, RR, FC, LFE | [M2] (snippet) |
| HDMI/eARC audio InfoFrame CA=0x0B | slot0 FL, 1 FR, 2 LFE, 3 FC, 4 RL, 5 RR | **ASSUMED / verify**: the search summary quoting this came from Linux kernel sound source, which was not opened (K33); confirm against the CTA-861 channel-allocation table |

Recommendation: the stream-format announcement carries an explicit per-channel
position list (as dwChannelMask does [M1]). Endpoints in a bonded theater set get
a channel map (K30), and ingest remaps from the transport's order once.

### 5.2 Bass management

Standard shape:
- THX/SMPTE crossover at 80 Hz [B1] (snippet).
- 12 dB/oct high-pass on each main channel, 24 dB/oct low-pass on the sub feed,
  designed to sum to about an LR4 acoustic alignment [B2].
- The LFE channel is "amplified by 10 dB on playback and summed into the signal
  going to the subwoofer" [B2]. Some AVRs also apply a 120 Hz low-pass to LFE
  [B2].

Where chorus does it (proposal): the **server** (or capture endpoint) renders
per-endpoint feeds for a bonded set, since only a node holding all channels can
sum redirected bass. The sub gets mono (sum of HPF'd-away bass + LFE +10 dB,
then LPF). The mains get HPF. Each endpoint keeps its own driver crossover, delay
and trims (BRIEF §5.6). All of it lives in the DSP library with shared fixtures.

### 5.3 Speech enhancement and night mode (permissive references only)

- Night mode = dynamic range compression. AC-3 streams carry decoder DRC words
  (ASSUMED from A/52 knowledge; the spec is [P3]). On PCM paths chorus runs its
  own compressor, reusing the §5.6 limiter.
- Speech enhancement:
  - with 5.1: boost FC in the voice band, about 1-4 kHz [V1] (snippet).
  - with stereo: extract a center (mid) signal and boost it. References: EUSIPCO
    2015 "Dialogue enhancement of stereo sound" [V2] (not opened; paper) and
    arXiv 2211.14378 on mid-side processing [V3] (not opened; paper).
  - These are papers, not code, so they are safe under K33.

---

## 6. Comparison

| Scope option | Works on any Roku TV | 5.1 | TV-remote volume | Added latency | Legal risk | Hardware to buy | Build effort |
|---|---|---|---|---|---|---|---|
| A. PCM stereo over optical (C1) | Yes (if the TV has optical; ASSUMED most do) | No (TV downmixes) | No (no CEC on optical) | Lowest | None | S/PDIF RX module (~$10-50) | Low |
| B. PCM stereo over ARC + CEC (C3 chip in ARC mode, or C2 box + separate CEC) | Yes (ARC is universal on HDMI 1.4+ TVs, ASSUMED) | No | **Yes** (Sonos-parity UX) | Low | None (kernel CEC via ioctl; no libcec) | eARC/ARC RX board + CEC | Medium (CEC audio-system role) |
| C. 5.1 LPCM over eARC (C3) | **Only eARC Roku models**, and only if they emit MCPCM | Yes | Yes (CEC) | Low | None | SiI9437-class board (devices design) + P4 or Linux endpoint | High (new board, TDM, 6-8 ch set) |
| D. AC-3 5.1 over ARC/optical (A or B + decoder) | Yes | Yes (lossy) | With B | **+32 to 70 ms** | Low (patents expired; trademark wording) | Same as A/B | Medium-high (decoder + fixtures) |
| E. E-AC-3 decode | Only via ARC/eARC passthrough | Yes | With B | as D | Moderate (2026 expiry, unconfirmed by counsel) | as B | Medium on top of D |
| F. DTS core | Some TVs | Yes | With B | as D | **Unclear until 2027** | as B | Medium |
| G. TrueHD / Atmos / MAT | n/a | n/a | n/a | n/a | Not implementable | n/a | Out |

---

## 7. Recommendation for K17 (RECOMMENDED, for Checkpoint K)

**K17 scope = "LPCM first, eARC multichannel second, AC-3 only as a fallback":**

1. **T1: LPCM stereo** (BRIEF phase 9 as written).
   - Start via TV optical into a DIR9001/WM8804-class S/PDIF receiver on a chorus
     theater capture endpoint, feeding the UDP+FEC low-latency path.
   - Then move the same endpoint to **HDMI ARC with a chorus CEC audio-system
     implementation** (Linux kernel CEC ioctls from Rust, or clean-room on the
     MCU; never libcec), so the TV remote drives chorus volume, mute and power.
     That is the Sonos-parity bar.
   - TV setting: PCM-Stereo, or declare only LPCM 2ch SADs.
2. **T2: 5.1 LPCM over eARC**, via a Lattice SiI9437-class eARC RX (US$4.10 IC,
   datasheet reported public, eval kit obsolete, so it becomes a devices board
   design) feeding TDM-8 into an **ESP32-P4** or a Linux capture endpoint.
   - The S3's 4-slot limit at 32-bit width makes it a poor fit.
   - The capture endpoint declares only LPCM 2ch/6ch/8ch in its eARC
     capabilities. Bass management and channel maps come from section 5, and the
     theater bonded set comes from K30.
   - **Gate:** the owner confirms that an eARC Roku TV actually sends multichannel PCM
     from its built-in apps (the Needs list 2). If none does, skip to T3 for that
     room.
3. **T3 (conditional): AC-3 decode** for 5.1 over ARC/optical, enabled when a TV
   lacks eARC or will not emit MCPCM, and only if the owner accepts about 32-70 ms of
   extra audio lag. The A/V trim then has to pull video-side margin, which a
   Roku TV cannot provide.
   - Clean-room Rust decoder from ATSC A/52:2018 with shared fixtures, or vendor
     oxideav-ac3 (MIT) after an audit. Never link libavcodec or liba52.
   - E-AC-3 is deferred until a legal check confirms the 2026 expiry.
   - UI wording: "AC-3".
4. **Keep out:** DTS (revisit after 2027-02-06), TrueHD, Atmos, MAT, DTS-HD and
   DTS:X.
   - BRIEF §2.3 needs one amendment only if T3 is chosen: state that AC-3
     (patent-expired) decode is allowed. §2.3 today excludes only
     Atmos/TrueHD/DTS, so AC-3 is not explicitly banned, but "set sources to
     PCM" implies it.

Why this order: T1 works on all three TVs, with no legal exposure and the
smallest latency. CEC volume is what makes it feel like Sonos. T2 is the only
path to discrete 5.1 without a frame-sized latency penalty, but it depends on
hardware facts about the owner's TVs that are unknown today. T3 buys 5.1 on non-eARC
TVs at a lip-sync cost that measurement must justify (BRIEF §3.1 rule 3).

---

## 8. the Needs list

1. **Model numbers of the 3 Roku TVs**, from Settings > System > About or the
   back label [R3]. Also note, per TV: whether it has an HDMI port labelled
   eARC (and which), whether it has optical out, and what the S/PDIF and ARC
   menu offers.
2. **Multichannel PCM probe** (if any TV has eARC and he still has a Sonos Arc,
   Beam gen 2 or other eARC soundbar/AVR connected):
   - Play known 5.1 content from a Roku built-in app with the TV on Auto.
   - Read the input format the sink reports (Sonos app shows it).
   - Result "Multichannel PCM" means T2 is viable on that TV. "Dolby Digital
     Plus" or "Stereo PCM" means it is not viable as-is.
3. **Buy list** (priced, never ordered by the program):
   - T1: DIR9001 S/PDIF-to-I2S module (Amazon US; price ASSUMED $10-20) or
     HiFiBerry Digi+ I/O (US$44.75 PiShop.us) plus a Toslink cable.
   - T1 ARC/CEC: Pulse-Eight USB-CEC adapter (price not read) or the RX board's
     own CEC.
   - T2: SiI9437 ICs (US$4.10 DigiKey) on a devices board, or a quote from
     Circal for their module.
4. **Decide at Checkpoint K:**
   - (a) accept the T1 > T2 > T3 order.
   - (b) whether AC-3 decode (T3) is allowed at all, and whether its latency is
     acceptable.
   - (c) whether to amend BRIEF §2.3 wording accordingly.
   - (d) E-AC-3 stays off until a legal check. The program cannot give legal
     advice. the owner decides his comfort with the 2017/2026 patent expiry evidence.

---

## Sources (all read 2026-09-29)

- [R1] https://support.roku.com/article/connect-surround-sound-to-your-roku-tv
- [R2] Search summary for "Roku TV digital audio output setting ..." (snippet); results incl. support.roku.com and Philips Roku TV manual https://www.manualslib.com/manual/1962418/Philips-Roku-Tv.html?page=131 (not opened)
- [R3] https://support.roku.com/article/roku-branded-tv-product-information
- [R4] https://www.flatpanelshd.com/news.php?subaction=showfull&id=1602227734 (snippet)
- [R5] https://support.roku.com/article/sound-is-out-of-sync
- [R6] https://support.tcl.com/audio-delay-with-tcl-roku-tv-and-hmdi-speaker
- [H1] https://www.hdmi.org/spec2sub/enhancedaudioreturnchannel
- [W1] https://www.tesmart.com/blogs/news/hdmi-arc-vs-earc-explained-soundbars-tvs-dolby-atmos-and-hdmi-switches (snippet)
- [W2] https://www.wyrestorm.com/blog/hdmi-arc-vs-earc/ (snippet)
- [A1] https://www.avsforum.com/threads/does-earc-need-cec-enabled-to-function.3239716/ , https://www.flatpanelshd.com/guide.php?subaction=showfull&id=1534479331 (snippets)
- [A2] https://www.analog.com/en/products/adv7672.html (timed out), https://www.analog.com/media/en/technical-documentation/data-sheets/adv7672.pdf (snippet)
- [S1] https://support.sonos.com/en-us/article/supported-home-theater-audio-formats
- [S2] https://en.community.sonos.com/home-theater-229129/arc-appletv-4k-new-gen-tcl-roku-tv-trouble-with-atmos-6886734 and related Sonos community threads (snippet)
- [S3] https://www.flatpanelshd.com/news.php?subaction=showfull&id=1605071857 (snippet)
- [S4] https://support.sonos.com/en-us/article/tv-audio-and-video-are-out-of-sync (snippet)
- [S5] https://en.community.sonos.com/home-theater-228993/sonos-arc-and-gaming-latency-6841211 (snippet)
- [I1] https://www.ite.com.tw/en/product/cate1/IT6620 ; IT6622 https://www.ite.com.tw/en/product/cate1/IT6622 (snippet)
- [L1] https://www.latticesemi.com/en/Products/ASSPs/HDMI21eARC
- [L2] https://www.digikey.com/en/products/detail/lattice-semiconductor-corporation/SII9437CNUC/7672175 (snippet; page 403), https://www.digikey.com/en/products/detail/lattice-semiconductor-corporation/CP9437/7672116 (snippet)
- [E1] https://www.orei.com/products/8k-hdmi-or-earc-audio-extractor-bk-931
- [E2] https://www.circalengineering.com/earc-arc-audio-receiver-module.html
- [D1] https://www.amazon.com/DIR9001-Coaxial-Receiver-Module-Assembled/dp/B09SVHHXL4 , https://www.audiophonics.fr/en/interface-modules/lhy-audio-interface-module-spdif-to-i2s-dir9001-24bit-96khz-p-17274.html (snippets)
- [D2] https://www.hifiberry.com/shop/boards/hifiberry-digi-io/ , https://www.pishop.us/product/hifiberry-digi-i-o/ (snippets)
- [D3] https://www.minidsp.com/products/usb-audio-interface/mchstreamer (403; snippet)
- [U1] Search "USB audio capture eARC input multichannel UAC2 device Linux eARC to USB" (no product found)
- [K1] https://docs.kernel.org/admin-guide/media/cec.html (documentation, not source)
- [C1] https://libcec.pulse-eight.com/ (snippet)
- [C2] https://github.com/ideaChenGo/esp-cec , https://github.com/lucadentella/ArduinoLib_CEClient , https://github.com/floe/CEC (snippets only; repos not opened, GPL)
- [X1] https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/i2s.html (v6.1)
- [X2] https://esp32.com/viewtopic.php?t=29494 (snippet)
- [P1] https://www.cnx-software.com/2017/02/10/dolby-digital-ac3-us-patent-has-expired-on-february-1-2017/ , https://www.avsforum.com/threads/dolbys-last-patent-related-to-ac-3-expired-today-3-20-17.2789001/ (snippets)
- [P2] https://gigazine.net/gsc_news/en/20170321-ac-3-patent-expired/ (snippet)
- [P3] https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf (snippet; PDF not parsed)
- [P4] https://www.loc.gov/preservation/digital/formats/fdd/fdd000209.shtml
- [P5] https://discussion.fedoraproject.org/t/dolby-digital-plus-e-ac3-patents-have-now-expired-time-to-add-it-to-fedora/180329 ; Phoronix https://www.phoronix.com/news/Dolby-Digital-Plus-E-AC3-2026 (403; snippet)
- [P6] https://forum.doom9.org/showthread.php?t=175103
- [P7] https://www.videolan.org/developers/libdca.html , https://www.etsi.org/deliver/etsi_ts/102100_102199/102114/01.06.01_60/ts_102114v010601p.pdf (snippets)
- [P8] https://www.dolby.com/about/legal/terms-of-use/ , https://www.vegascreativesoftware.info/us/forum/use-of-dolby-digital-not-logo--30090/ (snippets)
- [F1] https://ffmpeg.org/legal.html
- [F2] https://github.com/mackyle/a52codec/tree/master/liba52 (snippet; not opened, GPL)
- [F3] https://forum.qt.io/topic/65116/static-builds-and-lgpl-again-providing-object-files-instead-of-open-source , https://github.com/codyps/rust-systemd/issues/92 (snippets)
- [O1] https://github.com/OxideAV/oxideav-ac3 (README only; MIT)
- [O2] https://lib.rs/crates/oxideav-ac3 (snippet)
- [T1] http://www.nab.org/xert/scitech/pdfs/tv100509.pdf (snippet; PDF not parsed)
- [T2] https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.1359-1-199811-I!!PDF-E.pdf (listed; not parsed)
- [T3] https://eureka.patsnap.com/report-hdmi-2-1a-earc-lip-sync-jitter-and-cable-receiver-robustness , https://avlatency.com/terminology/history-of-audio-video-latency-and-input-lag/ (snippets)
- [T4] https://www.avforums.com/threads/lip-sync-delays-%E2%80%94-interesting-rtings-article.2487042/ ; RTINGS https://www.rtings.com/soundbar/learn/research/1-3-tbu-article and https://www.rtings.com/soundbar/reviews/sonos/arc (pages did not render)
- [M1] https://learn.microsoft.com/en-us/previous-versions/windows/hardware/design/dn653308(v=vs.85)
- [M2] https://www.csa.iisc.ac.in/~udayb/alsamch.shtml (snippet)
- [B1] https://www.audioholics.com/subwoofer-setup/bass-management-the-right-stuff , https://www.svsound.com/blogs/subwoofer-setup-and-tuning/tips-for-setting-the-proper-crossover-frequency-for-a-subwoofer (snippets)
- [B2] https://en.wikipedia.org/wiki/Bass_management
- [V1] https://www.makeuseof.com/soundbar-setting-clear-dialogue/ (snippet)
- [V2] https://www.eurasip.org/Proceedings/Eusipco/Eusipco2015/papers/1570096395.pdf (snippet)
- [V3] https://arxiv.org/pdf/2211.14378 (snippet)

Local reads: /workspace/BRIEF.md (whole), /cache/tmp/plan-2026-09-chorus/decisions.md (whole).

Not opened by rule (K33): Linux kernel `sound/` source (hdmi_chmap.c, hdmi-codec.c,
intel_hdmi_audio.c) that appeared in results; liba52, libdca, FFmpeg, esp-cec,
CEClient, floe/CEC, and ESPHome CEC sources.
