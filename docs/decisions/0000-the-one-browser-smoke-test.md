# 0000: the gate has one browser test: Playwright's headless Chromium loads the app from a real chorus-server through a fake login and asserts rendered text and no policy violation; a host with no root gives Chromium its libraries and fonts from two prefixes

- Status: accepted, 2026-10-05. Builds the "one browser smoke test" that proposal P5 and
  record 0181 left to a later change.
- Decided by: the owner for the test itself and its tool (proposal P5,
  `docs/proposals/P5-app-stack.md`, "The one browser smoke test" and "Tests", approved at
  Checkpoint K). The version, where the browser is kept, the shape of the fake login and what
  the step refuses are this record's.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/smoke/app.spec.js`, `web/smoke/fake-login.js`,
  `web/playwright.config.js`; `tools/web.sh` (`smoke`, `smoke-install`); the `web-smoke` and
  `web-smoke-install` targets of the `Makefile`; step `web-smoke` of `tools/gate.sh`; the step
  `Chromium for the browser smoke test` of `.github/workflows/ci.yml`;
  `tools/conventions/check-web.sh` (the settled stack); `web/src/rooms.js` and the list in
  `web/src/chorus-app.js` (what the test reads).

## Context

The app's unit tests run in node over happy-dom (record 0181, item 8). That holds an
element's logic and markup; it does not hold that a real browser, handed the files
`chorus-server` really serves under its real Content-Security-Policy, draws anything. The
rendering check that used to hold this for the control page was retired on 2026-09-30 with
the rest of its gate (`docs/verification-record.md`), and nothing has started a browser since.

P5 settled one browser test and no more: a browser is the slowest and least portable thing
in the gate, and every further screen can prove itself in node if one test proves the path
from the server to drawn text.

Two facts about the hosts bind how it runs:

- The development host has no root. Chromium's headless shell there lacks 14 shared libraries
  (`ldd`: glib, gobject, gio, nspr, nss, nssutil, atk, atk-bridge, atspi, dbus, Xcomposite,
  Xdamage, Xrandr, asound) and the host has no fonts. Seen on 2026-10-05 with the pinned
  build: without the libraries the browser exits 127 before it starts; with the libraries and
  no font it starts and the page crashes as soon as it lays out text; with both the test
  passes.
- CI's runner has root, so the libraries and fonts come from its package manager.

## Decision

1. **`@playwright/test` 1.63.0, exact, a development dependency of `web/`, and the fifth and
   last package of the settled stack.** It was the newest release when read (published
   2026-09-04, Apache-2.0, `https://registry.npmjs.org/@playwright/test`, read 2026-10-05).
   It brings `playwright` and `playwright-core` at the same version and licence and nothing
   else; the three have their lines in `web/licences.txt` and their digests in
   `web/pnpm-lock.yaml`. `check-web.sh` names it in the stack, so a sixth package still needs
   a record. It is never bundled: `web/build.mjs` reads `src/` alone.
2. **The browser is the one that version names, and only the headless shell.** A Playwright
   release carries its own list of browser builds; 1.63.0 names Chrome Headless Shell
   153.0.8010.12 (its build 1243). `tools/web.sh smoke-install` downloads that build and
   nothing else chooses a browser, so the pin and the lockfile's digest fix the browser as
   well. An upgrade of the pin is an upgrade of the browser, in one commit (rule 14).
3. **A run downloads nothing.** `make web-smoke` installs the locked packages as the other web
   steps do, and fails naming `bash tools/web.sh smoke-install` when the build is not there.
   The download is its own command, run once on a development host and by its own step in CI.
4. **The build is kept in a directory of chorus's own**, `/cache/chorus-playwright` on the
   development host (`PLAYWRIGHT_BROWSERS_PATH` overrides it; elsewhere Playwright's default).
   A Playwright install deletes the builds in its directory that no installed Playwright it
   knows of still uses. While this change was written, an install into a directory shared
   with another project removed that project's two builds (they were put back, the same
   builds, the same hour). A directory of its own makes that impossible.
5. **On a host with no root the libraries and the fonts are two prefixes the runner puts in
   the browser's environment, when they exist**: `CHORUS_CHROMIUM_LIBS` (default
   `/cache/opt/chromium-libs`) on `LD_LIBRARY_PATH`, and `CHORUS_CHROMIUM_FONTS` (default
   `/cache/opt/chromium-fonts`) through `FONTCONFIG_FILE`. Both are conda-forge environments;
   `web/README.md` has the two commands. Where neither exists (CI) nothing is set and the
   system's are used. The runner prints which it used.
