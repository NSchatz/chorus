# Checkpoint K: chorus goal 1 packet (for the owner)

- Program: `.claude/goals/2026-09-chorus.md` (brief v2), §32. Goal 1 ends here; goals 2-27 chain
  without further review once this is approved (K2).
- Written by chorus goal 1 on 2026-09-30. Goal-1 ledger: `.claude/goals/2026-09-chorus-g1.status.md`.
- **To approve:** commit `.claude/goals/CHECKPOINT-K.approved` on `main` with your own words:
  "Approved by the owner, <date>", plus any amendments: per proposal, approve, pick another option,
  or defer (a deferred proposal is built on its "If deferred" column, §0.2); per inference (I1-I20),
  keep or amend. Only you write that file, or a session you tell to in its own chat. No goal writes
  it. Goal 2 waits on it (its precondition prints the file and its commit).
- What goal 1 changed: no product code. It added `docs/audit/2026-09-audit.md` and
  `docs/proposals/` (P1-P10, P14), scrubbed the owner's name from three tracked lines (R15), closed
  PR #18 unmerged, filed six Needs items in shopkit's `NEEDS-NOAH.md` (`## chorus`) and one request
  row to devices (`chorus-1`).

## 1. Proposals P1-P10 and P14 (decide each: approve, pick another option, or defer)

Each line gives the recommendation as the proposal words it, what later goals build on if you
defer it (§0.2), and the file. Every proposal was checked by an adversarial verifier that tried
to refute the claims its recommendation rests on; its corrections are applied in the file.

### P1: Embedded platform, wired link and ESP-IDF release (K25, K37, K51)

- **Recommendation:** Option B, ESP32-S3 everywhere on ESP-IDF v6.1.x (W5500 wired with INT, PoE+ by a bought 802.3at splitter, native Wi-Fi for K91), because it is the only platform every speaker class can be built on from bought modules, and v6.1 outlives v5.3's January 2027 EOL with the PSA crypto API that K62 is then written against once.
- **If deferred:** ESP32-S3 on ESP-IDF v5.3.6 (no upgrade), Wi-Fi plus a W5500 wired path
- **Path:** `docs/proposals/P1-embedded-platform.md`
- **Adversarial check:** verifier 1: 17 claims confirmed, 0 refuted, 1 partly right, 0 unverifiable

### P2: Theater scope and TV capture hardware (K17, K72)

- **Recommendation:** Option A (stereo LPCM from optical and ARC, with CEC through the Linux kernel API on a Linux theater hub) as the committed scope, because it works on every Roku-made TV and, `ASSUMED` until the Needs item answers, on partner-brand Roku TVs with an optical output, with bought parts and no codec, patent or custom-board question; AC-3 (C) and eARC (B) are not built until the owner's TV answers and a bench measurement justify them.
- **If deferred:** Stereo PCM from optical and ARC only
- **Path:** `docs/proposals/P2-theater-scope.md`
- **Adversarial check:** verifier 1: 12 claims confirmed, 0 refuted, 3 partly right, 0 unverifiable

### P3: Speaker network placement (K35)

- **Recommendation:** Option B', a dedicated audio network with the server on it through a spare port, because it makes the speaker network alone the K92 adoption boundary (as B does) while keeping the speaker protocol and time sync on one switched segment, off the firewall and off the server's only uplink.
- **If deferred:** The homelab PR is drafted on a branch and not opened; the line reads PROPOSED
- **Path:** `docs/proposals/P3-speaker-network.md`
- **Adversarial check:** verifier 3: 10 claims confirmed, 0 refuted, 4 partly right, 0 unverifiable

### P4: The bench purchase packet (K38)

- **Recommendation:** Option B, the recommended tier at $791.31, because it is the smallest set that runs every bench phase through EMBEDDED-5 on the platform and link P1 recommends, including the compact speaker's PoE+ power path, with every priced line from a US seller except one Amazon listing whose ship-from must be checked.
- **If deferred:** The recommended tier is filed as the Needs packet, marked "awaiting choice"
- **Path:** `docs/proposals/P4-bench-purchase.md`
- **Adversarial check:** verifier 1: 8 claims confirmed, 0 refuted, 2 partly right, 3 unverifiable

