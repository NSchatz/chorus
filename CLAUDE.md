# chorus

From-scratch multiroom + surround audio: containerized server, custom sync protocol,
embedded speaker firmware. BRIEF.md is the guiding document: goals, constraints,
recommendations, and open decisions. It recommends; it does not dictate.

## Working agreement

1. Propose before committing to anything expensive to reverse. Cheap decisions:
   make them, note them in docs/decisions/.
2. Hard guardrails (BRIEF.md 3.1): clean-room vs GPL references; no eFuse burns on
   dev hardware; measurement-backed timing claims; monotonic clocks in the audio
   path; no em dashes anywhere. Clean-room means agents never open the source
   files of GPL projects or the design files of reciprocally licensed hardware
   (CERN-OHL-S, GPL); their docs, issues and specs only, and permissive source
   may be read and cited (`docs/clean-room.md`). (R3, decided 2026-09-29 by the
   owner, K33, K39.)
3. Prefer building over vendoring when small and instructive; vendor the large and
   undifferentiated; log gray-zone calls.
4. The sync engine is the project. Simulator first, hardware second, measurement
   always. Reports go in docs/measurements/.
5. Protocol, sync, and DSP cores are pure libraries with shared fixtures so the
   Rust and C implementations cannot drift apart.
6. Items marked "verify against the datasheet" are starting points, not truth.
7. Finish a phase's "success looks like" before moving on, or say why not.
8. A technology or toolchain choice here is justified on fitness for chorus's own
   requirements alone. Which toolchains happen to be installed, on `PATH`, or
   absent in any development container is never an admissible reason for or
   against one: an absent toolchain is installed, not designed around. A
   decision record that cites availability is a defect. (R4, decided 2026-09-29
   by the owner, K11: the old umbrella citation dropped, the rule kept.)

## The plan of record

(R1, decided 2026-09-29 by the owner, K1, K4, K11, K12, K28, K93: the retired
umbrella's section, its specs, stages and pointers, is removed.)

- Work arrives as tasks from the owner's agent harness, each with a finish line written as
  runnable checks; the /goal program's remaining goals are such tasks now. Its brief (§0 the
  parameters, §4 the engineering rules), goal files and ledgers are pinned at
  https://github.com/NSchatz/chorus/tree/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals. A phase's acceptance comes from
  BRIEF.md section 8's "success looks like" and the program's done-when lines.
- Conventions and the rule-to-check table: `docs/conventions.md`. CI is the gate
  (`docs/decisions/0140-ci-is-the-gate.md`, decided by the owner 2026-10-04):
  `.github/workflows/ci.yml` runs `make gate` (= `make tier-full`) on every pull request,
  on every push to main and nightly. A PR merges when an independent reviewer passes it;
  the merge does not wait for CI, and a red main gets a fix-forward task. `make gate-fast`
  is the conventions checks alone.
- Local runs are narrow tests only: one crate's focused test on a built tree, or one
  `tools/conventions/check-*.sh`. No gate, tier, whole-workspace cargo build or test, or
  image build runs on the development host. A measurement that has to run here (a QEMU
  run, a soak) holds `goals lock goals-heavy -- goals lock chorus-heavy -- ...`, subagents'
  included; a throwaway build keeps its cargo target directory where `tools/build-dir.sh`
  says (`/scratch` when it fits, else `/cache/wt/chorus/target/shared`).
- Flashing, eFuses, deploys to the homelab and OTA installs on installed speakers are the owner's actions (K4, K28, K93); nothing in this repo sets `CHORUS_OWNER_AT_BENCH`. Each one is an issue in the owner's queue in the owner's agent harness (`goals needs add`, `/goals:needs`), and a request to another repository is a `from:chorus` issue (`goals request add`).
- BRIEF.md is kept current by the work: verified corrections and the owner's
  decisions are written into it, each dated with its decision IDs. Its section
  3.1 guardrails are never relaxed or removed by any task; they may only be
  tightened. When measurement contradicts BRIEF.md, say so in `docs/decisions/`
  and correct the brief in the same change. (R2, decided 2026-09-29 by the owner,
  K47; limit I2.)
