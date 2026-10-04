# P2: Theater scope and TV capture hardware

- Decisions: K17, K72
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: Stereo PCM from optical and ARC only
- Builds on: goal 10 (§14, the Linux endpoint tier: multichannel, theater hub), goal 12 (§16, DSP: bass management, night and speech), goal 13 (§17, the TV path: capture per P2, CEC, theater bonding, A/V trim, TV autoplay), goal 26 (§30, the soundbar and LCR packages "limited to what P2 settled")

## Question

What does the chorus TV path capture, from which TV outputs, in which formats, on which hardware?
The options are stereo PCM from optical and ARC (with HDMI CEC for TV-remote volume and TV
autoplay), 5.1 LPCM over eARC, and AC-3 (Dolby Digital) decode.

The owner decisions that bound it:

- K17: "goal 1 researches ARC/eARC capture hardware, what TVs emit over ARC vs eARC, and the
  legal/licence status of each codec (LPCM, Dolby Digital AC-3, E-AC-3, DTS, TrueHD/Atmos), and
  proposes the theater scope at Checkpoint K. BRIEF §2.3 (no Atmos/TrueHD/DTS decode) stands
  unless the owner amends it there."
- K72: "Two variants (if K17's research keeps the theater path): a soundbar (touch + status LED
  on top, mic with hardware mute, eARC + optical) and a separate-LCR set (three clean speakers with
  no visible controls; TV inputs on a hidden hub or the rack amp)."
- K81: "TV autoplay (TV on via HDMI CEC or input signal -> theater room plays TV and leaves any
  group)".
- K96: Linux endpoints are a product tier "e.g. the 2U rack amp (K74) and the theater hub (K72,
  eARC capture)".
- K30: theater bonding of "sub + surrounds + front as one room", BRIEF §2.2's < 0.5 ms same-room
  target, per-endpoint channel maps.

The owner's TVs are three Roku TVs whose models are unknown (a goal-1 Needs item asks for each
model, its eARC or ARC port, its optical output and its audio menu).

## Constraints that bind every option

- **BRIEF §2.3** (the owner's document): "No Dolby Atmos, TrueHD, or DTS decode (licensing makes
  legal open implementation impossible; set sources to PCM)." AC-3 is not named; its "set sources
  to PCM" clause implies PCM. Any AC-3 scope is shown to the owner as a §2.3 wording change (the
  program keeps BRIEF current per K47, never beyond what the owner decides here).
- **Lip sync (BRIEF §2.2, §5.7):** TV audio within +/- 40 ms of video, audio never leading by more
  than 15 ms; wired endpoints, small buffers, FEC instead of retransmission.
- **Clean-room (BRIEF §3.1 rule 1, K33):** libcec is "dual licensed under GPLv2/Commercial"
  (research-theater.md [C1], a snippet) and is never opened or linked; every ESP32 CEC library the
  planning research found is GPL. CEC on Linux goes through the kernel CEC framework's userspace
  API (`/dev/cecX` ioctls), called from chorus's own Rust code. liba52 (GPL), libdca (GPL) and
  FFmpeg (LGPL, treated as off-limits) are never opened.
- **Licences (K26, K95):** chorus code is MIT OR Apache-2.0; a dependency with another licence
  needs an ADR.
- **devices' rules (§0.13):** "buy the module rather than fabricate it (a custom board exists only
  after the owner approves a proposal that says it departs from 'buy what can be bought')". Every
  option below says whether it needs a custom board.
- **Nothing measured or unknown is invented (§0.8):** the TV models, what each TV emits, and every
  latency figure below that is not computed from a standard are `ASSUMED` until the owner's
  answers and a bench report replace them. No goal waits on a measurement (K7).
- **Rule 8 (fitness only):** no option is preferred or dropped because of what is installed in a
  container.
