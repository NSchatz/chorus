# 0000: the app under web/ is Lit 3 bundled by esbuild, installed by pnpm with install scripts off and tested with node's own runner over happy-dom; its build output is committed and the gate rebuilds and compares it

- Status: accepted, 2026-10-05. Records the owner's approval of proposal P5, Option C, and
  fixes what the proposal left to the first change that builds it: the pinned versions, where
  the output lives and the checks that hold both.
- Decided by: the owner for the stack (proposal P5, `docs/proposals/P5-app-stack.md`, approved
  at Checkpoint K: "P5 approved (Option C: Lit 3 + esbuild, node --test + happy-dom): the
  recommendation is also its "If deferred" line, keeps the licence allowlist without an MPL-2.0
  exception, and K43 declined neither option as final",
  https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/CHECKPOINT-K.approved).
  BRIEF.md section 5.8 already carries it (R10, K43). The versions, the layout of `web/`, the
  committed output and the checks are this record's.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/`; `mise.toml` and `mise.lock` (node, pnpm); `tools/web.sh`; `Makefile`
  (`web-test`, `web-build`); `tools/gate.sh` (`web_step`, `web_build`, steps `web-test` and
  `web-build`); `.github/workflows/ci.yml` (step `Node and pnpm`);
  `tools/conventions/check-web.sh`; `docs/conventions.md` (rules 13 and 14).

## Context

The installable app (K16, K43, K86, K87) needs a stack before it has screens. P5 compared
no-build vanilla, Svelte 5 with Vite 8, and Lit 3 with esbuild on one feature skeleton, and
recommended the third: 43 locked packages against 103, nothing off the licence allowlist (Vite
8 makes MPL-2.0 lightningcss a hard dependency), no test framework, and the slowest release
churn. The owner approved it. What remained open was everything a first change has to pin
down, and three constraints bind it:

- `chorus-server` is a static binary built with no node, and it must stay so: the app's files
  reach it as bytes that are already in the tree.
