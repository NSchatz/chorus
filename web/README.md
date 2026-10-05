# web

The chorus app: Lit 3 elements bundled by esbuild (proposal P5, Option C; the decision record
`docs/decisions/0181-the-web-app-stack.md` says what is pinned and why). Today it is the shell
and the rooms screen: `chorus-app` holds the server's state, live, and shows every room with
its bonded set, its volume and its mute (`docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`),
and above them every saved and live group, with a drag and a list to move a room between them
and the server's group volume (`docs/decisions/0187-groups-in-the-app.md`). A room alone and
every formed group also say what they play and what is playing (title, artist, album, artwork
and whether it is playing, paused or buffering), with one button for each input the server
offers (`docs/decisions/0189-inputs-and-now-playing-in-the-app.md`). It installs behind the
household login: a manifest fetched with credentials, a hand-written service worker that
never answers or keeps the API, and the words "Signed out" with a link to sign in when the
login has lapsed (`docs/decisions/0190-the-app-installs-behind-the-login.md`). It lays itself
out for a phone, for a desktop and as a wall tablet's kiosk
(`docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md`; "Layouts and the kiosk"
below). Beyond the groups and the rooms it has further screens, each at an address of its own
in the fragment: a room's sound (bass, treble, loudness, night mode and speech enhancement), a
room's volume limit and quiet hours, the house's autoplay rules, and its alarms, stored sources
and sleep timers
(`docs/decisions/0197-further-screens-have-an-address-in-the-fragment.md`; "Further screens"
below).

