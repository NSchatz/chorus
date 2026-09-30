# Adversarial review: chorus 2026-09 program brief v1

Reviewer: fresh adversarial subagent, 2026-09-29 (about 40 min). Read-only: nothing in any repo was
changed. Scope: the draft in `/cache/wt/chorus/goals-2026-09` (brief `.claude/goals/2026-09-chorus.md`
as rebuilt at 23:36:47, 22 goal files, research folder, `.claude/settings.json`), the planning files
`decisions.md` (K1-K96) and `interview.md`, the live repo (`origin/main` = `1ea9f2a`), the live
devices brief (`gh api .../devices/contents/.claude/goals/2026-09-devices.md`, byte-identical to
the planning copy) and shopkit's shared files. Line numbers below are for the 23:36:47 build.
Scratch: worktree `/cache/tmp/plan-2026-09-chorus/scratch/review-wt` (removed at the end), logs
`scratch/review-fmt.log`, `scratch/review-clippy.log`, fixture tree `guardtest/` in the reviewer's
scratchpad.

## Summary

The brief is careful and mostly sound: all 22 goal files are under 4,000 characters (max 3,318),
every lettered line is byte-identical to the brief's done-when line, all 96 decision rows match
`decisions.md` after the name scrub, every goal has a precondition and a BLOCKED exit, and most
environment facts re-verify. Four things would break or quietly defeat it in a run:

1. The worker cap does not reach ESP-IDF builds: idf.py runs bare `ninja` (default `-j58` here) and
   reads `IDF_PY_BUILD_JOBS`, not `CMAKE_BUILD_PARALLEL_LEVEL` (I7 is false).
2. The flash/OTA guard's rg check catches 1 of 12 ways to set `CHORUS_OWNER_AT_BENCH` (it misses
   the JSON form in the very `settings.json` it names) and is not a gate check.
3. The devices path for goals 19-21 is unreachable as written: devices moves every open row at
   every goal end, so a chorus row is never "still open" when devices finishes; nothing re-checks
   in the finale.
4. Several goals are multi-day (14 and 15 clearly; 3, 5, 7, 19 at or over the day bound of K3),
   with a serialized 12-15 minute gate per PR on one lock.

Plus 15 MED and 24 LOW findings below, each with a fix.

## Findings

### HIGH

**H1. `CMAKE_BUILD_PARALLEL_LEVEL=2` never reaches ESP-IDF builds; idf.py runs ninja at -j58.**
Evidence: `/cache/esp/esp-idf-v5.3.6/tools/idf_py_actions/tools.py:506-519` builds the command from
`GENERATORS['Ninja']['command'] = ['ninja']` (`constants.py:16-23`) and appends `-j` only from
`IDF_PY_BUILD_JOBS`. `ninja -h` with the bundled ninja 1.12.1: `-j N ... [default=58 on this system]`
(the cgroup quota is not honoured; `nproc` and the affinity mask both say 56). ESP-IDF's own docs in
the installed tree (`docs/en/api-guides/build-system.rst:80-86`, Apache-2.0): "you can cap the number
of parallel build jobs by setting the `IDF_PY_BUILD_JOBS` environment variable ... If you invoke
CMake, ninja, or make directly instead of idf.py, use their native options". The planning cold
build (`scratch/verify-idf.sh`: `idf.py -B $B build`) therefore ran at -j58; only its `ninja -k 0 -j2`
re-runs were capped. Affected text: I7 (line 551), §0.11 row 1 (line 326), §0.12, goal 1's
precondition (line 745) and every goal file ("CMAKE_BUILD_PARALLEL_LEVEL=2 for the whole program").
Fix: add `"IDF_PY_BUILD_JOBS": "2"` to `.claude/settings.json` `env`; extend goal 1's precondition to
`echo "$CARGO_BUILD_JOBS $CMAKE_BUILD_PARALLEL_LEVEL $IDF_PY_BUILD_JOBS"` printing `2 2 2`; reword I7
("`IDF_PY_BUILD_JOBS=2` caps idf.py's ninja; `CMAKE_BUILD_PARALLEL_LEVEL=2` caps any `cmake --build`");
tell goal 2 to make the cap explicit in the Makefile too (`IDF_PY_BUILD_JOBS=2 idf.py build` or
`ninja -j2`), and re-verify the variable on any ESP-IDF upgrade (P1).