### P5: The app stack for the chorus PWA (K43)

- **Recommendation:** Option C, Lit 3 + esbuild with `node --test` + happy-dom: nearly Svelte's authoring fit at under half its supply chain (43 vs 103 locked packages), no MPL-2.0 exception, no test framework, half the runtime, and the slowest churn.
- **If deferred:** The recommendation
- **Path:** `docs/proposals/P5-app-stack.md`
- **Adversarial check:** verifier 3: 12 claims confirmed, 0 refuted, 2 partly right, 0 unverifiable

### P6: Casting protocols beyond UPnP AV, and where receivers run (K57, K58)

- **Recommendation:** Option B, the UPnP AV media renderer written in Rust inside chorus-server plus OpenHome Product, Volume, Info, Time and Playlist services on the same devices, with Soloist (P7) as the only sidecar, because OpenHome is the one other open protocol that adds real value (a renderer-held queue and input selection) at small cost.
- **If deferred:** UPnP AV only
- **Path:** `docs/proposals/P6-casting-receivers.md`
- **Adversarial check:** verifier 2: 14 claims confirmed, 1 refuted, 1 partly right, 1 unverifiable

### P7: Spotify through Soloist (K63, K66)

- **Recommendation:** Option C, a pool of Soloist slots each in its own network namespace on a macvlan network, because Soloist's own mDNS responder cannot share UDP 5353 with the homelab's avahi reflector (LEAD) and a bridge address is unreachable from phones (inference), so A and B would not appear in the Spotify app.
- **If deferred:** One instance per room only, on bridge networking
- **Path:** `docs/proposals/P7-spotify-soloist.md`
- **Adversarial check:** verifier 2: 15 claims confirmed, 0 refuted, 2 partly right, 0 unverifiable

### P8: The voice path into Home Assistant (K71)

- **Recommendation:** Option A, an `assist_satellite` entity per voice room in chorus's own integration, because it gives every feature K71 names with one HA device per room, puts announcements and replies straight into chorus's ducking mixer, and adds one server route instead of an ESPHome server per room.
- **If deferred:** The recommendation
- **Path:** `docs/proposals/P8-voice-path.md`
- **Adversarial check:** verifier 2: 16 claims confirmed, 1 refuted, 0 partly right, 0 unverifiable

### P9: Decoders per format (K79)

- **Recommendation:** Option A (Symphonia 0.6.1 under an MPL-2.0 ADR for MP3, FLAC, Vorbis, ALAC and WAV; libopus via the `opus` crate for Opus; dr_flac and libopus on the C endpoint) with AAC off, because it is the most mature decoder set with one API and gapless handling, and it runs the reference Opus code on both sides of the wire.
- **If deferred:** MP3, FLAC, Vorbis, Opus, ALAC and WAV as recommended; AAC off
- **Path:** `docs/proposals/P9-decoders.md`
- **Adversarial check:** verifier 1: 11 claims confirmed, 1 refuted, 1 partly right, 0 unverifiable

### P10: The Home Assistant dashboard UI, and what MQTT carries beside the integration (K84, K46)

- **Recommendation:** Stock cards only (Option A) in goal 19, no custom card or card features; MQTT off by default with an opt-in, read-only state and event publisher and no HA discovery (Option M2).
- **If deferred:** Stock cards; MQTT off by default
- **Path:** `docs/proposals/P10-ha-dashboard-mqtt.md`
- **Adversarial check:** verifier 3: 12 claims confirmed, 0 refuted, 2 partly right, 0 unverifiable

### P14: The devices seam (chorus PRs in devices before devices finishes) (K23)

- **Recommendation:** Option B, approve with limits: PRs only after devices goal 4 is COMPLETE, new files under `builds/chorus-*` and new inventory part records only, gated by devices' `make check` plus `shopkit inv bom` per build (invariants only where devices' own path rule asks), because it lands every design in devices under devices' rules while checking what chorus adds and holding no devices lock.
- **If deferred:** Drafts on chorus branches only
- **Path:** `docs/proposals/P14-devices-seam.md`
- **Adversarial check:** verifier 3: 13 claims confirmed, 0 refuted, 1 partly right, 0 unverifiable