| Path | What |
|---|---|
| `src/` | `index.html`, `app.css` and `tokens.css` (the document and the tokens), `main.js` (the entry point), the elements and their pure logic |
| `src/api.js`, `src/state.js` | the server as the app uses it (`api/state`, `api/events`, `POST api/command`) and the store that holds its last state message: the snapshot, then every event-stream update |
| `src/manifest.webmanifest`, `src/icons/` | the web manifest (name, icons, `start_url` and `scope` of `/app/`, standalone) and its icons, copied to `dist/` as they are; `icons.mjs` draws the icons, by hand when the drawing changes |
| `src/worker.js`, `src/sw.js`, `src/install.js` | the service worker: its rules as pure logic (what it answers, what it may keep), the worker that hands the browser's events to them (bundled to `dist/sw.js`), and its registration from the page |
| `src/rooms.js`, `src/room-card.js` | the rooms screen and one room's card |
| `src/groups.js`, `src/group-card.js` | the groups region (every saved group, then every live one) and one group's card, with the group volume |
| `src/playing.js` | what a room alone or a group plays: the now-playing record with its artwork (an image on the server's own `api/artwork` route), the source in words, and the picker of the offered inputs |
| `src/routes.js` | the further screens: the registry a screen adds itself to, its address in the fragment (`#/rooms/<room>/sound`), and the navigation over the browser's history |
| `src/sound.js` | a room's sound screen, registered at `#/rooms/<room>/sound`: one control for each field of the `sound` command |
| `src/limits.js` | a room's volume limit and quiet hours, registered at `#/rooms/<room>/limits`: the limit, the on/off switch, and the windows (days, start, end, limit) with the server's `effective_limit` and which window is active |
| `src/autoplay.js` | the house's autoplay rules, registered at `#/autoplay`: a switch and a target picker (rooms and saved groups) for each input |
| `src/alarms.js` | the house's alarms, stored sources and sleep timers, registered at `#/alarms`: an alarm's source picker offers the four kinds from the state (chimes, inputs, stored stream URLs, stored Spotify URIs) |
| `src/layout.js`, `src/mode.js`, `src/wake-lock.js` | the two layouts and the one breakpoint between them; the kiosk's switch (`?kiosk`) and its memory; the screen wake lock a kiosk holds |
| `src/grouping.js`, `src/drag.js` | a move (a room and a destination) as its one command, and the drag gesture on Pointer Events that makes one |
| `test/` | the unit tests (`*.test.js`), `setup.js` (happy-dom's globals, loaded before Lit), `label-query.js` (find an element by its label, through shadow roots) and `fake-server.js` (a scripted server: the three routes, answered as the test says) |
| `live/` | the live tests (`rooms.live.js`, `groups.live.js`, `playing.live.js`, `sound.live.js`, `limits.live.js`, `autoplay.live.js`, `alarms.live.js`): the same elements and store, in node with no browser, against a real `chorus-server`; `house.js` starts that server and `setup.js` gives the tests happy-dom's document and node's own network. `endpoint.js` is a scripted endpoint session that offers a line-in, and `control-point.js` a UPnP control point with the media and the cover it plays: what `playing.live.js` gives the server from outside (`autoplay.live.js` uses the endpoint too). `house.js` also starts what `alarms.live.js` needs for a stored source: a stream on loopback and the fake Soloist under the real receiver supervisor |
| `smoke/` | the one browser test (`app.spec.js`) and the fake login it signs in through (`fake-login.js`); `playwright.config.js` configures it |
| `build.mjs` | the build: `src/` into `dist/`, deterministic |
| `dist/` | the build's output, committed: `chorus-server` embeds it and never runs node |
| `licences.txt`, `licences.mjs` | the licence of every locked package, and the check of the installed ones against it |

From the repository root, with the pinned node and pnpm of `mise.toml` (`mise install`):

```sh
make web-test     # node --test over happy-dom: no browser
make web-build    # rebuild dist/; commit it with the change to src/
CHORUS_SERVER_BIN=<a built chorus-server> make web-live    # the screens against a real server, no browser
CHORUS_SERVER_BIN=<a built chorus-server> make web-smoke   # the one browser test
```

`make web-live` starts that server with two rooms, bonds a stereo pair in one of them, mounts
`chorus-app` over a store that reads it and holds the page to the server in both directions:
both rooms and the bond are rendered, a volume change and a mute made through the card's
controls are read back from the server's `/api/state`, and a change another client makes
appears in the card with no reload. `groups.live.js` does the same for grouping, with three
rooms and a saved group: a room dragged onto another forms a live group, the group slider
moves every room and keeps their ratio, a room taken out dissolves the group and forming the
saved group fills it, each read back from `/api/state` and compared with the levels the page
shows. `playing.live.js` does it for inputs and what is playing, on a server with one network
player and its renderers: an endpoint session offers a line-in and the picker lists it, by the
label a client then gives it; choosing it through the picker changes the group's `source` in
`/api/state`; and a track a control point plays on a room's renderer shows its title and
artist in that room's card, with an artwork image whose address the server answers `200` with
the cover. `sound.live.js` opens a room's sound screen from the room's card, changes every
control through the screen and reads each back equal from `/api/state`, and a `sound` command
a second client sends appears on the screen with no reload; a command the server refuses is
shown with the field and the words of the server's own answer. `limits.live.js` starts the
server with its civil clock held (`--civil-time wed-23:30`), so which window is active is the
server's answer and not the hour of the run: a limit set through the screen pulls the room's
volume down, a window added through the screen that covers the held time is `active` and lowers
`effective_limit` in `/api/state`, a volume asked for above it (by a second client, and on the
room's card) is shown as the server clamps it, and editing, switching off and removing the
windows are each read back. `autoplay.live.js` has an endpoint session offer a line-in, makes
that input's rule through the screen (a room, the switch, a saved group) and reads each step
back from the `autoplay` of `/api/state`. `alarms.live.js` runs the server's schedule ten times
faster from three schedule minutes before 07:00, sets an alarm of each of the four source kinds
through the screen and holds each to ringing as its own kind, stops them through the screen, and
lets a sleep timer end by itself (`docs/app.md`, "Running the tests", says what it starts and
which two test programs it needs beside the server). It ends `web-live: PASS`; without `CHORUS_SERVER_BIN` it
ends `web-live: SKIPPED`, which under `CI=true` and in the gate is a failure. **A later screen
proves itself the same way**: a file `live/<screen>.live.js` that starts the server its screen
needs, drives the screen's controls by their labels and compares with `/api/state`.

`make web-smoke` starts that server with two rooms, signs in through the fake login in front
of it, loads `/app/` in Playwright's headless Chromium and asserts what the page rendered (a
room's name as the server has it) and that the page reported no Content-Security-Policy
violation. Its tail is the install: the manifest link has `crossorigin="use-credentials"` and
the browser loads the manifest through the login; the service worker reaches the active state
and controls the page; after the page has read the state, followed an event and sent a command
through it, the caches hold the files of `dist/` and no entry under `/api/`; and with the fake
login expired the page says "Signed out", its link leads to the login page, and no cache holds
that page. Without `CHORUS_SERVER_BIN` it ends `web-smoke: SKIPPED`; under `CI=true` and in
the gate that is a failure. It is the only test that starts a browser: a later change that
needs one adds to `smoke/app.spec.js`. The tests after the first in that file are the layouts
and the kiosk, each named for what it holds: the phone layout at 390 pixels, the desktop
layout at 1280, the breakpoint (767 pixels is one column, 768 is two, followed as the window
is resized) and kiosk mode.

The Chromium build is the one the pinned `@playwright/test` names. Install it once with
`make web-smoke-install` (CI: `bash tools/web.sh smoke-install --with-deps`, which also
installs its libraries with apt); a run never downloads it. On a host with no root Chromium's
shared libraries and fonts come from two conda-forge prefixes that `tools/web.sh` puts in the
browser's environment when they exist (`CHORUS_CHROMIUM_LIBS`, `CHORUS_CHROMIUM_FONTS`):

```sh
micromamba create -y -p /cache/opt/chromium-libs -c conda-forge nss nspr glib dbus atk-1.0 \
    at-spi2-atk at-spi2-core xorg-libxcomposite xorg-libxdamage xorg-libxrandr alsa-lib
micromamba create -y -p /cache/opt/chromium-fonts -c conda-forge fontconfig fonts-conda-ecosystem
```

## Further screens

The groups and the rooms are the home, at `/app/` (or `/app/#/`). Every other screen has an
address in the fragment and is painted alone in the main region under a "Back" link, in every
layout and in the kiosk:

| Address | Screen | Module |
|---|---|---|
| `#/rooms/<room>/sound` | a room's sound | `src/sound.js` |
| `#/rooms/<room>/limits` | a room's volume limit and quiet hours | `src/limits.js` |
| `#/autoplay` | the house's autoplay rules | `src/autoplay.js` |
| `#/alarms` | the house's alarms, stored sources and sleep timers | `src/alarms.js` |

A later screen is one module and one link:

```js
import { registerScreen } from "./routes.js";

registerScreen({
  id: "alarms",
  path: "alarms", // or "rooms/:room/limits": a segment that starts with ":" is a parameter
  title: (params, view) => "Alarms", // the label of the screen's region
  render: (params, { view, refusals, refusalFields }) => html`<chorus-alarms .state=${view.state}></chorus-alarms>`,
});
```

and, wherever it is reached from, `<a href=${addressOf("alarms")} data-route>Alarms</a>`. The
shell (`chorus-app`) imports the module, opens a `data-route` link as an entry of the browser's
history (so the browser's back button returns from it) and passes the screen what the store
holds. A screen sends a command as every card does, with a `chorus-command` event whose detail
is `{ subject, body }` (and, where the screen has to know when the server has answered, a
`done` the shell then calls); the server's words for a refused one come back in `refusals[subject]`,
and the field it named in `refusalFields[subject]`. It proves itself as the sound screen does:
`test/<screen>.test.js` over the scripted server and `live/<screen>.live.js` over a real one.

## Layouts and the kiosk

| | When | What is painted |
|---|---|---|
| phone | the viewport is narrower than 48em (768 CSS pixels at the default text size) | one column, the groups and then the rooms; the navigation ("Groups", "Rooms") in a bar fixed to the bottom edge, under the thumb |
| desktop | the viewport is 48em wide or wider | two columns, the groups with what each plays beside the rooms; the navigation in the header |
| kiosk | the app was opened with `?kiosk` | either layout, by its width, with no wordmark (and, wide, no header), every control at least 64 by 64 CSS pixels and larger text; the screen kept on |

The breakpoint is `DESKTOP_MIN_EM` in `src/layout.js` and is written nowhere else: `chorus-app`
reflects the layout as its `layout` attribute and the styles select on it, so no style holds
a media query. There is no television layout (K86).

Kiosk mode is for a tablet on a wall. Its switch is the address: open `/app/?kiosk` once and
that browser is a kiosk from then on (the choice is kept in `localStorage`, so it survives a
reload and the installed app's start address); `/app/?kiosk=0` leaves it. A touch target is at
least 44 by 44 CSS pixels in the ordinary app (`--control-size` in `src/tokens.css`) and at
least 64 by 64 in the kiosk (`--kiosk-control-size` in `src/app.css`). A kiosk asks the
browser for a screen wake lock and asks again each time the page becomes visible, because a
browser takes the lock back when the page is hidden. The Screen Wake Lock API exists only in
a secure context (HTTPS, or the loopback address): without it, or refused, the kiosk works
the same and the screen sleeps as the tablet is set. The browser's own chrome is not the
page's to remove: install the app (it is `standalone`) or set the tablet's browser up as a
kiosk.

Rules, each held by a check (`docs/conventions.md`, rules 13 and 14):

- Install scripts are off (`pnpm-workspace.yaml`, where pnpm 12 reads it; there is no
  `.npmrc`); every dependency is an exact version; the direct ones are lit, esbuild, happy-dom, `@happy-dom/global-registrator` and
  `@playwright/test` (the smoke test's, never bundled) and no other. A lockfile change brings the new packages' lines in `licences.txt`.
- A change to `src/` comes with the rebuilt `dist/`: the gate step `web-build` rebuilds and
  fails on a difference.
- An element's styles name tokens of `tokens.css`, never a literal colour or length, and
  nothing is inline: the server's Content-Security-Policy allows neither inline script nor
  inline style.
- The service worker never answers a request for the server's `/api/` and never writes one to
  a cache; what it keeps is the app's own files, each a plain `200` of this origin
  (`mayStore` in `src/worker.js`, the one place that decides). A new rule for the worker is a
  function of `worker.js` with a unit test, not a line of `sw.js`.
- Every request of `src/api.js` is made with `redirect: "manual"`: a redirect is the login's
  answer, and the page says "Signed out" instead of reading a login page as the server's.
- A layout is an attribute `chorus-app` reflects (`layout`, `mode`), never a media query in an
  element's styles: the breakpoint is a length, and it is written once, in `src/layout.js`.
  What a browser paints in a layout is asserted in `smoke/app.spec.js`.
- A screen beyond the home has an address (`registerScreen` in `src/routes.js`) and is reached
  by a link that holds it, never by a property of the shell a reload would lose.
- A test finds a control by its label (`getByLabel`), which also proves it has one.
- The server owns the state. An element shows what the store's last state message says and
  keeps no value of its own; a command changes the page when the state that resulted comes
  back, never before (no optimistic value), and a refused one shows the server's words. The
  one thing an update does not touch is a control a person has hold of. A group's volume is
  the figure the server gives for the group: the app averages and scales nothing.