**H2. The flash/OTA guard check misses 11 of 12 assignment forms, and it is not in the gate.**
Brief §0.7 line 239: `rg -n 'CHORUS_OWNER_AT_BENCH\s*=' Makefile .github .claude/settings.json deploy config tools firmware`.
On a fixture tree with twelve ways to set or bypass the variable it printed only
`export CHORUS_OWNER_AT_BENCH=1`, plus a false positive on a read (`[ $CHORUS_OWNER_AT_BENCH = 1 ]`).
Missed: `.claude/settings.json` `{"env": {"CHORUS_OWNER_AT_BENCH": "1"}}` (the file it names; JSON
writes `":`), workflow YAML `CHORUS_OWNER_AT_BENCH: 1`, Makefile `:=` and `?=`, shell
`${CHORUS_OWNER_AT_BENCH:=1}`, the bypassing read `${CHORUS_OWNER_AT_BENCH:-1}`, compose `environment:`
YAML, Rust `std::env::set_var("CHORUS_OWNER_AT_BENCH", "1")`, and paths it never scans: `crates/`
(goal 12's OTA push and goal 13's `chorusctl` live there), `mise.toml`, `.cargo/config.toml`,
`integrations/`, hidden files. It runs once (goal 7 E); §4.7 invariant 3 has no enforcing check.
Fix: an allowlist check in `make gate` from goal 3, tested here (flags all 12, passes the read forms):
`git grep -n -I CHORUS_OWNER_AT_BENCH -- ':!docs/' | grep -v -F -e '"${CHORUS_OWNER_AT_BENCH:-}"' -e 'env::var("CHORUS_OWNER_AT_BENCH")' -e 'getenv("CHORUS_OWNER_AT_BENCH")'`
must print nothing (owner-facing `CHORUS_OWNER_AT_BENCH=1 tools/...` lines live only under `docs/`),
with a fixture test of the check itself; goal 7 E and goal 12 C cite it; §0.7 adds "no agent command
line sets it".

**H3. The devices path for goals 19-21 cannot happen as written (K23, K50).**
chorus serves its own rows only when "a row still open when its owner's program has finished"
(§0.13 line 390). But devices' rule, and shopkit's `PROGRAM-REQUESTS.md` header, say "every goal
ends with each row that was open at its start moved on" (devices brief §0.13, lines 322-331), so a
chorus row becomes ACCEPTED, DEFERRED or DECLINED at devices' next goal end and is never "still
open". Devices' brief knows only three programs (home, 3d, devices), and its §0.8 ("no second
project dressed as a prerequisite") and fixed goal list make DEFERRED or DECLINED the likely answer;
devices then routes the row to the owner ("start a follow-up goal for rows ..."). Devices is on goal
3 of 5 today, and goal 5 waits on the owner's Checkpoint A (a physical look approval, weeks away).
The finale (goal 22) never re-checks devices, so even if devices finishes mid-program nothing moves
the drafts into devices. "Devices' last COMPLETE" is never given as a command (goal 5 is last unless
`CHECKPOINT-D.approved` says "glide6 first"). K50's "every hardware design packaged in devices" is
then unreachable.
Fix: (a) chorus's rows to devices say "requested: DEFERRED (chorus serves after devices finishes,
K23)", and §0.13 treats a row DEFERRED with that note as servable by chorus after devices finishes;
(b) give the check: `git -C /cache/wt/chorus/sib/devices grep -l 'COMPLETE (goal 5)' origin/main -- .claude/goals/2026-09-devices-g5.status.md` (or goal 4 per `CHECKPOINT-D.approved`);
(c) goal 22 gains an item and line: re-check devices, and if finished, open the devices PRs from the
draft branches; (d) goals 19-21 run devices' gate locally in a devices worktree (nothing pushed) and
paste its tail in the row, so a draft is known to pass; (e) put the question in the Checkpoint K
packet: if the owner wants designs in devices during the run, only the owner can amend K23.

