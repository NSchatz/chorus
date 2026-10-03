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

- The plan of record is the /goal program in `.claude/goals/2026-09-chorus.md`
  (the contract is NSchatz/goals spec v1.1, the `/goals:spec` skill; the brief's §0 holds this
  program's parameters and §4 its engineering rules; each goal's ledger is
  `.claude/goals/2026-09-chorus-g<n>.status.md`). A phase's acceptance comes from
  BRIEF.md section 8's "success looks like" and the program's done-when lines.
- Conventions and the rule-to-check table: `docs/conventions.md`. The gate is
  `make gate` (a caller holds `goals-heavy`, then `chorus-heavy`:
  `goals lock goals-heavy -- goals lock chorus-heavy -- make gate`); `make gate-fast`
  is the conventions checks alone. Tiers: `make tier-fast` (conventions, fmt,
  clippy, the workspace tests) on every PR, `make tier-full` (= `make gate`) at a
  goal's end and nightly on main, except where a goal file names its gate.
- Heavy jobs and the disk (request #230 from the goals program, 2026-10-03: the container's disk is its bottleneck): every cargo build, test or clippy of the workspace, every gate tier and every image build (`make image`, `make soloist-image`) runs under `goals lock goals-heavy -- goals lock chorus-heavy -- ...`, subagents' runs included; only a single crate's focused test on a built tree runs outside it. A throwaway build (an image, a one-off worktree) sets `CARGO_TARGET_DIR` to a directory under `/scratch` when it fits, else the lane's shared `/cache/wt/chorus/target`, never a private directory of its own on the disk.
- Flashing, eFuses, deploys to the homelab and OTA installs on installed speakers are the owner's actions (K4, K28, K93); nothing in this repo sets `CHORUS_OWNER_AT_BENCH`. Each one is an issue in the owner's queue (NSchatz/goals: `goals needs add`, `/goals:needs`), and a request to another program is a `from:chorus` issue (`goals request add`).
- BRIEF.md is kept current by the program: verified corrections and the owner's
  decisions are written into it, each dated with its decision IDs. Its section
  3.1 guardrails are never relaxed or removed by the program; they may only be
  tightened. When measurement contradicts BRIEF.md, say so in `docs/decisions/`
  and correct the brief in the same change. (R2, decided 2026-09-29 by the owner,
  K47; limit I2.)
