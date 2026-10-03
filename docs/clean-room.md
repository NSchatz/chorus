# Clean-room provenance

chorus is written clean-room against GPL-licensed reference projects (BRIEF.md section 3.1
rule 1), tightened for agents on 2026-09-29 by the owner (K33, K39): a GPL project's
documentation, issues, papers and protocol descriptions may be read; its source files are never
opened; a reciprocally licensed hardware design (CERN-OHL-S, GPL) is never opened.
Permissively licensed source (MIT, Apache-2.0, BSD) may be read and cited. The rule and its check
are `docs/conventions.md`, "Clean-room provenance".

## Projects whose source is never opened

Snapcast and snapcast-rs, the ESP32 snapclient, shairport-sync, squeezelite, go-librespot,
ESPHome's C++ runtime, gmrender-resurrect, upmpdcli, MPD, libcec, and any other GPL or LGPL
project. The GPL tools chorus runs as unmodified binaries (shellcheck, yamllint, cppcheck,
ccache) are used, never read. Mosquitto (EPL-2.0 / EDL-1.0, the homelab's MQTT broker) is held
to the same rule by goal 15's envelope: its man pages, documentation and issue reports were read
for `docs/mqtt.md`, its source was not, and chorus's MQTT codec was written from the OASIS
standard (ADR 0000 lists what was read).

## What is recorded, and from when

- **From 2026-09-29 (the program):** every research file lists what it read
  (`.claude/goals/2026-09-chorus-research/`, each with a sources or "What I read" section), and
  every proposal under `docs/proposals/` has `## What was read`.
- **From goal 3 (2026-09-30):** every proposal, every research note under `docs/research/` and
  every decision record numbered from 0032 carries a `## What was read` section with each
  source and its date; `tools/conventions/check-provenance.sh` fails a file without one.
- **Before the program (decision records 0001-0024 and the code they describe; 0022-0024 were
  numbered 0012 and 0021 until 2026-09-30):** no reading log was kept. What each record cites is in
  the record itself; nothing more can be reconstructed honestly, so none is claimed. Records
  0025-0027 were written on 2026-09-30 (goal 4) for decisions made before the program; each lists
  what its writing session read, not what the original implementers read. The audit of 2026-09-30
  (`docs/audit/2026-09-audit.md`, guardrail 1 and B-12) found no violation (no third-party code
  vendored, no code comment or record citing a GPL project, no "ported from", "adapted from" or
  "copied from" wording) and no record of what the implementing sessions read. Code that a later
  goal rewrites is written under the rule above.

## BRIEF.md section 11

BRIEF.md section 11 still lists Snapcast's "client stream/sync code" and three other GPL
repositories as study material, which K33 forbids agents to open. Reversal R17 (decided
2026-09-29 by the owner, K33, K39) rewrites it to "docs, issues and protocol descriptions only";
goal 4 makes that edit with the other BRIEF reversals. Until then this file and K33 govern: no
agent opens those repositories' source.