P11-P13 are written later, in goals 17, 24 and 26 (I11), and built on their recommendation.

## 2. The audit summary

`docs/audit/2026-09-audit.md` (K20): **10 HIGH, 22 MED, 13 LOW**
findings, each with `file:line`, evidence and a proposed fix. Goal 2 fixes every HIGH and every
cheap MED one (I17).

| Phase, guardrail or area | Verdict | HIGH | MED | LOW |
|---|---|---|---|---|
| Phase FOUNDATION-1 (Foundation) | met (in simulation), with model gaps | 0 | 1 | 1 |
| Phase SOUND-2 (First sound) | code written, unproven on hardware | 0 | 2 | 1 |
| Phase RIG-3 (Measurement rig) | code written, unproven on hardware (and the device path is defective) | 1 | 3 | 0 |
| Phase SYNC-4 (Synchronization) | code written, unproven on hardware | 1 | 0 | 1 |
| Phase EMBEDDED-5 (Embedded bring-up) | partial (host-built cores only; the target image does not compile and plays nothing) | 3 | 2 | 0 |
| Phase PRODUCT-6 (Product hardening) | partial | 0 | 6 | 1 |
| Phase WIFI-7 (Wi-Fi tier) | code written, unproven on hardware (and the characterization run cannot work) | 1 | 1 | 0 |
| Phase DSP-8 (DSP) | not started | 0 | 0 | 0 |
| Phase TV-9 (TV/surround path) | not started | 0 | 0 | 0 |
| Phase FLEET-10 (Fleet) | not started | 0 | 0 | 1 |
| Guardrail 1: clean-room | partial (no violation found; not auditable) | 0 | 2 | 0 |
| Guardrail 2: no Secure Boot, Flash Encryption or anti-rollback eFuses on dev hardware | partial (code names are scanned; the configuration route that actually burns eFuses is not) | 1 | 1 | 0 |
| Guardrail 3: timing and sync claims backed by measurement | met for wording (no timing claim is presented as measured); the measuring path is broken (A-3, A-7, A-11, A-13) | 0 | 0 | 2 |
| Guardrail 4: monotonic clocks only in the audio/timestamp path | met in code; the enforcing scan is a name denylist | 0 | 0 | 1 |
| Guardrail 5: no em dashes | partial (one literal in the tree; no enforcement) | 0 | 1 | 0 |
| Known breakages, gates and CI, ADRs and docs, identity | cross-cutting | 3 | 3 | 5 |
| **Total** | | **10** | **22** | **13** |

Phases in short: FOUNDATION-1 is met in simulation (Rust and C read the same fixtures and agree
exchange by exchange) with a narrow jitter model; SOUND-2, RIG-3, SYNC-4 and WIFI-7 are code written
and unproven on hardware, and the bench tools that would prove RIG-3, SYNC-4, EMBEDDED-5 and WIFI-7
cannot capture what they claim (A-3, A-7, A-13); EMBEDDED-5 is partial (host cores only: the image
does not compile, plays nothing and runs no servo); PRODUCT-6 is partial (zones, groups, volume,
the UI and discovery work in software for Linux endpoints; the deploy script starts none of it and
the soak never ran); DSP-8, TV-9 and FLEET-10 are not started. Guardrails: no clean-room violation
was found but none can be shown (no reading log, no LICENSE); the eFuse guard misses sdkconfig
(A-15); no timing claim is presented as measured; monotonic clocks hold in code; one em dash is in
the tree and nothing enforces the rule.

### HIGH findings

