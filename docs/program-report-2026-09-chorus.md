# Program report: 2026-09-chorus

The closing report of the 2026-09-chorus program (program brief
`535ed28:.claude/goals/2026-09-chorus.md`, section 31 item 6; harness project 37,
task 263). Written 2026-10-07 at `origin/main` 0199d02, the `v0.2.0` tag. Every claim names
the path, run or command it comes from.

## What was built and where

| Component | What it is | Path | Main doc |
|---|---|---|---|
| chorus-server | The server: PCM ingest, chunking on one monotonic timeline, the host contract | `crates/server` | `docs/control-plane.md` |
| chorus-protocol | Wire protocol: framing, message catalog, encoder and decoder | `crates/protocol` | `docs/protocol.md` |
| chorus-sync | Clock sync: virtual clocks, jitter models, servo, deterministic simulator | `crates/sync` | `docs/verification-record.md` |
| chorus-audio | PCM ingest and chunking, stream format, the monotonic server timeline | `crates/audio` | `docs/sound-2.md` |
| chorus-audio-path | The committed list of the audio and timestamp path, and its checks | `crates/audio-path` | `docs/sound-2.md` |
| chorus-alsa | ALSA playback and capture for the Linux client, bound at run time | `crates/alsa` | `docs/sound-2.md` |
| chorus-client-linux | The Linux endpoint: receive, buffer, play out through ALSA | `crates/client-linux` | `docs/linux-endpoint.md` |
| chorus-control | Control catalog: versioned JSON, server-authoritative zone state, fanout | `crates/control` | `docs/control-plane.md` |
| chorus-controls | Controller model (controls, status LED, mic gate); Rust twin of `firmware/src/controls.c` | `crates/controls` | `docs/hardware/controls.md` |
| chorusctl | Command-line client of the control API | `crates/ctl` | `docs/chorusctl.md` |
| chorus-decode | Server decoders (MP3, FLAC, Vorbis, Opus, ALAC, WAV, L16), resampler, remix | `crates/decode` | `docs/decoders.md` |
| chorus-opus-sys | libopus from `third_party/opus`, built as the endpoint builds it | `crates/opus-sys` | `docs/decoders.md` |
| chorus-dsp | Biquads, LR4 crossovers, limiter, loudness, bass management, the endpoint chain | `crates/dsp` | `docs/dsp.md` |
| chorus-fetch | HTTP/1.1 and TLS fetcher, ICY stripping, packed-MP3 HLS | `crates/fetch` | `docs/streams.md` |
| chorus-discovery | mDNS and DNS-SD advertise and find | `crates/discovery` | `docs/control-plane.md` |
| chorus-upnp | UPnP AV renderer: SSDP, SOAP, GENA, DIDL-Lite, AVTransport | `crates/upnp` | `docs/upnp.md` |
| chorus-mqtt | Publish-only MQTT 3.1.1 | `crates/mqtt` | `docs/mqtt.md` |
| chorus-cec | HDMI-CEC for the Linux hub: codec, Audio System role, kernel adapter, fake bus | `crates/cec` | `docs/cec.md` |
| chorus-schedule | Civil time from TZif, weekly windows, alarms, sleep timers, ramps, chimes | `crates/schedule` | `docs/chimes.md` |
| chorus-wakeword | microWakeWord frontend and an integer TFLite interpreter | `crates/wakeword` | `docs/home-assistant.md` |
| chorus-soloist, chorus-soloistd, chorus-soloist-fake | Spotify Soloist: the API model and receiver pool, the per-receiver supervisor, and a test fake (never shipped) | `crates/soloist`, `crates/soloistd`, `crates/soloist-fake` | `docs/soloist.md` |
| chorus-hostctl, chorus-hostprobe | The container scheduling contract; host evidence (wakeup jitter, receive timestamps) | `crates/hostctl`, `crates/hostprobe` | `docs/sound-2.md`, `docs/bench-packet.md` |
| chorus-measure | Measurement harness: capture ingest, inter-device lag, free-run drift, reports | `crates/measure` | `docs/measurements/README.md` |
| ESP32-S3 firmware | The C endpoint core, the ESP-IDF glue and four board profiles | `firmware/src`, `firmware/main`, `firmware/boards` | `firmware/README.md`, `docs/firmware-updates.md` |
| Home Assistant integration | `custom_components/chorus` with its `_aiochorus` client and a dashboard | `integrations/homeassistant` | `docs/home-assistant.md` |
| The app | Lit 3 and esbuild installable web app, embedded in chorus-server | `web/` | `docs/app.md` |
| Server container, Linux endpoint package, Soloist image | Container, packaging and compose files | `deploy/`, `deploy/endpoint`, `deploy/soloist` | `deploy/README.md`, `docs/linux-endpoint.md`, `docs/soloist.md` |
| Release | `tools/release.sh` and the CI workflows that cut a release and push the images | `tools/release.sh`, `.github/workflows/` | `docs/release.md` |
| Gate and conventions | `tools/gate.sh`, `tools/changed.sh`, 26 `tools/conventions/check-*.sh` | `tools/` | `docs/conventions.md` |
| Bench, soak and measurement tools | Bench scripts, the house soak, the emulator runs | `tools/bench`, `tools/house-soak`, `tools/measure`, `tools/qemu` | `docs/bench.md`, `docs/bench-packet.md` |
| Shared fixtures | The vectors the Rust and C cores share | `fixtures/` | `fixtures/README.md` |
| Speaker designs | The compact speaker, the two-way, the subwoofer and the LCR set (and the controls, voice mic and multichannel notes) | `docs/hardware/` | `docs/hardware/*.md` |
| Measurements | 17 reports, their raw data and a baseline | `docs/measurements/` | `docs/measurements/README.md` |
| The parity checklist | Every item the program was asked for, with its state | `docs/parity.md` | `docs/conventions.md` rule 26 |

