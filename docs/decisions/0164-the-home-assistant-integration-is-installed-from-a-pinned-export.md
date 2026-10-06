# 0164: the Home Assistant integration is installed from a reproducible export of one commit, vendored and mounted read-only, never through HACS; the first pin is 180bbf8

- Status: accepted, 2026-10-04. Builds the install of goal 19 (program brief section 23,
  item 3) on 0138 (the integration) and 0163 (the dashboard).
- Decided by: the owner for the form (K61: installed as a custom integration from a pinned
  copy, HACS impossible; K42: the upstream submission is the owner's, carried as a draft;
  K28: changes to the owner's homelab repo are opened and never merged, applied or deployed
  by an agent); this record for the export's shape, the lock's format, what is checked
  where, and the pin.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `tools/ha-export.sh`, `tools/conventions/check-ha-export.sh`,
  `docs/home-assistant.md` ("Installing: the pinned copy", "The upstream draft"),
  `integrations/homeassistant/README.md` ("Installation instructions"); and, outside this
  repository, pull request #253 of the owner's homelab repo.

## Context

The integration (0138) lives in `integrations/homeassistant/custom_components/chorus` and is
what Home Assistant loads as `custom_components/chorus`. It has to reach the owner's Home
Assistant somehow. That Home Assistant runs from a digest-pinned image, its configuration is
code in the owner's homelab repo, bound read-only, and its public route is why the
integration may add no unauthenticated endpoint (brief section 4.8). The same repository
already vendors its dashboard cards by lock file and hash and does not install browser code
from HACS.

The integration's code is not changed by this record, and no release tag is cut for it.

## What was read

All on 2026-10-04. No GPL source and no reciprocally licensed design was opened.

- The owner's homelab repo at its `main`: the Home Assistant compose file (the image tag
  2026.9.3 and its read-only binds), its home-automation documentation (what is code and
  where, the dashboards and the dormant-card rule, the vendored cards and their lock, the
  owner-action format), its gate's definition (which steps a vendored directory passes
  through: Python syntax, YAML lint, the secret scan), its deploy script (a changed file
  under a stack recreates the service that binds it) and its dashboard check (which
  visibility conditions count as hiding a card). A private repository; no path of it beyond
  Home Assistant's own (`/config/custom_components`) is repeated here.
- This repository: `tools/ha-hassfest.sh` and 0138 ("The tier") for what differs between the
  custom and the core form; `integrations/homeassistant/custom_components/chorus`
  (`config_flow.py`, `const.py`, `manifest.json`) and `crates/` at `v0.1.0`, `a835a5f` and
  the pinned commit for "What was found"; 0163 and `dashboard/chorus.yaml`.
- `sha256sum --check --strict --quiet` of GNU coreutils 9.7, run here and not read (neither
  its source nor its manual): it accepts a checksum file whose first lines start with `#`
  and checks the rest. That is observed behaviour of one version; the lock's check in the
  gate (`check-ha-export.sh`) would fail on a `sha256sum` that did otherwise.
- Home Assistant's developer documentation, as cited in 0138 ("What was read"), for the
  core form; nothing of it was read anew for this record, which is why the list of what a
  submission needs is marked to be read again.

## Decision

**The install copy is an export of one commit.** `tools/ha-export.sh <dir> [<commit>]` writes
`<dir>/chorus`, the integration's tree as that commit has it, and `<dir>/chorus.lock`. It
reads the commit's objects from git and never the working tree, refuses anything that is not
a plain `100644` file, and writes files `0644` and directories `0755`. The lock is comment
lines (the commit, `manifest.json`'s version, the Home Assistant version of `harness.pin`
at that commit, the file count) followed by `sha256sum` lines sorted bytewise, so plain
`sha256sum --check --strict chorus.lock` verifies a copy where no chorus checkout exists. No
date, host or user is written: two exports of one commit are the same bytes.

**`--verify` is the stronger check**, for where a chorus checkout exists: the hashes, the
file list (nothing extra, nothing missing, no link), and the lock against a fresh export of
the commit it names. The last part is what catches a file edited together with its hash
line, which a self-contained hash file cannot.

**The check** is `tools/conventions/check-ha-export.sh`, a conventions check (rule 25) and so
a step of both gate tiers: two exports compared byte for byte and by mode; the lock compared
with the sha256 of each blob computed apart from the tool; an export made from a scratch
clone with an edited file, an untracked file and byte code beside the sources; an export over
an earlier one; a refused foreign directory left untouched; and six bad copies `--verify`
must refuse, each by name.

**The installation vendors the copy and mounts it read-only.** In the owner's homelab repo
the copy and its lock are committed beside the Home Assistant configuration and bound
read-only at `/config/custom_components/chorus`; that repository's own gate holds the copy
to the lock with coreutils. Home Assistant can load the integration and cannot write to it.

**HACS is not used**, and would not be if it could install it: an update would then arrive on
HACS's schedule from whatever the repository's head is, outside the review that every other
change to that Home Assistant goes through.

**A pin is a commit on `main` whose gate passed.** The first pin:

| | |
|---|---|
| Commit | `180bbf8897171e5c65b7f9940cfe0ecb06abd16c` (the head of `main` when the pin was taken; it holds the dashboard the installation copies. The integration's own files last changed in `ab17e8d`) |
| Integration version | 0.18.0 |
| Home Assistant it was tested under | 2026.9.3 |
| Files | 29 |
| sha256 of `chorus.lock` | `4f3603bcc550039eb2cd0b2eddfe2f62feb44be08365d18e0434dadfba88792d` |
| Delivered as | pull request #253 of the owner's homelab repo, opened and left open; merged 2026-10-04 on the owner's word (see "Install status") |

`tools/ha-export.sh <dir> 180bbf8897171e5c65b7f9940cfe0ecb06abd16c` reproduces it from any
checkout that has the commit.

**The dashboard is delivered beside it, adapted.** The owner's homelab repo hides a card
whose entity does not exist by a positive visibility condition, so its copy of
`dashboard/chorus.yaml` carries one on every section (its room's media player in one of the
states it has when it exists) and is otherwise this repository's file. The entity ids stay
the example house's until the rooms exist. `docs/home-assistant.md` says how an installation
adds the condition; the example here is unchanged.

**Two owner's actions follow, each an item in the owner's queue:** the install (merge and
deploy that pull request, then add the integration in Home Assistant's UI), and the upstream
draft (submit the integration to Home Assistant core, when the owner chooses).

**The upstream draft is the integration at a commit, plus a list.**
`docs/home-assistant.md`, "The upstream draft", is the list: the changes
`tools/ha-hassfest.sh` already makes to grade the integration as core (the directory, no
`version`, no `issue_tracker`, the core documentation URL, the domain in `.strict-typing`,
no `brand/`), and what a submission needs beyond them (the client library published instead
of vendored, the tests moved onto core's fixtures, the documentation page, the generated
files).

## What was found

The pinned integration cannot be set up against the chorus server the owner's homelab repo
deploys today. Its config flow asks the server for `GET /api/server` and for control catalog
version 2 (`custom_components/chorus/config_flow.py`, `const.py`). Read from this repository
at each commit: the v0.1.0 release has catalog version 1 and no `/api/server`; commit
`a835a5f` (the image a later open pull request there pins) has catalog version 2 and no
`/api/server`; the pinned commit has both. The flow refuses such a server by name and
creates nothing, so vendoring and mounting the copy is safe before the server moves, and
adding the entry waits for a server image built at the pinned commit or later. The install
item in the owner's queue says so. No server release or image is part of this record.

## Not chosen

- **HACS with a private repository token.** It puts a repository credential into Home
  Assistant and keeps the update outside review.
- **A release archive (a tarball attached to a tag) as the unit.** It needs a release per
  pin and a tag was not wanted for this; a commit already names the bytes, and the export is
  reproducible from it.
- **A git submodule or subtree in the installation.** Either brings the whole repository or
  its history into a repository that needs 29 files, and a submodule needs credentials at
  deploy time.
- **Binding a chorus checkout on the host into Home Assistant.** Whatever the checkout's
  working tree holds would run; the copy in the installation's own repository is reviewed
  and hash-held.
- **A lock in JSON.** `sha256sum` lines need no tool beyond coreutils to check.
- **Exporting from the working tree** (a copy of the directory). Byte code, an editor's
  backup or an uncommitted edit would ship, and the lock's commit would then be untrue.
- **The dashboard inside the export.** The installation has to adapt it (its rooms, its
  dormant-card rule), so a hash would hold nothing.

## Consequences

- An installation's copy can always be traced to a commit and reproduced; a difference is
  found by coreutils there and by `--verify` here.
- Nothing updates by itself. A new pin is a deliberate export, a reviewed change in the
  installation and a restart; `docs/home-assistant.md` says when one is due (the
  installation's Home Assistant version moved, the server moved past what the integration
  speaks, a fix is wanted).
- The lock records the Home Assistant version the tests ran under. It is not compared with
  the installation's Home Assistant version by any check here; the rule "the harness pin
  moves when the homelab's pin moves" (0138) is what keeps the two together.
- ASSUMED, not seen: that Home Assistant 2026.9.3 in the owner's image loads the copy from
  a read-only bind nested in its configuration directory (the tests load the same files
  under the same version; Python cannot write byte code beside them and does not need to),
  and that the frontend hides a section whose `state` condition names a missing entity.
  Both are checks of the owner's install action.

## Install status

(Harness task 25, 2026-10-06.) Homelab #253 was merged on 2026-10-04 (squash `10162a4`) on the
owner's word, "Merge on the replayed checks", without a `make ci` run of the merging session's
own (homelab's checks need a Docker daemon). Still the owner's: `scripts/update-all.sh` on the
host, adding the entry (Host `10.230.0.1`, the proxy bridge's gateway that homelab's
`networking/traefik/dynamic/chorus.yml` names; Control port `4020`) and naming each room's entity
prefixes so `dashboards/chorus.yaml` gets the real rooms.

The entry needed a server at 180bbf8 or later; the deployed `g17-a835a5f` predates it. Asked on
2026-10-06, the owner chose to publish `ghcr.io/nschatz/chorus-server:main-c78a008`
(`sha256:635b303b...e4b`, which also carries the real-time fix of 0225) and to re-pin homelab
to it (homelab task #206, the owner merges and deploys).