- **A-3** (Phase RIG-3): The capture tool plays its chirp to completion and only then starts recording, and it plays it on a local device rather than through the endpoints
- **A-7** (Phase SYNC-4): The SYNC-4 hour run streams a 440 Hz sine that the lag analyser is built to refuse
- **L-1** (Phase EMBEDDED-5): The ESP-IDF image does not compile for esp32s3: two source files fail
- **A-9** (Phase EMBEDDED-5): The firmware session counts audio chunks as "played" and discards them; no servo runs on the endpoint
- **A-10** (Phase EMBEDDED-5): The committed endpoint configuration stops the board before the network: the amp map is `unknown`, the Wi-Fi credentials are `unknown`, and the server is loopback
- **A-13** (Phase WIFI-7): The wireless characterization drives a serial console the firmware does not have and copies an offsets file nothing writes
- **A-15** (Guardrail 2: no Secure Boot, Flash Encryption or anti-rollback eFuses on dev hardware): The eFuse guard never reads sdkconfig, sdkconfig.defaults or CMake, where Secure Boot and Flash Encryption are actually switched on
- **B-14** (Known breakages, gates and CI, ADRs and docs, identity): The Docker image cannot build: the build stage copies no `docs/`, which the server includes at compile time
- **B-15** (Known breakages, gates and CI, ADRs and docs, identity): `deploy/run-server.sh` runs the server with bridge networking, no control plane, no advertisement and no persisted state
- **B-16** (Known breakages, gates and CI, ADRs and docs, identity): The declared MSRV 1.74 is false; nothing pins or tests a toolchain

## 3. Reversals R1-R17 (as worded in the brief §1.1)

