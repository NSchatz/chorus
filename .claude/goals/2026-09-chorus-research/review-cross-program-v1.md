# Cross-program seam review: chorus 2026-09 brief v1

Reviewer: fresh cross-program reviewer, 2026-09-29, read-only. Sources read: the brief draft
`/cache/wt/chorus/goals-2026-09/.claude/goals/2026-09-chorus.md` (line numbers as of about 23:50;
the file was being edited during the review, so cite by section first); shopkit
`origin/main` = `64740a2` (the sibling clone plus a blobless full-history clone under
`scratch/xp/shopkit`); devices `1553ba1`, 3d `71164c9`, home `27aeff9`, homelab `d82e2ae`,
inventory `2a9eded` (depth-30 clones under `scratch/xp/`); `gh pr list/view` on shopkit and
homelab; `/cache/locks`.

## Summary

The seams chorus shares with shopkit (package creation, the requests and Needs files, branch
names, the release protocol, lock names) mostly match what the other programs do. One seam does
not hold. The hardware path of goals 19-21 depends on devices having finished, and devices'
last goal, goal 5, is blocked on glide6's Checkpoint A. Checkpoint A needs the owner to print and
pick mouse shells twice, and nothing is picked yet. 3d is in the same state: its goal 8 waits on
the owner's PLA campaign. So neither program is likely to have finished when chorus reaches goals
19-21. Once devices' goal 4 finishes, devices will be idle, and nobody will read chorus's rows. The
brief's fallback ("a draft kept on a chorus branch") has no location, no gate and no end. The
brief's wording "after its last COMPLETE line" can also be read as satisfied today.

Next in weight: devices' own scope rules ("buy what can be bought", no selling or FCC) bind
`builds/chorus-*` but are never named. Every BOM line needs an inventory part record that devices
owns. The shopkit gate for a new package or a release tests all 28 packages, so it takes about
5x the 175 s the brief quotes. The identity step is skipped only in chorus's shopkit PRs. Two
homelab collisions are not handled. First, homelab's own open P3 network PRs (#227-#233) edit
the files chorus's network and telemetry PRs will touch. Second, the brief has no
"related open PRs" rule. Finally, the Soloist sidecar needs a second host-network exception that
no owner decision grants.

Counts: 1 HIGH, 11 MED, 12 LOW.

## Findings

### HIGH

**H1. "devices has finished" will almost certainly not happen during goals 19-21, so the
serve-your-own-row path is closed. The fallback is undefined, and "last COMPLETE" is ambiguous.**