- **Trademarks:** chorus text and UI say "AC-3 (ATSC A/52)", never "Dolby Digital" as a product
  claim and never a Dolby logo (research-theater.md §3, [P8], a snippet; ASSUMED until a goal reads
  Dolby's trademark terms in full).

## Re-verification of the planning research

What `research-theater.md` and `verify-theater-platform.md` said, what was fetched again on
2026-09-30, and what changed:

| Claim (planning research) | Re-check 2026-09-30 | Changed? |
|---|---|---|
| Roku optical carries 2 ch PCM or 5.1 compressed; eARC only on "select" models | Roku support [S1]: "The S/PDIF connection is limited to 2 channels of uncompressed stereo sound (PCM), or 5.1 channels of compressed surround sound (Dolby Digital or DTS)"; "HDMI eARC is only available on select Roku TV models" | No |
| Roku audio menu: Settings > System > Audio > S/PDIF and ARC: Auto detect, Auto passthrough, specific formats | Same page [S1]: Auto detect (default), "Auto passthrough - Delivers unmodified Dolby Digital, Dolby Digital Plus, or DTS audio when available", "Specific audio formats"; the page also says the Roku TV remote can adjust volume and mute on a connected home theater | No (the list of specific formats is still only from a snippet) |
| eARC ports per Roku series: Select 4K/QLED HDMI 1; Plus and Pro HDMI 4; Pro OLED HDMI 1; Select HD none | Roku product page [S2]: Select Series HD "3 HDMI (HDMI 1.4)", optical, audio "Stereo"; Select Series 4K "HDMI 1 eARC"; Plus Series "HDMI 4 eARC"; Pro Series "HDMI 4 eARC"; Pro OLED "HDMI 1 eARC"; every series lists an optical output | Small: the page as read lists no eARC for Select 4K QLED (the planning research grouped it with Select 4K); the page covers only Roku-made TVs ('If you have a Roku TV made by another manufacturer (TCL, Hisense, Philips, etc.), contact the manufacturer directly'), and Select HD and 4K ports 'May vary by model' |
| Multichannel PCM from a Roku TV is unverified | Not stated on either Roku page. A search surfaced only community threads (a Roku community thread "Does Roku output 5.1 or 7.1 LPCM", now redirecting to support.roku.com; Sonos and AVS threads), which report stereo PCM or bitstream only [L1] (`LEAD`) | Sharpened: no source says a Roku TV sends multichannel LPCM; several leads say it does not |
| No TV-side audio delay control for third-party sinks; set output to PCM | Roku "sound is out of sync" [S3]: "Adjust Audio Delay" in the Roku mobile app (for Roku audio products), "Set the digital audio output on your TV to PCM", "Some TV models have an A/V Sync feature" | No: chorus still carries its own signed A/V trim |
| AC-3 US patents expired by March 2017 (secondary sources) | **Primary sources added:** Dolby's own SEC Form S-1 (filed 2004-11-19) [P1]: "Patents relating to our Dolby Digital technologies expire between 2008 and 2017, and patents relating to our Dolby Digital Plus technologies, an extension of Dolby Digital, expire between 2019 and 2020." Google Patents [P2]: US5890106 (Dolby, filter bank with time-domain aliasing cancellation) "Expired - Lifetime", expiration 2017-03-19 | Stronger evidence; one correction below |
| US6449368 is an AC-3 patent expiring 2017-03-14 | Google Patents [P3]: US6449368 is "Multidirectional audio decoding" (a crossfeed canceller), Dolby, expired 2017-03-14. Its subject is not the AC-3 bitstream | **Corrected:** do not cite it as an AC-3 patent |
| Last E-AC-3 patent US7516064 expired 2026-01-30 (a Fedora thread) | Google Patents [P4]: US7516064 "Adaptive Hybrid Transform for Signal Analysis and Synthesis", Dolby, "Expired - Lifetime (expires January 30, 2026)"; A/52:2018 [P5] defines an E-AC-3 field `ahte` ("Adaptive Hybrid Transform Enabled"). Dolby's 2004 S-1 put DD+ patents at 2019-2020, yet this 2004-filed patent ran to 2026, so later-filed E-AC-3 patents cannot be excluded without a patent search | Patent record confirmed; "last" is still unproven |
| DTS status disputed; US7548853 listed to 2027-02-06 | Google Patents [P6]: US7548853 (DTS Inc, "Scalable compressed audio bit stream and codec using a hierarchical filterbank"), status "Active", adjusted expiration 2027-02-06. Whether it reads on DTS core decoding is not known | No: DTS stays out |
| ATSC A/52:2018 is a free PDF; 1536 samples per sync frame | Downloaded again [P5]: HTTP 200, 1.9 MB, 271 pages, "Doc. A/52:2018, 25 January 2018"; "Each synchronization frame contains 6 coded audio blocks (AB), each of which represent 256 new audio samples per channel"; front matter keeps ATSC's patent boilerplate ("compliance with this standard may require use of an invention covered by patent rights") | No |
| SiI9437 eARC RX: 8 ch I2S at 192 kHz, S/PDIF, public datasheet, US$4.10 at DigiKey, eval kit CP9437 obsolete | Lattice page [H1]: SiI9437 eARC receiver, 32-QFN 4 x 4 mm, "8-ch I2S interface @ 192 KHz", S/PDIF output, "automatic ARC fallback", eARC data channel "via I2C interface"; datasheet SiI-DB-02013 listed as a download; CP9437 starter kit listed with no status. DigiKey returned 403 | Price and kit status **not re-verified** (US$4.10 stays a 2026-09-29 snippet); a forum lead says the datasheet is under NDA [L2] (`LEAD`), which conflicts with the Lattice listing |
| IT6620: ARC/eARC RX, 8 I2S lines, TDM, S/PDIF, embedded CEC PHY, datasheet behind a login | ITE page [H2]: confirmed, 40-QFN 5 x 5 mm, "Datasheet access requires member login" | No |
| Circal eARC/ARC receiver module, price on request | Circal page [H3]: "eARC/ARC to I2S audio converter", "4-lane I2S outputs in eARC mode", "PCM up to 8 channels, 24-bit/192kHz", S/PDIF for ARC, SPI control, single +5 V; no price, no CEC mentioned | No |
| No consumer eARC-to-USB capture device; no buyable eARC-to-I2S board | One search for an eARC-to-I2S module found only DIY threads and HDMI 1.4 (not eARC) de-embedder boards [L2] (`LEAD`) | No |
| OREI BK-931: US$109.99, optical "PCM 2.0CH/Dolby/DTS 5.1CH" | OREI page [H4]: "$109.99" (sale, list $139.99), optical "PCM 2.0CH/Dolby/DTS 5.1CH", 3.5 mm "PCM 2.0CH", HDMI outputs to 7.1 LPCM, ARC and eARC input | No |
| DIR9001 S/PDIF receiver, I2S out | TI [H5]: "ACTIVE", 28-108 kHz, 24-bit I2S/left-justified output; datasheet SLES198A [H6]: pin 1 `AUDIO` "Channel-status data information of non-audio sample word, active-low", and "For non-PCM data, interpolation is not performed and data is directly output with no processing" | **Added:** the `AUDIO` pin flags non-PCM data, and on a parity error non-PCM data is 'directly output with no processing' (no interpolation), so an AC-3 bitstream is not concealed or altered by the receiver (general passthrough `ASSUMED` from the datasheet's data path; goal 13 checks it on the bench) |
| HiFiBerry Digi+ I/O US$54.90 (US$44.75 at PiShop.us) | HiFiBerry [H7]: "$54.90", optical and coax in and out, "all Pi's with 40pin GPIO connector"; it points to a "limitations" page for recording, whose URL returned 404 | PiShop price not re-read; recording limits unknown (`ASSUMED` usable until a goal reads HiFiBerry's docs); ship-from not stated, **NON-US EXCEPTION** (Swiss seller, `ASSUMED`) |
| Pulse-Eight USB-CEC price ASSUMED about US$40 | Pulse-Eight [H8]: "USD $48.08"; the kernel has a `pulse8-cec` driver [K1] | Price now read; ship-from not stated, **NON-US EXCEPTION** (UK seller, `ASSUMED`) |
| Kernel CEC framework: `/dev/cecX`, v4l-utils tools, USB dongles | docs.kernel.org [K1]: Pulse-Eight, RainShadow, Extron; "cec-gpio. If the CEC pin is hooked up to a GPIO pin then you can control the CEC line through this driver"; SoC CEC including Raspberry Pi; userspace API [K2]: `CEC_LOG_ADDR_TYPE_AUDIOSYSTEM` "Use for an audio system device", `CEC_OP_PRIM_DEVTYPE_AUDIOSYSTEM` | **Added:** cec-gpio means a hub can drive CEC from a GPIO with no dongle |
| Raspberry Pi SBCs support no TDM and no MCLK | Not re-fetched; `verify-theater-platform.md` (2026-09-29) quotes the Raspberry Pi whitepaper "TDM is NOT supported on any Raspberry Pi SBCs" and "All Raspberry Pi Ltd SBCs can act as either a producer or a consumer" [R1] | Carried as read on 2026-09-29 |

Adversarially verified 2026-09-30 (goal-1 verifier 1): 12 claims confirmed, 0 refuted, 3 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Options

All options put capture in a **theater hub at the TV**: the server is a rack host, not beside the
TVs (research-theater.md §2). The hub is a Linux endpoint (K96) that captures, timestamps onto the
server timeline with the monotonic clock, rate-matches the TV's foreign clock (goal 13 item 2) and
forwards over wired Ethernet. The same capture can live in the **rack amp** (K70 gives it an
optical input) when a TV is within an optical run of the rack, and a P4-class board could capture
optical over I2S, but CEC on an MCU means a clean-room CEC stack with no permissive library to
start from, so the Linux hub is the default endpoint for every option.

### Option A: stereo LPCM from optical and ARC, with CEC (the "If deferred" fallback)

- What:
  - Audio: the TV set to PCM-Stereo (or the hub declaring only 2 ch LPCM). Optical first: TV
    optical into a DIR9001-class S/PDIF receiver module (DIR9001 is the I2S clock producer, the Pi
    an I2S consumer) or a HiFiBerry Digi+ I/O HAT. ARC second: a bought ARC/eARC extractor
    (OREI BK-931 class) terminates ARC and CEC on the TV's ARC port and feeds its optical output
    into the same receiver.
  - Control: chorus claims the CEC Audio System logical address through the kernel API and
    implements the audio-system role itself (System Audio Mode, Give/Report Audio Status, volume
    and mute keys, standby, power status) in Rust. The CEC line comes from the hub's own HDMI
    port (Raspberry Pi SoC CEC), a Pulse-Eight USB adapter, or cec-gpio.
  - TV autoplay (K81): CEC power and Active Source messages, plus signal lock on the receiver.
  - Theater set: 2.0 source; bonding (K30) still gives a sub with bass management; surrounds get
    nothing or a documented upmix (a DSP-8 choice, not a format).
- Costs:
  - Money per TV room (bought modules only, never ordered by the program): HiFiBerry Digi+ I/O
    US$54.90 [H7] (ship-from not stated; **NON-US EXCEPTION**, Swiss seller `ASSUMED`; the
    planning research's PiShop.us price, US$44.75, not re-read, would be the US line) or a DIR9001
    module (Amazon listing; price not read, `ASSUMED` US$10-20); for ARC, an OREI BK-931
    US$109.99 [H4]; for CEC, nothing extra with the hub's HDMI port or a Pulse-Eight adapter
    US$48.08 [H8] (ship-from not stated; **NON-US EXCEPTION**, UK seller `ASSUMED`); a Pi-class hub priced in P4 (raspberrypi.com lists only the
    Pi 5 16 GB, US$305, on the page read [H9]; smaller models not re-read).
  - Effort: goal 13 items 2-3 as written (capture, rate matching, CEC on fakes, autoplay, A/V
    trim); goal 10's Linux packaging; goal 26's LCR hub. Maintenance: the CEC audio-system role
    against real TVs' quirks.
  - Gate time: fakes and fixtures only; no hardware in the gate.
- Risks:
  - Whether a Roku TV routes its remote's volume over CEC to an audio system that is not on the
    ARC port (optical plus CEC) is `ASSUMED`; the bench packet tests it.
  - An extractor box may itself claim the Audio System address, colliding with chorus
    (`ASSUMED`; the bench packet checks with `cec-ctl` monitoring).
  - 5.1 content is downmixed by the TV: no discrete surrounds.
- Fit: works on every Roku-made TV series (each lists optical [S2]); partner-brand Roku TVs (TCL,
  Hisense and others) are checked per model through the TV Needs item; no custom board; no codec or
  patent question; the lowest latency; matches BRIEF §5.7 ("a stereo TV path is the sensible first
  version") and the If-deferred cell. It falls short of Sonos parity on discrete surround.

### Option B: A plus 5.1 LPCM over eARC

- What: an eARC receiver (Lattice SiI9437, or the Circal module) on the TV's eARC port, 8 ch LPCM
  into the hub, the hub declaring only LPCM 2/6/8 ch in its eARC capabilities (never MAT, TrueHD
  or DTS-HD).
- Hub side: a Pi has no TDM [R1], so 8 ch arrive as 4 parallel I2S lanes on the consumer port
  (the SiI9437 and Circal both offer multi-lane I2S); an ESP32-P4 could take TDM-8 (research only,
  a forum lead [X2] in research-theater.md, never verified against the P4 TRM); the ESP32-S3 fits
  only 8 slots at 16-bit width (research-theater.md [X1]).
- Costs:
  - Money: SiI9437 about US$4.10 each (2026-09-29 snippet, not re-verified) **on a custom board**
    (HDMI connector, eARC front end, I2C control, clocking), or the Circal module at an unknown
    price.
  - Effort: a board design package in devices (goal 26), an I2C driver for the eARC data channel,
    multi-lane capture, 5.1 maps; large.
- Risks:
  - **No source says a Roku TV emits multichannel LPCM**, and community leads say it does not [L1].
    The owner's TVs may all be Select HD (no eARC) or send only bitstream.
  - **Needs a custom board**: a departure from devices' "buy what can be bought" that needs its
    own approved proposal (§0.13). The Circal module might avoid it, but its price and chip are
    unknown.
  - The datasheet's availability is contested (Lattice lists a download; a lead says NDA).
- Fit: the only route to discrete 5.1 without a codec, and the lowest-latency 5.1; blocked on
  facts about the TVs and on a devices departure.

### Option C: A plus AC-3 decode (5.1 from optical and ARC as a bitstream)

- What: the TV set to Dolby Digital (or Auto with the hub declaring AC-3 in its capabilities):
  over optical or ARC it sends an AC-3 bitstream (IEC 61937 in S/PDIF, `ASSUMED` framing detail
  from general knowledge; verify in goal 13). The DIR9001 flags non-PCM data and does not
  interpolate it on errors [H6] (`ASSUMED` passed unmodified). chorus de-frames and decodes AC-3 to 5.1 PCM on the hub, then the theater set
  plays it with 5.1 maps and bass management (goal 12).
- Decoder: a clean-room decoder written from ATSC A/52:2018 [P5] as a pure library with shared
  fixtures (CLAUDE.md rule 5), or the MIT pure-Rust `oxideav-ac3` vendored after a provenance and
  quality audit (research-theater.md [O1], README only, not re-read). Never liba52, libdca or
  libavcodec.
- Costs:
  - Money: the same hardware as A.
  - Effort: an AC-3 decoder (six-block frames, bit allocation, MDCT, downmix and DRC words) with
    conformance fixtures: medium-large, fits goal 13 beside item 2, or a follow-up goal. Size and
    CPU on a Pi-class hub are `ASSUMED` small (the planning session measured ffmpeg decoders at
    0.16%-0.52% of one Xeon core per stereo stream, research-casting-decoders.md §5.1; no AC-3
    figure).
  - BRIEF §2.3 needs a wording change (AC-3 decode allowed; the other formats stay out).
- Risks:
  - **Latency:** one sync frame is 1536 samples [P5], **32 ms at 48 kHz** (computed); a
    frame-based decoder buffers at least one frame, and a TV that transcodes app audio into AC-3
    adds its own encoder delay (`ASSUMED` at least one more frame). With the PCM path's `ASSUMED`
    20-30 ms (research-theater.md §4), the AC-3 path lands near 52-94 ms audio-late before any
    trim, against a +/- 40 ms target. Roku's A/V sync help names an audio-delay adjustment only
    for Roku audio products and says some TVs have an A/V Sync (audio delay) feature [S3]. An
    audio delay cannot correct late audio, and no source read shows a Roku TV setting that delays
    the picture. Only a measurement decides.
  - Patent position (not legal advice): Dolby's own S-1 [P1] and the patent record [P2] support
    "expired by March 2017"; ATSC's boilerplate still warns of patents [P5].
  - Trademark wording (see Constraints).
- Fit: the only 5.1 format every Roku TV can send over optical and ARC [S1]; no custom board;
  permissive or own code. The latency cost may make it fail BRIEF §2.2 for video, while still
  fitting music-from-TV-apps use.

### Formats kept out (status)

- **E-AC-3 (DD+):** its adaptive hybrid transform patent (US7516064) expired 2026-01-30 [P4], but
  whether any later E-AC-3 patent is live is unknown; out unless the owner amends BRIEF §2.3 after
  a patent check.
- **DTS (core):** a DTS patent (US7548853) is active to 2027-02-06 [P6]; out per BRIEF §2.3.
- **TrueHD, Atmos, MAT, DTS-HD, DTS:X:** out per BRIEF §2.3; the hub never advertises them in its
  capabilities or short audio descriptors.

## Comparison

| Criterion | A: stereo optical + ARC + CEC | B: A + 5.1 LPCM over eARC | C: A + AC-3 decode |
|---|---|---|---|
| Works on the owner's Roku TVs | Yes on every Roku-made TV series (each lists optical [S2]); partner-brand Roku TVs (TCL, Hisense and others) checked per model through the TV Needs item | Only eARC models, and only if they send multichannel LPCM (no source says they do) | Yes, optical and ARC carry 5.1 AC-3 [S1] |
| Discrete 5.1 | No (TV downmix) | Yes | Yes (lossy) |
| TV-remote volume, autoplay | Yes (kernel CEC) | Yes | Yes |
| Added latency | Lowest (BRIEF §5.7: 15-35 ms, `ASSUMED`) | As A | +32 ms minimum (computed) plus TV encode (`ASSUMED`) |
| Patent or licence exposure | None | None | AC-3 patents expired per Dolby's S-1 and patent records |
| BRIEF §2.3 change | No | No | Yes (allow AC-3) |
| Custom board | No | **Yes** (SiI9437), or an unpriced module | No |
| Hardware per TV room | US$54.90 HAT (or `ASSUMED` US$10-20 module), +US$109.99 for ARC, CEC US$0-48.08, plus a hub | A plus a custom eARC board | Same as A |
| Effort | Goal 13 as written | Large: board, driver, multi-lane capture | Medium-large: decoder and fixtures |

## Recommendation

**Recommendation:** Option A (stereo LPCM from optical and ARC, with CEC through the Linux kernel API on a Linux theater hub) as the committed scope, because it works on every Roku-made TV and, `ASSUMED` until the Needs item answers, on partner-brand Roku TVs with an optical output, with bought parts and no codec, patent or custom-board question; AC-3 (C) and eARC (B) are not built until the owner's TV answers and a bench measurement justify them.

Why: A is what BRIEF §5.7 asks for first, the If-deferred cell, and the only option whose inputs
are all known today. It costs goal 13 as written plus, per TV room, a hub and about US$55-215 of
bought modules. What the owner gives up for now is discrete 5.1 from the TV: a theater set plays
2.0 with a sub (bass management), surrounds idle or on an upmix. The two ways to 5.1 each carry an
unknown only the owner or the bench can remove:

- **C (AC-3)** is the realistic 5.1 path for Roku TVs and has the cleanest legal footing of any
  surround codec (Dolby's own filing puts the Dolby Digital patents' end at 2017). Its cost is
  latency. Suggested owner choice at Checkpoint K: allow AC-3 in principle (amend BRIEF §2.3's
  wording) and let goal 13 build the decoder as a pure library with fixtures, off per room until a
  bench report shows the AC-3 path inside BRIEF §2.2's lip-sync bound. If the owner prefers the
  smallest scope, C is simply not built.
- **B (eARC 5.1 LPCM)** stays out: no source shows a Roku TV sending multichannel LPCM, and it
  needs a custom board (a departure from devices' rules that would need its own proposal).

For K72: the LCR set's hidden hub is the Option A hub. The soundbar's "eARC + optical" becomes
"optical + an HDMI port for ARC through a bought extractor and CEC"; a real ARC/eARC audio input
inside the bar needs the custom board of B and is not designed until B is approved.

## If the owner defers

Later goals build "Stereo PCM from optical and ARC only": goal 13 builds optical and ARC capture on
the Linux hub, TV-clock rate matching, CEC volume and power through the kernel API, TV autoplay,
2.0 plus sub bonding and the A/V trim; goal 13's line D lists eARC 5.1 and AC-3 as not built; goal
26 packages the soundbar and LCR within that scope. The cost: no discrete surround from the TV
until a later proposal, and 5.1 channel maps are tested only with synthetic sources.

## Open inputs

- The three TVs' makers (Roku-made or a partner brand such as TCL or Hisense, since [S2] covers
  only Roku-made TVs), models, eARC or ARC port, optical output and S/PDIF-and-ARC menu options: the
  goal-1 Needs item "The three TVs: model, eARC port, optical out and audio menu". Until answered
  the TV is a parameter marked `ASSUMED` (§0.8).
- Whether any of the TVs sends multichannel LPCM over eARC (only matters if B is ever wanted): a
  bench probe with an eARC sink the owner has, if any.
- `ASSUMED` values: the DIR9001 module price (US$10-20); the PCM-path latency (20-30 ms, BRIEF §5.7
  decomposition); the TV's AC-3 encoder delay (one frame or more); Roku TVs routing volume keys
  over CEC to a non-ARC audio system; an extractor box's CEC behaviour; IEC 61937 framing details;
  HiFiBerry Digi+ I/O recording limits; the SiI9437 price (US$4.10, 2026-09-29 snippet) and its
  datasheet access; the Circal module's price; P4 TDM-8 capture.
- The owner's calls: allow AC-3 (and amend BRIEF §2.3's wording) or not; whether a B-class custom
  board is ever wanted (it would need its own approved departure proposal).

## Sources

All read 2026-09-30 unless marked.

- [S1] Roku support, "Connect surround sound to your Roku TV", https://support.roku.com/article/connect-surround-sound-to-your-roku-tv
- [S2] Roku support, "Roku-branded TV product information", https://support.roku.com/article/roku-branded-tv-product-information
- [S3] Roku support, "Sound is out of sync", https://support.roku.com/article/sound-is-out-of-sync
- [P1] Dolby Laboratories, SEC Form S-1, filed 2004-11-19, https://www.sec.gov/Archives/edgar/data/1308547/000119312504200308/ds1.htm (sentence found by text search)
- [P2] Google Patents, US5890106A, https://patents.google.com/patent/US5890106A/en
- [P3] Google Patents, US6449368B1, https://patents.google.com/patent/US6449368B1/en
- [P4] Google Patents, US7516064B2, https://patents.google.com/patent/US7516064B2/en
- [P5] ATSC A/52:2018, Digital Audio Compression (AC-3, E-AC-3), https://www.atsc.org/wp-content/uploads/2021/04/A52-2018.pdf (downloaded, text extracted with pypdf)
- [P6] Google Patents, US7548853B2, https://patents.google.com/patent/US7548853B2/en
- [H1] Lattice, HDMI 2.1 eARC TX/RX (SiI9437, SiI9438), https://www.latticesemi.com/en/Products/ASSPs/HDMI21eARC
- [H2] ITE, IT6620, https://www.ite.com.tw/en/product/cate1/IT6620
- [H3] Circal Engineering, eARC/ARC audio receiver module, https://www.circalengineering.com/earc-arc-audio-receiver-module.html
- [H4] OREI, BK-931 audio extractor, https://www.orei.com/products/8k-hdmi-or-earc-audio-extractor-bk-931
- [H5] TI, DIR9001 product page, https://www.ti.com/product/DIR9001
- [H6] TI, DIR9001 datasheet SLES198A, https://www.ti.com/lit/ds/symlink/dir9001.pdf
- [H7] HiFiBerry, Digi+ I/O, https://www.hifiberry.com/shop/boards/hifiberry-digi-io/
- [H8] Pulse-Eight, USB-CEC adapter, https://www.pulse-eight.com/p/104/usb-hdmi-cec-adapter
- [H9] Raspberry Pi 5 product page, https://www.raspberrypi.com/products/raspberry-pi-5/
- [K1] Linux kernel docs, HDMI CEC admin guide, https://docs.kernel.org/admin-guide/media/cec.html
- [K2] Linux kernel docs, CEC_ADAP_G/S_LOG_ADDRS, https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-log-addrs.html (and cec-intro.html)
- [R1] Raspberry Pi whitepaper RP-009699-WP-1, as quoted in `verify-theater-platform.md` (read 2026-09-29, not re-read)
- [L1] `LEAD`: web search "Roku TV eARC multichannel PCM" results (Roku community thread 677253, now redirecting; Sonos community "Stereo PCM only from eARC with PS5"; quadraphonicquad thread), titles and summaries only
- [L2] `LEAD`: web search "eARC receiver board I2S output 8 channel" results (diyaudio threads, minidsp forum), titles and summaries only
- Planning research sources carried without re-reading: research-theater.md [C1] libcec licence, [O1] oxideav-ac3 README, [X1] ESP32-S3 I2S TDM limits, [X2] P4 TDM forum lead, [P8] Dolby trademark terms, [T1]-[T4] lip-sync standards (all read 2026-09-29, several as snippets).

## What was read

- Files: `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`,
  `/cache/tmp/chorus-g1/prompt-PB.md`, `/cache/tmp/chorus-g1/needs-items-1.md` (the TV item);
  [`.claude/goals/2026-09-chorus.md`](https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md) (§0.8, §0.9, §0.11, §0.13, §1, §1.1, §1.2, §2,
  §3.2-3.4, §5, §14, §17, §20, §30); [`.claude/goals/2026-09-chorus-research/`](https://github.com/NSchatz/chorus/tree/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus-research)
  `research-theater.md` (whole), `verify-theater-platform.md` (whole), `verify-ha-casting.md`
  (whole), `research-casting-decoders.md` (header and §4-§7); `/cache/wt/chorus/chorus/baseline/BRIEF.md`
  §2, §3, §5.7, §6, §8, §12.
- URLs: every entry in Sources marked read 2026-09-30, plus Wikipedia "Dolby Digital" raw wikitext
  (to find the SEC filing URL), DigiKey SII9437CNUC (HTTP 403), HiFiBerry limitations page (404),
  and two web searches (listed as [L1], [L2]).
- No GPL or LGPL source file was opened (no libcec, liba52, libdca, FFmpeg, ESP32 CEC libraries
  or kernel `sound/` or CEC driver source); only kernel documentation pages. No reciprocal
  hardware design file was opened.
