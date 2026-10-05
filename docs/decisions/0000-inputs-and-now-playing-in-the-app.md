# 0000: the app shows what a room alone or a formed group plays and what is playing, loads artwork from the server's own route only, and offers exactly the inputs the server offers

- Status: accepted, 2026-10-05.
- Decided by: the owner for what is asked (K64: the app controls rooms, groups, inputs and
  sound, not content; proposal P5, `docs/proposals/P5-app-stack.md`). Where the view sits,
  what the picker lists, which command a choice is, how a cover is told from the next and how
  the live test gives a real server an input and a record are this record's: cheap to
  reverse, each in one module.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/src/playing.js`, `web/src/state.js` (`playingOf`, `inputsOf`),
  `web/src/api.js` (`takeSourceCommand`, `artworkUrl`), `web/src/room-card.js`,
  `web/src/group-card.js`, `web/src/rooms.js`, `web/src/groups.js`, `web/src/chorus-app.js`,
  `web/src/tokens.css`; `web/test/playing.test.js`; `web/live/playing.live.js`,
  `web/live/endpoint.js`, `web/live/control-point.js` and `web/live/house.js`.

## Context

The state message has said what each formed group plays (`source`), what is playing in it
(the now-playing record) and which line-ins are offered (`inputs`, with a person's names for
them in `input_labels`) since goals 16 and 17 (`docs/control-plane.md`). Record 0184 gave the
record's artwork an address on the server's own origin, because the page's
Content-Security-Policy loads no image from anywhere else. The app showed none of it. Record
0185's rule holds here as everywhere: the server owns the state, the app shows its last state
message and keeps no value of its own.

## Decisions

1. **One element, `chorus-playing`, says what a group plays, and it sits where the group
   is.** A room alone is the group named for it, so its room card carries the element. A room
   in a saved or live group does not: the group's card does, once, because a choice of input
   there is the whole group's. A saved group no room is in plays nothing and shows nothing.

2. **The now-playing view is the record, and only the record.** Title, artist, album and
   whether it is playing, paused or buffering, each as the state message has it. What the
   record does not know is left out, not made up. A group without a record shows its source
   in words (`Source: The server's stream`) and no title. There is no transport, queue or
   browsing (K64); `playback` and the Spotify receivers' controls are not offered here.

3. **The artwork is an `<img>` on `api/artwork?group=<id>` and never on the record's own
   address.** The record's `art_url` is not requested by the page and is not written into the
   document. It is used for one thing: the route's address is the same for every track of a
   group, so a short tag computed from `art_url` rides in the address's fragment, which a
   browser does not send. A new cover is then a new address to the page and a new `<img>`
   (Lit's `keyed`), so the page asks again. That a browser asks again for a new fragment is
   the reasoning here and is NOT MEASURED in a browser: the unit tests hold that the address
   changes and the element is replaced, no more.

4. **An image that fails to load gives way to a placeholder.** The route answers without an
   image for many reasons (`404`, `502`, `503`, `504`). The `<img>`'s `error` event swaps in
   the placeholder a record without artwork has, for that address only: the next cover is
   tried.

5. **The picker lists the state's `inputs`, in the server's order, and nothing else.** One
   button per offered input, called by its label from `input_labels` or else by its id, with
   `aria-pressed` on the one that is the group's `source`. The stream, a player, a chime or a
   Spotify receiver is not something a person picks in this list, so when the group plays one
   of those no button is pressed and the source line says which. Buttons and not a `<select>`:
   a list has to have a selected option, and there is none to show then.

6. **Choosing an input is `take` with the group as target and the input as source.** The
   target is the formed group's own id, so no room moves (`docs/control-plane.md`, "Take the
   room (K78)"). The one case where naming the group would move rooms is a saved group that
   is only partly formed (a `take` of a saved group pulls in every room of its definition),
   so its card shows what plays and offers no picker until it is active. There is no
   optimistic mark: the button is pressed when the state that resulted says so, and a
   refusal is shown in the server's words by the card.

7. **The live test gives the server its input and its record from outside, the way a house
   does.** No command offers an input or sets a record. An input is offered by an endpoint's
   session, so `web/live/endpoint.js` is a scripted endpoint in node's standard library that
   completes the handshake of `docs/protocol.md` and offers one line-in with signal present;
   the shipped client could do it only with an ALSA capture device carrying a signal, which
   a CI runner has not got. A record with artwork is set by a player playing something
   described, so `web/live/control-point.js` is a UPnP control point that plays a WAV, with
   a title, an artist and a cover, on a room's renderer, from an origin on loopback (the
   server is started with `--media-allow-loopback`, a flag for tests).

## Consequences

- `house.js` takes more server arguments and a kept identity, and can wait for a line the
  server prints; the two earlier live tests start their houses as before.
- A scripted endpoint in a second language is a second reading of the handshake and of three
  messages. It is a test helper and nothing ships it; when the protocol's version moves, it
  fails `web-live` by name and is corrected from `docs/protocol.md`.
- Where a room in a group plays is on the group's card only. A person looking at the room's
  card sees "Plays with" and finds the group above.
- Labelling an input, the stored alarm sources and the visualizer are not in the app yet.

## What was read

- `docs/control-plane.md` (sources and the now-playing record in "The state a server holds";
  "Stored sources and input labels (goal 17)"; "Take the room (K78)"; the state message's
  members; "Now-playing artwork").
- `docs/inputs.md`, `docs/upnp.md`, `docs/protocol.md` (the handshake, the session's messages,
  the source role).
- `docs/decisions/0184-artwork-is-proxied-not-the-policy-widened.md`,
  `docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`,
  `docs/decisions/0187-groups-in-the-app.md`.
- `fixtures/control/v2/` (`state-playing.json`, `state-inputs.json`, `state-rich.json`,
  `take-source.json`), `fixtures/protocol/v2/`.
- This repository's own `crates/protocol`, `crates/server/tests/common/` (the scripted
  endpoint and the control point the Rust tests use) and `crates/server/src/` (what makes an
  input offered, what a renderer's `Play` sets).
- No source outside this repository was opened.