- Evidence:
  - devices brief §0 goal table (`devices/.claude/goals/2026-09-devices.md:24`): goal 5 runs
    "after the owner's Checkpoint A". Its precondition (§9, line ~954) needs `pick.toml` `round2`
    set and a `look_approved` entry.
  - `hardware/mouse/glide6/pick.toml` is "Committed UNSET".
  - `CHECKPOINT-D.approved:5` reads "glide6 goal 2 last (goal 5) ... goals 2-4 do not wait on
    Checkpoint A". Line 7 of the same file says "Not approved here (stay the Needs list): Checkpoint A
    (A1-A2 print and pick, A3 the look ...)".
  - devices is on goal 3 (`2026-09-devices-g3.status.md`, research rows TODO).
  - 3d: goals 1-7 are COMPLETE. shopkit `NEEDS-NOAH.md:190-199` says "3d goal 8 ... is BLOCKED
    until your PLA campaign's records are complete".
  - chorus brief §0.1 line 79 says "after its last `COMPLETE` line on `main`". Read literally,
    that is true today (`COMPLETE (goal 2)`). §0.13 line 390 has the right sense ("the owner's
    program has finished") but never names the goal.
  - Goal 19 item 3 (line ~1318) says "otherwise the row plus the draft kept on a chorus branch
    named in the row, and NEEDS-OWNER-PROGRAM". It names no repo, gate or follow-up.
  - K50 counts "every hardware design packaged in devices" as program success.
  - Once devices goal 4 completes, devices goes idle until the owner finishes Checkpoint A. Rows
    filed in chorus goals 19-21 then sit unread, and devices goal 5 (glide6) is the next goal that
    would even see them.
- Fix to the brief:
  1. Define "finished" per owner, by file:
     - devices: `COMPLETE (goal 5)` in `.claude/goals/2026-09-devices-g5.status.md` on devices
       `main`.
     - 3d: `COMPLETE (goal 8)` in `2026-09-3d-g8.status.md`.
     - home: finished (`COMPLETE (goal 9)`, 2026-09-29).

     Also add: "a newer devices/3d program brief with an open ledger means not finished".
     Replace "after its last COMPLETE line" in §0.1 and K23's restatement with that definition.
  2. Add a Checkpoint K question, and file one devices row in **goal 1** while devices goals 3-4
     still run and can answer it. The question: may chorus open PRs in NSchatz/devices limited to
     `builds/chorus-*` before devices' goal 5 completes? The PRs would run under devices'
     `make check` and `make invariants`, hold a new `/cache/locks/devices-merge.lock`, and touch
     nothing else in devices. The same question covers the inventory part records those BOMs
     need, under `inventory-merge.lock`. This is a namespace delegation, like "packages a program
     creates are its own". devices' §0.13 (line 356: "edited only by their own program") needs
     the owner's word for it, so it goes in `CHECKPOINT-K.approved`.
  3. Make the fallback concrete:
     - The draft lives in **NSchatz/chorus** on a pushed branch `chorus-g<n>/devices-<build>`
       with no PR, and never in NSchatz/devices.
     - It passes devices' `make check` in a local, never-pushed devices worktree with the draft
       applied, and the row carries that tail plus the branch SHA.
     - Add a Needs item modelled on 3d's: "start a devices session to serve rows chorus-N..."
       (`NEEDS-NOAH.md:190`).
     - The finale report lists every such row.

### MED

**M1. devices' scope rules bind `builds/chorus-*` and the brief never states them.**

- Evidence: `devices/CLAUDE.md:47-61` ("Out of scope, don't research it, write it or add it to a
  BOM"):
  - no selling "in any form" (FCC/CE/UKCA, licence-for-distribution);
  - "Fabricating what can be bought, controllers above all. Buy the module; the custom work is the
    device";
  - no "second projects dressed as prerequisites".

  devices brief §0.8 adds: "A build that departs from 'buy what can be bought' exists only after
  the owner approves its proposal". The chorus side pulls against this. Goals 19-21 design electronics
  (TAS58xx, PoE+, a 2U multichannel amp, a soundbar), and K1/K9 frame the work as a project
  "outsiders could run". A custom board where a module can be bought would break devices' rule.
  A devices session could DECLINE such a row, or a devices-gate PR could be refused on review.
- Fix: in §0.13 "Printed and built parts" (or goal 19 item 3), quote devices' three
  out-of-scope rules as binding on every `builds/chorus-*` package:
  - bought modules only, unless P1/P12/P13 carries an explicit "departs from buy what can be
    bought" proposal that the owner approves at Checkpoint K or later;
  - no distribution, compliance or selling content in devices.

  Also have P1's options table flag which options need a custom PCB.

**M2. Every BOM line in devices must be an inventory part record, and the brief plans inventory
work only for the ESP32-S3 boards.**

- Evidence:
  - `devices/builds/README.md` says "`part_id`, `qty`, `notes`: part IDs must exist in the
    inventory repo". `devices/CLAUDE.md:180` says parts are inventory `PRT-` records.
  - devices owns "parts, fasteners and items" (devices brief §0.13).
  - home owns the "tools, consumables, stock, suppliers, locations, materials and receipts
    records", and "A supplier another program needs is a request row to home". devices goal 4
    files exactly such supplier rows.
  - chorus §0.13 line 376 lists inventory owners as "home (root, schemas), 3d (spools), devices
    (parts, fasteners, items)" and omits home's suppliers. §3.4 plans only I12's boards and
    "bench parts", and says "To home: nothing planned".
- Fix:
  - Goals 19-21 file (or, under H1's delegation, serve) the inventory part records for every BOM
    line, US-first by ship-from with price, date and URL, per devices goal 4 item 1's standard.
  - Supplier records are rows to home. home has finished, so chorus serves them itself in
    inventory under `inventory-merge.lock` and inventory's `make check`, citing the row.
  - Correct the §0.13 ownership row, and change §3.4 "To home" to "supplier records for new US
    sources (self-served; home has finished)".

**M3. shopkit's gate for a new package or a release is the full suite, not 175 s.**

- Evidence:
  - chorus §0.11 line 342 quotes "171-175 s". The planning run it rests on printed "affected
    packages: (none)" (`research-toolchain-env.md:311-366`), so no package tests ran.
  - `docs/ARCHITECTURE.md` §11: "a root file (`pyproject.toml`, `uv.lock`, ...) -> all
    packages". Adding a package edits root `pyproject.toml` and `uv.lock` (§12 steps 4 and 7).
    `release.py set` edits every `pyproject.toml`.
  - Measured elsewhere at 3 workers: shopkit #155 (new package) "make ci: PASS in 836 s"; #188
    894 s; #191 776 s.
- Consequences:
  - At chorus's 2 workers this should run roughly 20-30 min, held under `shopkit-merge.lock`.
  - devices and 3d wait on that lock with `flock -w 1800` (devices brief line 82), so their
    waits may time out.
  - The brief sets no `timeout` for shopkit's `make ci` (§0.12 lists only chorus's own commands).
- Fix: record in §0.11 that a package-add or release PR runs all 28 packages (about 13-15 min
  at 3 workers elsewhere, so expect 20-30 min here). Put `timeout 3600` on shopkit `make ci`.
  Run the full gate once on the branch before taking the merge lock, then take the lock only for
  the final up-to-date re-run and the merge.

**M4. The identity step is skipped only in chorus's shopkit PRs. The home clone also
contradicts "NSchatz/home: never touched".**

- Evidence:
  - The shopkit Makefile `identity` step: "a home without private/scrub-strings.txt, prints
    'skipped: no terms'". The planning run printed exactly that (`research-toolchain-env.md:364`).
  - Other programs run in a container with home's `private/` present, so their scans are real.
  - `verify-toolchain-conventions.md:213`: "`SHOPKIT_REPO_HOME=<empty dir>` satisfies the
    sibling check".
  - chorus §0.1 line 85 says home is "never touched", yet goal 19's precondition (line ~1305)
    wants "a sparse `home` clone".
- Fix: for every shopkit `make ci`, set `SHOPKIT_REPO_HOME=/cache/chorus-private/shopkit-home`,
  a directory whose `private/scrub-strings.txt` is generated from I5's
  `/cache/chorus-private/identity-terms.txt` in shopkit's format. The scan then really runs on
  chorus's text, commit messages and PR bodies, and no home clone is needed. Drop "sparse home
  clone" from §0.11 and goal 19.

**M5. Pushing a shopkit tag and committing to shopkit `main` are outside §0.7's GitHub allow
list, and K41 says tags "on chorus only".**

- Evidence:
  - §0.7 line 244 allows "branches and PRs in `NSchatz/chorus` ..., `v*` tags and GitHub
    releases on chorus (K41), PRs in shopkit, devices, 3d and inventory ...".
  - K41 (line 460): "Amends the contract's 'branches and PRs only' for `v*` tags and GitHub
    releases on chorus only".
  - Against that: §0.13's release protocol ("tag the release PR's merge commit by SHA and push the
    tag"), goal 19 line A ("released (PR, tag ...)") and K6's ledger-only commits to shopkit
    `main`. A goal's adversarial report checker could flag the tag push as a contract breach.
- Fix: add to §0.7 "a `v*` tag on shopkit, on the release PR's merge commit, per §0.13 (K44,
  K49), and ledger-only commits to shopkit `main` for chorus's rows and Needs items (K6)". Add a
  line to §1.1 noting that K44 extends K41 to shopkit tags.

**M6. No lock order is stated, so an inversion inside chorus is possible.**

- Evidence: §0.4 line 172 puts "shopkit's `make ci`" under `chorus-heavy.lock`. §0.3 and
  §0.13 put `make ci` "under the merge lock" and releases under the release lock. The order among
  the three is never stated, and the brief allows up to 3 agents.
- Two failure modes:
  - Agent A holds `chorus-heavy` (a 15-min `make gate`) and then wants `shopkit-merge`, while
    agent B holds `shopkit-merge` and waits for `chorus-heavy`. B's heavy wait has no `-w`, so
    the pair stalls until A's 1800 s wait expires.
  - Meanwhile B holds shopkit's shared merge lock idle, which stalls devices and 3d.
- The cross-program order is already safe: devices also nests release outside merge (brief §0.13
  C13, step 2).
- Fix: add one sentence to §0.4: "Lock order, outer to inner: `shopkit-release` ->
  `shopkit-merge` (or `inventory-merge`) -> `chorus-heavy`. Never request a shared lock while
  holding `chorus-heavy`. Take `chorus-heavy` with `-w 1800` when a shared lock is already
  held."

**M7. homelab collisions: the brief has no "related open PRs" rule, and homelab's own P3 network
PRs edit the files goals 6 and 13 will touch.**

- Evidence:
  - devices brief §0.3 line 96: "Each homelab PR lists the other program's open homelab PRs that
    touch the same files". Every devices and home PR does this: #224's body has a "Related open
    PRs" section naming #191, #192, #225, #230 and #233.
  - chorus K28 (line 447) and §0.1 have no such rule.
  - Open now, by homelab's own sessions:
    - #227 edits `ansible/inventory/host_vars/opnsense.yml` and the OPNsense plugins, including
      the mDNS repeater (Trusted, IoT, Servers) and the SSDP relay (Trusted, Servers). These are
      the exact levers of chorus's P3 speaker-network PR.
    - #229 readdresses the server and edits `monitoring/stack/prometheus/prometheus.yml`, where
      goal 13's scrape job goes.
    - #228-#233 retire services.
  - chorus §2 says OPNsense is "staged, not live". That is still true, but the cutover is in
    flight.
  - The label "P3" also collides: homelab titles its network work "(P3)", while chorus's P3 is
    speaker placement.
  - Goal 15's integration PR will touch `home-automation/homeassistant/` (the compose bind,
    perhaps `integrations.json` and `registry.json`), which devices' #224/#225 and home's
    #191/#192 edit.
