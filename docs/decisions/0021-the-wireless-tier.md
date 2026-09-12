# 0021: the wireless tier

- Status: decided
- Recorded by: WIFI-7
- Implemented in: `config/transport.conf`, `firmware/config/endpoint.conf` (the
  link section), `firmware/src/wifi.c`, `crates/control/src/transport.rs`,
  `crates/measure/src/jitter.rs`, `docs/wireless-expectations.md`

## The question

Until this phase a zone was wired or hoped for. The endpoint set no Wi-Fi power
save mode at all, so it inherited the platform default; there was one global
playout latency with no notion of a looser wireless one; and no saved
measurement anywhere said what Wi-Fi costs this system. A house cannot be told
what a wireless room is promised, because nothing had said it.

## Decision

**A zone is DECLARED wired or wireless. A wireless zone is held to the multiroom
bound and gets the deeper buffer that bound is affordable with, the endpoint
SETS its radio's power save mode from committed configuration and reports the
mode it set, and every servo constant SYNC-4 fixed stays where SYNC-4 put it.**

The one thing this must not become is a servo tuned against Wi-Fi. BRIEF.md
section 9 is explicit about the response to Wi-Fi jitter: "bigger buffers or
wired-only for that zone; never chase Wi-Fi with servo aggression". So the whole
of this phase's answer to a wireless link is buffer depth and playout latency,
and `config/sync.conf` is byte-identical to what SYNC-4 left.

## Where every committed value came from

Nothing below is a measurement. Two criteria of this phase measure a wireless
zone, both need hardware this pipeline does not have, and both are recorded as
NOT passed in `docs/verification-record.md`.

### The bounds, in `config/transport.conf`

| value | where it came from |
|---|---|
| `wired_bound_us = 500` | BRIEF.md section 2.2, "Stereo pair / same room, acceptable < 0.5 ms". Adopted, not derived. It is the bound SYNC-4 was written against and this phase does not move it |
| `wireless_bound_us = 5000` | BRIEF.md section 2.2, "Multiroom music, different rooms, acceptable < 5 ms", which is also the phase's own assertion word for word. Adopted, not derived |
| `transports = wired wireless` | BRIEF.md section 5.4's two endpoint tiers, and section 3.2's "Wi-Fi is a convenience tier with bigger buffers and looser expectations" |
| `default_transport = wired` | This phase's own choice. An undeclared zone must not inherit the looser tier by silence: a zone nobody has thought about is a wired zone held to the tighter bound, which fails safe. AC-6 is that choice made checkable |

Both bounds ARE copied from BRIEF.md, and this record is where that is said. The
reason it is legitimate here and was not for the servo constants SYNC-4 fixed is
the difference between a TARGET and an OUTPUT. A servo gain is an output of
tuning against a rig, so a borrowed one is a number nobody measured pretending
to be one somebody did. A bound is the owner's requirement, stated in the
owner's document, and copying it is the only honest thing to do with it.

### The wireless buffer policy, in `config/transport.conf`

| value | where it came from |
|---|---|
| `wireless_playout_latency_us = 500000` | BRIEF.md section 5.2's recommendation: "start with uncompressed PCM over TCP unicast, ~20 ms chunks, ~500 ms default buffer for music". The brief recommends it for the buffered music path in general; this phase applies it to the tier that needs it and leaves the wired tier at the 180 ms SYNC-4 measured against |
| `wireless_max_us = 900000` | Derived, and the derivation is the one relation `crates/client-linux/src/config.rs` enforces: the playout latency has to sit strictly below the maximum bound, because at or above it the buffer is discarding what the loop is waiting for. 900 ms leaves 400 ms of headroom above the 500 ms latency, which is the same proportion the wired numbers carry (300 ms bound above a 180 ms latency is 120 ms, or two thirds of a latency; 400 ms above 500 ms is four fifths) |
| `wireless_device_target_us = 300000` | Derived from the same relation at the other end: the playout latency has to sit strictly ABOVE the device delay target, or there is no queue left to hold the difference. 300 ms is the wired maximum bound, which is the deepest device queue this repository has ever asked an endpoint to hold |
| `wireless_min_us = 120000` | The wired start fill. An occupancy floor has to be above zero (a floor of zero makes an empty buffer "inside the bounds", which is the state an underrun comes from) and far enough below the start fill that a run has somewhere to sit |
| `wireless_start_fill_us = 400000` | Strictly between the bounds, and below the playout latency so the first frame is written before the latency has elapsed rather than after it |

The five numbers are checked as a set rather than one at a time, and the check is
not a restatement: `crates/client-linux/tests/wireless_policy.rs` builds a real
`ClientConfig` from them and runs the client's OWN `validate()` over it. A policy
the shipped client would refuse to start with is a policy this repository cannot
hold a zone to, and that is the failure the check exists to catch.

