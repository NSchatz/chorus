# web

The chorus app: Lit 3 elements bundled by esbuild (proposal P5, Option C; the decision record
`docs/decisions/0181-the-web-app-stack.md` says what is pinned and why). Today it is the shell
and the rooms screen: `chorus-app` holds the server's state, live, and shows every room with
its bonded set, its volume and its mute (`docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`).

| Path | What |
|---|---|
| `src/` | `index.html`, `app.css` and `tokens.css` (the document and the tokens), `main.js` (the entry point), the elements and their pure logic |
| `src/api.js`, `src/state.js` | the server as the app uses it (`api/state`, `api/events`, `POST api/command`) and the store that holds its last state message: the snapshot, then every event-stream update |
| `src/rooms.js`, `src/room-card.js` | the rooms screen and one room's card |
| `test/` | the unit tests (`*.test.js`), `setup.js` (happy-dom's globals, loaded before Lit), `label-query.js` (find an element by its label, through shadow roots) and `fake-server.js` (a scripted server: the three routes, answered as the test says) |
| `live/` | the live test (`rooms.live.js`): the same elements and store, in node with no browser, against a real `chorus-server`; `setup.js` gives it happy-dom's document and node's own network |
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
appears in the card with no reload. It ends `web-live: PASS`; without `CHORUS_SERVER_BIN` it
ends `web-live: SKIPPED`, which under `CI=true` and in the gate is a failure. **A later screen
proves itself the same way**: a file `live/<screen>.live.js` that starts the server its screen
needs, drives the screen's controls by their labels and compares with `/api/state`.

`make web-smoke` starts that server with two rooms, signs in through the fake login in front
of it, loads `/app/` in Playwright's headless Chromium and asserts what the page rendered (a
room's name as the server has it) and that the page reported no Content-Security-Policy
violation. Without `CHORUS_SERVER_BIN` it ends `web-smoke: SKIPPED`; under `CI=true` and in
the gate that is a failure. It is the only test that starts a browser: a later change that
needs one adds to `smoke/app.spec.js`.

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

Rules, each held by a check (`docs/conventions.md`, rules 13 and 14):

- Install scripts are off (`pnpm-workspace.yaml`, where pnpm 12 reads it; there is no
  `.npmrc`); every dependency is an exact version; the direct ones are lit, esbuild, happy-dom, `@happy-dom/global-registrator` and
  `@playwright/test` (the smoke test's, never bundled) and no other. A lockfile change brings the new packages' lines in `licences.txt`.
- A change to `src/` comes with the rebuilt `dist/`: the gate step `web-build` rebuilds and
  fails on a difference.
- An element's styles name tokens of `tokens.css`, never a literal colour or length, and
  nothing is inline: the server's Content-Security-Policy allows neither inline script nor
  inline style.
- A test finds a control by its label (`getByLabel`), which also proves it has one.
- The server owns the state. An element shows what the store's last state message says and
  keeps no value of its own; a command changes the page when the state that resulted comes
  back, never before (no optimistic value), and a refused one shows the server's words. The
  one thing an update does not touch is a control a person has hold of.