- Fix:
  - Add to §0.1's homelab line: "each PR lists every other open homelab PR (any program or
    homelab's own) touching the same files, with overlap or not per hunk". Model it on #224.
  - Goals 6 and 13 re-read the state of homelab's P3 cutover (`docs/network.md` "Cutover phase
    B") at goal start and base the PR on current `main`.
  - In homelab titles and bodies, write "chorus P3" (or rename the proposal "speaker network").

**M8. The Soloist sidecar needs a second host-network exception, and no owner decision grants
it.**

- Evidence:
  - Goal 14 item 3 (line 1165): "one `chorus-soloist` container on the host network". K34
    grants host networking to **chorus-server** only.
  - homelab `CLAUDE.md:201`: "never `privileged` or the host network". `docs/security.md:35`
    lists only `plex` and `mdns-reflector`, and ci.yml line 196 enforces `"network_mode host":
    {"mdns-reflector", "plex"}` name for name.
- Fix: P7 must weigh two options for the sidecar and recommend one. Option 1 is bridge
  networking plus the existing `mdns-reflector`. Option 2 is host networking, which needs a new
  exception. If host networking is recommended, the goal-14 PR adds `chorus-soloist` to both
  lists with its justification, and the PR's Needs item says the owner is approving a **new**
  exception beyond K34. State this in goal 14.

