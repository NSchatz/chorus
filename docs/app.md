# The app

The chorus app is the installable web app a household uses on a phone, a desktop and a wall
tablet: rooms, groups, volume, inputs and what is playing. This page says what was built and how
to check it. Its source is `web/` (`web/README.md` is the map of the files and the rules a change
there follows); the decisions behind it are records 0181 to 0191 in `docs/decisions/`, named
where each applies below. The stack is the owner's choice of proposal P5, Option C
(`docs/proposals/P5-app-stack.md`, K43): Lit 3 elements bundled by esbuild, tested with node's own
runner over happy-dom, with one browser test.

Nothing on this page is a measurement, and nothing here has been seen on a phone: what an agent
can show is a headless Chromium on a development host. What only the owner's phones can show is
the "Phone check" section at the end, which is the owner's (K4) and has an item in the owner's
queue.

## Where it is served

`chorus-server` serves the app itself, under `/app/` on its control listener, from files compiled
into the binary (`docs/decisions/0182-the-app-is-served-under-app.md`; the routes and headers are
in `docs/control-plane.md`, "The app under `/app/`").

- `GET /app/` and `GET /app/index.html` are the document; `GET /app/<path>` is the file of
  `web/dist` at that path; `GET /app` answers `308` to `/app/`; anything else under `/app/` is a
  `404`.
- `/app/` is permanent. An installed app keeps its start address and its scope, so the path does
  not move, also when the app later replaces the control page at `/` (`docs/control-page.md`).
- `web/dist` is the build's output, committed. `crates/server/build.rs` compiles it in with the
  standard library alone, so the server, the image and CI's Rust build need no node.
- The document, `sw.js` and the manifest are served `no-cache`; a file named by the hash of its
  content (`assets/<name>-<hash>.<ext>`) is served `immutable` for a year. Every file has a
  strong `ETag` and is answered `304` when the browser already holds it.
- Every response carries the server's one Content-Security-Policy, the control page's with
  `manifest-src 'self'` added: no inline script, no inline style, images from this origin only.

The server has no authentication and the app adds none (K40). A household reaches it through its
reverse proxy: HTTPS, local network only, behind the household login. HTTPS is not optional for
the app: a service worker, an install and the screen wake lock exist only in a secure context, so
the app opened on the server's plain HTTP port from another machine works as a page and installs
nothing.

## The screens

One document and one element (`chorus-app`). Its home is two regions, the groups and the rooms:
the navigation ("Groups", "Rooms") brings a region to the top and puts the focus on it. Every
further screen has an address of its own (below).

| Region | What it shows | What it does |
|---|---|---|
| Groups | every saved group, then every live group, each with its rooms, what it plays and what is playing | group volume (the server scales every room and keeps their ratio, K77); "Remove" a room; "Group these rooms" for a saved group that is not formed; choose an input |
| Rooms | every room with its bonded set, its volume and its mute; a room alone also says what it plays and what is playing | volume, mute; move the room by dragging its handle onto a room or a group, or with the "Plays with" list on its card; choose an input; open the room's sound screen ("Sound") |

**Further screens and their addresses** (`docs/decisions/0197-further-screens-have-an-address-in-the-fragment.md`).
A screen beyond the home has an address in the fragment, `#/` and a path, and is painted alone
in the main region, across both columns on a desktop, under a "Back" link:

| Address | Screen |
|---|---|
| `/app/` or `/app/#/` | the home: the groups and the rooms |
| `/app/#/rooms/<room id>/sound` | the sound of that room |

