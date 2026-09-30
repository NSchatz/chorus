# Research: PWA stack (K43), chorus conventions (K18), shopkit-acoustics (K44), devices builds (K23), chorusctl (K46)

Written 2026-09-29 for the chorus /goal program plan. Nothing in any repo was changed. Scratch work lives in
`/cache/tmp/plan-2026-09-chorus/scratch/pwa/` (npm scaffolds) and `scratch/devices/` (read-only clone).
Every external fact carries its URL and "read 2026-09-29"; anything from memory is marked **ASSUMED**.
Note: this session's WebSearch budget was exhausted before this task began, so external facts come from
WebFetch of known URLs and from registry APIs (npm, PyPI); gaps are marked ASSUMED rather than guessed.

---

## 1. K43: PWA stack for the app-grade controller

### 1.1 What the app has to carry

K16/K30/K31 make the page an app: rooms, groups, volume, now playing, browse/queue (MA or chorus), alarms and
sleep timer, bonded sets and channel maps, tone/loudness EQ, a room-correction flow (mic capture, progress,
result), onboarding (discover, name, assign, firmware check). That is 10+ views with shared live state from SSE
and several multi-step flows. Today's page is vanilla JS/CSS/HTML, 51,433 bytes of source
(`crates/server/src/ui/`: chorus.js 30,906, tokens.css 11,002, chorus.css 8,422, index.html 1,103), embedded
with `include_str!` at `crates/server/src/control.rs:213-217`. BRIEF §5.8 says "A web UI can stay minimal
(single page, no framework) for a long time"; K16 changed the scope that sentence assumed.

### 1.2 Measured in scratch (pnpm 10.34.5, node 22.23.3, `ignore-scripts=true`, cold store)

Package count = entries in `node_modules/.pnpm` (every transitive package, incl. platform binaries).

| Variant (devDependencies) | Packages | Install |
|---|---|---|
| `svelte@5 vite@8 @sveltejs/vite-plugin-svelte@7` | **39** | 4 s |
| + `vitest@5 happy-dom @testing-library/svelte` | **78** | 11 s |
| `svelte@5 vite@7 @sveltejs/vite-plugin-svelte@6` | 37 | 9 s |
| + `vitest@3 happy-dom @testing-library/svelte` | 97 | 11 s |
| `svelte@5 vite@7 plugin@6 vite-plugin-pwa` (Workbox) | **348** | 18 s |
| `preact vite@7 @preact/preset-vite` (pulls Babel) | 86 | 10 s |
| `lit` alone (no bundler) | 6 | 2 s |
| `vitest@3 happy-dom` alone (what (a) needs to unit-test DOM code) | 60 | 5 s |
| `esbuild` alone | 2 | 2 s |

A minimal Svelte 5 controller skeleton (App + Zone components, a `$state` store fed by `EventSource`, a volume
slider POSTing to the control API) built to **35.7 KB raw / 13.9 KB gzip** JS (Vite 8; 35.1 / 13.7 KB on
Vite 7). Build: **0.58 s** as Vite 8 reports it (2.6 s wall incl. `mise exec` start); Vite 7 took 3.1 s. A
component test (`@testing-library/svelte` + happy-dom: render, read the slider by its label, fire input,
assert the callback got 0.55) **passed with no browser** in 3.4 s (vitest 3). The esbuild postinstall did not
need to run; `ignore-scripts=true` worked for every variant.

Versions and licences (npm registry `https://registry.npmjs.org/<pkg>/latest`, read 2026-09-29): svelte
5.57.1 MIT; vite 8.3.1 MIT; @sveltejs/vite-plugin-svelte 7.3.1 MIT; vitest 5.0.2 MIT; happy-dom 20.14.5 MIT;
jsdom 30.1.1 MIT; lit 3.3.3 BSD-3-Clause; preact 10.29.8 MIT; vite-plugin-pwa 1.3.0 MIT; workbox-build 7.4.1
MIT; esbuild 0.28.2 MIT. All compatible with K26 (MIT OR Apache-2.0) as dev-only build tools; none ships in
the binary except the Svelte runtime compiled into the bundle (MIT).

Svelte's own testing guide recommends Vitest + jsdom (or happy-dom) + `@testing-library/svelte`, with
`resolve.conditions: ['browser']` under `process.env.VITEST` (https://svelte.dev/docs/svelte/testing, read
2026-09-29). Confirmed working above.

### 1.3 The three options against the criteria