- The server's Content-Security-Policy is `default-src 'none'; script-src 'self'; style-src
  'self'; ...`: no inline script and no inline style.
- Everything that builds or checks chorus is pinned to an exact version and a digest, install
  scripts are off, and every dependency's licence is on the allowlist, build-only ones included
  (brief sections 0.9 and 4.7).

## Decision

1. **The stack is P5's Option C, and nothing beyond it.** Four direct dependencies, each an
   exact version: `lit` (the one runtime library), `esbuild` (the bundler), `happy-dom` and
   `@happy-dom/global-registrator` (the DOM the unit tests run on). No Vite, no vitest, no
   testing library, no CSS tool. `check-web.sh` fails on any other direct dependency, so a fifth
   package arrives with a record of its own.
2. **The pinned versions.** Read 2026-10-05, each still the newest release of its package
   except pnpm (item 3):

   | What | Version | Published | Licence | Read from |
   |---|---|---|---|---|
   | lit | 3.3.3 | 2026-05-14 | BSD-3-Clause | https://registry.npmjs.org/lit |
   | esbuild | 0.28.2 | 2026-08-08 | MIT | https://registry.npmjs.org/esbuild |
   | happy-dom | 20.14.5 | 2026-09-12 | MIT | https://registry.npmjs.org/happy-dom |
   | @happy-dom/global-registrator | 20.14.5 | 2026-09-12 | MIT | https://registry.npmjs.org/@happy-dom/global-registrator |
   | pnpm | 12.8.1 | 2026-09-28 | MIT | https://registry.npmjs.org/pnpm |
   | node | 24.21.0 | 2026-09-07 | MIT | https://nodejs.org/dist/index.json |

   They resolve to 43 locked packages, 18 of them installed on linux-x64 (25 are esbuild's
   binaries for other platforms), the figures P5 measured. `web/licences.txt` records the
   registry's licence for each of the 43 at its version: 37 MIT, 5 BSD-3-Clause, 1
   BSD-2-Clause.
3. **node and pnpm are gate tools, pinned in `mise.toml` with a sha256 each in `mise.lock`.**
   node 24.21.0 is the current release of the active long-term-support line (24, "Krypton").
   pnpm is 12.8.1, the version P5 read on 2026-09-30, rather than 12.9.1, which the registry
   published on 2026-10-03, two days before this record: a package manager two days old has had
   no time to be found wrong, and nothing here needs what it changed. `tools/web.sh` reads the
   two pins from `mise.toml` and refuses to run on any other version, so `web/package.json`
   carries no `packageManager` field: with one, pnpm 12 locks itself and its 14 platform
   binaries into the lockfile and fetches another version of itself when the field and the
   binary disagree.
4. **Install scripts are off in three places, and none of them is an `.npmrc`.**
   `web/pnpm-workspace.yaml` sets `ignoreScripts: true`; it denies the one build script in the
   tree by name (`allowBuilds: esbuild: false`; pnpm 12 stops an install on a build script that
   is neither allowed nor denied); and `tools/web.sh` installs with `--frozen-lockfile
   --ignore-scripts`. esbuild's script only verifies its platform binary, which is the optional
   package `@esbuild/<platform>` and runs without it. The task and P5 both name an `.npmrc`
   with `ignore-scripts=true`. Measured here with the pinned pnpm 12.8.1: `pnpm config get
   ignore-scripts` answers `undefined` with that file present and `true` with the workspace
   setting, so the line in an `.npmrc` would switch nothing off while reading as if it did.
   `check-web.sh` therefore fails on a `web/.npmrc`. (The development container's commit hook
   also refuses a file of that name as a possible credential file; that is not the reason.)
5. **Layout.** `web/src` holds the page, the stylesheet, the tokens and the elements;
   `web/test` the tests and their two helpers; `web/build.mjs` the build; `web/dist` its
   output. The shell is one element, `chorus-app`, which paints a header and a labelled main
   region that later screens are rendered into. The design tokens are the control page's
   (`crates/server/src/ui/tokens.css`), copied whole: they are custom properties on `:root`, so
   they inherit into every shadow root, and an element's `static styles` name tokens and never
   a literal. Lit adopts those styles as constructable stylesheets and the tokens ship as a
   linked file, so nothing is inline and the server's policy needs no change.
6. **The build is deterministic and its output is committed.** `make web-build` empties
   `web/dist` and writes `index.html` and two assets named by the hash of their content
   (`assets/main-<hash>.js`, `assets/app-<hash>.css`), minified, with relative links so the
   page works wherever the server mounts it. Nothing in the build reads a clock, the
   environment or the directory's path; two builds here, one of them in another directory, gave
   the same sha256 for all three files. Lit's licence notices stay at the end of the bundle
   (esbuild's legal comments): BSD-3-Clause asks that redistributions keep them, and the bundle
   is redistributed inside the server.
7. **The gate rebuilds the output and fails on a difference.** Step `web-build` runs `make
   web-build` and then requires `git status --porcelain -- web/dist` to be empty: a changed
   file, a new one and a removed one all fail it. Step `web-test` runs the unit tests. Both
   are in the fast tier and the full one, after the cheap steps and before the builds, and each
   is green only when its target printed its own `PASS` line (decision 0142's rule for a step
   that needs a tool: it runs or it is red).
8. **Unit tests run in node, with no browser.** `node --import ./test/setup.js --test`: the
   preload registers happy-dom's globals before Lit is imported, so elements define, render
   into their shadow roots and update. Tests find elements by accessible label through
   `web/test/label-query.js`, which looks through shadow roots; that is the small helper P5
   said replaces a testing library's label queries. `tools/web.sh` fails a run whose summary
   counts no test or fewer passes than tests, so a skipped test is not a green step.
9. **Licences are held twice.** `check-web.sh` needs no install: every locked package has a
   line in `web/licences.txt`, no line is left over, and every licence is on rule 13's
   allowlist. `tools/web.sh` then holds the `license` field of each package an install really
   put in `node_modules` to its line, on every gate run. The 25 binaries of other platforms are
   never installed here, so their lines rest on the registry reading alone.

## Not chosen

- **Building in `chorus-server`'s `build.rs`, or in the image build.** It would make node and
  the registry a dependency of `cargo build`, of the image and of every contributor who only
  touches Rust. Committed output keeps the Rust build node-free; the price is a generated
  directory in the tree (about 21 kB today) and one more thing to commit with a change to
  `web/src`, which the gate step catches when forgotten.
- **An ignored `web/dist` with CI publishing it as an artifact.** The server could not embed
  what is not in the tree at build time, and a release would depend on a CI run's artifact.
- **Source maps.** They would be committed output too, larger than the bundle, for a page that
  is served to a household's own devices; the source is in the same repository.
- **`packageManager` and `engines` in `package.json`.** See item 3: `mise.toml` is the one
  place a tool version is written, and `tools/web.sh` enforces it.
- **npm instead of pnpm.** npm ships with node and would save a pin, but it has no per-package
  build denial and installs a flat `node_modules` in which a package can import what it never
  declared. P5 settled pnpm with the stack.
- **A licence scan at install time alone** (`pnpm licenses list`). It needs an install and sees
  only the installed platform's packages; the committed list covers every locked package and
  is checked without node.
- **Sharing one `tokens.css` between the control page and the app.** The control page is not
  changed by this work and is served from another directory; a build that read its file would
  turn an edit of the control page into a failing `web-build` step. The copy says where it came
  from and when; the app replaces the control page later, and the older file goes with it.

## Consequences

- A change to `web/src` is committed together with `web/dist` (`make web-build`). A change that
  forgets is red at step `web-build`, which prints the files that differ.
- An upgrade of any of the six pins is its own commit saying why (rule 14). An esbuild upgrade
  changes the bundle's bytes and so `web/dist`; a lockfile change needs the new packages' lines
  in `web/licences.txt`.
- The gate's runner needs the registry once per cache, to fill pnpm's store. CI's `Pinned
  tools` step installs node and pnpm from `mise.toml` against `mise.lock`'s digests, and the
  `Node and pnpm` step fails the job when the runner's own node is the one on `PATH`.
- Not built here, each a later change: serving and embedding `web/dist` in `chorus-server`; the
  one browser smoke test; the screens, the API client, the service worker and the manifest.
- `ASSUMED`, carried from P5 and still unchecked: Safari adopts constructable stylesheets under
  the server's policy as Chromium does. The phone check of the app's first installable version
  settles it.

## Sources

- npm registry JSON, `https://registry.npmjs.org/<package>` for lit, esbuild, happy-dom,
  @happy-dom/global-registrator and pnpm (latest version, its publication time, its licence),
  and `https://registry.npmjs.org/<name>/<version>` for each of the 43 locked packages, read
  2026-10-05.