**M9. The Needs file has no "Checkpoint K" item and no Gates line.**

- Evidence:
  - `NEEDS-NOAH.md:9-15`: the Index's "Gates" list holds every program's checkpoint (H, P, D,
    glide6's A). Each program's section has "Checkpoint X: review goal 1 and approve"
    (`NEEDS-NOAH.md:31`, `:224`, `:450`).
  - chorus goal 1 item 5 lists five Needs items and none is Checkpoint K. Done-when E checks
    only those five. §0.6 and §27 never mention it.
  - Goal 2 is BLOCKED until the owner writes `CHECKPOINT-K.approved`.
- Fix: goal 1 adds "Checkpoint K: review chorus goal 1 and approve" to `## chorus`, and adds
  "Checkpoint K (chorus)" to the Index's Gates list with the packet path. Extend done-when E to
  "the five items plus the Checkpoint K item and its Index gate line".

**M10. The skeleton commit to shopkit's `PROGRAM-REQUESTS.md` is underspecified: the header,
the ownership row and the row ID prefix are missing.**

- Evidence:
  - The file's header and "How it works" name only "home, 3d and devices (2026-09-28 programs)"
    and their three briefs (`PROGRAM-REQUESTS.md:1-7`).
  - The ownership table (lines 20-25) has never been edited since its creating commit `53f77f1`
    (the planner's "request board" commit).
  - Row IDs are `<program>-<n>` (`3d-1`, `dev-7`, `home-12`), and the row format has a `#` and a
    "From (program, goal)" column (lines 27-30).
  - chorus §0.10 and goal 1 check only for `## To chorus` and `## chorus`. §0.13 line 382 lists
    the row fields without `#` or "From". K44 says "chorus joins shopkit's ownership table", but
    no step adds the row.
- Fix: state what the planner's landing commit (not a goal) must contain, and extend the goal-1
  precondition check to match:
  - "home, 3d, devices and chorus" in the title, and `2026-09-chorus.md` in "How it works";
  - an ownership row `chorus | acoustics (created by chorus goal 19)`;
  - `## To chorus` with the row-format header;
  - no other line changed.

  In §0.13, give chorus's row IDs as `chorus-<n>` and the full column list.

**M11. The HA integration must not add an unauthenticated endpoint (a homelab rule).**

- Evidence: homelab `CLAUDE.md:191-192`: "HA gets no Docker socket, ... and no integration that
  adds an unauthenticated endpoint to its public route". `docs/home-automation.md:161` shows a
  core integration refused for exactly this reason. HA is public (`<public-hostname>`,
  K40). Goals 15 and 16 (announcements, artwork, voice) are where a `requires_auth=False` view
  or a non-`local_only` webhook tends to appear.
- Fix: goal 15 item 4 adds this rule. A test asserts that every HTTP view the integration
  registers has `requires_auth=True` and that every webhook is `local_only`. The homelab PR body
  states that it adds no unauthenticated endpoint.

### LOW

- **L1. The status facts in §0.13 are stale** (line 364). devices is on goal 3. home has
  **finished** (`COMPLETE (goal 9): 2026-09-29`; `NEEDS-NOAH.md` Index "home's program is
  finished"). 3d has finished goals 1-7, and goal 8 is blocked on the owner. Consequence: a "row
  to home" (§0.1 line 85) will never be served, because home has finished and chorus may not
  touch home. Fix: say so. Home needs that live outside the home repo (inventory suppliers,
  homelab `registry.json` areas) are self-served under home's rules. Anything inside the home
  repo is NEEDS-OWNER.
- **L2. HA areas are home's vocabulary.** home brief lines 92-94: "HA labels and areas come from
  homelab's existing `registry.json`, or are added in one PR agreed through a request row". Fix:
  goal 15 uses existing areas (or only `suggested_area`). A new area name follows that rule,
  self-served since home has finished.
- **L3. "Ask once" is scoped too narrowly.** §0.6 line 217 says "Search the section before
  adding", but the file's rule is to search all of `NEEDS-NOAH.md` and the devices build logs
  (devices brief §0.6). Example: filament goes on 3d's "one list", and there is one HA token,
  home's. Fix: search the whole file and the build logs.
- **L4. Device Needs items belong in the build log.** devices §0.6 puts device items in
  `builds/<device>/log.md` "the Needs list" (the template's heading), and K27's "no personal name in
  any tracked file" collides with devices' template and prose. Fix: in devices, follow devices'
  template (K27 governs chorus's repo and PR bodies), and point from `## chorus` to the build
  log.
- **L5. The shared-root-files list omits the toolchain manifest.** shopkit-toolchain's
  `manifest.toml` is shared (`PROGRAM-REQUESTS.md:25`; devices §0.13) but missing from chorus's
  §0.13 table. Add it.
- **L6. The licence default is left open.** `docs/ARCHITECTURE.md` §12 step 1 requires
  `license = "LicenseRef-Proprietary"` for every package. No test enforces it, and shopkit has no
  LICENSE file. Fix: default to shopkit's rule unless the owner amends it at Checkpoint K. chorus
  never vendors shopkit-acoustics code and consumes only its exported design record (data), so
  chorus's allowlist (§4.7) is unaffected. Say this in goal 19.
- **L7. The package checklist misses one root edit.** shopkit's `shopkit.toml [rules.prefixes]`
  says "a new letter is a deliberate edit here", and ARCHITECTURE §12 does not list it. If
  shopkit-acoustics has rules, goal 19 adds its prefix there.
- **L8. Decision IDs can be misread in shared files.** shopkit-cad's rules use K-numbered IDs
  (`packages/cad/rules/K.md`, e.g. `cad:K29`, shopkit #142's title "cad: K29"). In
  shopkit, devices and homelab text, cite chorus decisions as "chorus K44". The Checkpoint letter
  K itself is unused: A, D, F, H, P and T are taken, and holdfast is T.
- **L9. homelab title format.** In practice titles read `<type>(<scope>): <summary> (<program>,
  never merged by agents)` (#224, #225, #191, #192). Spell the full format out in §0.1.
- **L10. Telemetry alerts are Prometheus rules.** Goal 13's alerts go in
  `monitoring/stack/prometheus/rules.yml` with promtool unit tests (homelab ci.yml "Prometheus
  rules and rule unit tests"), routed through homelab's single HA-webhook channel. There is no
  Alertmanager. Add a pinned rootless `promtool check rules/test rules` to K28's rootless check
  list for that PR.
- **L11. The Traefik route needs a tinyauth group.** homelab's `tinyauth.yml` is deny-by-default
  (CLAUDE.md "Identity"), and the CI "Router policy" step checks it. Goal 3's Traefik route
  therefore needs an explicit `household` group entry.
- **L12. Rows open at the end need an owner-facing item.** devices §0.13's end rule reads "both
  programs' final reports list it, and the owner's the Needs list section gets 'start a follow-up
  goal for rows ...'". chorus's finale lists open requests but adds no such Needs item. Add it.
  Also note that devices' `CLAUDE.md` is at 197/200 lines, so there is no room there for a
  chorus pointer.

## What checked out

- **Branch names** match shopkit practice. `chorus-g<n>/<topic>` and `chorus-release/v<ver>`
  follow `dev-g3/auto-v14`, `3d-g7/...`, `home-g6/carpentry` (a new package) and
  `dev-release/v1.19.0`, `3d-release/...` (`gh pr list`).
- **The release protocol** in §0.13 matches devices brief §0.13 (C13) step for step: `fetch
  --tags`, the next free version, a minor if anything is new, `release.py set` then `uv lock`,
  `make ci` under the merge lock, and tagging the merge commit by SHA. `tools/release.py` behaves
  as described.
- **Creating and owning a package is allowed.** "Packages a program creates are its own"
  (devices §0.13 C12). home created hass, sizing, repair and carpentry this way. ARCHITECTURE §12
  asks for "an ADR or a brief amendment", and chorus plans an ADR plus the §12 checklist.
- **The requests protocol** matches the file's own rules: append rows, only the owner changes
  state, the requester may withdraw, ledger-only commits with pull --rebase and retry, and
  self-service after the owner finishes (`PROGRAM-REQUESTS.md:9-18`; devices §0.3 and §0.13).
- **The Needs file plan fits.** A `## chorus` section is in line with its grouping by repo, and
  the Index says "each program updates its own lines" (`NEEDS-NOAH.md:9`). home wrote the Index
  and has finished, so each program maintains its own lines.
- **Lock names do not collide.** `/cache/locks` has `3d-kernel`, `dev-kernel`, `home-kernel`,
  `home-merge`, `holdfast-heavy`, `inventory-merge`, `shopkit-merge`, `shopkit-release` and
  `chorus-heavy`. The shared names match.
  - `flock -o` locks the same flock(2) lock as the others' plain `flock`.
  - `-w 1800` matches devices.
  - Nesting release outside merge matches devices, so there is no cross-program deadlock.
  - `/cache` is the shared `claude-cache` volume (homelab `claude/*/docker-compose.yml`), so the
    locks work across containers.
- **Inventory ownership is right**: devices owns parts, fasteners and items, so I12's row to
  devices goes to the right owner.
- **The homelab mechanics match.** Host-network exceptions are one list, in `docs/security.md`
  and in ci.yml's `network_mode host` set, name for name. K34's plan to add chorus-server to both
  is the right mechanism. Never-merged PRs, the owner-action text in the body (homelab issues
  are the owner's) and the rootless checks match devices' #224. K24 and K28 are owner decisions,
  so C15's "home and devices only" does not bind chorus.
- **`SHOPKIT_REPO_HOME`**: any directory satisfies the sibling check (verified in the planning
  research). See M4 for making the scan real.
- **The Checkpoint letter K** is unused as a checkpoint by any program.

## Verdict

**Not ready to launch as written. Fix H1 first.** H1 is cheap to fix in the brief: a precise
"finished" definition, a goal-1 delegation row plus a Checkpoint K question, and a concrete
fallback. Without that fix, goals 19-21 will almost certainly end with five NEEDS-OWNER-PROGRAM
packages and no gate evidence, and K50's "every hardware design packaged in devices" cannot be
met. M1-M11 are one-to-three-sentence edits each, and several (M3, M4, M6, M9, M10) change what
goal 1 or goal 19 actually does on the host. The LOW items can ride along. Once H1 and the MED
items are applied, the seams with shopkit, inventory and homelab are sound.