Goal 1 wrote R15 (PR #20). Goal 2 writes R12 and R13; goal 4 writes R1-R11, R14, R16 and R17.

| # | File and rule | Change | Decisions |
|---|---|---|---|
| R1 | `CLAUDE.md:28-46`, "This repo is a submodule of the SDD umbrella" (specs, `just implement`/`just land`, tier floor, `documentation/roadmaps/chorus.md`) | Section removed; replaced by a pointer to this brief as the plan of record and one kept line: "Flashing, eFuses, deploys to the homelab and OTA installs on installed speakers are the owner's actions (K4, K28, K93); nothing in this repo sets `CHORUS_OWNER_AT_BENCH`." | K1, K4, K11, K12, K28, K93 |
| R2 | `CLAUDE.md:45-46`, "BRIEF.md is the owner's document ... do not silently edit the brief" | BRIEF.md is kept current by the program; its §3.1 guardrails are never relaxed by the program (I2) | K47 |
| R3 | `CLAUDE.md` working agreement 2 (clean-room) | Adds: agents never open GPL source or reciprocal hardware design files; docs and specs only | K33, K39 |
| R4 | `CLAUDE.md` rule 8's "(umbrella ADR-0035)" citation | Citation dropped; the rule's substance stays (I1, inferred) | K11 |
| R5 | `BRIEF.md:26, :37, :103` and BRIEF §6 Docker notes: "Proxmox homelab (Dell R730xd)" | Bare-metal Debian 13 host running Docker Compose; chorus-server uses host networking | K24, K34 |
| R6 | BRIEF §1 Vision and BRIEF §2.1 | Sonos parity across software, hardware and HA; inputs-only content (UPnP AV casting, Spotify Soloist receivers, line-ins, the TV); no Music Assistant or Sendspin; chorus groups rooms | K13, K54, K56, K57, K64, K66 |
| R7 | BRIEF §2.3 non-goals | Adds portable/battery speakers and Bluetooth input (K53), reverse-engineered receivers (K60); "No mobile app" stays, with an app-grade PWA (K16); the streaming line reads: official receivers run as separate processes, no DRM code in chorus (per P6/P7 as approved) | K16, K53, K60, K66 |
| R8 | BRIEF §5.4 and BRIEF §6 "use the APLL", "W5500 vs RMII" on the S3 | The ESP32-S3 has no APLL and no Ethernet MAC; platform and link per P1 | K25, K37 |
| R9 | BRIEF §5.5 TAS notes | TAS5805M 7-bit I2C addresses 0x2C-0x2F (0x4C-0x4F is the TAS5825M); "2x38 W" is TI's 10% THD+N instantaneous rating (30 W continuous at 1%); the TAS5805M is 2x23 W at 21 V and 32-96 kHz only, so not a power-equivalent sibling | K47 |
| R10 | BRIEF §5.8 "WebSocket for UIs", "single page, no framework", "MQTT/HA later" | HTTP + SSE (as built, ADR in goal 4); the app stack per P5; a chorus HA integration plus MQTT extras | K43, K46, K61 |
| R11 | BRIEF §5.9 "provisioning ... serial console" | Auto-adoption with trust-on-first-use keys; OTA installs only on the owner's command | K92, K93 |
| R12 | `docs/decisions/0002-*` CI shape (one CI job is the gate) | `make gate` is the gate; CI calls it and is informational | K19 |
| R13 | The retired convention records and ADRs (comment-density baseline, contrast-ratio record, frontend and styling records, pinning clauses) | Superseded by `docs/conventions.md`; their checks and demo trees removed | K18 |
| R14 | `deploy/run-server.sh`, `deploy/README.md`: bridge networking, no control plane | Host networking, the control plane on, compose in homelab | K24, K34 |
| R15 | `BRIEF.md:4` (`Owner:`), `BRIEF.md:24` and `docs/decisions/0001-*:4`, the three tracked occurrences of the owner's name on `origin/main` | "the owner" | K27 |
| R16 | BRIEF §0 items 2 and 5 (the owner confirms before expensive decisions; recommendations lose to evidence) as far as editing BRIEF goes | The program keeps BRIEF current (I2's limit applies); the confirm-before-expensive rule stays | K47 |
| R17 | BRIEF §3.1 rule 1 ("reference projects may be read ... no code copied") | Tightened: GPL projects' docs, issues and specs only, never their source; reciprocal hardware design files never opened. A tightening, so I2 holds; BRIEF's Appendix A (the CLAUDE.md starter) is updated to match R1-R4 or removed | K33, K39 |

## 4. Planner inferences I1-I20 (keep or amend each)

| # | Inference | From |
|---|---|---|
| I1 | CLAUDE.md rule 8 keeps its substance (fitness, never availability); only its umbrella citation goes | K11 |
| I2 | "The program owns BRIEF" never extends to relaxing or removing a §3.1 guardrail | K47 |
| I3 | Commit subjects `<area>: <summary>`, ledger commits `goals: ...`, until `docs/conventions.md` says otherwise | K18 |
| I4 | Speaker microphones feed only the voice path, never a shareable source; room correction uses the phone (K87), so K65's "a mic for room correction" example applies to the phone path | K65, K87 |
| I5 | The identity term list lives at `/cache/chorus-private/identity-terms.txt`, built from `git config user.name`/`user.email`; the scan is case-sensitive; its absence fails the gate; the checkpoint file reads "Approved by the owner, <date>" | K27 |
| I6 | The program's state for owner steps is NEEDS-OWNER; the shared file keeps its name | K6, K27 |
| I7 | `IDF_PY_BUILD_JOBS=2` caps idf.py's ninja (it ignores other variables); `CMAKE_BUILD_PARALLEL_LEVEL=2` caps any `cmake --build`; both join the worker variables | K29 |
| I8 | Only the chorus and homelab surveys and the research are committed to the research folder; nothing about the retired umbrella | K11 |
| I9 | The homelab deploy PR lands in goal 4; the speaker-network PR (P3) in goal 7; telemetry in goal 15; the Soloist sidecar in goal 17; the HA integration's pinned copy in goal 19 | K28, K35 |
| I10 | Every volume path (HA, UPnP, alarms, the app, controller buttons) is clamped by K81's limits | K81 |
| I11 | Proposals P11-P13 are written in the goals that build them (17, 24, 26), since their inputs arrive then | K74, K76, K88 |
| I12 | ESP32-S3 boards the owner owns are recorded in inventory through a row to devices once identified | K5, K21 |
| I13 | "The owner approves each update" (K93) means an explicit install action from the app, chorusctl or HA by anyone with the household login, since the app has everyone equal (K85) and HA has no per-user permissions; nothing installs by itself | K85, K93 |
| I14 | K61's "a media_player per room and per group" means per room and per saved group; live groups appear as room members, as HA's own group modelling works (research-ha-integration.md §2) | K59, K61 |
| I15 | K27's name scrub stays in goal 1 (three lines, R15) with the private term list built there; the identity and secret scan joins `make gate` in goal 3, because the gate is created in goal 2 | K27 |
| I16 | K48's PR #18 closes in goal 1; its other loose ends (ADR numbers, three missing ADRs, stale docs, measurement relabels) land in goal 4 with the other docs work | K48 |
| I17 | K20's "the next goal fixes them": goal 2 fixes every HIGH finding and every cheap MED one; every other finding is assigned to a named later goal or DROPPED with a reason, listed in goal 2's report | K20 |
| I18 | `NSchatz/<repo>` slugs (the repos' namespace, needed by `gh` and git remotes) are the one allowed form of the account handle; the private term list holds the full name, the first name, the email and its local part, matched case-sensitively, and allowlists exactly the `NSchatz/` prefix | K27 |
| I19 | K32's order moves control catalog v2 (goal 11) and the K30/K31 items that DSP-8 and TV-9 need (bonded sets, alarms, limits, quiet hours, tone and loudness, room-correction fitting, theater bonding: goals 11-13) ahead of FLEET-10 (goals 14-15) | K32 |
| I20 | K44 extends K41's tag permission to shopkit `v*` release tags on shopkit's release PR merge commits, per shopkit's own protocol | K41, K44 |

Goal 1's note on I18: chorus's own term list holds all four terms and is matched
case-sensitively (I5); the copy for shopkit's scan leaves out the bare first name, because
shopkit's scan is case-insensitive and that name matches the shared file `NEEDS-NOAH.md` on 388
lines of shopkit (goal-1 ledger, Decisions).

## 5. The gate on a fresh clone

Run 2026-09-30 by chorus goal 1 on a fresh `git clone git@github.com:NSchatz/chorus.git` at `8cf490dc3dc3d07e8fd32b4f69864408656b1973` (origin/main after PRs #20, #21 and #22), with an empty `CARGO_TARGET_DIR` inside the clone:
`timeout 3600 flock -o -w 1800 /cache/locks/chorus-heavy.lock mise exec rust@1.98.0 -- bash ci-equiv.sh <clone>` (the CI-equivalent steps of `.github/workflows/ci.yml` without the browser steps; goal 2 replaces it with `make gate`). Exit 0; 587 s wall-clock from lock to end (clone included: 588 s). The ESP-IDF compile is not in this gate yet (it fails on the two files of audit finding L-1; goal 2 adds it).

```
toolchain: cargo 1.98.0 (797e8a9bc 2026-08-05) / rustc 1.98.0 (88d9e12ae 2026-08-18)
build                              rc=0      47.3s
protocol                           rc=0       0.3s
sync                               rc=0       0.2s
audio-path                         rc=0       3.2s
playout                            rc=0      79.5s
control                            rc=0       0.1s
discovery                          rc=0       0.3s
soak-72h-model                     rc=0      14.2s
regress-stalled                    rc=0      13.9s
determinism                        rc=0     116.1s
workspace-tests                    rc=0      89.2s
fw-golden                          rc=0       2.9s
fw-sync                            rc=0       2.2s
fw-safety                          rc=0       0.9s
fw-wireless                        rc=0       2.1s
firmware-check                     rc=0     199.2s
verify-pinning                     rc=0       0.5s
comment-density                    rc=0       0.5s
verify                             rc=0      13.7s
wall-clock: 587 s; result: PASS
```

## 6. The Needs items (shopkit `NEEDS-NOAH.md`, `## chorus`)

- Checkpoint K: review chorus goal 1 and approve (chorus goal 1, 2026-09-30) - a gate: chorus goal 2 waits on it
- The three TVs: model, eARC port, optical out and audio menu (chorus goal 1, 2026-09-30; P2) - not a blocker
- Your ESP32-S3 boards: module markings and a read-only chip report (chorus goal 1, 2026-09-30; I12) - not a blocker
- The room list and its wiring (chorus goal 1, 2026-09-30; K74, K75, K91) - not a blocker
- The rack: free units and depth (chorus goal 1, 2026-09-30; K74) - not a blocker
- Spotify Soloist terms: the clause on instances (chorus goal 1, 2026-09-30; P7) - not a blocker

The Index's Gates list carries the Checkpoint K line.

## 7. The goal-1 ledger

`.claude/goals/2026-09-chorus-g1.status.md`: the precondition as checked, the Baselines table
with timings, one table per phase, the decisions taken (with where their reasoning lives), the
requests filed (`chorus-1` to devices) and served (none were addressed to chorus).
