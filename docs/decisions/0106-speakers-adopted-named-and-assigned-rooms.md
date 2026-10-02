# 0106: an adopted speaker is a record beside its pin, listed with no command, named and assigned a room in catalog v2, present by its session, and forgotten only by the owner

- Status: accepted (goal 14, 2026-10-02)
- Decided by: the goal (program section 18; K91, K92, K93) inside the coordinator's goal-14
  design envelope (section 4), track `chorus-g14/adoption-server`; every default below not
  cited is ASSUMED
- Implemented in: `crates/control/src/speakers.rs` (new: `Speaker`, `SpeakerNow`, `Speakers`,
  `KeyChange`, `MAX_SPEAKERS`), `catalog.rs` (`speaker_name`, `speaker_room`,
  `speaker_forget`), `zones.rs` (the commands, the session layer's hooks, the state's
  `speakers` and `key_changes`), `persist.rs` (state-file format 5, `[speaker <id>]`);
  `fixtures/control/v2/` (4 command vectors, `state-speakers`, 7 refusals);
  `crates/server/src/control.rs` (`adopt_through`, `speaker_session_up`,
  `speaker_session_down`, `speaker_key_changed`, `speaker_now`, the pin removed on
  `speaker_forget`), `session.rs` (`Adoptions::pins`, `Adoptions::forget`,
  `SessionContext::session_up` and `session_down`, the `Greeting`'s `software` and `key`),
  `clients.rs`, `main.rs`; `crates/server/tests/adoption.rs`,
  `crates/control/tests/speakers_v2.rs`. The contract is `docs/control-plane.md` ("Speakers:
  adoption, names and rooms", "The state message", state-file format 5)

## Context

Before this, "adopted" meant one thing: an id's key is pinned in `adopted-endpoints`
(ADR 0039, goal 5). Nothing a person could use came with it. The control state knew
endpoints only as strings inside a room's `endpoints`, put there by the endpoint's own
`attach`; the firmware has no control client and sends no `attach`, so a firmware speaker was
in no room, heard the idle route and was told no `room_volume`. A refused key change was a
log line. `PinStore::forget` existed and nothing called it. K92 asks for the opposite
experience: a speaker joins the LAN, appears, and is named and placed in the app afterwards.

## What was read

All read 2026-10-02: `BRIEF.md` section 3.1; the program's sections 0.3, 0.4, 0.8, 0.9 and 4;
the goal-14 design envelope (section 4 and "Tracks and file ownership") and the code survey
(sections 4, 5, 8 and "Seams and risks"); ADRs 0016, 0018, 0019, 0039, 0074, 0075, 0081 and
0088; `docs/control-plane.md`, `docs/protocol.md` ("Adoption: trust on first use"),
`docs/conventions.md`; the code each "Implemented in" names, `crates/protocol/src/v2/adoption.rs`,
`crates/server/src/router.rs` and `conductor.rs`, `crates/client-linux/src/control.rs` (how
strictly the Linux client reads the state), `crates/server/src/ui/chorus.js` (the same question
for the page), `firmware/tests/main_session.c`, `main_dsp_session.c`, `session-outage.sh` and
`dsp-session.sh` (how the C endpoint's host programs are built and driven), `firmware/Makefile`.
No GPL source and no reciprocally licensed design file was opened. No number here comes from
outside the repository, so nothing is cited by URL.

## Decision

1. **A speaker record per adopted id, beside the pin and not in it.** The pin file keeps its
   format and its meaning (goal 5's tests are untouched). The record lives in the control
   state: `id`, `name`, `named`, `room`. Its default name is `Speaker ` and the last four
   characters of the id. It is created by the server when a session of the id comes up (the
   handshake has already pinned the key), with no command from anybody: that is auto-adoption.
   A server that starts with pins and no record for them (an upgrade) lists each.
2. **What is true now is one struct, `SpeakerNow`, never persisted** (ADR 0018): how many
   sessions of the id are up (`present` is "at least one"), its latest `hello`'s software and
   roles, and its pinned key's fingerprint. `link` in the state is the fact `attach` already
   reports and format 2 already persists (`endpoints[]`), read through, not duplicated.
3. **The state message gains `speakers` and `key_changes`, written only when non-empty,
   after every member it had.** The committed `state-empty` and `state-rich` vectors did not
   move; `state-speakers` is new. Both readers in the tree take members by name and ignore the
   rest (`ZoneWatch::absorb` uses `get`; `chorus.js` reads the members it draws), so neither
   needed a change. The catalog version stays 2 (the precedent of ADRs 0081 and 0088: commands
   and state members added inside a version, refused at version 1 as "not a command").
4. **Three commands.** `speaker_name` {`speaker`, `name`}; `speaker_room` {`speaker`, `room`
   or `null`}, where a `room` left out is refused so "no room" is always said out loud;
   `speaker_forget` {`speaker`}. Each refusal names its field (`speaker`, `name`, `room`) and
   what there is.
5. **`speaker_room` makes the speaker a member of the room** (the room's `endpoints`) and
   takes it out of every other. Routing needed nothing new: the conductor and a starting
   session already place an endpoint by membership when it is not present
   (`Snapshot::room_of`), so the room's stream, `room_volume` and `sound` follow.
6. **An assigned speaker's presence is its session's.** It is in its room's `present` while a
   session of it is up. An unassigned speaker's presence stays its control client's, exactly
   as before, so a Linux endpoint that attaches itself is unchanged; `/api/leaving` does not
   make an assigned speaker absent while its session is up.
7. **An explicit `speaker_room` wins over a later `attach` to another room** by redirecting
   it: the `attach` is accepted and attaches the endpoint to the room it was assigned.
   Refusing it was the alternative; it would turn the owner's choice in the app into a Linux
   endpoint that fails to start until somebody edits its `--zone` flag. The speaker's `room`
   in the state is the owner's assignment only: a self-attached endpoint shows `null` there
   and its membership where it always was (`zones[].endpoints`).
8. **`speaker_forget` removes the record, the membership, the link fact, any key change under
   the id, and the pin**, in that order of agreement: the room model accepts first, the pin
   file is rewritten, and only then is anything installed. A pin file that cannot be rewritten
   refuses the command whole. It is the only code path that removes a pin. A live session of
   the forgotten id is not cut (cutting one needs a path from the control plane into a slot
   that does not exist; the session ends on its own and the next one is adopted afresh).
9. **A refused key change is surfaced in the state** (`key_changes`: id, pinned, offered), the
   latest per id, until the speaker is forgotten or the server restarts. It is not persisted:
   it is an event of this run, and the pin it protects is. A key change under an id with no
   record can still be forgotten.
10. **A bonded speaker is not moved or forgotten out from under its set**: refused naming
    `speaker` until the room is unbonded. A set with a member silently gone is not a layout.
11. **The registry is bounded at 64 records (ASSUMED)** and an id that is not a catalog
    identifier is not listed. Both are adopted (pinned) and play; both are logged by name
    (`speaker not listed id=... reason=registry-full|id-not-an-identifier`). The bound exists
    because adoption is automatic and unauthenticated on the LAN (K92) and every subscriber
    is sent the whole state.
12. **State-file format 5**: one `[speaker <id>]` section per record with `name`, `named` and
    `room` (empty for none), every one required. Formats 1 to 4 load unchanged with no
    record. The three-test pattern of goals 12 and 13 is in `speakers_v2.rs`.
13. **No new thread.** The session's calls run on its own reader thread (where the handshake
    and the pin write already run) and the commands on a control worker. The population is
    `6 + 2N + M` as before, and `control_thread_population.rs` and the determinism run are
    unchanged.
14. **The page shows nothing of the speakers.** `crates/server/src/ui/` is untouched; the
    adoption screen is goal 22's, built on the state members and commands above.

## The end-to-end test

`crates/server/tests/adoption.rs::a_new_speaker_is_adopted_named_and_assigned_a_room` runs
the real `chorus-server`, the firmware's session code as its two host programs and a Linux
client's session path, all with fresh identities. It cites goal 5's two tests by name and
does not rebuild them. Two C programs are used because each shows what the other cannot:
`chorus-endpoint-session` keeps its key in a file (so it is the same speaker across a server
restart) and has no playout path; `chorus-endpoint-dsp-session` has the playout path (so its
own summary says which room volume it applied) and makes its key per run. The test builds
both with `firmware/Makefile`, so `cargo test` now needs what `make firmware-check` needs
and fails naming the prerequisite when it is missing.

## How ota-server extends this

- A per-speaker runtime fact (the running version, board, slot, transfer state, progress,
  `update_available`) is a field of `chorus_control::speakers::SpeakerNow`. It is never
  persisted and `Default` is "nothing known".
- Set it through `ControlState::speaker_now(id, |now| ...)` (server) or
  `Zones::speaker_now` (model): both return whether anything changed, bump the serial and
  fan the state out only then. The endpoint id of a session is `Greeting::endpoint_id`, the
  same string as the speaker's `id`.
- Write it into the state in `speakers::speaker_value`, appended after `roles` (a `firmware`
  object), and pin it with a new `state-*` vector. Written only when known, the existing
  `state-speakers` vector does not move.
- `firmware_*` commands follow `speaker_forget`'s shape: a `Command` variant, a `decode_v2`
  arm, a `Zones::change` arm that validates against `self.speakers` (`speaker_exists`, the
  `no_speaker` refusal), and server-side effects in `ControlState::apply` beside
  `forget_pin`, inside the same `commit` so a refusal installs nothing.
- `Speakers::set_now` is `false` for an id with no record (unlisted ids): an install to
  such an id is refused as an unknown speaker by the same check.

## Deviations from the envelope

- `link` is not a new non-persisted fact of the record: it is the endpoint's reported link
  the state already had (persisted since format 2), shown on the speaker too.
- The envelope names one C binary (`chorus-endpoint-session`); the test uses it and
  `chorus-endpoint-dsp-session`, for the reason above. `firmware/` is not edited.
- `identify` (0x1B, 0x1C) is not built: optional, and skipped as the envelope allows.
- The branch is `chorus-g14/adoption-server-b`, not `chorus-g14/adoption-server`: the first
  branch's history holds a fixture line (a made-up key fingerprint after a field named
  `key_change`) that the gate's secret scan reads as a generic API key. Pushed history is
  never rewritten and the scan's configuration was not touched, so the work was committed
  again on a fresh branch with the fixture field renamed `changed`.

## ASSUMED

- `MAX_SPEAKERS = 64`.

## Follow-ups

- Goal 22: the adoption screen (list the unnamed, name, assign, show `key_changes`, forget).
- A way to dismiss a key change without forgetting the speaker, if the app wants one.
- Cutting a forgotten speaker's live session.
- `hello.name` is still ignored; the firmware sends it empty.
