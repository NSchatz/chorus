# 0197: the app's further screens each have an address in the fragment (`#/rooms/<room>/sound`), register themselves with one registry, are opened as entries of the browser's history, and the first is a room's sound screen whose every control sends one `sound` command with its one field

- Status: accepted, 2026-10-05.
- Decided by: the owner for what is asked (the task: a way to open screens beyond groups and
  rooms that works in the phone, desktop and kiosk layouts and with the browser's back button,
  one that later screens can register with, and a room's sound settings as the first). Where a
  screen's address is written, how a screen registers, what "Back" does, how the screen is
  painted in each layout and what each control sends are this record's: each is one module or
  one rule, cheap to reverse.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/src/routes.js`, `web/src/sound.js`, `web/src/chorus-app.js`,
  `web/src/room-card.js`, `web/src/api.js` (`soundCommand`, a refusal's `field`),
  `web/src/state.js` (`soundOf`, `roomOf`); `web/test/navigation.test.js`,
  `web/test/sound.test.js`, `web/live/sound.live.js`.

## Context

Until this record the app was one view: the groups and the rooms, with two buttons that move
the focus between them (`docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md`,
decision 5, which also says "a later screen adds a navigation button"). A room has more
settings than a card can carry (sound now; limits, quiet hours and bass management in later
tasks) and the house has screens that belong to no room (alarms, speakers). A button for each
in a bar of two does not scale, and a view held only in a property of the shell is lost by a
reload and unknown to the browser's back button, which on a phone is the gesture a person uses
without thinking.

What bears on where an address can be written:

- `chorus-server` serves one document at `/app/` and, under it, the app's built files and
  nothing else. A path such as `/app/rooms/living/sound` is the server's to answer, and a server
  change is outside this task.
- The query string is taken: `?kiosk` is the kiosk's switch (`web/src/mode.js`).
- The app sits behind a login and a service worker, and both act on requests
  (`docs/decisions/0190-the-app-installs-behind-the-login.md`).
- Exactly one test starts a browser (`docs/decisions/0183-the-one-browser-smoke-test.md`); the
  unit and live tests run under happy-dom, whose `location` and `history` model a browser's
  (a pushed entry, `history.back()`, `popstate`, `hashchange`; seen 2026-10-05 with the pinned
  happy-dom 20.14.5 by a probe script, and held from then on by `web/test/navigation.test.js`).

## Decisions

1. **A further screen's address is in the fragment: `#/` and a path.** `/app/#/rooms/living/sound`
   is the sound of the room `living`; `/app/` and `/app/#/` are the home. The fragment is the one
   part of the address that is the page's alone: it is never sent, so the server, the login and
   the service worker see the same request for every screen and none of them changes; and the
   query string stays the kiosk's, so `/app/?kiosk#/rooms/living/sound` is a kiosk on that
   screen. A parameter is percent-encoded, so any room id survives. An address that names no
   registered screen is the home, not an error page: a bookmark of a screen a later version
   drops still opens the app.
   - Not chosen: a path under `/app/` with the History API alone. It reads better, and it needs
     the server to answer every such path with the document, and the service worker to know
     them; both are changes outside the app for no gain a household sees.
   - Not chosen: the view as a property of the shell with no address. A reload, a bookmark and
     the back button would all lose it.
   - This is not what record 0191's decision 5 turned down. That was a link to an element's id
     in a shadow root, which a fragment does not reach; here the fragment is read by the app as
     a route.
2. **A screen registers itself; the shell knows no screen by name.** `registerScreen({ id,
   path, title, render })` in `web/src/routes.js`: `path` is segments, a literal or `:name`
   (`rooms/:room/sound`, or `alarms` for a house-wide screen), `title(params, view)` labels the
   screen's region, and `render(params, { view, refusals, refusalFields })` is the screen as a
   Lit template over what the shell's store holds. `addressOf(id, params)` writes a screen's
   address and `routeOf(fragment)` reads one; both are pure. Two screens with one id or with
   paths that cannot be told apart are refused when the second registers. A later screen is one
   module that registers, one import of it in the shell and one link to it.
3. **A link to a screen is a real link, and the app opens it as an entry of the history.**
   The "Sound" link on a room's card is `<a href="#/rooms/<room>/sound" data-route>`. The shell
   hears a click on any `data-route` link under it, through shadow roots, and opens the address
   with `history.pushState`, marking the entry's state (`{ chorus: true }`). A click with a
   modifier key or another button is left to the browser, which opens the same address in a new
   tab. The shell follows `popstate` and `hashchange`, so the browser's back and forward
   buttons and an address edited by hand all change the screen.
4. **"Back" on a screen is one step back in the history when the app opened the screen, and
   otherwise puts the home in the screen's place.** The mark of decision 3 is kept by the
   browser with the entry, through back, forward and a reload, so the app knows whether the
   entry below is its own home. If it is, "Back" is `history.back()`: the same step as the
   browser's button, and no entry is added, so going in and out of screens does not grow a
   trail a person has to press back through. If the screen was the address the app was opened
   at, or was typed, a step back would leave the app, and "Back" replaces the entry with the
   home instead. The navigation's "Groups" and "Rooms" buttons do the same and then go to their
   region.
5. **A further screen is painted alone, in the main region, with "Back" inside it.** The groups
   and the rooms are not painted under it. On a desktop it runs across both columns. "Back" is
   in the screen's region and not the header because a wide kiosk paints no header (0191,
   decision 9); it is a control of the least size of its mode like every other. On a phone the
   bar at the bottom edge stays. The screen's region takes the focus when it opens, and the
   rooms' region when the home returns, as a page's start does after a navigation.