- Node.js release index (v24.21.0, 2026-09-07, lts "Krypton"),
  https://nodejs.org/dist/index.json, read 2026-10-05.
- `docs/proposals/P5-app-stack.md` (the comparison, its measurements and its sources, read
  2026-09-30 by its author).
- Measured here, 2026-10-05, source `host`: `pnpm config get ignore-scripts` under pnpm 12.8.1
  with and without `web/.npmrc` and with `ignoreScripts: true` in `web/pnpm-workspace.yaml`
  (item 4); `make web-test` (10 tests pass, about 1.4 s
  wall-clock warm), `make web-build` (about 0.6 s warm; `index.html` 408 B, the script 16,628
  B, the stylesheet 3,883 B), and the two-directory build comparison of item 6.

## What was read

- `docs/proposals/P5-app-stack.md` (Option C, "Common to every option", "If the owner
  defers") and the approval line of Checkpoint K quoted above.
- The retired program's brief, section 25 (the app, part 1), and its section 1 of
  `research-pwa-conventions.md`, at the commit CLAUDE.md pins.
- `docs/conventions.md` (rules 13, 14, 15 and 25), `tools/gate.sh` (`step`, `ha_step`),
  `tools/ha-test.sh`, `tools/conventions/check-pins.sh`, `check-wakeword.sh` (a check that
  tests itself on damaged copies), `check-conventions.sh`, `check-adrs.sh` and `lib.sh`,
  `.github/workflows/ci.yml`, `mise.toml`, `mise.lock`, `.yamllint`.
- `docs/decisions/0142-the-home-assistant-gate-steps-run-or-fail.md` (a step runs or is red).
- `crates/server/src/ui/tokens.css`, `chorus.css` and `index.html` (the tokens and how the
  control page uses them); `crates/server/src/control.rs` was not opened, its policy is quoted
  from P5.
- The registry and release pages under Sources; pnpm's own output for an ignored build script
  (`ERR_PNPM_IGNORED_BUILDS`) and its lockfile when `packageManager` is set, both seen while
  installing here.
- Installed and run, not read: lit, esbuild, happy-dom and their locked dependencies (MIT,
  BSD-2-Clause, BSD-3-Clause). No GPL source and no reciprocally licensed design file was
  opened.
