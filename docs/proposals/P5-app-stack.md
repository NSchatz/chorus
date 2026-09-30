# P5: The app stack for the chorus PWA

- Decisions: K43
- Status: PROPOSED (chorus goal 1, 2026-09-30); decided at Checkpoint K
- If deferred: The recommendation
- Builds on: goal 21 (the app, part 1: stack, core screens, install, kiosk), goal 22 (the app, part 2: sound, alarms, room correction, fleet screens); §4.2's "exactly one browser smoke test" in `make gate`

## Question

Which stack does the installable chorus PWA use? K43: "goal 1 compares no-build vanilla (ES
modules/web components), Svelte + Vite, and one other against the parity feature list, bundle size,
supply chain and test tooling; decided at Checkpoint K. BRIEF §5.8 'no framework' stands meanwhile.
Not chosen as final: Svelte + Vite; no-build vanilla." The planning research recommends Svelte 5 +
Vite (brief §5 table).

What the app has to carry (K16, K86, K87, goals 21 and 22): rooms and bonded sets; saved and live
groups with drag-to-group and Sonos-style group volume (K77: "the group slider scales every room
relatively"); now playing with artwork; inputs (K64: "its app controls rooms, groups, inputs and
sound, not content"); tone, loudness, night mode, limits, quiet hours, autoplay; alarms and sleep
timers; adoption, naming, Wi-Fi provisioning and firmware approval (K92, K93); room correction with
the phone's mic (K87: "browser mic access; defeat OS mic processing where possible"); phone,
desktop and a wall-tablet kiosk mode (K86: "always on, big touch targets, no browser chrome"). All
of it is a live view of server state arriving over the existing HTTP + SSE control API.

## Constraints that bind every option

- **Fitness only** (CLAUDE.md rule 8, K95): what happens to be installed here is no argument.
- **Licences** (K26, §4.7 item 5): every dependency on the allowlist MIT, Apache-2.0, BSD, ISC,
  Zlib, public domain; "anything else by ADR". §4.7 does not exempt build-only dependencies.
- **Supply chain** (§0.9, container rules): exact pins, a committed lockfile, install scripts off
  (`ignore-scripts=true`, goal 21 item 1).
- **Tests** (§4.2): unit tests without a browser; exactly one browser smoke test in the gate
  (install and service worker behind a fake login). Chromium here runs only with the conda-forge
  library env and a conda-forge font env (§0.11).
- **Embedding** (goal 21 item 1): the built output is embedded in `chorus-server` (a static musl
  binary, 1.6 MB, research-toolchain-env.md §5) "without a build-time Node dependency in the Rust
  build".
- **Access** (K40, K82, K85): behind Traefik `lan-only` plus the household login; chorus adds no
  auth; everyone equal. The manifest needs `crossorigin="use-credentials"` (MDN manifest page).
- **Secure context:** service workers, `getUserMedia` (K87) and the Screen Wake Lock API (useful
  for K86's "always on") are available only in secure contexts (MDN pages below). The PWA is
  therefore used through Traefik's HTTPS origin, not chorus-server's plain HTTP port. This binds
  every option equally.
- **The existing Content-Security-Policy** (`crates/server/src/control.rs`, `CONTENT_SECURITY_POLICY`):
  `default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:;
  base-uri 'none'; form-action 'none'; frame-ancestors 'none'`. A stack must work under it (no
  inline script or style) or the policy must be loosened, which this proposal does not want.
- **K18:** a check must be much smaller than what it protects; the retired `tools/ui` graders are
  not reintroduced.

## Re-verification of the planning research

The planning research (`research-pwa-conventions.md` §1) and its verification
(`verify-toolchain-conventions.md` claim 9) measured tiny one-component skeletons and recommended
Svelte 5 + Vite 8 + pnpm, with Lit 3 + esbuild as runner-up. I rebuilt the comparison with the
**same feature skeleton in all three stacks** (header with live/signed-out state from SSE, rooms
grouped by group, a room card with artwork, volume slider, input select and "leave group", a
Sonos-style group-volume bar, HTML drag-to-group, a `?kiosk` mode class, a manifest with
credentials, a hand-written service worker; a shared pure `groupvol.js` for K77's scaling), in
`/cache/tmp/chorus-g1/p5-scaffolds/`, node 22.23.3, pnpm 10.34.5, `ignore-scripts=true`, exact pins.

Registry versions re-read today (npm registry JSON, `https://registry.npmjs.org/<pkg>`, read
2026-09-30): svelte 5.57.1 (2026-09-18, MIT), vite 8.3.1 (2026-09-24, MIT),
@sveltejs/vite-plugin-svelte 7.3.1 (2026-09-23, MIT), vitest 5.0.2 (2026-09-25, MIT), happy-dom
20.14.5 (2026-09-12, MIT), @happy-dom/global-registrator 20.14.5 (MIT), @testing-library/svelte
5.4.2 (2026-06-23, MIT), lit 3.3.3 (2026-05-14, BSD-3-Clause), esbuild 0.28.2 (2026-08-08, MIT),
@playwright/test 1.63.0 (2026-09-04, Apache-2.0), vite-plugin-pwa 1.3.0 (2026-05-05, MIT),
lightningcss 1.33.0 (2026-07-20, MPL-2.0), pnpm 12.8.1 (2026-09-28, MIT). The planning versions
all still stand except pnpm (10.34.5 then; 12.8.1 is current).

What changed:

1. **Vite 8 brings an MPL-2.0 package.** Vite 8.3.1's dependencies are `lightningcss`, `picomatch`,
   `postcss`, `rolldown`, `tinyglobby` (registry). The Vite 8 announcement (2026-03-12) says
   "lightningcss is now a normal dependency" (it was an optional peer in Vite 7). lightningcss is
   MPL-2.0 (its LICENSE file and registry entry). `pnpm licenses list` in the Svelte scaffold: 67
   MIT, 4 Apache-2.0, 1 BSD-2, 1 BSD-3, 1 ISC, **2 MPL-2.0** (`lightningcss` and its linux-x64
   binary). The planning research listed only top-level licences and missed it. It is build-only
   (nothing of it reaches the bundle), but §4.7 item 5 makes it an ADR. vitest 5 takes Vite as a required peer
   (`^6.4.0 || ^7.0.0 || ^8.0.0`), which pnpm resolves to Vite 8, so any option that tests with
   vitest carries lightningcss unless it pins Vite 7 (last release 7.3.6, 2026-06-25; Vite 7 lists
   lightningcss only as an optional peer), and staying on Vite 7 is a maintenance cost of its own.
2. **Package counts reproduce.** Svelte 5 + Vite 8 + vitest + happy-dom + testing-library: 78
   installed on linux-x64, 103 in the lockfile (the verification's figures exactly).
3. **Lit needs no test framework.** Lit and plain custom elements need no compile step, so node's
   built-in runner (`node --test`) with `@happy-dom/global-registrator` runs their component tests.
   Neither the planning research nor its verification tried this.
4. **Bundle numbers with a like-for-like app** (below) replace the one-component figures (14.1 kB
   gzip for Svelte).
5. **CSP compatibility is now measured**, not assumed: all three builds render and work under
   chorus-server's exact policy in headless Chromium with zero violations (below).
6. Chromium: the smoke check needed both the library env and the font env, which confirms the
   verification's correction. Playwright's installed browser build (Chromium 141.0.7390.37 under
   `/cache/playwright`) belongs to Playwright 1.56; the current release is 1.63.0.

### Measurements (2026-09-30, this container, 2 CPUs; `host` source)

| | A: vanilla, no build | B: Svelte 5 + Vite 8 | C: Lit 3 + esbuild |
|---|---|---|---|
| Shipped JS + CSS, raw | 7,949 B (source as written, unminified; 8,783 B with `sw.js`, which the B and C figures exclude) | 41,518 B JS + 324 B CSS | 22,248 B |
| Same, gzip -9 | 2,949 B (the same files concatenated; 3,284 B with `sw.js`) | 15,998 B + 234 B (Vite prints 16.22 kB) | 8,564 B |
| Growth per extra room-card-sized component | not measured (hand-written) | +1.9 kB raw (10 copies: 60,513 B) | +1.9 kB raw (10 copies: 40,858 B) |
| App source for the same features | 6,199 B (incl. a 1.1 kB DOM helper) | 3,834 B | 4,722 B |
| Build | none | 580 ms (2.0 s wall) | 14 ms (0.4 s wall) |
| Packages, tests with `node --test` + happy-dom | **10 installed / 10 locked** (MIT, BSD-2) | n/a (components need the compiler: vitest) | **18 installed / 43 locked** (MIT, BSD-2, BSD-3; 25 of the 43 are esbuild binaries for other platforms) |
| Packages, tests with vitest + happy-dom (Vite 8 resolved as vitest's peer) | 44 / 69, incl. MPL-2.0 | **78 / 103, incl. MPL-2.0** | 52 / 102, incl. MPL-2.0 |
| Unit tests (component + group-volume) | 2 pass, 1.1 s wall (`node --test`) | 4 pass, 2.8 s (vitest) | 1 pass, 1.0 s (`node --test`); 4 pass, 1.6 s (vitest) |
| Headless Chromium under chorus's CSP | renders, SW active, POSTs the command, 0 CSP violations | same | same |
| Deterministic output | n/a | yes (verification: byte-identical hash in two dirs) | yes (two builds, identical sha256) |

The fixed runtime dominates the bundle: Svelte's is about 36 to 38 kB raw and Lit's about 16 kB raw
at this size; per component both grow alike. Extrapolated (not measured) to 20 to 30 components,
B lands near 80 to 100 kB raw and C near 55 to 75 kB raw. chorus-server sends no
`Content-Encoding` today, so raw bytes are what crosses the LAN once; hashed assets are then
cached. At these sizes bundle size decides nothing.

Release churn (npm registry `time`, read 2026-09-30): Vite majors 6.0.0 (2024-11-26), 7.0.0
(2025-06-24), 8.0.0 (2026-03-12, which replaced esbuild and Rollup with Rolldown); Svelte 5.0.0
2024-10-19 (runes); Lit 2.0.0 2021-09-21 and 3.0.0 2023-10-10; esbuild is pre-1.0 (0.28.0
2026-04-02) and treats minor versions as breaking. Versions published in the last 365 days: svelte
114, vite 78, esbuild 13, lit 2.

One observation from writing the vanilla skeleton: a live SSE update re-renders a room card while
someone drags its slider, so the hand-written render needs a focus special case (the vanilla
`room.js` returns early when the slider has focus). Lit and Svelte patch attributes in place and
need none. Every interactive control in a hand-written app carries this kind of care.

Adversarially verified 2026-09-30 (goal-1 verifier 3): 12 claims confirmed, 0 refuted, 2 partly right, 0 unverifiable; corrections applied; the recommendation stands.

## Options

### Option A: No-build vanilla (ES modules, custom elements in light DOM)

- What: `web/` holds hand-written modules served as written: custom elements, a small DOM builder
  and keyed-list helper (about 1 kB), `tokens.css` and a stylesheet. Tests: `node --test` +
  `@happy-dom/global-registrator` (10 packages). Embedded by a std-only `build.rs` with
  `include_bytes!` straight from the source tree; there is no build step at all.
- Costs: money none. Effort: goals 21 and 22 write the most code (the same features took 1.6 times
  the source of B), including their own keyed updates, focus handling and shared-state plumbing for
  20 or more views and multi-step flows (room correction, adoption, alarms). Maintenance: no
  toolchain churn; the hand-rolled render layer is chorus's to maintain forever. Gate time: tests
  about 1 s.
- Risks: the render layer grows into a private framework without a framework's tests; this is how
  `tools/ui` grew to 6,613 lines of grading for a 51 kB page (research-pwa-conventions.md §1.3).
  Bugs of the "control replaced mid-gesture" kind recur per control.
- Fit: best supply chain and simplest embedding; BRIEF §5.8 "no framework" holds unamended. Weakest
  fit to the parity scope K16 made app-grade.

### Option B: Svelte 5 + Vite 8 (the planning research's recommendation)

- What: `web/` with `.svelte` components, shared state in `*.svelte.js` runes modules fed by SSE,
  Vite 8 build, vitest 5 + happy-dom + @testing-library/svelte for tests; hand-written service
  worker (no vite-plugin-pwa, which adds about 311 packages: verification claim 9); committed build
  output embedded by a std-only `build.rs`, which the gate rebuilds and diffs.
- Costs: money none. Effort: the least code per feature (3,834 B for the skeleton); testing-library
  queries by accessible label, which also enforces labels. One ADR for MPL-2.0 lightningcss
  (build-only). Maintenance: 103 locked packages to review at each upgrade; Vite has shipped three
  majors between 2024-11 and 2026-03 and Svelte 114 releases in a year; agents must keep to Svelte 5 runes syntax
  rather than Svelte 4's (ASSUMED risk, from the size of the Svelte 4 corpus). Gate time: build
  about 2 s, tests about 3 s.
- Risks: the largest dependency tree of the three; a Vite major can change the bundler again (as 8
  did); the MPL item needs a recorded exception to §4.7.
- Fit: strong for the parity scope (reactive shared state, keyed `#each`, scoped CSS extracted into
  a file that passes `style-src 'self'`). Amends BRIEF §5.8.

### Option C: Lit 3 + esbuild

- What: `web/` with Lit elements (templates in plain JS, no compile step), state held by one app
  element that receives the server snapshot and passes it down as properties (the server-authoritative
  model `docs/control-page.md` describes: "The page holds no state of its own"); esbuild bundles
  and minifies (one binary); tests with `node --test` + `@happy-dom/global-registrator`; the same
  hand-written service worker and the same committed-output embedding as B.
- Costs: money none. Effort: about 23% more source than B for the skeleton (4,722 B vs 3,834 B),
  far less than A; tests query inside shadow roots (`el.shadowRoot.querySelector`), so goal 21
  writes a small label-query helper instead of using testing-library. Maintenance: 18 installed /
  43 locked packages, all on the allowlist, no ADR; Lit has had one major (3.0.0, 2023-10-10) since 2.0.0 (2021-09-21); esbuild is
  pre-1.0, so its pin moves only in its own commit (as §0.9 already requires). Gate time: build
  under 1 s, tests about 1 s.
- Risks: shadow DOM means global styles reach components only through CSS custom properties (the
  existing `tokens.css` is already custom properties, so the tokens carry over); `static styles`
  use constructable stylesheets, which passed the CSP check in Chromium 141 (Safari behaviour not
  checked here: `ASSUMED` equal, a goal-21 phone check). Cross-view state beyond the snapshot (a
  multi-step flow's progress) is plain JS objects owned by the flow's element; Lit's signals
  package is still `@lit-labs` (0.3.0) and is not proposed.
- Fit: declarative templates and in-place patching like B, half of B's runtime, the smallest
  supply chain of the two framework options, and no test framework. Amends BRIEF §5.8 by one
  library (the Lit runtime, BSD-3-Clause, about 16 kB raw).

Preact + Vite was measured by the planning research (86 packages with Babel through the preset)
and, on Vite 8, would carry the same MPL item as B; it is not carried forward. SolidJS needs a
compile step like Svelte and was not measured.

### Common to every option (goal 21 builds these whichever stack wins)

- Hand-written service worker: `/api/*` and the SSE stream network-only; navigations network-first
  with the cached shell as fallback; never cache a redirect, a non-200 or a cross-origin response
  (the login page); `sw.js` and `index.html` `no-cache`, hashed assets immutable. A fetch handler is
  kept because Chromium still ties its install prompt to one (Chrome install-criteria post: the
  requirement was removed "for installation from the menu, since version 108 on mobile and 112 on
  Desktop").
- Signed-out state: API calls use `redirect: "manual"`; an `opaqueredirect` shows "Signed out,
  tap to sign in" (in all three skeletons).
- Drag-to-group on phones and tablets: HTML drag and drop was used in the skeletons; touch
  support for it on iOS is `ASSUMED` and goal 21 should implement the gesture with Pointer Events,
  which is stack-independent.
- Room correction (K87): `getUserMedia` with `echoCancellation`, `noiseSuppression` and
  `autoGainControl` set false (MDN `MediaTrackConstraints`), capture through an AudioWorklet
  module served same-origin (allowed by `script-src 'self'`, `ASSUMED`, checked in goal 22), per-phone
  limits documented. Stack-independent.
- Kiosk (K86): Screen Wake Lock, a kiosk layout class, big targets; stack-independent.
- The one browser smoke test: Playwright headless Chromium with `/cache/opt/chromium-libs` on
  `LD_LIBRARY_PATH` and the conda-forge font env (`FONTCONFIG_FILE`), against chorus-server behind
  a tiny fake forward-auth, asserting rendered text (never a PNG size). Pin `@playwright/test`
  to the current release at goal 21 (1.63.0 at read time) and install its Chromium build rootless
  with the §0.11 library and font envs. My check here (Python `playwright==1.56.0`, three pages) ran
  in about 10 s including launch.
- Embedding: `crates/server/build.rs` (std only) walks the committed web output and emits an
  `include_bytes!` table with MIME types and ETags; `build.rs` never runs node, so `cargo build`,
  the image and CI stay node-free; the gate rebuilds the web output and fails on `git diff`
  (outputs measured deterministic for B and C).

## Comparison

| Criterion | A: vanilla | B: Svelte 5 + Vite 8 | C: Lit 3 + esbuild |
|---|---|---|---|
| Fit to the parity list | weakest: hand-written render, focus and state plumbing per control | strongest authoring (least code, runes) | strong (templates, keyed `repeat`, in-place patching); 23% more source than B |
| Shipped size (skeleton, gzip) | 2.9 kB | 16.2 kB | 8.6 kB |
| Supply chain (installed / locked) | 10 / 10 | 78 / 103 | 18 / 43 |
| Licences | allowlist | allowlist + MPL-2.0 (ADR) | allowlist |
| Tests without a browser | `node --test` + happy-dom | vitest + happy-dom + testing-library | `node --test` + happy-dom |
| The one browser smoke test | same Playwright recipe | same | same |
| Under chorus's CSP (measured) | clean | clean | clean |
| Service worker, install | hand-written, same | same | same |
| Embedding in chorus-server | source directly, no build | committed build output + diff | committed build output + diff |
| Churn for one owner | none but own code | highest (3 Vite majors 2024-11 to 2026-03; 114 Svelte releases a year) | low (Lit 3 since 2023-10; esbuild pinned) |
| BRIEF §5.8 "no framework" | holds | amended | amended (one small runtime library) |

## Recommendation

**Recommendation:** Option C, Lit 3 + esbuild with `node --test` + happy-dom: nearly Svelte's authoring fit at under half its supply chain (43 vs 103 locked packages), no MPL-2.0 exception, no test framework, half the runtime, and the slowest churn.

The planning research chose Svelte for authoring speed, and B is still the most pleasant to write.
The measured gap to C is small (about 23% more source for the same features). The gaps on the
criteria K43 names are large and all favour C: 43 locked packages instead of 103; nothing
off the allowlist (B needs an ADR for MPL-2.0 lightningcss because Vite 8 made it a hard
dependency); no test framework at all instead of vitest + Vite; and a toolchain of one pinned binary
instead of a bundler that changed engines in its last major. A is rejected for the app-grade scope:
its supply chain is the best, but every control needs render care the frameworks give for free, and
that code is chorus's to maintain. What the owner gives up with C: Svelte's terser components,
testing-library's label queries (goal 21 writes a small helper), and light-DOM styling (tokens pass
through as CSS custom properties, which `tokens.css` already is). Cost: goals 21 and 22 as planned;
one BRIEF §5.8 amendment recorded as an owner decision; pins for lit, esbuild, happy-dom,
global-registrator, @playwright/test and pnpm; gate time about 2 s plus the smoke test's about 10 s.
If the owner weighs authoring speed above supply chain, B is the runner-up, with the MPL ADR and the
103-package lockfile as its price.

## If the owner defers

The If-deferred cell is "The recommendation", so goal 21 builds Option C: `web/` with Lit 3 and
esbuild, exact pins, `ignore-scripts`, `node --test` + `@happy-dom/global-registrator` unit tests,
the hand-written service worker, committed output embedded by a std-only `build.rs` with a
rebuild-and-diff gate step, and one Playwright smoke test. BRIEF §5.8's "no framework" sentence is
then superseded by this proposal as the owner's deferred acceptance, which goal 21 records in
`docs/decisions/` without editing the brief silently (CLAUDE.md). Switching later to B is a rewrite
of the components (templates and state), not of the service worker, embedding, API client, group
volume logic or smoke test, which are stack-independent.

The brief's §5 table puts "the planning research recommends Svelte 5 + Vite" beside this cell;
"The recommendation" here means this proposal's own (Option C), per §0.2's "where the approval is
silent, on the recommendation", and the Checkpoint K packet says so in one line so a deferral is not
read as Svelte.

## Open inputs

- `ASSUMED`: Safari and iOS behaviour for constructable stylesheets under the CSP, HTML drag and
  drop on touch, `getUserMedia` inside an installed standalone PWA, and whether the login redirect
  completes in iOS standalone mode. Checked by goal 21's Needs item "a phone check that install and
  login work on the owner's phones" (brief §25 item 5); the mic path by goal 22.
- `ASSUMED`: that Traefik serves the app over HTTPS with a certificate the phones trust (secure
  context is required for the service worker, the mic and Wake Lock). The deploy PR (goal 4) should
  confirm it.
- `ASSUMED`: AudioWorklet module loading under `script-src 'self'` (goal 22 test).
- `ASSUMED`: agent familiarity with Svelte 5 runes versus Svelte 4 syntax (affects B only).
- Owner input: whether authoring speed (B) outweighs supply chain and churn (C); nothing else.

## Sources

- npm registry JSON for svelte, vite, vite/8.3.1, vite 7.x (7.1.0 peers; 7.3.6 in `time`), @sveltejs/vite-plugin-svelte, vitest, vitest/5.0.2, happy-dom, @happy-dom/global-registrator, jsdom, @testing-library/svelte, lit, preact, @preact/preset-vite, esbuild, @playwright/test, playwright, pnpm, vite-plugin-pwa, solid-js, vite-plugin-solid, @lit-labs/signals, @preact/signals, lightningcss, rolldown: `https://registry.npmjs.org/<pkg>`, read 2026-09-30
- Vite 8 announcement (2026-03-12; "lightningcss is now a normal dependency"; Rolldown; Node 20.19+/22.12+), https://vite.dev/blog/announcing-vite8, read 2026-09-30
- lightningcss LICENSE (Mozilla Public License 2.0), https://raw.githubusercontent.com/parcel-bundler/lightningcss/master/LICENSE, read 2026-09-30
- Svelte testing guide (Vitest with jsdom), https://svelte.dev/docs/svelte/testing, read 2026-09-30
- Lit production build docs (bare module specifiers resolved by a bundler), https://lit.dev/docs/tools/production/, read 2026-09-30
- MDN, web app manifest (`crossorigin="use-credentials"`), https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest, read 2026-09-30
- MDN, Service Worker API (secure contexts only), https://developer.mozilla.org/en-US/docs/Web/API/Service_Worker_API, read 2026-09-30
- MDN, `MediaDevices.getUserMedia()` (secure context), https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia, read 2026-09-30
- MDN, `MediaTrackConstraints.echoCancellation`, https://developer.mozilla.org/en-US/docs/Web/API/MediaTrackConstraints/echoCancellation, read 2026-09-30
- MDN, Screen Wake Lock API (secure context), https://developer.mozilla.org/en-US/docs/Web/API/Screen_Wake_Lock_API, read 2026-09-30
- Chrome, "Changes to installability criteria" (fetch handler no longer needed to install from the menu since 108 mobile / 112 desktop), https://developer.chrome.com/blog/update-install-criteria, read 2026-09-30
- Measurements: `/cache/tmp/chorus-g1/p5-scaffolds/` (`svelte`, `lit`, `vanilla`, `lit-nodetest`, `vanilla-nodetest`, `svelte-x10`, `lit-x10`, `smoke/smoke.out`), run 2026-09-30

## What was read

- `/cache/tmp/chorus-g1/agent-rules.md`, `/cache/tmp/chorus-g1/proposal-format.md`, `/cache/tmp/chorus-g1/prompt-PD.md`, `/cache/tmp/chorus-g1/verify/verify-3.md` (P5 section)
- `/workspace/.claude/goals/2026-09-chorus.md`: header, §0.8, §0.9, §0.11, §1 (K10, K16, K18, K19, K26, K40, K43, K46, K51, K54, K56, K59-K64, K77, K81-K87, K92, K93, K95, I1-I20), §3.3, §4, §5, §19, §22, §23, §25, §26
- `/workspace/.claude/goals/2026-09-chorus-research/research-pwa-conventions.md` (§1, C10, the recommendation list), `verify-toolchain-conventions.md` (claims 1-10 and corrections), `research-toolchain-env.md` (§2 Chromium/Playwright, §5 OCI), `research-ha-integration.md`, `verify-ha-casting.md`
- `/cache/wt/chorus/chorus/baseline/BRIEF.md` §5.8; `crates/server/src/control.rs` (CSP, responders, `include_str!` embedding); `crates/server/src/ui/` (sizes, `index.html`); `docs/control-page.md` (first 80 lines)
- The planning scaffolds in `/cache/tmp/plan-2026-09-chorus/scratch/pwa/` (`app.sh`, `verify-s8/package.json`)
- The URLs listed under Sources
- No GPL source was opened. Every package opened or installed is MIT, Apache-2.0, BSD-2/3, ISC or (lightningcss, installed as a Vite dependency, not read) MPL-2.0.
