# 0233: goal 25 is closed: the two-way's and the subwoofer's acoustic design records landed in the devices repo, with their exports byte for byte and their totals

- Status: decided, 2026-10-06 (harness task 241, project 30)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: the devices repo, pull requests 120 and 121 (devices harness tasks 55 and 56);
  nothing in chorus changes but this record and its index line

## Context

Goal 25 (the 2026-10 program's section 29, retired with the goal-program files in #141; its
landing rule is quoted in `docs/proposals/P14-devices-seam.md`) asked for the active two-way and
the subwoofer: each designed in chorus with its design record, then landed in the owner's
devices repo as `builds/chorus-<build>/acoustics.md` with the record's export beside it, the
same seam the compact speaker used. The old feature's finish line (goals issue #325) is that
both landed. This record makes it checkable from chorus's main.

## Decision

Goal 25 is closed. For each build:

| | Two-way | Subwoofer |
|---|---|---|
| chorus design doc | `docs/hardware/twoway-speaker.md` | `docs/hardware/subwoofer.md` |
| chorus design record | `fixtures/design-record/chorus-twoway-v1.json` | `fixtures/design-record/chorus-sub-v1.json` |
| Landed in devices by | https://github.com/NSchatz/devices/pull/120 (task 55) | https://github.com/NSchatz/devices/pull/121 (task 56) |
| devices paths | `builds/chorus-twoway-v1/acoustics.md`, `builds/chorus-twoway-v1/chorus-twoway-v1.json` | `builds/chorus-sub-v1/acoustics.md`, `builds/chorus-sub-v1/chorus-sub-v1.json` |
| Budget total, good tier (designed) | 203.92 USD (under the proposed 225.00) | 360.69 USD (under the proposed 400.00) |

The totals are re-read from the two docs on chorus main at `e00b730` ("The budget"); both tiers and
both class budgets stay PROPOSED, the owner's to decide.

### The landed exports are the committed ones

`sha256sum` of each export on devices main (`origin/main` at `eae8676`), against the `sha256`
line of the matching chorus `.provenance`:

| Export | devices main | chorus `.provenance` |
|---|---|---|
| `chorus-twoway-v1.json` | `b2dbe5805ce21b43ebc7404f4f627c78020bb3a7a5c8613f6ed7235218e0dcf5` | `b2dbe5805ce21b43ebc7404f4f627c78020bb3a7a5c8613f6ed7235218e0dcf5` |
| `chorus-sub-v1.json` | `7efc717e20999a509a2c5790601803249dd0462a750836cbf6f0363433b6e0ed` | `7efc717e20999a509a2c5790601803249dd0462a750836cbf6f0363433b6e0ed` |

Both equal. `cargo test -p chorus-dsp --test design_record` passes on this branch, 8 of 8,
among them `the_twoway_record_is_the_exported_one_and_runs` and
`the_sub_record_is_the_exported_one_and_runs`, so the records chorus holds are the exports and
each runs its crossover through chorus's DSP.

### What stays open (named, not implemented here)

The subwoofer doc names these follow-ups and leaves them open ("The controls", "The DSP sections",
"Amplifier sizing", "Open items"):

1. **A continuous phase control.** The phase knob has thirteen positions and the chain has two
   phases; until a variable all-pass section (or a delay) on the `LFE` feed is designed, the
   knob is a polarity switch with a wide throw (`firmware/include/chorus/endpoint_dsp.h` says
   the same).
2. **The subwoofer's high-pass and voltage limit in the playback chain.** The fourth-order
   high-pass at 24 Hz and the 19.8 V limit at the driver run nowhere yet: chorus's chain has no
   per-endpoint high-pass stage for an `LFE` role (`docs/dsp.md`, chain item 8) and no
   per-endpoint voltage limit. Until they do, nothing protects the cone below the tuning but the
   amplifier's own response.

The enclosures, boards and bills of materials for both builds are the devices repo's (its
project 7), not chorus's.

## Not chosen

- **Closing the goal on the harness tasks' state alone.** It would leave the finish line
  checkable only in the harness, not from chorus's main.

## What was read

- Harness tasks 241, 55 and 56, read 2026-10-06.
- chorus `origin/main` at `e00b730`: `docs/hardware/twoway-speaker.md` and
  `docs/hardware/subwoofer.md` ("The budget", "The controls", "Open items"),
  `fixtures/design-record/chorus-twoway-v1.provenance` and `chorus-sub-v1.provenance`,
  `docs/proposals/P14-devices-seam.md`, read 2026-10-06.
- The owner's devices repo, `origin/main` at `eae8676`: the two exports (hashed) and the log of
  `builds/chorus-twoway-v1` and `builds/chorus-sub-v1`; its merged pull requests 120 and 121,
  read 2026-10-06.