- Opening a screen from the app (the "Sound" link on a room's card) is a new entry of the
  browser's history, so the browser's back button, a phone's back gesture and the forward
  button work as on any site, and an address can be bookmarked, reloaded or sent to another
  person of the household.
- "Back" on the screen is that same one step back. Where the screen was the address the app was
  opened at, there is nothing of the app to step back to, and "Back" puts the home in its place.
- The navigation's "Groups" and "Rooms" leave a further screen for the region they name.
- An address that names no screen is the home.
- The fragment is never sent to the server, so the login, the service worker and the kiosk's
  `?kiosk` switch are untouched by it: `/app/?kiosk#/rooms/living/sound` is a kiosk on that
  screen.
- A later screen registers itself with `registerScreen` in `web/src/routes.js` (an id, a path
  such as `rooms/:room/limits` or `alarms`, a title and what it renders) and is linked with
  `<a href=${addressOf(id, params)} data-route>`; nothing else of the shell changes.

**A room's sound** (`web/src/sound.js`; the catalog's `sound` command, `docs/control-plane.md`,
"Per-room sound"):

| Control | What it is | What it sends |
|---|---|---|
| Bass, Treble | a slider each, whole dB from -10 to 10, with the value beside it ("+3 dB") | `sound` with `bass` or `treble` alone, when the slider is let go |
| Loudness, Night mode, Speech enhancement | a button each, pressed when on, with "On" or "Off" beside it | `sound` with `loudness`, `night` or `speech` alone: the opposite of what the server holds |

Each control sends one command that carries its one field, so a setting another client has just
changed is never written back from a page that had not yet heard. Nothing changes on the screen
until the server's answer says so; a refused command is shown as "Refused (<field>): <the
server's words>"; a setting the state does not carry is "Unavailable". A change made anywhere
else appears on the screen with no reload, except under a slider a person has hold of. Bass
management, the TV upmix and a room's limits are not on this screen.

- **The server owns the state** (`docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`).
  The app reads `GET /api/state`, follows `GET /api/events` and sends `POST /api/command`. An
  element shows what the last state message says and keeps no value of its own: a control changes
  on the page when the state that resulted comes back, and a refused command shows the server's
  words. The one thing an update leaves alone is a control a person has hold of.
- **Groups** (`docs/decisions/0187-groups-in-the-app.md`). A move is a room and a destination and
  becomes one command. The drag is written on Pointer Events, not HTML drag and drop, so that it
  starts from a finger: press the handle on a room's card ("Move <room>"), drag it onto another
  room or a group, let go. A release over nothing, or Escape, changes nothing. The same move
  without a drag is the "Plays with" list on the card, a native list a keyboard and a screen
  reader have for nothing.
- **Inputs and now playing** (`docs/decisions/0189-inputs-and-now-playing-in-the-app.md`). One
  button for each input the server offers. What is playing is the title, the artist, the album,
  the artwork and whether it is playing, paused or buffering. The app controls rooms, groups,
  inputs and sound, not content (K64): there is no library and no queue.
- **Artwork** (`docs/decisions/0184-artwork-is-proxied-not-the-policy-widened.md`). The image is
  fetched by the server and served on its own `GET /api/artwork?group=<id>`, so the policy's
  `img-src 'self'` stands. A record with no artwork, or one whose image does not load, shows a
  placeholder.

Not built yet, and later tasks: limits, quiet hours and autoplay; bass management and the TV
upmix; alarms and sleep timers;
room correction with the phone's microphone; adoption, naming and firmware approval.

## Installing, and the service worker's cache rules