### The endpoint's link, in `firmware/config/endpoint.conf`

| value | where it came from |
|---|---|
| `link_transport = wireless` | This phase exists to give the endpoint a wireless tier. A wired endpoint sets `wired`, brings no radio up and has no wireless claim made about it |
| `link_wifi_power_save = none` | The carried ESP-IDF guide, quoted below. `WIFI_PS_NONE` "minimizes the delay in receiving Wi-Fi data in real time", and BRIEF.md section 6 lists `esp_wifi_set_ps(WIFI_PS_NONE)` as "mandatory on Wi-Fi" |
| `link_wifi_coexistence = no` | Declared rather than probed, exactly as `board_octal_psram` is, and for the same reason: it is a fact about a board, and a pin map and a radio policy are both checked before an image exists |
| `link_wifi_ssid = unknown`, `link_wifi_secret = unknown` | Not declared, and deliberately never declared here. A network name and a secret are facts about somebody's house, and a secret committed once is in a git history no rotation reaches |

## What the carried source says

`esp_wifi_set_ps` semantics are quoted from the ESP-IDF Wi-Fi performance and
power save guide for ESP32-S3, at
`https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/wifi-driver/wifi-performance-and-power-save.html`.
Three sentences of it are load-bearing here, all from its Station Sleep section:

1. **The default this phase refuses to inherit.** "The default Modem-sleep mode
   is WIFI_PS_MIN_MODEM."
2. **Why the mode is set rather than left.** "Call `esp_wifi_set_ps(WIFI_PS_NONE)`
   to disable Modem-sleep mode entirely. Disabling it increases power
   consumption, but minimizes the delay in receiving Wi-Fi data in real time.
   When Modem-sleep mode is enabled, the delay in receiving Wi-Fi data may be
   the same as the DTIM cycle (minimum power-saving mode) or the listening
   interval (maximum power-saving mode)." The DTIM cycle is the AP's to choose,
   not this endpoint's, which is what makes it a delay no servo can correct.
3. **Why setting it is not the same as it being in effect.** "Note that in
   coexist mode, Wi-Fi will remain active only during Wi-Fi time slice, and
   sleep during non Wi-Fi time slice even if `esp_wifi_set_ps(WIFI_PS_NONE)` is
   called."

The same section fixes the ORDER the binding uses: the mode is set "after
calling `esp_wifi_init()`", and "When station connects to AP, Modem-sleep will
start. When station disconnects from AP, Modem-sleep will stop." So the mode is
set between init and the join, which is where `firmware/main/esp_hal.c` sets it.

### The release gap, stated rather than papered over

The page carried into this work is the **`stable`** revision of that guide.
`firmware/config/endpoint.conf` pins `espidf_version = v5.3`. All three
quotations above are present in the carried text, and the carried page is not a
page about the pinned release: `stable` moves when Espressif tags a release, and
nothing in the carried document names a version. Two things follow and both are
recorded rather than assumed.

- What this phase relies on is API SEMANTICS that have been stable across the
  v5 line: the four `wifi_ps_type_t` values, the default being
  `WIFI_PS_MIN_MODEM`, and the coexistence caveat. Nothing here depends on a
  sentence that is new, and nothing depends on a behaviour change between
  releases.
- The gap is not closed by this phase and is not pretended to be. An implement
  session in this pipeline has no network, so re-fetching the v5.3 revision of
  the page was not available; the honest record is that the wording quoted is
  `stable`'s wording. Whoever next brings an image up against a real v5.3
  toolchain should read that release's own page and, if a sentence has moved,
  change this record deliberately.

## What is NOT decided here

- **The control catalog.** `CATALOG_VERSION` stays 1, `fixtures/control/` is
  byte-identical and `docs/control-plane.md`'s message table is unextended. A
  zone's transport is declared where the zone set is declared, on the server's
  command line, because that document already states the set of zones is
  configured and not commanded. A control message that tries to change a
  transport is refused, which is that rule extended to the new field rather than
  a new rule.
- **The Linux client's own radio.** Nothing here turns Linux power save off:
  that needs privilege and a different authority, and the criterion's cited
  authority is the ESP-IDF default. A Linux endpoint in a wireless zone still
  gets the wireless buffer policy, and its report says its power-save mode is
  unknown rather than claiming one.
- **Where the wireless buffer's bytes live.** The endpoint tree prohibits
  external-RAM placement outright, so the policy is stated in depth and latency
  and a PSRAM buffer would arrive as a deliberate change to
  `firmware/endpoint-units.conf` rule 3 rather than as a quiet attribute.
- **Any servo constant.** None moved. `config/sync.conf` is what SYNC-4 left.
- **Compression, FLAC and multicast on Wi-Fi.** BRIEF.md lists all three as open
  and this phase asks for none of them.