**H4. Several goals are multi-day, against K3 ("no goal over a day").**
Each PR needs a 12-15 minute gate under one lock (§0.3), at most 3 fix rounds: a goal of 8 PRs spends
2-5 hours in serialized gate runs alone. By content:
- Goal 14 (line 1155): a UPnP AV MediaRenderer from the OCF specs (SSDP, device and service XML,
  SOAP AVTransport, RenderingControl, ConnectionManager, GENA eventing, gapless) per room, saved
  group and live group; the decoder crate (Symphonia plus libopus, a C dependency); the Soloist
  sidecar, supervisor and fake; P6 protocols; alarm sources; line-in sharing; P11 measurements; a
  homelab PR. Clearly more than a day. Split: 14a decoders + HTTP streams + UPnP renderer;
  14b Soloist + P6 + alarm sources + line-in sharing + P11 + homelab PR.
- Goal 15: an HA integration at a quality-scale tier (Python, a pinned HA test harness installed
  rootless), every entity family of K83, the P10 dashboard (possibly a custom Lovelace card in JS),
  a homelab PR. Split: 15a config flow, discovery, media_players, tests; 15b sound, diagnostics,
  event and update entities, the dashboard, the homelab PR.
- Goal 3: seven tracks (conventions plus about six new gate checks, licence, identity, 13 reversals,
  K48 loose ends, v0.1.0 with a first ever linked firmware image, the homelab deploy PR). Split:
  3a conventions, licence, identity (the gate-bound work, keeps the (foundation) line); 3b reversals,
  loose ends, v0.1.0, homelab PR.
- Goal 7: board support per P1 (possibly two targets), the wired link driver, the I2S playout path,
  amp registers from the datasheet, controls for four classes, the guard, the packet. Split: 7a
  targets, link, playout; 7b amp, controls, guard, EMBEDDED-5 packet.
- Goal 19: a whole new shopkit package with worked-example tests, shopkit's ADR, gate (175 s) and
  release protocol, plus P12 and a priced compact-speaker package. Split: 19a shopkit-acoustics and
  its release; 19b P12 and the compact speaker.
- Goals 1, 4, 5 and 10 are at the bound (goal 1: audit plus ten proposals each re-verified with web
  citations, on a finite search allowance; goal 4: an authenticated key exchange plus vectors for
  every v2 message; goal 5: codecs on ESP-IDF plus a possible ESP-IDF major upgrade; goal 10: a Rust
  and C DSP library). Keep them, but order their tracks so the (foundation) line lands first.
Splitting 3, 7, 14, 15 and 19 gives 27 goals; the checkpoint stays after goal 1 (K2).

### MED

**M1. The gate budget will be exceeded mid-program, and the budget sits inside a (foundation) line.**
§0.11's figures: build 23-53 s, tests 81-134 s, firmware-check 213 s (130 s deliberate outage),
verify 15 s, determinism 116 s, plus clippy (about 10 s warm here, `scratch/review-clippy.log`), fmt,
and an ESP-IDF cold build of 124-144 s with the link step unmeasured: about 9.7-12.3 minutes at goal 2.
Later goals add the conventions checks, DSP and v2 vectors in Rust and C, OTA fault injection, web
unit tests, one Chromium smoke test, the HA integration's pytest suite, and a second firmware target
if P1 approves the P4 (another 2+ minutes): about 15-18 minutes by goal 17. Only goal 2 A checks the
budget, and it is (foundation), so one slow run on a shared host ends the goal INCOMPLETE and halts
the chain for weeks. Fix: take "within 15 minutes" out of goal 2 A into its own non-foundation line;
the gate prints per-step wall-clock; every GOAL REPORT states the gate wall-clock and, past 15 min,
the split proposal; goal 2 installs ccache rootless (idf.py enables it when found; none is installed
now) with `CCACHE_DIR` under `/cache` and keeps a persistent firmware build dir, and makes the 130 s
deliberate outage in `firmware-check` configurable for the gate.