The app installs from the browser (`docs/decisions/0190-the-app-installs-behind-the-login.md`).
Its manifest (`manifest.webmanifest`: name "chorus", `start_url`, `scope` and `id` of `/app/`,
`standalone`, icons of 192 and 512 pixels and a 180 pixel icon for a phone's home screen) is
linked with `crossorigin="use-credentials"`, because behind a login a manifest fetched without
the session is refused and an app with no manifest does not install.

The service worker is written by hand: `web/src/worker.js` is its rules as pure functions,
`web/src/sw.js` hands the browser's events to them, and the build bundles both into `dist/sw.js`.
It exists so that the app opens when the server cannot be reached, and it must never serve the
login page as the app. Its rules:

| Request | What the worker does |
|---|---|
| anything under the server's `/api/` (the state, a command, the artwork, every event stream) | nothing: the request is the browser's own, exactly as with no worker; it is never fetched by the worker and never written to a cache |
| another origin, the server's other pages, any request that is not a `GET` | nothing, the same |
| a navigation to the app | the network first; the document it answers with is kept as the shell; the shell is the answer when the network gives none, or gives a `5xx` (a proxy answering for a server that is down); a redirect to the login is handed to the browser, which follows it |
| a file named by its hash (`assets/<name>-<hash>.<ext>`) | the cache first, the network when it is not there yet |
| any other file of the app | the network first, the cache when the network fails |

One function, `mayStore`, decides what is written to a cache, and every write goes through it:
only a `GET` of a file under `/app/` that is not the API, answered with status `200`, of type
`basic` (this origin's), not redirected. So a redirect that was not followed, the page a followed
redirect ended on, a `401`, a `404`, a gateway's `502` and another origin's response are never
kept, whichever path they arrive on.

- At install the worker fetches every file of the build (the shell) with `redirect: "manual"` and
  keeps them all or none: a worker installed while the login has lapsed is not installed, and the
  browser tries again at the next load.
- The cache is `chorus-app-<version>`, the version a digest of the build's files; activating
  deletes the other `chorus-app-` caches and nothing else.
- The worker does not skip waiting and claims no open page. A new build reaches an open page at
  its next load, and its worker takes over when the old one's pages are closed.
- Offline, or with the server down, a person gets the shell, which says the server cannot be
  reached and follows it when it is back. Nothing can be controlled offline.

## The signed-out state

A login lapses, and the proxy then answers any request with a redirect to its login page or with
a refusal. Every request of `web/src/api.js` is made with `redirect: "manual"`, so the page never
reads a login page as the server's answer. An `opaqueredirect` or a `401` is read as signed out:
`chorus-server` answers neither on any route, so both can only come from in front of it.

- The page says "Signed out." with a link, "Sign in", to its own address. Following it is a
  navigation: the login shows its page and returns to the app.
- The rooms last known stay on the page, marked as last known. Nothing can be changed: a command
  is answered by the login and never reaches the server, and the page says so at once.
- The event stream keeps trying, so a person who signs in in another tab is live again with no
  reload.

Not known from here: whether the real login answers an API call with a redirect or a `401` (both
are read as signed out), and whether its redirect completes inside an installed app on a phone.
The second is step 5 of the phone check.

## The three layouts, and kiosk mode

`docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md`. There is no television layout
(K86).

| Layout | When | What is painted |
|---|---|---|
| phone | the viewport is narrower than 48em (768 CSS pixels at the default text size) | one column, the groups and then the rooms; the navigation in a bar fixed to the bottom edge |
| desktop | the viewport is 48em wide or wider | two columns, the groups beside the rooms; the navigation in the header |
| kiosk | the app was opened with `?kiosk` | either of the two by its width, with no wordmark (and, wide, no header), every control at least 64 by 64 CSS pixels, larger text, and the screen kept on |

The one breakpoint is `DESKTOP_MIN_EM` in `web/src/layout.js` and is written nowhere else:
`chorus-app` reflects the layout as an attribute and the styles select on it, so no element holds
a media query. A touch target is at least 44 by 44 CSS pixels outside the kiosk.

A further screen (a room's sound) is the same in all three: one column as wide as the page, with
its "Back" link inside the screen's own region, because a wide kiosk paints no header. On a
phone the navigation bar stays at the bottom edge and leads back to the groups or the rooms.

**Entering kiosk mode.** Kiosk mode is for a tablet on a wall, and its switch is the address:

1. Open `/app/?kiosk` once in the tablet's browser. That browser is a kiosk from then on: the
   choice is kept in `localStorage`, so it survives a reload and the installed app's start
   address, which carries no query.
2. Install the app from that browser, or set the tablet's browser up as a kiosk. The browser's
   own chrome is not the page's to remove; the manifest's `standalone` is what removes it.
3. To leave, open `/app/?kiosk=0`.

A kiosk asks for a screen wake lock and asks again each time the page becomes visible, because a
browser takes the lock back when the page is hidden. Without a secure context, or when the browser
refuses, the kiosk works the same and the screen sleeps as the tablet is set.

## Running the tests

From the repository root, with the pinned node and pnpm of `mise.toml` (`mise install`). CI runs
all three in `make gate`; here each is a narrow run of its own.

| Command | What it is | It ends with |
|---|---|---|
| `make web-test` | the unit tests: `node --test` over happy-dom, no browser, no server. The elements by their labels, the store, the worker's rules against a scripted network and cache, the signed-out state, the layouts and the kiosk's switch, the navigation over happy-dom's own address and history, and the sound screen over a scripted server | node's own summary, `fail 0` |
| `CHORUS_SERVER_BIN=<a built chorus-server> make web-live` | the live tests (`web/live/*.live.js`): the same elements and store in node, no browser, against a real `chorus-server`. Each screen's controls are driven by their labels and what the page shows is compared with the server's `/api/state`, in both directions | `web-live: PASS` |
| `CHORUS_SERVER_BIN=<a built chorus-server> make web-smoke` | the one browser test (`web/smoke/app.spec.js`, `docs/decisions/0183-the-one-browser-smoke-test.md`): Playwright's headless Chromium loads `/app/` from a real `chorus-server` through a fake login | `web-smoke: PASS` |

- `web-live` and `web-smoke` without `CHORUS_SERVER_BIN` end `web-live: SKIPPED` and
  `web-smoke: SKIPPED`; under `CI=true` and in the gate a skip is a failure.
- `make web-smoke-install` downloads, once, the Chromium build the pinned `@playwright/test`
  names; a run never downloads it. On a host with no root Chromium's libraries and fonts come
  from two prefixes, as `web/README.md` writes out.
- `make web-build` rebuilds `web/dist` after a change to `web/src`; the output is committed with
  the change, and the gate rebuilds it and fails on a difference.

What the smoke test holds, since it is the only test in which a browser paints the app: the
page renders a room's name as the server has it and reports no Content-Security-Policy
violation; the manifest loads through the login; the service worker becomes active and controls
the page; after the page has read the state, followed an event and sent a command, the caches
hold the build's files and nothing under `/api/`; with the fake login expired the page says
"Signed out", its link leads to the login page and no cache holds that page; the phone layout
at 390 pixels, the desktop layout at 1280, the breakpoint (767 is one column, 768 is two) and
kiosk mode. A later change that needs a browser adds to that one file.

What no test here holds is everything below.

## Phone check

The owner's, on the owner's own phones: no agent has a phone, the household's login or the
household's network. Its item is in the owner's queue (issues in the owner's agent harness;
`goals needs add`, `/goals:needs`), and this section is what it carries. It settles what
`docs/proposals/P5-app-stack.md` ("Open inputs") still marks `ASSUMED` about Safari and iOS, and
it is the first time the app is in a hand.

**Before it starts.** All three are the owner's own steps and none is done by this repository:

- A `chorus-server` whose build serves `/app/` is deployed. `GET /app/` on it answers `200`; a
  `404` means the deployed image is older than the app.
- The household's reverse proxy serves it over HTTPS behind the household login, with a
  certificate the phones trust.
- At least two rooms exist, and something with artwork can be played in one of them (a control
  point casting a track with a cover is enough).

**Which phones.** Every phone the household will use, and at least one iPhone in Safari, because
the open inputs are Safari's. Run steps 1 to 8 on each phone, in order. For each phone write down
its model, its OS version and its browser with its version.

1. **Open it.** On the house Wi-Fi, open `https://<the address chorus is served on>/app/` in the
   phone's browser and sign in at the household login when it asks.
   Report: whether the browser showed any certificate warning; whether the login returned to
   the app; whether the rooms appear, and whether the page looks styled (cards, a bar with
   "Groups" and "Rooms" along the bottom edge) or is bare unstyled text. Bare text on Safari
   means element styles are refused under the policy there.
2. **Install it from the browser.** Safari: the share button, then "Add to Home Screen". Chrome
   on Android: the menu, then "Install app" or "Add to Home screen".
   Report: whether the browser offered it; the name and the icon the home screen shows (it
   should be "chorus", a dot and two rings on a dark square).
3. **Launch it from the home screen.** Tap the icon.
   Report: whether it opens with no browser address bar; whether it asks to sign in again and,
   if so, whether signing in comes back to the app inside the installed window or lands in the
   browser instead; whether the rooms are live (change a volume from another device and watch it
   move with no reload).
4. **Relaunch it.** Close the app from the app switcher, then tap the icon again. Do it once
   more with the phone in airplane mode, then turn airplane mode off.
   Report: online, whether it opens straight to the rooms with no sign-in; in airplane mode,
   whether the app's own page opens, saying the server cannot be reached or the connection is
   lost (and not a browser error page); whether it goes live again by itself after airplane mode is turned off.
5. **Signed-out recovery.** End the session while the installed app is open: sign out at the
   household login (in the browser on Android, which shares its session with the installed app;
   on an iPhone the installed app has a session of its own, so end that session at the login's
   own session list, or leave the app until the login lapses). Bring the installed app to the
   front and move a volume slider.
   Report: whether the page says "Signed out." with a "Sign in" link, and how long that took to
   appear; whether the rooms stay on the page; whether tapping "Sign in" shows the login, and
   after signing in returns to the app, in the installed window, live again; anything else it
   did instead (a blank page, the login page shown inside the app's frame for good, a browser
   tab opening).
6. **Artwork.** Play something with a cover in one room.
   Report: whether the cover appears on that room's card beside the title and the artist, and
   whether it changes when the track changes; if a placeholder shows instead, the title that
   was playing.
7. **Drag to group on touch.** Press the handle on one room's card, drag it onto another room's
   card and let go. Then drag the room out again, or use "Remove" on the group's card.
   Report: whether the drag starts from a finger or the page scrolls instead; whether the room
   under the finger is marked during the drag; whether the group appears under "Groups" after
   the release, and dissolves again; whether the group's volume slider moves both rooms. If the
   drag does not work, whether the "Plays with" list on the card does the same move.
8. **Anything else the hand notices.** Controls too small to hit, text cut off, the bottom bar
   under the phone's own home indicator, a slider that fights the finger.

Optional, when a tablet is on hand: open `/app/?kiosk`, install it, and report whether the
controls are larger, whether the screen stays on for longer than the tablet's own sleep time, and
whether the kiosk is still a kiosk after the installed app is closed and opened again.

**What to report.** Per phone: the model, OS and browser line, then for each of steps 1 to 8
"as written" or what happened instead, in a sentence. A screenshot helps for a layout fault; no
address, account name or household name belongs in what is pasted back.

**How it ends.** The owner tells any session of the owner's agent harness what happened, in the
owner's own words; that session records it and closes the item. What the answer changes:

- The `ASSUMED` lines of `docs/proposals/P5-app-stack.md` ("Open inputs") for Safari are
  replaced by what the phones did: element styles under the policy (step 1), the login's
  redirect inside an installed app (steps 3 and 5), the drag on touch (step 7), and HTTPS with
  a certificate the phones trust (step 1).
- A step that did not go as written becomes a task of its own, with the phone's line and the
  report as its evidence.
- Not part of this check: the microphone inside an installed app (`getUserMedia`), which belongs
  to the room-correction screen and is checked when that screen exists.
