# The parity checklist

Every item the program was asked to deliver, with one state each: BRIEF.md section 8 (the ten
roadmap phases and the program's phases of section 8.1) and the owner's decisions K30, K31 and
K57 to K94, read from the program brief at commit 535ed28
(`.claude/goals/2026-09-chorus.md`, section 1; brief section 31 item 1). Written 2026-10-06 for
goal 27.

How to read it:

- **State** is one of `done`, `partial`, `deferred`, `dropped`, `not started`. `done` means built
  and tested on fakes and the simulator (K50): real-hardware measurements are never success
  criteria here, they are the owner's bench sessions. A roadmap phase whose own "success looks
  like" line is a hardware measurement is `partial` until a report with `Source: hardware` is
  under `docs/measurements/` (BRIEF.md section 3.1 rule 3).
- **Evidence** names one or more repository paths, each in backticks; the decision record or
  doc that says what was built, and the code or measurement behind it.
- **Reason or follow-up** is required for every state but `done`.
- `tools/conventions/check-parity.sh` (`docs/conventions.md` rule 26) holds the table: every
  item of the list above present once, each with a known state, at least one evidence path that
  exists, and a reason for any state but `done`. It prints the count per state.

## BRIEF.md section 8: the roadmap

| Item | What | State | Evidence | Reason or follow-up |
|---|---|---|---|---|
| 8-1 | Foundation: repo, CI, protocol core with cross-language fixtures, the sync simulator | done | `docs/decisions/0006-sync-simulator-and-servo.md`; `fixtures/`; `docs/measurements/sim-house-8-rooms.md` | - |
| 8-2 | First sound: server and Linux client playing timestamped PCM via ALSA | partial | `docs/sound-2.md`; `docs/verification-record.md`; `docs/bench-packet.md` | Clean audio and a plausible DAC delay need a sound card; the bench packet's SOUND-2 session is the owner's |
| 8-3 | Measurement rig and a free-run drift baseline | partial | `docs/decisions/0013-the-measurement-rig.md`; `docs/measurements/rig3-free-run-noiseless-fixture.md`; `docs/bench-packet.md` | The rig is proven on fixtures; the free-run baseline between two clients is the owner's RIG-3 bench session |
| 8-4 | Synchronization: sub-millisecond error on two wired Linux clients | partial | `docs/decisions/0014-the-sync-loop-on-the-real-path.md`; `docs/measurements/sim-house-8-rooms.md`; `docs/bench-packet.md` | Held to its targets in simulation; the measured error on two wired clients is the owner's SYNC-4 bench session |
| 8-5 | Embedded bring-up: ESP32-S3, I2S, TAS5825M, sync measured against Linux | partial | `docs/decisions/0015-the-esp32-s3-endpoint.md`; `docs/decisions/0065-the-embedded5-bring-up-and-the-gpio-marker.md`; `firmware/` | Built, tested on the host and booted under the emulator; the bring-up and disconnect abuse on a board are the owner's bench session |
| 8-6 | Product hardening: groups, volume, control plane, UI, discovery, reconnect storms, multi-day soak | partial | `docs/decisions/0075-control-catalog-v2.md`; `docs/decisions/0078-the-house-soak.md`; `docs/measurements/house-soak-8-rooms.md` | The house soak is one hour on ALSA null; a multi-day soak on the installed house is the owner's |
| 8-7 | Wi-Fi tier: one wireless endpoint characterized honestly | partial | `docs/decisions/0024-the-wireless-tier.md`; `docs/wireless-expectations.md`; `docs/verification-record.md` | AC-2 and AC-3 need a board on the house's Wi-Fi and the rig; the owner's bench session |
| 8-8 | DSP: the filter library on both platforms; the active two-way demo | partial | `docs/decisions/0082-the-dsp-library.md`; `docs/dsp.md`; `docs/hardware/twoway-speaker.md` | The library is fixture-validated on both platforms; the two-way demo needs the built speaker, the owner's |
| 8-9 | TV path: wired, small buffer, FEC, stereo first, A/V calibrated | partial | `docs/decisions/0091-the-low-latency-wire.md`; `docs/decisions/0094-the-tv-relay-and-the-low-latency-endpoints.md`; `docs/cec.md` | Built and tested in software; the A/V calibration on a real TV is bench session S8 (`docs/decisions/0092-tv-fixes-autoplay-levels-stereo-fold-flakes-and-bench-s8.md`), the owner's |
| 8-10 | Fleet: OTA with rollback, MQTT and HA, dashboards, provisioning, whole-home rollout, long soak | partial | `docs/decisions/0111-ota-under-the-emulator.md`; `docs/measurements/ota-qemu-rollback.md`; `docs/firmware-updates.md` | Rollback is demonstrated under the emulator; the whole-home rollout and its long soak are the owner's |

## BRIEF.md section 8.1: the program's phases

| Item | What | State | Evidence | Reason or follow-up |
|---|---|---|---|---|
| 8.1-1 | Start-up, audit and proposals (goal 1) | done | `docs/audit/2026-09-audit.md`; `docs/proposals/` | - |
| 8.1-2 | Fixes and the gate (goal 2) | done | `tools/gate.sh`; `docs/decisions/0112-a-faster-gate-with-the-same-checks.md`; `docs/decisions/0140-ci-is-the-gate.md` | - |
| 8.1-3 | Conventions, licence and identity (goal 3) | done | `docs/conventions.md`; `LICENSE-MIT`; `LICENSE-APACHE`; `tools/conventions/check-identity.sh` | - |
| 8.1-4 | Reversals, loose ends, first release, deploy PR (goal 4) | done | `docs/release.md`; `deploy/README.md`; `docs/proposals/P3-speaker-network-homelab-pr.md` (the homelab repo has since merged it as its PR 264) | - |
| 8.1-5 | Protocol v2 (goals 5, 6) | done | `docs/protocol.md`; `docs/decisions/0039-the-v2-key-exchange.md`; `docs/decisions/0041-protocol-v2-framing.md`; `docs/decisions/0044-the-vendored-decoders.md` | - |
| 8.1-6 | Sound, rig and sync in software; bench packets (goal 7) | done | `docs/bench-packet.md`; `docs/measurements/sim-house-8-rooms.md`; `docs/decisions/0048-the-house-scale-simulation.md` | - |
| 8.1-7 | The embedded platform (goals 8, 9) | done | `docs/decisions/0057-board-profiles-and-the-wired-link.md`; `docs/decisions/0058-the-endpoint-playout-path.md`; `docs/decisions/0062-the-guarded-flashing-tool.md`; `docs/decisions/0064-the-tas5825m-register-map-and-bring-up.md` | - |
| 8.1-8 | The Linux endpoint tier (goal 10) | done | `docs/linux-endpoint.md`; `docs/decisions/0068-the-output-map.md`; `docs/decisions/0069-the-linux-endpoint-package.md` | - |
| 8.1-9 | Rooms and groups (goal 11) | done | `docs/decisions/0075-control-catalog-v2.md`; `docs/decisions/0076-the-schedule-runtime.md`; `docs/decisions/0077-stream-slots-and-one-event-writer.md` | - |
| 8.1-10 | DSP (goal 12) | done | `docs/dsp.md`; `docs/decisions/0081-per-room-sound-in-the-catalog-and-on-the-wire.md`; `docs/decisions/0083-room-correction-fitting.md`; `docs/decisions/0085-the-dsp-chain-on-the-endpoints.md` | - |
| 8.1-11 | The TV path (goal 13) | done | `docs/cec.md`; `docs/decisions/0087-cec-audio-system-on-the-hub.md`; `docs/decisions/0088-theater-maps-av-trim-and-tv-autoplay.md`; `docs/decisions/0094-the-tv-relay-and-the-low-latency-endpoints.md` | - |
| 8.1-12 | Fleet (goals 14, 15) | done | `docs/firmware-updates.md`; `docs/telemetry.md`; `docs/mqtt.md`; `docs/chorusctl.md` | - |
| 8.1-13 | Inputs (goals 16, 17) | done | `docs/upnp.md`; `docs/decoders.md`; `docs/soloist.md`; `docs/inputs.md` | - |
| 8.1-14 | The Home Assistant integration (goals 18, 19) | done | `docs/home-assistant.md`; `integrations/homeassistant/`; `docs/decisions/0138-the-home-assistant-integration.md` | - |
| 8.1-15 | Voice and announcements (goal 20) | done | `docs/decisions/0172-the-voice-run-and-the-run-scoped-mic-route.md`; `docs/decisions/0175-announcements-are-mixed-per-room-on-the-slots-grid.md`; `docs/decisions/0176-the-home-assistant-voice-satellite.md` | - |
| 8.1-16 | The app (goals 21, 22) | done | `docs/app.md`; `web/`; `docs/decisions/0181-the-web-app-stack.md` | - |
| 8.1-17 | Acoustic design tools (goal 23): the acoustics package lives in the owner's shared Python library; chorus reads its released v1.49.0 | done | `docs/hardware/compact-speaker.md`; `docs/decisions/0216-the-compact-speaker-drivers-and-alignment.md` | - |
| 8.1-18 | The speaker designs (goals 24, 25, 26) | done | `docs/hardware/compact-speaker.md`; `docs/hardware/twoway-speaker.md`; `docs/hardware/subwoofer.md`; `docs/hardware/lcr-set.md`; `docs/decisions/0234-goal-26-closed-no-rack-amp-or-soundbar-and-the-lcr-set-landed.md` | - |
| 8.1-19 | Finale (goal 27) | done | `docs/release.md`; `docs/parity.md`; `docs/program-report-2026-09-chorus.md`; `docs/decisions/0140-ci-is-the-gate.md` | - |

## K30 and K31: the parity features

| Item | What | State | Evidence | Reason or follow-up |
|---|---|---|---|---|
| K30-1 | Bonded sets: stereo pairs, theater bonding, per-endpoint channel maps | done | `docs/decisions/0075-control-catalog-v2.md`; `docs/decisions/0068-the-output-map.md`; `docs/decisions/0088-theater-maps-av-trim-and-tv-autoplay.md` | - |
| K30-2 | Alarms and sleep timer, server-owned, wall clock for scheduling only | done | `docs/decisions/0072-the-schedule-library.md`; `docs/decisions/0076-the-schedule-runtime.md`; `docs/decisions/0079-the-schedule-runtime-wired-into-the-server.md` | - |
| K30-3 | Line-in sharing to any group | done | `docs/decisions/0066-line-in-capture-as-the-source-role.md`; `docs/decisions/0129-stored-alarm-sources-line-in-sharing-and-streamer-inputs.md` | - |
| K30-4 | Tone and loudness EQ, night mode, speech enhancement | done | `docs/decisions/0081-per-room-sound-in-the-catalog-and-on-the-wire.md`; `docs/decisions/0085-the-dsp-chain-on-the-endpoints.md`; `docs/dsp.md` | - |
| K31-1 | Room correction with the phone's microphone | done | `docs/room-correction.md`; `docs/decisions/0083-room-correction-fitting.md`; `docs/decisions/0207-the-room-is-recorded-with-an-audio-worklet.md` | - |
| K31-2 | Voice: rooms as Assist satellites and announce targets | done | `docs/decisions/0176-the-home-assistant-voice-satellite.md`; `docs/decisions/0179-a-rooms-choice-of-wake-words.md` | - |
| K31-3 | Announcements with ducking | done | `docs/decisions/0173-the-announcement-mixer.md`; `docs/decisions/0175-announcements-are-mixed-per-room-on-the-slots-grid.md`; `docs/measurements/2026-10-05-announcement-duck-timing.md` | - |
| K31-4 | Setup and onboarding from the app | done | `docs/decisions/0103-wifi-provisioning-over-softap.md`; `docs/decisions/0106-speakers-adopted-named-and-assigned-rooms.md`; `docs/decisions/0110-explicit-firmware-installs.md` | - |

## K57 to K94: the owner's decisions

| Item | What | State | Evidence | Reason or follow-up |
|---|---|---|---|---|
| K57 | Every room and group a UPnP/DLNA renderer, plus any legal protocol | done | `docs/upnp.md`; `docs/decisions/0125-the-upnp-av-media-renderers.md`; `docs/proposals/P6-casting-receivers.md` | - |
| K58 | Where receivers run (research and propose) | done | `docs/proposals/P6-casting-receivers.md`; `docs/proposals/P7-spotify-soloist.md`; `docs/decisions/0130-the-soloist-receiver-supervisor.md` | - |
| K59 | Rooms, saved groups and live groups as cast targets | done | `docs/decisions/0125-the-upnp-av-media-renderers.md`; `docs/decisions/0132-the-soloist-receivers-in-the-server.md` | - |
| K60 | Strict legal bar for receivers: open only | done | `docs/proposals/P6-casting-receivers.md`; `docs/clean-room.md` | - |
| K61 | A core-quality HA integration, installed as a custom integration | done | `integrations/homeassistant/`; `docs/decisions/0138-the-home-assistant-integration.md`; `docs/decisions/0164-the-home-assistant-integration-is-installed-from-a-pinned-export.md`; `docs/decisions/0227-the-home-assistant-integration-is-not-submitted-upstream-yet.md` | - |
| K62 | FLAC and Opus on the wire, encrypted sessions with pairing | done | `docs/decisions/0039-the-v2-key-exchange.md`; `docs/decisions/0040-codec-negotiation.md`; `docs/decisions/0044-the-vendored-decoders.md` | - |
| K63 | The Spotify and phone-app path (research and propose) | done | `docs/proposals/P7-spotify-soloist.md`; `docs/soloist.md`; `docs/decisions/0224-the-soloist-dashboard-terms-have-no-instance-clause.md` | - |
| K64 | Inputs only, no content in chorus | done | `docs/inputs.md` | - |
| K65 | Metadata and artwork, controller, visualizer and source roles | done | `docs/decisions/0184-artwork-is-proxied-not-the-policy-widened.md`; `docs/decisions/0067-the-linux-front-panel-and-one-controller-model.md`; `docs/decisions/0084-the-visualizer-stream.md`; `docs/decisions/0066-line-in-capture-as-the-source-role.md` | - |
| K66 | Spotify Soloist per room, saved group and live group | done | `docs/decisions/0130-the-soloist-receiver-supervisor.md`; `docs/decisions/0131-the-chorus-soloist-image.md`; `docs/decisions/0132-the-soloist-receivers-in-the-server.md` | - |
| K67 | Compact speaker controls: buttons, status LED, mic with hardware mute (the logic; the image's GPIO, LED and I2S-input binding is not written yet, `docs/hardware/controls.md`) | done | `docs/hardware/controls.md`; `firmware/tests/test_controls.c`; `docs/decisions/0063-controls-led-and-mic-gate.md`; `docs/decisions/0230-the-compact-on-bought-modules.md` | - |
| K68 | Two-way controls: hidden pairing button, rear status light | done | `docs/hardware/twoway-speaker.md`; `docs/hardware/controls.md` | - |
| K69 | Subwoofer controls: pairing button, status LED, level and phase knobs | done | `docs/hardware/subwoofer.md`; `docs/decisions/0229-the-subwoofer-driver-alignment-and-amplifier.md` | - |
| K70 | The streaming amp with rack variant | dropped | `docs/proposals/P4-bench-purchase.md`; `docs/decisions/0231-no-rack-amp-and-no-soundbar.md`; `docs/proposals/P13-rack-amp-zones.md` | Dropped by the owner's house plan of 2026-10-04 (P4), which wires no room back to the rack; P13 records zero zones, accepted by the owner on 2026-10-07 |
| K71 | Voice path to HA (research and propose) | done | `docs/proposals/P8-voice-path.md`; `docs/decisions/0166-the-voice-role-on-the-wire.md`; `docs/decisions/0176-the-home-assistant-voice-satellite.md` | - |
| K72 | Theater front: a soundbar and a separate LCR set | partial | `docs/decisions/0232-the-lcr-set-is-three-two-ways-and-a-pi-hub.md`; `docs/hardware/lcr-set.md`; `docs/decisions/0231-no-rack-amp-and-no-soundbar.md` | The LCR set is designed; the soundbar is not, since the house plan puts a soundbar in no room (P13 and decision 0231, accepted by the owner on 2026-10-07) |
| K73 | Wake word on the server, permissive models only | done | `docs/decisions/0167-the-wake-word-runtime.md`; `tools/conventions/check-wakeword.sh` | - |
| K74 | A 2U rack-mount multi-zone amp | dropped | `docs/proposals/P4-bench-purchase.md`; `docs/proposals/P13-rack-amp-zones.md`; `docs/decisions/0231-no-rack-amp-and-no-soundbar.md` | Dropped by the owner's house plan of 2026-10-04 (P4); P13 counts zero zones and zero channels, accepted by the owner on 2026-10-07 |
| K75 | Up to 8 rooms | done | `docs/decisions/0048-the-house-scale-simulation.md`; `docs/measurements/sim-house-8-rooms.md`; `docs/measurements/house-soak-8-rooms.md` | - |
| K76 | Concurrent streams limit (research and propose) | done | `docs/proposals/P11-concurrent-streams.md`; `docs/measurements/concurrent-streams-host.md` | - |
| K77 | Sonos-style group volume | done | `docs/decisions/0075-control-catalog-v2.md` | - |
| K78 | Casting to a group takes busy rooms | done | `docs/decisions/0075-control-catalog-v2.md`; `docs/decisions/0132-the-soloist-receivers-in-the-server.md` | - |
| K79 | Decoders per format (research and propose) | done | `docs/proposals/P9-decoders.md`; `docs/decisions/0122-the-server-decoders.md`; `docs/decoders.md` | - |
| K80 | Alarm sources: chimes, a Spotify playlist, a stored stream URL, a line-in | done | `docs/inputs.md`; `docs/decisions/0129-stored-alarm-sources-line-in-sharing-and-streamer-inputs.md`; `docs/chimes.md` | - |
| K81 | TV autoplay, line-in autoplay, volume limits, quiet hours | done | `docs/decisions/0076-the-schedule-runtime.md`; `docs/decisions/0074-room-volume-on-the-audio-wire.md`; `docs/decisions/0087-cec-audio-system-on-the-hub.md`; `docs/decisions/0150-quiet-hours-switched-off-and-on-per-room.md` | - |
| K82 | Remote access through HA or the VPN; no remote path of chorus's own | done | `docs/proposals/P10-ha-dashboard-mqtt.md`; `docs/app.md` | - |
| K83 | HA sound controls, diagnostics, button events, update entities | done | `docs/decisions/0152-the-home-assistant-sound-controls.md`; `docs/decisions/0157-the-home-assistant-speaker-diagnostics.md`; `docs/decisions/0161-the-home-assistant-speaker-button-events.md`; `docs/decisions/0154-the-home-assistant-speaker-devices-and-firmware-updates.md` | - |
| K84 | HA dashboard (research and propose) | done | `docs/proposals/P10-ha-dashboard-mqtt.md`; `docs/decisions/0163-the-home-assistant-dashboard-is-stock-cards-only.md` | - |
| K85 | One household login, everyone equal | done | `docs/app.md`; `docs/decisions/0190-the-app-installs-behind-the-login.md` | - |
| K86 | Phones, wall tablets in kiosk mode, desktop | done | `docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md`; `docs/app.md` | - |
| K87 | Room-correction microphone: the phone, through the app | done | `docs/decisions/0200-a-recording-is-fitted-and-not-kept.md`; `docs/decisions/0207-the-room-is-recorded-with-an-audio-worklet.md`; `docs/room-correction.md` | - |
| K88 | Enclosures per class (research and propose) | done | `docs/proposals/P12-enclosures.md`; `docs/decisions/0242-p12-accepted-enclosures-per-class.md` | - |
| K89 | Compact speaker parts under about 150 USD | partial | `docs/hardware/compact-speaker.md` | The priced list is 156.09 USD, 6.09 over; the ways back under are listed there for the owner |
| K90 | PoE+ for the compact speaker, mains for the other classes | done | `docs/hardware/compact-speaker.md`; `docs/hardware/twoway-speaker.md`; `docs/hardware/subwoofer.md` | - |
| K91 | Some compact speakers on Wi-Fi, never bonded | done | `docs/decisions/0103-wifi-provisioning-over-softap.md`; `docs/decisions/0075-control-catalog-v2.md`; `firmware/sdkconfig.compact-s3-wifi` | - |
| K92 | Auto-adopt on the LAN with trust-on-first-use pins | done | `docs/decisions/0104-a-stored-identity-per-board-and-dns-sd-discovery-in-c.md`; `docs/decisions/0106-speakers-adopted-named-and-assigned-rooms.md`; `docs/decisions/0039-the-v2-key-exchange.md` | - |
| K93 | Firmware updates installed only on the owner's approval, A/B with rollback | done | `docs/decisions/0108-the-ota-state-machine-and-the-firmware-wire.md`; `docs/decisions/0110-explicit-firmware-installs.md`; `docs/decisions/0111-ota-under-the-emulator.md`; `docs/decisions/0154-the-home-assistant-speaker-devices-and-firmware-updates.md` | - |
| K94 | Line-in latency grows seamlessly when rooms join | done | `docs/decisions/0071-latency-growth.md`; `docs/measurements/latency-growth-sim.md` | - |
