# web

The chorus app: Lit 3 elements bundled by esbuild (proposal P5, Option C; the decision record
`docs/decisions/0181-the-web-app-stack.md` says what is pinned and why). Today it is the shell
that later screens are added to: one element, `chorus-app`, with the design tokens.

| Path | What |
|---|---|
| `src/` | `index.html`, `app.css` and `tokens.css` (the document and the tokens), `main.js` (the entry point), the elements and their pure logic |
| `test/` | the unit tests (`*.test.js`), `setup.js` (happy-dom's globals, loaded before Lit) and `label-query.js` (find an element by its label, through shadow roots) |
| `smoke/` | the one browser test (`app.spec.js`) and the fake login it signs in through (`fake-login.js`); `playwright.config.js` configures it |
| `build.mjs` | the build: `src/` into `dist/`, deterministic |
| `dist/` | the build's output, committed: `chorus-server` embeds it and never runs node |
| `licences.txt`, `licences.mjs` | the licence of every locked package, and the check of the installed ones against it |

From the repository root, with the pinned node and pnpm of `mise.toml` (`mise install`):

```sh
make web-test     # node --test over happy-dom: no browser
make web-build    # rebuild dist/; commit it with the change to src/
CHORUS_SERVER_BIN=<a built chorus-server> make web-smoke   # the one browser test
```

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