The release: [v0.2.0](https://github.com/NSchatz/chorus/releases/tag/v0.2.0), 21 assets
(`gh release view v0.2.0`), published 2026-10-06 by CI from 0199d02; the images
`ghcr.io/nschatz/chorus-server:0.2.0` and `ghcr.io/nschatz/chorus-soloist:0.2.0` were pushed by
[run 37546458284](https://github.com/NSchatz/chorus/actions/runs/37546458284).

The speaker designs on devices `origin/main` (section 31 item 4): every build's record is
there, so no devices task was needed.

| Build | devices path | Landed in devices |
|---|---|---|
| compact | `projects/chorus-compact/v1/acoustics.md` | 5673283 (devices PR 126, the move to `projects/`) |
| two-way | `projects/chorus-twoway/v1/acoustics.md` | 5673283 |
| subwoofer | `projects/chorus-sub/v1/acoustics.md` | 5673283 |
| LCR set | `projects/chorus-lcr/v1/acoustics.md` | 4858734 (devices PR 128) |
| rack amp, soundbar | none: not designed (`docs/decisions/0231-no-rack-amp-and-no-soundbar.md`, P13) | - |

Command: `git -C devices ls-tree -r --name-only origin/main | grep 'chorus.*acoustics'`.

## Test counts and runtimes

Counted in the source at 0199d02 (nothing built):

| Area | Count | Command |
|---|---|---|
| Rust, all crates | 1926 tests | `rg -c '#\[(tokio::)?test' crates`, summed |
| Rust, the largest crates | server 471, client-linux 198, control 197, upnp 144, measure 126, fetch 105, sync 104, protocol 81, dsp 71 | the same, per crate |
| Firmware C host tests | 22 suites, 177 sections, 1170 checks written | `ls firmware/tests/test_*.c`; `rg -c 'chorus_section\('` and `rg -c 'chorus_check\('` over them |
| Home Assistant (Python) | 256 tests | `rg -c '^\s*(async )?def test_' integrations` |
| Web unit | 299 tests in 26 files | `rg -c '^\s*(test\|it)\(' web/test/*.test.js` |
| Web live, browser smoke | 54 and 6 (CI prints the same) | the same over `web/live/*.live.js`, `web/smoke/app.spec.js` |
| Conventions checks | 26 scripts | `ls tools/conventions/check-*.sh` |

Runtimes, from CI (`gh run view <id> --json jobs`, `gh run view <id> --log`):

| Run | Commit | Result | Wall-clock |
|---|---|---|---|
| [Nightly full gate 37546457877](https://github.com/NSchatz/chorus/actions/runs/37546457877) (by hand) | 0199d02 | success | 49 min 59 s; `make gate` printed `gate: PASS, wall-clock 2808.5s` |
| [Release v0.2.0 37546458410](https://github.com/NSchatz/chorus/actions/runs/37546458410) | 0199d02 | success | 8 min 39 s (`make release` 8 min 18 s, the GitHub release 14 s) |

The slowest gate steps in that nightly: determinism 470.1 s, test 443.3 s, ota-qemu 398.1 s,
endpoint-packages 196.4 s, firmware-check 149.4 s, the four firmware images 113 to 116 s each,
image 115.9 s, ha-test 111.3 s. A pull request runs only `make gate-changed` (ADR 0140). The one
scheduled nightly so far (run 37479085171, c78a008) failed; the run above, on the release
commit, passed.

## Needs items

Steps only the owner can do, safety first, then what unblocks the most. Each chorus step is a harness item
(`goals item list`) or an owner decision task (`goals task list chorus`); none is a GitHub issue.

Safety, electrical and licence:

1. **The Soloist image's LGPL and GPL-2 source** (task 316). ADR 0131 item 9 left it open because
   nothing was distributed; the repository is now public and v0.2.0 publishes the image (the
   release tarball and `ghcr.io/nschatz/chorus-soloist:0.2.0`, which an anonymous pull reads).
   Attach the 47 source packages, accept the snapshot.debian.org pointer, or stop publishing it.
2. **First power-up of the amplifier, bench S7** (item 69): a current-limited supply, and the
   bridge-tied outputs never joined to ground (`docs/bench-packet.md`, S7). Flashing stays behind
   `CHORUS_OWNER_AT_BENCH=1`, and no eFuse is burned (`docs/bench.md`).
3. **OTA rollback on a board, bench S10** (item 63; `docs/firmware-updates.md`).
4. **Mains wiring for the subwoofer**: a finished plate amp or a bare board with its mains
   supply (`docs/hardware/subwoofer.md`). The two-way's is settled: an external 24 V adapter
   (devices `projects/chorus-twoway/v1/log.md`, "Power"). Not a chorus item: devices' plan for
   those builds (devices roadmap project 7, open) carries it.
5. **Credentials**: the Spotify Soloist API key (item 59); the optional Mosquitto user (item 62).

What unblocks the most (parity items in brackets, `docs/parity.md`):

1. Buy the bench on P4's final list (item 79): every bench session [8-2 to 8-7, 8-9, 8-10].
2. Bench S0, the bench machine (item 76): S1 to S4, S7, S8, S12 [8-2 to 8-6, 8-9].
3. Bench S2, RIG-3 (item 74) [8-3; it gates S3, S4 and S7].
4. Bench S1, SOUND-2 (item 75) [8-2].
5. Bench S3, the SYNC-4 hour (item 73) [8-4].
6. Bench S5, a DevKitC boot (item 71) [8-5; it gates S6, S9, S10, S11].
7. Bench S7 (item 69) [8-5]; S6, the chip's decode and DSP cost (item 70).
8. Bench S4, the three-day soak (item 72) [8-6].
9. Bench S8, TV capture (item 68) with the TVs' models and menus (item 78) [8-9].
10. Bench S10 and S11 (items 63, 61) [8-10]; optional S9 Wi-Fi provisioning (item 65) [8-7].
11. Bench S12 and S14, real UPnP control points (items 60, 55); S13, a Soloist receiver
    measured (item 57).
12. The host probes: the wakeup-jitter probe (item 66) and the Linux endpoint package on a Pi
    (item 67); the rack measured (item 77).
13. The app phone checks, parts 1 and 2 (items 54, 53).

## Owner's steps for v0.2.0 and the program

Section 31 item 5, each filed in the harness:

| Step | Where it is filed | State |
|---|---|---|
| Re-pin homelab's chorus-server and chorus-soloist to the 0.2.0 digests, merge and apply | item 130 | open |
| Make the ghcr packages public | none needed | done: an anonymous pull of both `0.2.0` manifests answers 200 |
| Push the `g17-a835a5f` images | item 56 | served by CI ([run 37474179001](https://github.com/NSchatz/chorus/actions/runs/37474179001)); the owner may tick it off |
| Place the Soloist build and renew it every 90 days | item 58 | open |
| Create and place the Soloist API key | item 59 | open |
| Add the Home Assistant integration | item 117 | open |
| The upstream draft (the Home Assistant core submission) | task 26 | decided: not yet (`docs/decisions/0227-the-home-assistant-integration-is-not-submitted-upstream-yet.md`); reopened only by the owner |
| The Soloist image's source question | task 316 | open |
| A follow-up goal for open requests | none needed | each open request is a ready chorus task (below) |

## Proposals awaiting the owner

| Proposal | State | Filed |
|---|---|---|
| P11, concurrent streams | accepted 2026-10-05 (`docs/proposals/P11-concurrent-streams.md`) | - |
| P12, enclosures per speaker class (K88) | PROPOSED (`docs/proposals/P12-enclosures.md`) | task 317 |
| P13, the rack amp's zones (K70, K72, K74, K96): zero zones, with ADR 0231 | PROPOSED (`docs/proposals/P13-rack-amp-zones.md`) | task 318 |
| P14, the devices seam | decided at Checkpoint K, then overtaken: the designs landed through devices' own pull requests 120, 121 and 128 (its Outcome line); not awaiting the owner | task 114 adds its dated note |

There is no proposal after P14. P1 to P10 were decided at Checkpoint K or later (each file's
Status line).

## Open requests

The requests ledger. Requests were issues in NSchatz/goals until the harness replaced them
(goals `DECISIONS.md` item 16); the list is `gh issue list -R NSchatz/goals --label to:chorus`
and `--label from:chorus`, all states.

Addressed to chorus:

| Request | What | State |
|---|---|---|
| goals #230 (from goals) | Throwaway build target dirs on tmpfs; image builds under the heavy lock | DONE: chorus PR 134 (b5eb6ca), `tools/build-dir.sh` |
| goals #191 (from devices, dev-14) | The six `acoustics.md` records devices' Audio lane waits on | DONE: four landed (table above); the rack amp and soundbar records are DECLINED as not designed (ADR 0231, P13); devices task 283 (ready) will record them as dropped |
| harness task 201 (from devices, for devices task 50) | The compact's board profile for its separate modules | DONE: chorus PR 230 (747bde4), board profile `devkitc-s3-louderhat-wired` |
| harness task 320 (from devices, for devices task 274) | The two-way's board profile and `docs/hardware/twoway-speaker.md` v2 for its separate modules | ACCEPTED: a ready chorus task in devices roadmap project 7 |
| harness task 10 (from goals) | `CLAUDE.md` to the essentials, and `CARD.md` | DONE (`CARD.md`) |
| harness task 114 (from devices) | Rename `builds/chorus-*` to `projects/chorus-*/v<n>/` in chorus docs | DEFERRED: a ready chorus task; it waits on devices task 112 |

Filed by chorus:

| Request | What | State |
|---|---|---|
| goals #222 (to goals) | A heavy-lock waiter held a slot | done by goals (#223, #224) |
| goals #110 (to devices, chorus-1) | chorus's speaker designs in devices | accepted by devices; its roadmap projects 6, 7 and 8 build them (open) |
| harness tasks for siblings | homelab 181, 206, 207 (re-pins, the speaker network); devices 55, 56, 244 (the records); inventory 1 (driver records) | all done (`goals task list <repo>`) |

The open requests addressed to chorus are tasks 320 (accepted) and 114 (deferred), each already
a ready chorus task, so no follow-up goal is needed for them.

## Follow-ups

- Tasks 114 and 320: the devices path rename and the two-way's board profile, above.
- Tasks 316 to 319: the owner decision tasks in this report.
- `docs/pins.md`: one change for the pins that are behind, and a proposal for the next Rust.
- K67: the compact image's GPIO, LED and I2S-input binding is not written (`docs/parity.md`).
- The subwoofer's per-endpoint LFE high-pass and voltage limit, and a continuous phase control
  (ADRs 0229, 0233).
- Why CI and workstation image digests differ is not known (ADR 0222).
- What the bench packet does not have yet: a reader for the GPIO marker capture, a WIFI-7 run,
  a grader for S8, a script for the production-host jitter run (`docs/bench-packet.md`).
- Alarms: a bound on an unanswered start, snooze as a command (ADR 0129).
- The "Follow-ups" sections of the decision records (`grep -l '^## Follow-ups' docs/decisions`).

## Worker variables for the owner to remove

Program brief section 0.12: the committed `.claude/settings.json` keeps the program's worker
variables for the owner to remove after the finale (task 319 asks):

- `env.CARGO_BUILD_JOBS = "12"`
- `env.CMAKE_BUILD_PARALLEL_LEVEL = "12"`
- `env.IDF_PY_BUILD_JOBS = "12"`
- `autoMemoryEnabled: false`