**M2. §0.11's Rust row is not true when re-run; goal 1's baselines would run on Rust 1.74 and fail.**
Line 331 says "`rustup` has no default toolchain; mise has rust 1.98.0". Re-run: `rustup show` ->
`1.74.0-x86_64-unknown-linux-gnu (active, default)` (plus 1.80-1.85 in `~/.rustup`), and bare
`cargo --version` in `/workspace` -> `cargo 1.74.0`; `mise ls --current` selects nothing, so the shim
falls back to rustup's default. 1.74 cannot build the workspace (§0.11's own MSRV row). The fmt and
clippy counts were taken on 1.98.0 (re-verified: 325 hunks in 83 of 129 files; 56 distinct clippy
warnings). Fix: §0.11 gives the exact baseline command for goal 1 (`mise exec rust@1.98.0 -- cargo ...`
or `RUSTUP_TOOLCHAIN=1.98.0` with `RUSTUP_HOME=/cache/rustup`) and says the default toolchain varies by
container; goal 2's `rust-toolchain.toml` removes the ambiguity afterwards.

**M3. The account handle `NSchatz` is committed 63 times, against K27, and it collides with I5's scan.**
`origin/main` has zero occurrences of `NSchatz` (`git grep -c -i nschatz` prints nothing); the draft
adds 63 (brief, all 22 goal files, research). K27 bans an "account ID" in tracked files, and the
handle embeds the owner's surname. I5 builds the term list from `git config user.name` ("first last")
and the email, case-sensitive; the research (C8, `research-pwa-conventions.md:233`) says `git grep -i`
and lists "account IDs" as terms. Either the term list contains the surname or handle and goal 3's
gate fails on the program's own brief, or it holds only the full name and the bare first name slips
through. Fix: an I-row decided at Checkpoint K: either `NSchatz/<repo>` slugs are allowed (the repos'
namespace, needed by `gh`) and the scan allowlists exactly that form, or the slugs become
`<owner>/<repo>` resolved at run time; the term list is {full name, first name, email, email local
part}, case-sensitive (so `NEEDS-NOAH.md` passes), and I5 says so.

**M4. Decisions moved out of goal 1 or narrowed without an I-row.**
K27 says "goal 1 replaces the owner's name ... and adds an identity + secret scan"; the brief does it
in goal 3 (line 846). K48 is titled "Loose ends (goal 1)"; only PR #18 stays in goal 1, the rest moves
to goal 3. K20 says "the next goal fixes them before any new capability"; goal 2 fixes every HIGH and
"MED ones where cheap" (line 814). Each is a defensible engineering call (the scan needs `make gate`),
but none is recorded in §1.2. Fix: add I15 (identity scrub and scan in goal 3 because the gate starts
in goal 2), I16 (K48 items other than PR #18 in goal 3), I17 (goal 2 fixes HIGH and cheap MED; every
other finding is assigned to a named later goal or DROPPED with a reason, listed in the goal-2
report); or do the three-line name scrub in goal 1, which is trivially cheap.

