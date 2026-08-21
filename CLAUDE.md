# chorus

From-scratch multiroom + surround audio: containerized server, custom sync protocol,
embedded speaker firmware. BRIEF.md is the guiding document: goals, constraints,
recommendations, and open decisions. It recommends; it does not dictate.

## Working agreement

1. Propose before committing to anything expensive to reverse. Cheap decisions:
   make them, note them in docs/decisions/.
2. Hard guardrails (BRIEF.md 3.1): clean-room vs GPL references; no eFuse burns on
   dev hardware; measurement-backed timing claims; monotonic clocks in the audio
   path; no em dashes anywhere.
3. Prefer building over vendoring when small and instructive; vendor the large and
   undifferentiated; log gray-zone calls.
4. The sync engine is the project. Simulator first, hardware second, measurement
   always. Reports go in docs/measurements/.
5. Protocol, sync, and DSP cores are pure libraries with shared fixtures so the
   Rust and C implementations cannot drift apart.
6. Items marked "verify against the datasheet" are starting points, not truth.
7. Finish a phase's "success looks like" before moving on, or say why not.

## This repo is a submodule of the SDD umbrella

Everything above is the owner's working agreement and governs the code. This
section is the umbrella's half of the contract, and it binds any session that
reaches this checkout through `just implement`:

- Work here arrives as an approved spec and rides the umbrella stages. A change
  made directly in this checkout with no spec is invisible to the ledger and
  will not land: `just land` moves the pin, nothing else does.
- Tier floor is `sensitive` (`documentation/tier-map.md` in the umbrella). It is
  a FLOOR: a spec that burns eFuses, deploys onto the Proxmox host, or ships an
  OTA image to a wall-mounted device proposes `critical` and takes the human
  gate. Guardrail 2 above is not softened by any tier.
- The direction lives in `documentation/roadmaps/chorus.md` on the umbrella side,
  derived from BRIEF.md section 8. A spec cites a phase as `chorus#<phase-id>`
  and INHERITS its acceptance rather than restating it, so the brief stays the
  one source of truth for what a phase means.
- BRIEF.md is the owner's document. When measurement contradicts it, say so in
  `docs/decisions/` and propose the change; do not silently edit the brief.