6. **The sound screen has one control for each field of the `sound` command, and each sends
   that field alone.** Bass and treble are sliders over the catalog's whole dB from -10 to 10
   (`TONE_DB` in `web/src/api.js`, the bounds of `crates/control/src/sound.rs`), sent when the
   slider is let go; loudness, night mode and speech enhancement are buttons with
   `aria-pressed`, each asking for the opposite of what the server holds. `sound` is a partial
   update, and sending one field means a setting another client has just changed is never
   written back from a page that had not heard yet. `tv_upmix`, also a field of `sound`, is not
   on this screen (a later task's, with bass management).
7. **The screen keeps no value; a refusal names its field.** As on a room's card
   (`docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`), a control changes when
   the state that resulted comes back, and the one thing held back from an update is a slider a
   person has hold of. A setting the state does not carry is "Unavailable", never a default.
   The catalog's refusal carries the `field` it refused beside its `detail`; `command` in
   `web/src/api.js` now returns it, and the screen shows "Refused (bass): <detail>". The
   screen's commands have a subject of their own (`sound:<room>`), so its refusal is shown on
   the screen and not on the room's card.

## Consequences

- No browser has painted the sound screen in a test. The unit tests hold the navigation
  against happy-dom's history and the styles to the tokens (the "Back" link and the "Sound"
  link take `--control-size`, which the kiosk raises to 64 pixels), and the smoke test's
  touch-target check already walks every link of the home, the new "Sound" link included. What
  the screen looks like at each width is asserted when the smoke test is next extended (a later
  task's: the gate allows one browser test file).
- An address with a fragment is what "Sign in" links back to (the signed-out link is the
  page's own address), so signing in returns to the screen a person was on.
- Record 0191's "a later screen adds a navigation button" is replaced by decision 2: a screen
  registers and is linked from where it belongs. The bar keeps its two buttons.
- A house-wide screen has no card to be linked from. The first one decides where its link
  sits (the header, the bar); the registry and the shell's `data-route` handling need no change
  for it.
- `command` results now carry `field` on a catalog refusal. The room and group cards show the
  words alone, as before.

## What was read

- The task as the owner's agent harness gave it, and the files it points to: `web/README.md`,
  `web/src/chorus-app.js`, `web/src/api.js`, `web/src/state.js`, `web/src/room-card.js`,
  `web/src/mode.js`, `web/src/layout.js`, `web/test/fake-server.js`, `web/test/label-query.js`,
  `web/live/house.js`, `web/live/rooms.live.js`, `web/smoke/app.spec.js` (the touch-target
  check), `docs/app.md`; read 2026-10-05.
- `docs/control-plane.md` ("Per-room sound", the catalog's table of commands and the state
  message's `zones[]`), `crates/control/src/sound.rs` (`TONE_DB`), and the vectors
  `fixtures/control/v2/sound.json`, `sound-partial.json`, `state-rich.json` and
  `error-sound-bass-out-of-range.json`; read 2026-10-05.
- `docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`,
  `docs/decisions/0190-the-app-installs-behind-the-login.md`,
  `docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md`; read 2026-10-05.
- happy-dom 20.14.5 as installed from the lockfile, by running it: a probe that clicked a
  fragment link, set `location.hash`, called `history.pushState` and `history.back()` and
  printed the events and `history.length`; 2026-10-05. Its source was not read.
- Not read again for this record, and stated from the web platform's specifications as the
  author knows them: that a fragment is not part of the request a browser sends, that
  `history.pushState` adds an entry and fires no event, that the state given to it is kept with
  the entry across traversal and reload, and that traversal fires `popstate` (HTML, "Session
  history and navigation"; the URL standard). The unit tests hold the app to these as
  happy-dom models them; a real browser's behaviour is not asserted by a test of this change.
- No GPL source and no reciprocally licensed hardware design was opened.
