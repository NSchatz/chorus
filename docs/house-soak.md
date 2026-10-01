# The house soak

A software soak of a whole house on one development host: one `chorus-server --slots 8`, eight
rooms, ten `chorus-client` endpoints on ALSA `null`, and a seeded command load, for an hour (goal
11's line E: "a software soak of at least one hour at 8 rooms is committed, labelled, with its
duration"). Decided in `docs/decisions/0078-the-house-soak.md`.

**It is a host software soak on ALSA null, not a hardware measurement and not timing evidence**
(BRIEF.md section 3.1 rule 3). ALSA's `null` device accepts every frame at once and reports a
delay of zero, so the soak says nothing about when a sample reaches a DAC and grades no sync bound.
What it grades is that the house keeps working for as long as it runs. The three-day hardware soak
on real endpoints (AC-4) is a different run, below.

## Running it

```sh
timeout 4300 flock -o -w 3600 /cache/locks/chorus-heavy.lock \
    env CHORUS_HOUSE_SOAK_SECONDS=3600 make verify-house-soak
```

About 62 minutes of wall clock (the hour of load, up to 40 s for the endpoints to come up and
settle, 90 s for them to stop by their run length, and the build when it is not already built).
It needs a libasound: the system's, or the rootless alsa-lib at `CHORUS_ALSA_PREFIX` (default
`/cache/opt/chorus-alsa`) that `make verify-alsa-null` uses; with neither it refuses by name
(exit 3). It is not in `make gate`: an hour is too long, and `make verify` checks only that it
refuses visibly without a device (`tools/unrun-checks-are-visibly-unrun.sh`).

The environment it reads, every value recorded in the report:

| variable | default | what |
|---|---|---|
| `CHORUS_HOUSE_SOAK_SECONDS` | 3600 | the load window, at least 60 |
| `CHORUS_HOUSE_SOAK_SEED` | 20261001 | the command load's seed: the same seed and the same answers replay the same load |
| `CHORUS_HOUSE_SOAK_INTERVAL_MS` | 2000 | one command every this many ms |
| `CHORUS_HOUSE_SOAK_CIVIL_TIME` | `mon-23:30` | the server's fixed civil time (`--civil-time`), so the load's quiet-hours windows are active or not by what it sets |
| `CHORUS_HOUSE_SOAK_DIR` | `$TMPDIR/chorus-house-soak/<UTC stamp>` | the run directory: every raw file, kept |
| `CHORUS_HOUSE_SOAK_REPORT` | `docs/measurements/house-soak-8-rooms.md` | the report; empty writes none |
| `CHORUS_HOUSE_SOAK_LABEL` | `one-hour soak` at 3600 s and more, else `harness short run` | what the report calls the run |
| `CHORUS_HOUSE_SOAK_BUILD` | `HEAD` | the commit the report names as built; one other than HEAD must hold the same `crates/`, manifests and `config/` (checked) |

Exit 0 when every criterion passes, 1 when one fails. A kept run directory can be graded again
with `python3 tools/house-soak/report.py --run-dir <dir> [--out <report>]`.

## What runs

- The server: `--slots 8 --control-listen --serve-forever --source tone --max-clients 16
  --control-workers 8 --civil-time ...`, with the stream contract from
  `config/verification.conf`. Eight rooms, ASSUMED names (the house of
  `docs/measurements/sim-house-8-rooms.md`; the owner's room list is a Needs item): living,
  kitchen, dining, primary, office, patio, bathroom (declared wireless), guest.
- Ten endpoints on ALSA `null` (the client has no fake sink, by design): one per room and a second
  in living and kitchen, whose pairs are attached `link: wired` and bonded FL/FR before they play.
  The bathroom endpoint runs `--transport wireless`. The setup also asks for a bond in the
  wireless room and must be refused naming the room (K91).
- One event-stream subscriber for the whole run, recording every state.
- The command load (`tools/house-soak/load.py`): a weighted table of volume, volume_step,
  group_volume, group_volume_step, mute, join, take (with and without a source), ungroup,
  group_save, group_delete, limit, quiet_hours, alarm_set, alarm_stop, alarm_delete and sleep,
  each built from the state the last answer carried. A command the server does not know is
  counted as unknown, not fatal, so the table can run against a build that has not grown it.
- RSS and thread counts of every process off `/proc/<pid>/status` every 10 s.
- Everything bounded: the script under `timeout` at the soak plus 600 s, the server under the
  same, every endpoint under its own `--run-seconds`.

## What is graded

Each criterion with its bound is in the report: the thread population unchanged (the server's
also equal to the documented `6 + 2N + M`), RSS growth bounded (8 MiB per process after a 30 s
warm-up, ASSUMED), control fanout drops zero, every state's volume and every `room_volume` an
endpoint took at or below its effective limit, underruns and hard resyncs either within 5 s
(ASSUMED) of a logged source switch of the endpoint's own room or counted as unexplained (none
allowed), every endpoint up at the end on the one session it opened, every command answered, and
the window's duration measured on `CLOCK_MONOTONIC` with its wall-clock start and end. The
control counters and the sessions are graded over the window (snapshots of `GET /api/report`,
the state and the server log at its two ends); ten endpoints starting in one instant are a small
restart storm, and what happens before the window (connections turned away, a first session
rejoined) is reported beside the result, not graded as the soak.

## The three-day hardware soak (the owner's run, a Needs item)

AC-4 is three days of wall clock on real endpoints with the RIG-3 capture rig measuring the sync
bound; `tools/soak-run.sh` (`make verify-soak`) is that run and refuses anywhere it cannot be
done. The house soak does not replace it and changes nothing in it. On the bench the owner runs:

```sh
CHORUS_SOAK_SECONDS=259200 CHORUS_SECOND_ENDPOINT=<user>@<second endpoint> \
CHORUS_CAPTURE_DEVICE=<capture card> CHORUS_CLIENT_DEVICE=<playback card> \
    make verify-soak
```

and, for a house-scale software load on real endpoints over the same three days, the house soak
pointed at the real devices on the bench host (`CHORUS_CLIENT_DEVICE=<card>`,
`CHORUS_HOUSE_SOAK_SECONDS=259200`, `CHORUS_HOUSE_SOAK_LABEL="three-day bench soak"`,
`CHORUS_HOUSE_SOAK_REPORT=docs/measurements/house-soak-bench-<date>.md`), whose report is still
labelled `Source: host` and is not timing evidence: only the capture rig's report is. Both are the
owner's actions on the owner's hardware (K45, K50); this repository never runs them.