| Criterion | (a) no-build vanilla ES modules + custom elements | (b) Svelte 5 + Vite 8 + pnpm | (c) Lit 3 (+ Vite or esbuild) |
|---|---|---|---|
| Bundle | source is the bundle (51 KB today, grows linearly with hand-written DOM updates) | compiled, ~14 KB gzip skeleton; grows with components, no VDOM runtime | Lit runtime + components; bare specifiers need a bundler or an import map (**ASSUMED** from Lit's docs pointing to Rollup for specifier resolution, https://lit.dev/docs/tools/production/, read 2026-09-29) |
| Supply chain | 0 to ship; 60 to unit-test DOM code (vitest + happy-dom) | 39 to build, 78 with tests | 6 runtime; ~8 with esbuild; +60 for tests |
| Tests without a browser | vitest + happy-dom on modules | vitest + happy-dom + testing-library (verified) | vitest + happy-dom; shadow DOM makes queries more awkward (**ASSUMED**) |
| Reactivity for 10+ views, SSE state | hand-rolled render/diff; the risk that grew `tools/ui` to 6,613 lines of grading | runes (`$state`, `$derived`) and keyed `#each`; the compiler does the DOM work | reactive properties per element; state sharing is hand-rolled or via a context lib |
| Long-term | no toolchain churn; most code per feature | one compiler + one bundler, both MIT, very active; majors every 1-2 years (Vite 7 to 8 happened in this window) | web-standards-first, small API, Google-backed (**ASSUMED**) |
| Hiring/agent familiarity | universal | high (**ASSUMED**) | medium (**ASSUMED**) |

Preact + Vite was measured (86 packages, Babel via the preset) and is heavier than (b) on supply chain for no
gain here; Lit is the better "serious other".

### 1.4 PWA behind Traefik `lan-only` + tinyauth (K40)

homelab puts every non-public router behind `lan-only` + `tinyauth` forward auth with Pocket ID passkeys
(`survey-homelab.md` §Traefik, from `networking/traefik` and `networking/identity/tinyauth.yml`). Tinyauth is
AGPL-3.0 (https://github.com/steveiliop56/tinyauth, read 2026-09-29): a separate proxy, so no licence effect on
chorus; read its docs only (K33). Its docs site (https://tinyauth.app) did not expose cookie/expiry details to
the fetcher; session lifetime and the unauthenticated response (302 to the login host) are **ASSUMED** and are
a goal-1 reading.

Rules that follow:
1. **Manifest needs credentials**: "If the manifest requires credentials to fetch, the `crossorigin` attribute
   must be set to `use-credentials`, even if the manifest file is in the same origin as the current page."
   (https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest, read 2026-09-29). So
   `<link rel="manifest" href="/manifest.webmanifest" crossorigin="use-credentials">`.
2. **A service worker is not needed to install** in Chromium since 108 mobile / 112 desktop; the install
   *prompt* still wants a fetch handler (https://developer.chrome.com/blog/update-install-criteria, 2023-12-05,
   read 2026-09-29). iOS/iPadOS installs by Share > Add to Home Screen only, supports `standalone` only, and
   "each installation will have its own isolated storage" (https://web.dev/learn/pwa/installation, read
   2026-09-29). So the installed iOS app logs in separately from Safari; whether the tinyauth/Pocket ID
   redirect completes inside iOS standalone mode (cross-origin navigation out of the app's scope) is
   **ASSUMED to work but unverified**: a the Needs list phone check at the first PWA goal.
3. **Service worker design (hand-written, ~100 lines, no Workbox: 348 packages is the wrong trade)**:
   - `/api/*` and the SSE stream: network only, never cached (live state must never be stale).
   - navigations: network first, cached shell as the offline fallback; never cache a redirect, a non-200, or
     any response whose `url` left the origin (that is the login page).
   - API calls use `redirect: "manual"`; an `opaqueredirect` (session expired) shows "Signed out, tap to sign
     in", which does a top-level `location.reload()` so tinyauth can run. `EventSource` errors in a loop are
     treated the same way after one failed `fetch` probe.
   - `sw.js` served with `Cache-Control: no-cache`, scope `/`, versioned by the build hash; the old cache is
     deleted on `activate`. Whether the SW script fetch carries the same-origin session cookie is **ASSUMED**
     (worker script fetches default to same-origin credentials); the PWA smoke test below verifies it.
4. **What needs a real browser**: service worker registration, offline shell, manifest parse with credentials,
   installability. happy-dom and jsdom do not run service workers (**ASSUMED**, consistent with their scope).
   So: keep SW routing as a pure function (`classify(request) -> "network-only" | "network-first" | ...`)
   unit-tested in vitest, plus **one** Playwright smoke spec in the gate against the real server behind a tiny
   fake forward-auth (a Rust test binary that 302s without a cookie). K19 already allows a rootless browser.

### 1.5 Embedding the built output in a zero-crate Rust binary

- `crates/server/build.rs` (std only) walks the committed build output directory, emits
  `cargo:rerun-if-changed=<dir>`, and writes `$OUT_DIR/ui_assets.rs`: a
  `&[(&str /*path*/, &[u8] /*include_bytes!*/, &str /*mime*/, u64 /*etag*/)]` table. The ETag is a
  hand-written FNV-1a 64 over the bytes (std has no hasher with a stable output across releases; ~10 lines).
  This removes the fixed-name `include_str!` list and handles Vite's hashed filenames.
- **build.rs never runs node or pnpm.** `cargo build`, the Docker image and CI stay node-free. The built
  output (`crates/server/ui-dist/`) is committed and the gate rebuilds it and runs
  `git diff --exit-code -- crates/server/ui-dist` (the regenerate-and-diff pattern devices uses for layouts,
  `scratch/devices/Makefile:452-460`). Vite builds are deterministic for fixed inputs and versions
  (**ASSUMED**; the gate's diff proves it on the first run). Alternative if the owner dislikes committed output: a
  node stage in the Dockerfile and `make ui` before `cargo build`; costs node in every build path.
- Hashed asset names get `Cache-Control: max-age=31536000, immutable`; `index.html`, `sw.js` and the manifest
  get `no-cache`.

### 1.6 Recommendation (labelled)

**RECOMMENDED: (b) Svelte 5 + Vite 8 + pnpm, vitest + happy-dom + @testing-library/svelte for tests, a
hand-written service worker (no vite-plugin-pwa/Workbox), one Playwright PWA smoke spec, committed build output
embedded by a std-only build.rs.** 78 dev packages replaces a 6,613-line bespoke grader and a hand-rolled DOM
layer that would have to grow roughly tenfold for the parity set. Pin exact versions in `package.json` (no `^`),
`packageManager: "pnpm@10.34.5"`, `pnpm-lock.yaml` committed, `ignore-scripts=true`.
**Runner-up: (c) Lit 3 + esbuild** (about 8 packages to build) if Checkpoint K weighs supply chain above
authoring speed. **Not recommended: (a)** for an app-grade scope (fine for BRIEF's original "minimal" UI).
BRIEF §5.8's "no framework" should be amended at Checkpoint K (a recorded owner decision, not a silent edit).

---

## 2. K18: chorus conventions from scratch (`docs/conventions.md` outline)

Design rule for the conventions themselves: **a check must be much smaller than what it protects, name the rule
and the fix when it fails, and never write a per-file record that changes on every PR.** Every check below is a
tool invocation or a script under ~80 lines, all run by `make gate` (K19). Lessons baked in from the survey:
comment-density's per-file record conflicted on every Rust change and caused a red main (survey §7.14);
`tools/ui` was 4.4x the UI it graded (§7.15); the three duplicate ADR numbers came from parallel branches
claiming "next" (§7.6); the MSRV was false and untested (§7.3).

Proposed outline, each rule with its enforcement:

**C1. Rust**
- C1.1 Format: `rustfmt.toml` holds only `edition` (defaults otherwise). Check: `cargo fmt --all --check`. One
  formatting PR first (83/129 files are not clean today, survey §7.8).
- C1.2 Lints: `[workspace.lints]` in the root `Cargo.toml` and `lints.workspace = true` per crate (Cargo
  respects `[lints]` from 1.74, https://doc.rust-lang.org/cargo/reference/manifest.html, read 2026-09-29).
  Set: `clippy::all = warn`, `rust.unsafe_code = "deny"` (allowed per crate below),
  `clippy::undocumented_unsafe_blocks = "warn"`, `clippy::dbg_macro = "warn"`, `clippy::todo = "warn"`.
  Check: `cargo clippy --workspace --all-targets --locked -- -D warnings` (~50 warnings to clear first).
- C1.3 Toolchain and MSRV: `rust-toolchain.toml` with an exact `channel = "1.98.0"`, `components = ["rustfmt",
  "clippy"]`, `profile = "minimal"` (exact versions are valid channels,
  https://rust-lang.github.io/rustup/overrides.html, read 2026-09-29). chorus is an application, so **MSRV =
  the pinned toolchain**: `rust-version` equals the channel and the Docker build stage uses the same version.
  Check: a script asserts the three agree (toolchain file, `rust-version`, Dockerfile `FROM rust:<ver>`). This
  replaces the false 1.74 claim instead of testing it. Bumps are a one-line PR with a gate run.
- C1.4 Unsafe: denied workspace-wide; `#![allow(unsafe_code)]` only in `crates/alsa` (29 uses, FFI),
  `crates/hostctl` (9), `crates/audio-path/src/realtime.rs` (2) and the measure capture binary (2), each block
  with a `// SAFETY:` comment (the lint above). Adding a crate to that list needs an ADR.
- C1.5 Errors: libraries return typed error enums implementing `Display` + `std::error::Error`; no panic on a
  path fed by network or file input; binaries map errors to documented exit codes (the pattern
  `crates/client-linux/src/main.rs` already uses, `EXIT_CONFIG`). Check: review only at first (238
  `unwrap`/`expect` outside `tests/` today); promote `clippy::unwrap_used` per crate as each is cleaned, starting
  with `protocol` and `sync`.

**C2. C firmware**
- C2.1 Flags stay as they are (`firmware/Makefile:23-29`: `-std=c11 -Wall -Wextra -Werror -Wshadow
  -Wpointer-arith -Wstrict-prototypes -ffp-contract=off -fno-fast-math`); the FP flags are load-bearing for
  bit-exact Rust/C agreement and get a one-line comment rule: never remove.
- C2.2 Format: `.clang-format` (a named base style, few overrides), pinned via PyPI wheel:
  `uv run --with clang-format==<ver> clang-format --dry-run --Werror firmware/{src,include,tests}/*.[ch]`.
  Verified rootless here (wheel runs in 0.6 s). One formatting PR first.
- C2.3 Static analysis: cppcheck via `uv run --with cppcheck==<ver> cppcheck --std=c11 --error-exitcode=1
  --enable=warning,portability firmware/src` (verified rootless: Cppcheck 2.17.1 from the `cppcheck` PyPI
  wheel). clang-tidy also installs rootless (`uv run --with clang-tidy`, LLVM 22.1.8 verified) but needs a
  `compile_commands.json`; start with cppcheck, add clang-tidy only if it finds something cppcheck does not.
  Running a GPL tool is not a derivation; K33 only forbids reading GPL source (cppcheck's licence GPL-3.0 is
  **ASSUMED** from memory).

**C3. Shared fixtures (Rust and C)**
- Protocol, sync and (new) DSP behaviour is specified by files under `fixtures/<domain>/` that both
  implementations read (existing: `fixtures/protocol` 3 pairs, `fixtures/sync` 5+5 read by
  `firmware/tests/test_protocol.c` and `test_sync.c`). A behaviour change adds or updates a fixture, never a
  constant in one language only. Check: a script lists `fixtures/{protocol,sync,dsp}/*` and fails if a file is
  not referenced from both a Rust test and a C test (`fixtures/control` and `fixtures/discovery` stay
  Rust-only by declaration in the script).

**C4. Tests and measurement**
- No test in `make gate` needs hardware, a sound device or the network; hardware is faked (sim, fixtures).
- Environment-gated checks refuse by name and exit non-zero (keep `tools/lib.sh` +
  `unrun-checks-are-visibly-unrun.sh`).
- Timing claims appear only in `docs/measurements/` reports whose header states `source: measured | fixture |
  modelled` and `build: <sha>`. Check: the `build` SHA exists (`git cat-file -e <sha>^{commit}`), so the
  dangling-SHA problem (survey §7.7) cannot recur; a squash-merged branch cites the merge commit, not a branch
  commit.

**C5. Dependencies**
- Zero external crates is the default (BRIEF §3.2); adding one needs an ADR answering the §3.2 question.
- Licence allowlist: `MIT`, `Apache-2.0`, `MIT OR Apache-2.0`, `BSD-2/3-Clause`, `ISC`, `Zlib`,
  `Unicode-3.0`. Check today: a script over `cargo metadata --format-version 1 --locked` (jq) asserting every
  package has an allowed `license` and counting non-path packages (0 today). Switch to `cargo deny check
  licenses bans sources` (already installed rootless here via mise aqua) when the first external crate lands.
- npm: dev-only, pinned exact, lockfile committed, `ignore-scripts=true`; the same allowlist checked with
  `pnpm licenses list --json` (**ASSUMED** command shape, verify in goal 1).

**C6. Pinning**
- Exact versions or digests for everything that builds or runs chorus: `rust-toolchain.toml`, `mise.toml`
  (node, pnpm, uv, gitleaks, tools), Dockerfile `FROM ...@sha256:`, GitHub Actions `uses: ...@<40-hex>`,
  ESP-IDF by exact tag, `package.json` exact + `packageManager`, `Cargo.lock` with `--locked`,
  `pnpm-lock.yaml` with `--frozen-lockfile`. Check: one script under 80 lines (grep the four file kinds for a
  floating reference); no records, no demo trees.

**C7. Docs**
- ADR IDs cannot collide between parallel branches: **the ADR number is the PR number** that adds it (the Rust
  RFC process renames `0000-` to the PR number, https://github.com/rust-lang/rfcs, read 2026-09-29). Draft as
  `docs/decisions/0000-slug.md`, rename once the PR exists. Check: no two ADR files share a number (the three
  historical duplicates 0012 x2 and 0021 x3 are listed once in the script as grandfathered, or renamed with a
  redirect line), and no `0000-` file on main. ADR sections: Status, Date, Context, Decision, Consequences,
  Alternatives, Sources (URL + read date; ASSUMED for memory).
- `CLAUDE.md` <= 200 lines (K52). Check: `test "$(wc -l < CLAUDE.md)" -le 200`.
- No em dashes (BRIEF §3.1.5): check `git grep -n -P '\x{2014}'` over tracked text plus
  `git log --format=%B origin/main..HEAD` (commit messages) and the PR body file when one is given. Today's only
  hit is inside `tools/ui/reads.js`, which K18 retires.

**C8. Identity and secrets (K27)**
- gitleaks (MIT, https://github.com/gitleaks/gitleaks, read 2026-09-29) with a repo `.gitleaks.toml` using
  `[extend] useDefault = true`; run `gitleaks git` on the branch range and `gitleaks dir` on the tree.
- The private term list (names, LAN ranges, MACs, SSIDs, account IDs) lives outside the repo; the gate reads
  its path from an env var and **fails if it is missing** (never silently skips), then
  `git grep -n -i -F -f "$TERMS"` over tracked files and the branch's commit messages. gitleaks' own
  `GITLEAKS_CONFIG` / `--config` loading order makes a private gitleaks config an alternative.

**C9. Commits and branches**
- Subject `area: summary` where area is a crate, `firmware`, `pwa`, `docs`, `tools` (the style shopkit uses,
  `scratch/shopkit/CLAUDE.md` Git flow). Full Conventional Commits is not adopted: no changelog tool consumes
  it and releases are few (K9). Not gated beyond C7's em-dash and C8's identity checks on messages.

**C10. UI (only what (b) needs)**
- Sources in `ui/src/`, shared state in `*.svelte.js` runes modules fed only by the control API/SSE; tokens stay
  in `tokens.css`. Every interactive control has an accessible label (tests query by label, which enforces it for
  tested controls). Check: `vitest run`, `vite build` + committed-output diff, one Playwright PWA smoke.
  Optional later: `svelte-check` (type checking) only if it earns its packages.

**What is deliberately not a rule**: comment density, styling-token scans, per-file conformance records,
demonstration trees per rule. A rule that cannot be checked cheaply is review guidance in `docs/conventions.md`,
not a gate.

---

## 3. K44: `shopkit-acoustics`

### 3.1 What shopkit requires (read in `scratch/shopkit`, HEAD fb1d536, 2026-09-29, v1.18.0)

- uv workspace, Python 3.13, 28 packages, lockstep versions, one `uv.lock` (`CLAUDE.md`, `README.md`).
- Package layout (`docs/ARCHITECTURE.md` §12 "Adding a package"): `packages/acoustics/pyproject.toml`
  (`name = "shopkit-acoustics"`, lockstep version, `requires-python = ">=3.13"`,
  `license = "LicenseRef-Proprietary"`, `uv_build>=0.12,<0.13`, every import declared, a `[tool.shopkit]` table
  with `layer`, `kernel = false`, `cli`, `goal`), `src/shopkit_acoustics/__init__.py` (docstring, `__all__`,
  `__version__`) + `py.typed`, `README.md` in the table format, `CLAUDE.md` <= 60 lines, `rules/` (a
  `.gitkeep` at least), `tests/test_smoke.py`; root `pyproject.toml` import-linter entries (layers,
  `no-data-repos`, `kernel-free`); `tests/test_layers.py` `BRIEF_LAYERS` and `tests/test_kernel_free.py`
  `KERNEL_FREE`; a CLI via `[project.entry-points."shopkit.cli"]`; `uv lock` (never hand-merged), `make ci`.
  A new package "comes with an ADR or a brief amendment that says why".
- **Licence mismatch to decide**: shopkit packages are `LicenseRef-Proprietary`; chorus is MIT OR Apache-2.0
  (K26). Fine while both are private, but K44 should state that acoustics results enter chorus/devices as
  data, never as copied shopkit code.
- Ownership: `PROGRAM-REQUESTS.md` has one section per owner (home, 3d, devices, shared root files); append to
  the owner's section; only the owner changes state (`OPEN` > `ACCEPTED (goal n)` > `DONE <PR/sha>`, or
  `DEFERRED`/`DECLINED`); a row open after its owner's program finished may be served by the requester under
  the owner's rules and gate. chorus must **add a `chorus` row to the ownership table** and a `## To chorus`
  section (a shared-root small PR).
- ADRs: `docs/adr/NNNN-slug.md`, "the next free number", taken at merge time (devices brief §0.13); 0001-0019
  exist. Sections: Status, Date, Context, Decision, Consequences, Alternatives, Sources; row added to
  `docs/README.md`.
- Gate `make ci` (`Makefile:48-52`): lock-check, sync, lint (ruff + format), typecheck (pyright), imports
  (import-linter), root tests, rules lint, identity scan, affected packages' tests in exact per-package envs.
- Release (`tools/release.py` show/set/tag, lockstep; devices brief §0.13 "Releases (C13)"): under
  `flock /cache/locks/shopkit-release.lock`, branch `dev-release/v<ver>`, `release.py set`, `uv lock`,
  `make ci` under `flock /cache/locks/shopkit-merge.lock`, PR, merge, tag the merge commit by SHA, push the
  tag, then re-pin the consumer in its own PR. Merges into shopkit hold the merge lock from the final
  `git pull --rebase` to `gh pr merge`.
- The nearest model is **shopkit-sizing** (`packages/sizing`): stdlib only, every constant in a cited TOML
  data table (`sizing:S1`), refuse and name the blank (`S2`), each result names its method (`S3`), **every
  calculator reproduces a published worked example in its tests, cited, tolerance stated (`S4`)**, sources
  carry a terms verdict (`S5`). shopkit-acoustics should copy these five rules nearly verbatim.

### 3.2 Proposed shape

`packages/acoustics/src/shopkit_acoustics/`, layer 1 (imports shopkit-core only), kernel-free, CLI group
`shopkit acoustics` (every command `--json`, per shopkit's CLI rule):

| Module | Content |
|---|---|
| `driver.py` | `Driver` T/S record (fs, Qes, Qms, Vas, Re, Le, Sd, Xmax, Bl, Mms, Cms, Pe) with derivations: Qts = Qms*Qes/(Qms+Qes), EBP = fs/Qes, eta0 = 4*pi^2*fs^3*Vas/(c^3*Qes), Vas = rho*c^2*Sd^2*Cms (https://en.wikipedia.org/wiki/Thiele/Small_parameters, read 2026-09-29, citing Thiele 1961/1971 and Small 1972-1974 JAES); a consistency check that refuses an over-determined set that disagrees |
| `air.py` | rho, c at a stated temperature (cited table entry) |
| `sealed.py` | closed box: Vb for a target Qtc, fc and f3 (Qtc = Qts*sqrt(1+Vas/Vb), fc = fs*sqrt(1+Vas/Vb): standard Small closed-box relations, **ASSUMED** from memory, to be cited in goal) |
| `vented.py` | alignments (QB3, SBB4, SC4, B4) from cited alignment tables; fb, Vb, f3; losses QL |
| `port.py` | port length for area/fb/Vb with end correction; peak port air velocity at Xmax/Pe; a refusal above a stated velocity limit (cited) |
| `baffle.py` | baffle-step frequency from baffle width and the diffraction loss (the f = 115/W(m) rule is **ASSUMED**, to be cited) and a BSC network (L parallel R) |
| `crossover.py` | LR2/LR4 (LR4 = two cascaded 2nd-order Butterworth, -6 dB at fc, flat sum, https://en.wikipedia.org/wiki/Linkwitz%E2%80%93Riley_filter, read 2026-09-29, citing Linkwitz 1976 JAES); passive component values into a resistive load; active Sallen-Key stage values from the Q table (Linkwitz lists Q0 per LR order, https://www.linkwitzlab.com/filters.htm, read 2026-09-29); **export biquad coefficients** at 48 kHz for chorus's DSP |
| `response.py` | SPL and cone excursion vs frequency for sealed/vented at a drive voltage (the transfer functions of `sealed`/`vented`), max SPL limited by Xmax and Pe |
| `export.py` | the design record for consumers (below) |

**Output consumable by devices and chorus**: `shopkit acoustics design --input box.toml --json` writes a design
record `shopkit-acoustics-design/1` (box net volume, port diameter/length/count, fb, f3, baffle step, crossover
topology and component values, LR4 biquads per sample rate, predicted SPL/excursion at stated power), every
value with its method and source. devices' `builds/chorus-*/` commit it (and a Make target regenerates and
diffs it, the devices pattern); the enclosure's `hardware/*.py` reads the volume and port from it (so CAD and
acoustics cannot drift); chorus's DSP library reads the biquads as a **shared fixture** (C3), so the Rust/C
runtime crossover is checked against the design tool's numbers.

**Python dependencies**: numpy 2.5.3 (BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0) and scipy 1.18.1
(BSD) are already in shopkit's `uv.lock` (lines 1142, 1792; licences from https://pypi.org/pypi/<pkg>/json,
read 2026-09-29). `scipy.signal` (butter, sosfreqz) is enough for LR biquads and responses. Following
shopkit-sizing, keep the alignment math stdlib-only (complex arithmetic and `math`) and use numpy/scipy only in
`response.py`/`crossover.py`, as an extra if the layer tests prefer that.

**Tests against published worked examples** (rule `acoustics:A4`, modelled on `sizing:S4`):
- Vas/Mms from added-mass measurement: ESP "Measuring Loudspeaker Driver Parameters" worked example (fs 27 Hz,
  added mass 45.80 g, Mms 121.16 g, Vas 40.57 L; https://sound-au.com/tsp.htm, read 2026-09-29).
- Vented-box design-chart tables for QL = 3, 5, 7, 10, 20, infinity as numeric `.prn` files from W. Marshall
  Leach's ECE4445 page (https://leachlegacy.ece.gatech.edu/ece4445/, read 2026-09-29; no licence stated, so use
  as test oracles with a terms verdict, not redistributed data).
- LR4 sum flatness and -6 dB at fc: from the Wikipedia/Linkwitz definitions above; LR Q0 table from Linkwitz.
- Cross-check implementations: pyfar's `pyfar.dsp.filter.crossover(signal, N, frequency, sampling_rate)`
  (Linkwitz-Riley from cascaded Butterworth, https://pyfar.readthedocs.io/en/stable/modules/pyfar.dsp.filter.html
  and MIT licence https://github.com/pyfar/pyfar, read 2026-09-29) as a one-off transcript parity check (not a
  dependency).
- Still needed in the goal (not found in this pass): a published sealed-box and vented-box design worked end to
  end (Small's 1972/1973 JAES papers are paywalled; Dickason's Loudspeaker Design Cookbook is a book, both
  **ASSUMED** to contain them) and a passive LR4 worked example (ESP's passive crossover article covers only
  6 and 12 dB/octave, https://sound-au.com/lr-passive.htm, read 2026-09-29; Rane Note 160 has no component
  values, https://www.ranecommercial.com/legacy/note160.html, read 2026-09-29).

**Prior art to learn from (permissive only, K39/K33)**: pyfar (MIT); scipy.signal (BSD). python-acoustics'
PyPI entry did not resolve in this pass (**ASSUMED** BSD-3 and archived). WinISD, VituixCAD and Hornresp are
closed freeware (**ASSUMED**): usable only as black-box cross-checks by the owner, never as sources.

---

## 4. K23: devices builds (read-only clone `scratch/devices`, HEAD 5bff0e8, 2026-09-29)

- **Structure**: `builds/<device>-v<n>/`, one directory per physical revision, copied from `builds/_template`
  (`bom.csv` + `log.md`). A new revision is a new directory; an old log stays true to what was made
  (`builds/README.md`).
  - `bom.csv`: `part_id,qty,notes` (+ optional `variant`, `budget`); part IDs must exist in NSchatz/inventory
    (`PRT-` records); `budgets.csv` optional; costed by `shopkit inv bom builds/<x>` which exits 1 over budget
    or on an unpriced line. EUR and USD never summed; a missing cost is unknown, not free.
  - `log.md`: Started/Finished, links (spec, firmware, BOM, hardware), Intent, Platform and why, Order log
    table, dated Build log, Problems (one heading each), Verdict, and a **"the Needs list" list with the exact
    command, what to expect and what changes** (`CLAUDE.md:191`).
  - Committed renders/diagrams beside it (e.g. `node-v1-wiring.{svg,png,dxf}`), regenerated by Make targets and
    freshness-checked by `make check`; renders freeze when the PCB is ordered.
  - Physical design code lives outside `builds/`: `hardware/<area>/*.py` (e.g. `hardware/cars/canlog_v1.py`,
    `hardware/nodes/`), specs in `specs/<area>/*.toml`, PCBs in `hardware/pcb/<dev>/`, firmware in
    `firmware/<name>/`.
- **Rules**: devices' rules live in the shopkit packages they govern (`electronics:`, `pcb:`, `build:`,
  `cad:`, `fdm:` ...), cited qualified; `CLAUDE.md` <= 200 lines (196 today, `make lint`); identity scan in
  `make check` and pre-commit; hardware read-only for agents (never flash, order, upload or spend).
- **Sourcing**: "US vendors first, judged by where the seller ships from" (V10, `docs/decisions.md:230`;
  brief `2026-09-devices.md:411`: DigiKey, Mouser, Amazon US; EU only when nothing in the US has it).
- **Order packets**: an explicit goal deliverable (glide6 goal 2, brief goal 5 "the order packet"): the
  fab-ready files plus the priced BOM handed to the owner; not a directory convention of its own.
- **Gate**: `make check` (doctor, identity scan, tests, lint, regenerate-and-diff of committed outputs, PCB
  checks and DRC, firmware checks, BOM); `make invariants` for PRs touching `firmware/`, `hardware/`,
  `specs/`, `pyproject.toml`, `uv.lock`, `Makefile` (brief §4.2). Budget < 15 min.
- **Program status**: goal 1 `COMPLETE (goal 1): 2026-09-29` (`2026-09-devices-g1.status.md:137`); goal 2 in
  progress (ledger's last lines: release + pin, node-v1, homelab PRs next); goals 3-5 not started; goal 5
  (glide6 goal 2) is gated on the owner's Checkpoint A and "prints BLOCKED until it is done". So devices' **last
  COMPLETE may be far off**; K23's "serve own rows after the owner finishes" will not trigger soon.
- **§0.13 rule** (devices brief, `devices-brief.md:288-327`): "A row still open when its owner's program has
  finished (the owner's last `COMPLETE` line on its `main`) may be served by the requesting program itself, in
  the owner's package or repo, under the owner's rules and gate, with the row cited in the PR"; otherwise both
  final reports list it and the owner's the Needs list gets "start a follow-up goal for rows ...".

**What `builds/chorus-compact-v1/` needs** to satisfy devices' rules:
`log.md` (template sections + Intent citing chorus K22, Platform and why: ESP32-S3 + TAS58xx + PoE, a Needs
the owner list), `bom.csv` with inventory `PRT-` IDs (new parts are rows to inventory's owner first, US-first
sourcing), `budgets.csv`, the acoustics design record from shopkit-acoustics (§3.2) and its regenerate target,
committed renders (enclosure, wiring) regenerated by a Make target and freshness-checked, enclosure code in
`hardware/speakers/chorus_compact_v1.py` with a spec `specs/speakers/chorus_compact_v1.toml`, PCB under
`hardware/pcb/chorus-compact/` if custom, and a pointer to the firmware in NSchatz/chorus (the endpoint firmware
stays in chorus; devices must not fork it). Enclosure parts that print go through 3d's rules.

---

## 5. K46: `chorusctl`

Existing pattern (read in `/workspace`): binaries parse `std::env::args().skip(1)` by hand into a config
struct (`ClientConfig::from_args(args)` in `crates/client-linux/src/main.rs:66-71`, same shape in
`crates/server/src/main.rs:171`), return `std::process::ExitCode`, and map failures to named exit constants
(`EXIT_CONFIG`). Extra bins live in `crates/<crate>/src/bin/*.rs`.

Proposed conventions for a zero-dependency CLI:
- New crate `crates/ctl` with `src/main.rs` (binary `chorusctl`) and a `lib.rs` holding the parser and the
  control-API client so both are unit-testable; the HTTP/SSE client is hand-written over `std::net::TcpStream`
  against the existing control catalog (fixtures in `fixtures/control` are its test oracle).
- Grammar: `chorusctl [--server URL] [--json] <noun> <verb> [args]` (zones, groups, volume, endpoints, ota,
  diag); a hand-written parser (a small `Args` iterator with `--flag value` and `--flag=value`), `--help` per
  noun generated from one table, unknown flags are errors naming the closest valid flag.
- Output: human text by default, `--json` for scripts (stable, documented, what HA/automation would consume);
  errors to stderr with a fix line; exit codes documented (0 ok, 1 usage, 2 unreachable, 3 refused by server).
- Behind Traefik: chorusctl talks to the server's LAN control port directly (K40's trusted LAN); it never
  handles the household login.

---

## Recommendations the brief should adopt (one line each)

1. Adopt Svelte 5 + Vite 8 + pnpm for the PWA (K43), with vitest + happy-dom + testing-library; amend BRIEF §5.8 "no framework" by recorded owner decision at Checkpoint K.
2. Hand-write the service worker; never add vite-plugin-pwa/Workbox (348 packages vs 39).
3. `<link rel="manifest" crossorigin="use-credentials">`, network-only for `/api` and SSE, network-first shell, `redirect: "manual"` and a "signed out, tap to sign in" state.
4. One Playwright PWA smoke spec in `make gate` against the server behind a fake forward-auth; everything else tested without a browser.
5. Embed the UI with a std-only `build.rs` that `include_bytes!` the committed build output; build.rs never runs node; the gate rebuilds and diffs.
6. Pin npm deps exactly, commit `pnpm-lock.yaml`, set `packageManager`, keep `ignore-scripts=true`.
7. Add a the Needs list item: install the PWA on the owner's iPhone behind tinyauth and confirm login completes in standalone mode.
8. `rust-toolchain.toml` pins an exact toolchain; MSRV is defined as that toolchain and a script keeps toolchain, `rust-version` and Dockerfile in agreement.
9. `[workspace.lints]` with `unsafe_code = "deny"` (allowlisted crates with `// SAFETY:`), `cargo clippy -D warnings`, `cargo fmt --check`, after one formatting PR.
10. C firmware: clang-format and cppcheck via pinned PyPI wheels (`uv run --with`), both verified rootless.
11. A shared-fixture check: every file in `fixtures/{protocol,sync,dsp}` is read by both a Rust and a C test.
12. Measurement reports carry `source:` and a `build:` SHA that must exist in the repo.
13. Zero external crates by default; a licence-allowlist script over `cargo metadata` now, `cargo deny` when the first crate lands.
14. ADR number = the PR number that adds it (Rust RFC precedent); the gate rejects duplicate numbers and `0000-` files on main.
15. Gate checks for CLAUDE.md <= 200 lines, em dashes in tracked text and branch commit messages, gitleaks, and a private term list whose absence fails the gate.
16. Commit subjects `area: summary`; no Conventional Commits tooling.
17. Every check is a tool call or a script under ~80 lines, names its rule and fix, and writes no per-file record.
18. shopkit-acoustics copies shopkit-sizing's five rules (cited data tables, refuse blanks, named method, a published worked example per calculator, terms verdicts) and is layer 1, kernel-free.
19. shopkit-acoustics emits a versioned design record (`shopkit-acoustics-design/1`) consumed by devices' enclosure code and by chorus's DSP as a shared fixture.
20. chorus's first shopkit PR adds a `chorus` row to the ownership table and a `## To chorus` section in PROGRAM-REQUESTS.md.
21. Plan for devices' last COMPLETE being late (goal 5 waits on the owner's Checkpoint A): chorus rows to devices need workarounds, not a serve date.
22. `builds/chorus-*` follow devices' template (log with the Needs list, inventory `PRT-` BOM, US-first sourcing, committed regenerated renders); endpoint firmware stays in chorus.
23. `chorusctl` is a new std-only crate with a hand-written noun/verb parser, `--json` output and documented exit codes, tested against `fixtures/control`.

---

## What I read

Local: `/cache/tmp/plan-2026-09-chorus/decisions.md` (K5, K9, K16, K18, K19, K22, K23, K26, K27, K30, K31, K33,
K39, K40, K43, K44, K46, K49, K52); `survey-chorus.md` §2, §5, §7; `survey-homelab.md` lines 215-230;
`devices-brief.md` §0.3 (lines 76-84), §0.13 (288-345), §4.1-4.3 (603-645); `/workspace/CLAUDE.md`;
`/workspace/BRIEF.md` §3, §5.8; `/workspace/crates/server/src/control.rs` (213-217 and route area),
`crates/server/src/ui/*` (sizes only), `crates/client-linux/src/main.rs` (arg parsing), `firmware/Makefile:20-30`,
`.gitignore`; crate `unsafe`/`unwrap` counts via rg.
shopkit (`scratch/shopkit`, read-only): `CLAUDE.md`, `README.md`, `PROGRAM-REQUESTS.md` (rules and headings),
`docs/README.md` (ADR rules), `docs/adr/` listing, `docs/ARCHITECTURE.md` §12, `Makefile` (ci),
`tools/release.py` (header), `packages/sizing/{pyproject.toml,CLAUDE.md,README.md}`, `mise.toml`, `uv.lock`
(numpy/scipy lines).
devices (`scratch/devices`, cloned `--depth 1` read-only): `CLAUDE.md` (100-196), `builds/README.md`,
`builds/_template/log.md`, `builds/node-v1/bom.csv`, `builds/canlog-v1/log.md` (head), `hardware/` listing,
`Makefile` (check), `.claude/goals/2026-09-devices.md` (goal table, V10), ledgers' COMPLETE lines,
`2026-09-devices-g2.status.md` (tail).
Web (all read 2026-09-29): developer.mozilla.org (Manifest; ServiceWorkerContainer.register);
developer.chrome.com/blog/update-install-criteria; web.dev/learn/pwa/installation; svelte.dev/docs/svelte/testing;
lit.dev/docs/tools/production; github.com/steveiliop56/tinyauth; tinyauth.app; github.com/gitleaks/gitleaks;
github.com/rust-lang/rfcs; doc.rust-lang.org/cargo/reference/manifest.html and rust-version.html;
rust-lang.github.io/rustup/overrides.html; en.wikipedia.org Thiele/Small parameters and Linkwitz-Riley filter;
linkwitzlab.com/filters.htm; sound-au.com/tsp.htm and lr-passive.htm; ranecommercial.com/legacy/note160.html;
leachlegacy.ece.gatech.edu/ece4445; pyfar.readthedocs.io (dsp.filter); github.com/pyfar/pyfar;
registry.npmjs.org (11 packages); pypi.org JSON (numpy, scipy, pyfar). Failed: sound.whsites.net (DNS),
sound-au.com/articles/baffle-step.htm and bafflestep.htm (404), tinyauth.app/docs (404).
No GPL source was opened (tinyauth is AGPL: only its README page was read; cppcheck was run, not read).
Tools run in scratch: pnpm installs of 9 variants, Vite 7/8 builds, one vitest run; `uv run --with` clang-format,
clang-tidy, cppcheck (version checks only).