**M5. Superseded rows are not marked where a goal reads them, and the declined MA/Sendspin study has no banner.**
§0.8 (line 261) lists only three supersessions. Rows that still carry declined options: K14 and K15
("goal 1 researches Music Assistant ... compares a Sendspin bridge"), K16 ("browse/queue via whatever
K14/K15 decide"), K26 (compatibility "with Sendspin SDKs ... librespot"), K31 (voice "depends on
K15"), K42 (upstream drafts include "Music Assistant provider ... Sendspin feedback"), K46 ("the HA/MA
path K15"), K62 ("PIN/QR at setup"). `research-ha-ma-sources.md` opens with a Sendspin headline and
its recommendation (presented at the reopen: a Sendspin client bridge plus an MA hub) was declined,
yet goals 4 and 16 are told to read it. Fix: append "(superseded by K56)" or "(refined by K92)" in the
Decision cell of each such row, extend §0.8's list, and put a first-line banner in the committed
`research-ha-ma-sources.md`: "Declined by K56: read only §1.1 (feature list), §3 (voice), MQTT and
security facts; its recommendation is not an option."

**M6. K65's visualizer/colour role is specified but never built.**
Lines 633 and 885 mention it (component table, goal 4's spec). No goal builds the producer (levels,
beat, colour from the audio on the server), the endpoint LED consumer, or the HA side ("through HA,
lights"). Fix: goal 10 (DSP) computes the levels/beat stream with fixtures; goal 7 (or 7b) drives the
status LED from it on fakes; goal 15 exposes it (an event or sensor entity HA automations can map
to lights); each with a lettered line.

**M7. Wi-Fi onboarding for K91's compact speakers has no goal.**
K31 replaces "serial-console provisioning" with setup from the PWA; K92 auto-adopts "any chorus
speaker appearing on the audio network". A Wi-Fi speaker cannot appear on the network until it has
credentials, and no goal provisions them (goal 12 covers LAN adoption only; goal 7 keeps "the Wi-Fi
tier"). Fix: goal 12 adds Wi-Fi credential provisioning (ESP-IDF's `wifi_provisioning` component,
Apache-2.0, SoftAP or BLE, chosen by citation) tested on the host build, with the phone step as a
Needs packet; goal 18's adoption screen drives it.

**M8. §0.8 forbids what goal 21 instructs.**
§0.8 (line 271): a missing input such as "a room list ... is refused by name, never replaced with a
typical value". Goal 21's precondition (line 1360): "build on assumptions marked ASSUMED if
unanswered", and goal 21 A needs "the zone and channel count". §0-§4 win, so goal 21 A cannot be DONE
without the owner's room list, yet it is not a physical impossibility either. Fix: one rule in §0.8:
a design that needs an unanswered owner input is built parameterised, with the example value marked
ASSUMED in the design and the line reading DONE only when the report names the ASSUMED input and its
Needs item; apply the same wording to goal 11 (TV models) and goal 19 (board model).

**M9. A deferred proposal has no defined fallback.**
§0.2 (line 111): later goals build "on the approved option, or on the recommendation where the
approval is silent". Checkpoint K allows "defer" per proposal (§27), which is neither. Goals 5 D
(ESP-IDF per P1), 6 F (P3's homelab PR; K35 says do not open until the owner decides), 11 D (P2),
14 (P6, P7, P9), 15 (P10), 16 (P8), 17 (P5) all depend on one. Fix: "Deferred means the status quo
that keeps everything building (§0.2): P1 deferred keeps the S3 on v5.3.6; P3 deferred leaves the PR
drafted on a branch, unopened, line PROPOSED; P5 deferred keeps the vanilla page; ..." one line per P.

**M10. R1 removes the only CLAUDE.md sentence that reserves deploys and OTA for the owner.**
R1 (line 525) removes `CLAUDE.md:28-46`, including lines 37-40 ("a spec that burns eFuses, deploys
onto the Proxmox host, or ships an OTA image to a wall-mounted device ... takes the human gate"), and
replaces it with a pointer to the brief. After the program, a future session reading CLAUDE.md sees
no statement that deploys and OTA installs are the owner's. Fix: R1's replacement keeps one line:
"Flashing, eFuses, deploys to the homelab and OTA installs on installed speakers are the owner's
actions (K4, K28, K93); nothing in this repo sets `CHORUS_OWNER_AT_BENCH`." R2's anchor is
`CLAUDE.md:45-46`, not 44-46.

**M11. Two BRIEF.md rules the decisions change have no reversal row.**
K47 reverses "BRIEF §0 items 2/5 as far as editing goes"; R2 covers only CLAUDE.md. K33 declines
"read-never-paste (BRIEF)", but BRIEF §3.1 rule 1 still says "reference projects may be read ... no
code copied"; R3 changes only CLAUDE.md. BRIEF's Appendix A (a CLAUDE.md starter, lines 343+) also goes
stale after R1-R4. Fix: R16 (BRIEF §0 items 2 and 5: the program keeps BRIEF current, K47); R17 (BRIEF
§3.1 rule 1 tightened to docs, issues and specs only, never GPL source or reciprocal design files,
K33, K39; a tightening, so I2 holds); goal 3 updates or removes Appendix A.

**M12. K32's order is changed without saying so.**
K32: "protocol/control-catalog versioning" is a foundation; "parity features (K30, K31) after
FLEET-10". The brief puts control catalog v2 in goal 9, and K30 bonded sets, alarms, sleep, limits and
quiet hours (goal 9), K30 tone and loudness and K31 room-correction fitting (goal 10), and K30 theater
bonding (goal 11) before FLEET-10 (goals 12-13). The order is sensible (DSP-8 and TV-9 need them), but
it is a deviation. Fix: one sentence in the intro's "The shape" plus an I-row shown at Checkpoint K.

**M13. Bench-result PRs (K45) are never merged by any goal.**
§0.6 (line 222-225): "the next goal validates and merges it", but no goal's first actions or lines
list open `bench/*` PRs, and the owner-machine setup Needs item (K45: "needs a chorus checkout + gh
there") is not filed by any goal. Fix: §0.1 first actions gain "list open `bench/*` PRs; validate each
against its schema and merge (K45)"; goal 6 files the setup Needs item; the universal H line names
the check (`gh pr list --search 'head:bench/'`).

**M14. Goal 19's precondition includes things the goal itself creates.**
Line 1304: "a sibling clone of shopkit (and a sparse `home` clone ...) and of devices". A fresh
container has none, and a failed precondition means BLOCKED with no work. §0.1 already says sibling
clones are made and pulled at goal start. Fix: move the clones into item 0 ("clone if missing"); also
say a read-only sparse `home` clone for shopkit's gate is not "touching" home (K49 says "never
touched").

**M15. The only guard on `CHECKPOINT-K.approved` is prose.**
§0.10 and §27 say no goal writes it; nothing a transcript shows proves goal 1 did not. Fix: goal 1's
H line (or a new line) adds "`git ls-tree origin/main .claude/goals/CHECKPOINT-K.approved` prints
nothing (the file does not exist at goal-1 end)"; goal 2 prints `git log --format='%H %s%n%b' -- .claude/goals/CHECKPOINT-K.approved`
so the approval commit is visible in its report.

### LOW

L1. §2's identity fact is wrong: the owner's name is on 2 lines of BRIEF.md (4 and 24), 0 lines of
CLAUDE.md and 1 of ADR 0001 (`git grep -c -i noah`), not "BRIEF.md (7 lines), CLAUDE.md and ADR 0001"
(line 600; also K27's list and `review-chorus.md:397`). Fix the numbers; goal 3 scans anyway.

L2. The name scrub left broken prose in §1: "carries no the Needs list file" (K6), "a the Needs list
step" (K41), "a the Needs list item" (K42), "finale the Needs list flip" (K9), "no the Needs list item"
(K11); K7 and K50 still say "NEEDS-NOAH" as a state while §0.5 uses NEEDS-OWNER (I6). Fix the
substitution ("the Needs list" -> "Needs" as a noun phrase) and write NEEDS-OWNER in K7, K50.

L3. Frozen counts in done-when lines: goal 7 C "the 8 unknown keys are gone" and goal 3 item 5 "all six
measurement files". Fix: "no `unknown` key remains in the amp section (count 0 with the command)";
"every file in `docs/measurements/`".

L4. Lines a transcript cannot judge: goal 17 D "(screenshot paths ...)" (the evaluator sees no image;
add a test asserting the three layouts' breakpoints and kiosk mode, with its tail); goals 20 A/B and
21 B/C "(URL or row)" do not show "priced" (add the BOM totals and the design-record paths); the
universal H line gives no commands (add `git status --short` per repo, `gh pr list --author @me
--state open`, the Needs section tail); the universal Requests line gives no evidence (add the
`## To chorus` tail); goal 1 B "covers every phase" (add the audit's headings).

L5. Goal 9 E ("with its duration") sets no minimum; a two-minute soak passes. Give a floor (for
example, at least 1 hour wall-clock or N simulated hours at 8 rooms).

L6. K91 "never bonded into stereo pairs or theater sets" is not enforced anywhere after §1. Add to
goal 9 A: a Wi-Fi endpoint is refused from a bonded set (test).

L7. Coverage gaps: K44 "chorus joins shopkit's ownership table" (the table in `PROGRAM-REQUESTS.md`
lists home, 3d, devices only; goal 19 adds the row); K42 upstream-submission Needs item (goal 15 or
22 files it); K61 "a Needs Owner step per release" for installing the integration (goal 22 files it);
K70 "sub out" missing from goal 21's rack amp; K90 "mains" missing for the soundbar; K9 "contributor
README" in no goal (goal 22 item 1); K75 "PoE/power budget sized to 8 rooms" (goal 19 states the
compact's PoE class against the switch's 250 W at 8 rooms).

L8. Goal 14 item 3 omits the research's no-redistribution design: the Soloist binary is "mounted from
a host volume (chorus never ships or downloads it)" (`research-casting-decoders.md:211`). Add it, and
to goal 22's release item: no Soloist file in any artifact.

L9. MPL-2.0 Symphonia: fine as a dependency with the research's ADR and a cargo-deny exception for
`symphonia*` only; the release notes must say where the MPL source is (MPL-2.0 §3.2). Add to
`docs/release.md` (goal 3) and the finale.

L10. libopus is a C dependency; the static musl image (§0.11, goal 2's `make image`) has no musl C
compiler (`which musl-gcc` -> none; no cmake on PATH). Goal 14 must keep `make image` green (options:
`cargo-zigbuild` via mise, a glibc distroless base, or a mise cmake plus a musl cross compiler).

L11. Goal 4 C and goal 12 D both test "a changed key is refused and surfaced". Goal 12 should cite
goal 4's test and add only discovery, naming and room assignment.

L12. `timeout 3600 flock -o ... make gate` counts lock wait inside the timeout, and a recipe inside
`make gate` that itself takes `chorus-heavy.lock` would deadlock until the timeout. Add: "`make gate`
recipes never take the heavy lock; callers do."

L13. "retry on conflict" for shopkit ledger commits has no bound (§0.6, §0.13). Say "up to 5 times,
then record it in the ledger and carry on".

L14. shopkit's `make ci` skips its identity step here (§0.11 line 342), so chorus's shopkit PRs get
no identity check. Run chorus's own identity scan over the shopkit diff and paste it in the PR body.

L15. `ci.yml` reduced to `make gate` will always fail where the private term list is absent (by
design). Give CI a visible SKIPPED-by-name path for that one step (the repo's refuse-by-name style), so
a red CI means something if Actions ever runs.

L16. gitleaks is not on PATH by default: the shim says "No version is set for shim: gitleaks"; it runs
as `mise exec aqua:gitleaks/gitleaks@8.30.1 -- gitleaks` (0 leaks in history and tree, re-verified).
Pin it in a repo `mise.toml` in goal 3.

L17. The committed `review-homelab.md` keeps umbrella content I8 says is excluded (lines 144-148:
homelab "is still a submodule of super (pinned `7b73005`)") and names homelab session-stack groups
(line 69: `cosyte 20, personal 7, super 1`); if `cosyte` is an employer or client name it is private
data. Drop both.

L18. §0.13 (line 363) says devices is "on goal 2 of 5 at planning"; devices' `g3.status.md` exists
(started 2026-09-29). Harmless, but say "goal 3" or drop the number.

L19. Goal 14 item 1 "what P9 approves for radio URLs" reads like a radio feature (K64 declines a radio
directory). Say "stream URLs (Icecast, HLS) that UPnP renders, HA TTS and stored alarm URLs carry".

L20. P7 must not recommend dropping Soloist because its terms are unclear: K66 chose "Yes, it's
official" over "only if terms are clear". Say P7 reports the terms and the instance limits and builds
the design either way.

L21. K57 row text says Google Cast is infeasible and K60 excludes it; P6 may still list AirPlay 2 or
Spotify Connect as "legal" only if official and unreverse-engineered. State that P6 cannot re-open
librespot, reverse-engineered AirPlay 2 or Cast (it is in §0.8, but P6's question line does not say so).

L22. "at most ~3 agents at once" in every goal file; K29 is exactly 3. Drop the tilde.

L23. §0.11 says `claude --version` = 2.1.284; it is 2.1.285 now (auto-update). Say "2.1.28x at
planning" or drop the row.

L24. §0.11's ESP32-P4 revision fact re-verified locally (`ESP32P4_REV_MAX_FULL` default 199 in
v5.3.6), but the ESP-IDF pin should record the tag's commit (§0.9 pins "version and digest"); goal 2
records `git -C /cache/esp/esp-idf-v5.3.6 rev-parse HEAD` in the toolchain doc.

## What checked out

- Goal files: all 22 under 4,000 characters (`LC_ALL=C.UTF-8 wc -m`, max 3,318 for g3); every
  lettered line byte-identical to the brief's done-when block (script comparison, 0 mismatches, re-run
  after the 23:36:47 rebuild); each has a precondition and a BLOCKED exit; goal 1 says it never writes
  `CHECKPOINT-K.approved`; goal 2 prints the approval file.
- Decisions: 96 of 96 rows in §1 equal `decisions.md` after the name scrub (script); the K30 edit made
  at 23:36 is in the rebuilt brief. K56 over K14/K15, K92 over K62's pairing (goal 4 and §4.8 use
  trust-on-first-use), K82 over K53, K28 amended (open-only PRs) are applied in §0 and the goals.
  Declined options stay out of the goal sections (§0.8 list is complete against the "Not chosen"
  cells); the research's OpenHome note checks K64; `research-ha-integration.md:256` keeps K64.
- Environment facts re-run: `cpu.max` `200000 100000`; `memory.max` 17179869184; `gh auth status`
  logged in as the owner's account via `GH_TOKEN`; repo PRIVATE, default `main`; SSH to GitHub works
  (workflow pushes over SSH are possible); `cargo fmt --check` on 1.98.0: 325 hunks in 83 of 129 `.rs`
  files; clippy on 1.98.0: 56 distinct warnings; ESP-IDF v5.3.6 at `/cache/esp` (3.0 G) activates
  (`idf.py --version` -> v5.3.6, xtensa GCC 13.2.0); gitleaks 8.30.1: no leaks in history or tree;
  no `/dev/snd`, no USB serial, `ulimit -r` 0; `docker` CLI with no daemon; Python 3.13.5;
  `core.hooksPath` as stated; about 585 G free now.
- Repo facts in §2 re-run: 437 tracked files; 12 crates; 44,272 lines of Rust; 10,270 lines of C;
  33 shell scripts; ADR numbers 0012 x2 and 0021 x3; 6 measurement files besides the README; 20
  commits, 17 S-numbered; no LICENSE; `verification-record.md` 1,243 lines; PR #18 open with branch
  `sdd/S0124-...`; `control.rs:217` includes `docs/control-page.md` and `deploy/Dockerfile` copies no
  `docs/`; `sync_sim.c:47` `finite` and `monotonic.c` `vTaskDelay`; 8 `unknown` amp keys;
  `link_transport = wireless`; no `i2s_channel_write`; README:84 "never driven a pin".
- Goal 1's docs-only PRs do not trip the still-active pinning scanner (it reads Dockerfiles,
  workflows and `*.sh`/`*.yml`, not Markdown).
- Licences: MIT OR Apache-2.0 for chorus is compatible with MPL-2.0 Symphonia as an unmodified
  dependency (with the ADR and notice, L9) and with BSD libopus, dr_flac and ESP-IDF (Apache-2.0).
  shopkit packages are `LicenseRef-Proprietary` (`packages/core/pyproject.toml:7`); chorus consumes
  only exported design records (data) from `shopkit-acoustics`, so its licence does not reach chorus;
  the ADR goal 19 writes is the right place.
- Identity in the draft: no owner first or last name as a word, no email, no private IPv4, MAC, SSID
  or `.lan`-style hostname in `.claude/` (the scan allowing `NEEDS-NOAH`); the only issue is the account
  handle (M3) and L17.
- No em dash in any draft file.
- Loops are bounded (3 fix rounds, one re-run per hung or rate-limited agent, timeouts on every long
  command) except L13.
- The owner's physical steps never gate a goal except Checkpoint K (by design, K2) and INCOMPLETE
  foundation lines; hardware answers are re-checked, never waited on.
- `.claude/settings.json` matches the sibling programs' shape (`autoMemoryEnabled: false` plus `env`).

## Verdict

Not ready to land as v1. Fix H1-H4 (a settings line, a gate check, the devices row lifecycle and the
goal splits) and M1-M15 in v2; the LOW items are one-line edits. None of the findings needs the owner
before landing except the choices the brief should put in the Checkpoint K packet: M3 (the account
handle), H3 (e) (designs in devices during the run) and M4's inferences.
