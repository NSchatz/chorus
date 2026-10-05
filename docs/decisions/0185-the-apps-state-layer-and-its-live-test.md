# 0185: the app holds the server's state message and nothing of its own, reads the event stream with fetch so the page and the tests are one subscriber, never moves a slider a person holds, and proves each screen in node against a real chorus-server (`make web-live`)

- Status: accepted, 2026-10-05.
- Decided by: the owner for the model (proposal P5, `docs/proposals/P5-app-stack.md`, Option
  C: "state held by one app element that receives the server snapshot and passes it down as
  properties", approved at Checkpoint K) and for a browserless live test as the way every
  screen proves itself (the task this record's change builds). How the stream is read, what a
  held control means, the bounds and the shape of the live test are this record's: cheap to
  reverse, each in one module.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/src/api.js`, `web/src/state.js`, `web/src/rooms.js`,
  `web/src/room-card.js`, `web/src/chorus-app.js`, `web/src/main.js`; `web/test/` (the unit
  tests and `fake-server.js`); `web/live/rooms.live.js`, `web/live/setup.js`; `tools/web.sh`
  (`live`); the `web-live` target of the `Makefile`; step `web-live` of `tools/gate.sh`.

## Context

Until this change the app read `GET /api/state` once when the page opened and listed the
rooms' names. This change makes it live: every room with its bonded set, its volume and its
mute, following the server, and able to change the two.

`docs/control-page.md` and `docs/control-plane.md` settle the model: one server owns the
state, a state message is a complete snapshot, and a page holds no copy of its own that
could disagree. What was open is how the app's modules carry that, and how a screen is
tested against a server rather than against the author's idea of one.

## Decisions

1. **One store holds the last state message, and nothing derived from what the app did.**
   `state.js` starts with `GET api/state` (something to show where the stream cannot be
   opened) and then follows `GET api/events`, whose every message replaces the last. Once the
   stream has delivered, a snapshot that answers late is not applied. `chorus-app` is the one
   subscriber and passes rooms down as properties.

2. **No optimistic value.** A command is sent and the page changes when the state that
   resulted arrives. The answer to an accepted `POST api/command` is that state (the bytes
   every subscriber is sent), and the store holds it when its `serial` is newer than what it
   has, so the page does not wait for the stream; a stream message is always applied, so a
   server that restarted (and counts from zero again) is followed all the same. A volume the
   server clamped shows as the server's volume, because that is the only one the page has.

3. **A refusal is shown in the server's words**, on the card of the room the command was for:
   the `detail` of the catalog's `error` or `refused` message, else the answer's own text (a
   `403`, `415` or `503` says its reason in plain text), else the status. It stays until that
   room's next command.

4. **The event stream is read with `fetch` and a stream reader, not `EventSource`.** The
   reader is then the app's own code wherever it runs, so the browser, the unit tests (a
   scripted stream cut at any byte) and the live test are the same subscriber, which is the
   property `docs/control-plane.md` asks of the UI and its verification. It also puts the two
   things `EventSource` hides in one testable place: a stream that ends or fails is reported
   `lost` and opened again after 1 s, and a stream that delivers no byte for 40 s is closed
   and opened again. The second is the case a connection alone cannot show (a server that
   has stopped answering under an established connection); the server writes a keepalive
   comment after 15 s of nothing to say (`KEEPALIVE`, `crates/server/src/control.rs`), so 40 s
   is two missed keepalives and some slack. Both bounds are ASSUMED, not measured. The old
   control page's probe admits a stalled server inside nine seconds; the app's bound is
   looser and a later change that needs it tighter moves one constant or adds the probe.

5. **An update never moves a slider a person holds.** Lit patches a card in place, so an
   update does not replace the slider (the observation in P5's measurements). While the
   slider has focus an update does not write its position either; the figure beside it
   follows the finger during a drag and otherwise says the server's volume, so a difference
   between the two is visible and never silent. The slider takes the server's value again
   when it loses focus, and at once when the server refuses a command.

6. **The command bodies are built by hand** around a three-digit volume literal, as the
   control page's are: the catalog writes `0.500` and `JSON.stringify` writes `0.5`. A unit
   test holds them to `fixtures/control/volume.json` and `mute.json`.

7. **A screen proves itself in node against a real `chorus-server`: `make web-live`.** The
   test mounts the app's real elements over the real store and client under happy-dom, with
   node's own `fetch`, starts the server `CHORUS_SERVER_BIN` names the way a house would be
   configured, drives controls found by their labels and compares what is rendered with the
   server's `GET /api/state`, in both directions. It is not a second browser test (record
   0183 keeps exactly one): it holds the path from a control to the server's state and back,
   which the scripted server of the unit tests cannot, and costs about a second. Like every
   step that needs a built server it says `web-live: PASS` or `web-live: SKIPPED`, and a
   skip is a failure under `CI=true` and in the gate (record 0142). Later screens add a
   `web/live/<screen>.live.js` each.

## Consequences

- `web/src/rooms.js` is now the rooms screen; the read-once `loadRooms` it held is gone. The
  smoke test of record 0183 reads the rooms' names from the cards' headings.
- The app shows a room's bonded set by each member's channel role and the speaker's name
  where the state lists one for that endpoint, else the endpoint's id. It reads no other
  member of the state yet: groups, inputs and what is playing are later changes.
- The app has no dependency it did not have: the stack of record 0181 is unchanged.

## What was read

This section was added after the record, by the change that made the provenance check green
again; it lists only the sources the record itself names above.

- `docs/proposals/P5-app-stack.md` (Option C and its measurements) and the records it builds
  on: `docs/decisions/0181-the-web-app-stack.md`,
  `docs/decisions/0183-the-one-browser-smoke-test.md` and decision 0142's rule on a skipped
  step.
- `docs/control-page.md` and `docs/control-plane.md` (the state model, and what is asked of
  the UI and its verification).
- `crates/server/src/control.rs` (`KEEPALIVE`), `fixtures/control/volume.json` and
  `fixtures/control/mute.json`.
- The record names no source outside this repository.