6. **The server is real and the login is fake.** The test starts the `chorus-server` that
   `CHORUS_SERVER_BIN` names, with two rooms, no audio device and loopback ports of its own
   choosing, and gives one room a name with an ordinary `name` command. In front of it stands
   `web/smoke/fake-login.js`, about 150 lines of node's own `http`: without a session a page
   request is redirected to a login form and anything else is refused with `401`; the form
   takes any name and sets a session cookie; with a session the request is passed to the
   server with a `Remote-User` header and the browser's own `Host`. That is the shape of a
   forward-auth reverse proxy, which is how the app is meant to be deployed, and no more of
   it: no password, no proxy product, no file of any deployment.
7. **What it asserts is what the page rendered.** Signed out, the browser ends at the login
   and the API answers `401`. Signed in, it is at `/app/`, the document carries the server's
   policy, the wordmark is visible, and the rooms region lists the rooms by the names the
   server has for them, the given name among them, with a box of more than zero width and
   height. No screenshot and no image size is compared.
8. **Zero Content-Security-Policy violations, counted by a listener that is shown to hear.**
   The page's `securitypolicyviolation` events and its console and page errors are collected
   from before the first navigation and must be empty. The test then adds an inline style,
   which the policy forbids, and requires exactly that one violation to arrive: a count of
   zero from a listener that could not hear would prove nothing.
9. **The step runs or it is red.** `make web-smoke` without `CHORUS_SERVER_BIN` ends
   `web-smoke: SKIPPED` and exits 0, so a developer without a built server sees what did not
   run; under `CI=true` it fails instead. In the gate the step runs after `build`, against
   the server that step made, beside `ha-live`, and is green only with its own `web-smoke:
   PASS` line and no `SKIPPED` line (decision 0142's rule). The runner also holds Playwright's
   own account: the tests it lists are in exactly one file, and its summary passed all of
   them with none skipped, flaky or left out.
10. **The shell lists the server's rooms, read once.** Before this change the app showed
    nothing of a server, so there was no server text to assert. `web/src/rooms.js` reads
    `GET ../api/state` when the page opens and `chorus-app` lists each room's name. It is the
    least that makes the assertion real, and the live state layer (the snapshot followed by
    the event stream, the room card) replaces it.

## Not chosen

- **A browser in the unit tests.** Slower by an order of magnitude per test and it would put
  the browser's environment between every contributor and `make web-test`.
- **Asserting a screenshot or its size.** A PNG's bytes move with the font, the build of the
  browser and the host; the retired check's history is the argument. Text and a non-empty box
  say that something was drawn without saying which pixels.
- **The full Chromium instead of the headless shell.** A headless run does not start it, it
  is about twice the download, and it needs more libraries than the prefix holds.
- **Letting the test download the browser when it is missing.** A gate step that reaches the
  network on its own is a step whose result depends on the network that day.
- **`--with-deps` everywhere.** It needs root. Where there is none the two prefixes do the
  same job, and where there is, the package manager does.
- **Checking the login in the server.** `chorus-server` has no authentication by design
  (`docs/control-plane.md`); the login is the deployment's. The test holds that the app works
  behind one, not that the server has one.
- **A second test file for later browser needs.** The service worker and the install add to
  `web/smoke/app.spec.js`; the runner fails a run whose tests are in more than one file.

## Consequences

- The gate's full tier gains one step of about 5 s here (1 test, 1.7 s in Playwright, the
  rest the install check and the server's start). The fast tier does not build the server and
  does not run it.
- CI's job gains one step that installs the locked packages, downloads one browser build
  (about 114 MB) and installs its libraries with apt.
- A development host needs the build once (`make web-smoke-install`) and, with no root, the
  two prefixes.
- An upgrade of `@playwright/test` changes the browser: the commit says so, and the build has
  to be installed again.
- `ASSUMED`, unchanged from record 0181: Safari adopts constructable stylesheets under the
  server's policy as Chromium does. This test runs Chromium alone.

## Sources

- npm registry JSON for `@playwright/test`, `playwright` and `playwright-core`
  (`https://registry.npmjs.org/<package>` for the latest version and its publication time,
  `https://registry.npmjs.org/<package>/1.63.0` for the licence), read 2026-10-05.
- The removal of unused builds on install, the 14 missing libraries and the crash without a
  font are what was seen on the development host on 2026-10-05, not read from a document.
  Playwright's page on browsers (https://playwright.dev/docs/browsers) has sections named
  "Chromium: headless shell" and "Stale browser removal"; only its table of contents was read
  (2026-10-05), so the wording above rests on the runs.
- The run of `playwright install --dry-run chromium-headless-shell` and of `make web-smoke`
  on the development host, 2026-10-05.
