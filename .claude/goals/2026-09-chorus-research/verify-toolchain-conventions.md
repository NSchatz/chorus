# Verification: research-toolchain-env.md and research-pwa-conventions.md (2026-09-29)

Adversarial re-run on this machine (2 CPUs, no sudo). Heavy work under
`flock -o /cache/locks/chorus-heavy.lock`, all under `timeout`. Worktree:
`/cache/tmp/plan-2026-09-chorus/scratch/verify-wt` (detached origin/main), removed at the end.

Status: COMPLETE (all 10 claims checked, 2026-09-29 23:12-23:35 UTC)

| # | claim | verdict |
|---|---|---|
| 1 | ESP-IDF v5.3.6 builds firmware except exactly two source errors | CONFIRMED (cold 124 s; incremental compile 0.2-1.6 s, link unmeasured) |
| 2 | v5.3.x P4 only to rev v1.99; v5.3 EOL; v6.1 stable | CONFIRMED; correct 'rev v3.0 in v5.4.x' to v5.4.4+/v5.5.2+ (v3.1: v5.5.3+/v6.0+) |
| 3 | smallest compiling Rust is 1.83.0 (1.82 E0369) | CONFIRMED (1.74 and 1.82 E0369 at client-linux main.rs:179-180; 1.83 check and build pass) |
| 4 | fmt ~325 hunks / 83 files; clippy ~56 distinct warnings | CONFIRMED exactly (325 hunks / 83 files; 56 distinct / 100 summed) on toolchain 1.98.0 |
| 5 | daemonless OCI tarball runs | CONFIRMED, stronger: unpacked binary serves GET / 200 (default CMD needs ulimits) |
| 6 | Playwright Chromium headless with chromium-libs | PARTLY: launches, but no fonts, so text never renders; research screenshots are blank |
| 7 | gitleaks 0 findings | CONFIRMED (0 / 0) |
| 8 | shopkit make ci ~3 min, identity vacuous | CONFIRMED (171 s); no private home clone needed, any dir passes; shopkit clone is stale |
| 9 | Svelte/Vite8 78 pkgs, <3 s build; vite-plugin-pwa 348 | PARTLY: 78/348 are installed-on-linux counts (lockfile 103/398); 348 is the Vite 7 stack; build < 3 s CONFIRMED |
| 10 | include_str! at control.rs:213-217; Dockerfile lacks COPY docs | CONFIRMED; consequence: deploy/Dockerfile cannot build (no docs/, and rust:1.74) |

## Claim 1: ESP-IDF v5.3.6 builds `firmware/` for esp32s3 except exactly two source errors

**CONFIRMED.** Fresh build dir `scratch/verify-idf-build`, `scratch/verify-idf.sh` under the heavy lock.
`idf.py --version` -> `ESP-IDF v5.3.6`; set-target rc 0 9.8 s; cold `idf.py build` rc 2 **124.2 s**
(research: 144.3 s) stopping at `[1028/1044]`; `ninja -k 0 -j2` rc 1 7.0 s. Only two FAILED objects,
three error lines, zero warnings:

```
FAILED: esp-idf/main/CMakeFiles/__idf_main.dir/__/src/sync_sim.c.obj
FAILED: esp-idf/main/CMakeFiles/__idf_main.dir/__/src/monotonic.c.obj
firmware/src/sync_sim.c:47:12: error: static declaration of 'finite' follows non-static declaration
firmware/src/monotonic.c:16:5: error: implicit declaration of function 'vTaskDelay' [-Werror=implicit-function-declaration]
firmware/src/monotonic.c:16:16: error: implicit declaration of function 'pdMS_TO_TICKS' [-Werror=implicit-function-declaration]
ninja: build stopped: cannot make progress due to previous errors.
### v-ninja-k0: rc=1 wall=7.0s
```

Objects that did compile: `app_main.c esp_hal.c amp conf endpoint_config i2s protocol session sync_rng
sync_scenario sync_servo telemetry wifi endpoint.conf.S`; `bootloader.bin` 21,552 B; no `.elf` (link
not reached). Flags `-std=gnu17 -Wall -Werror=all -Wextra`. Build dir 126 MB (matches).

**Incremental timings** (no code change; link never reached, so these EXCLUDE link + image gen):
`ninja -k 0` no-op 0.2 s; after `touch src/protocol.c` 0.2 s; `idf.py build` no-op 1.6 s (rc 2).
So "an incremental one is seconds" holds for compile, but the link/elf2image cost is unmeasured
until the two errors are fixed.

## Claim 2: v5.3.x supports ESP32-P4 only to rev v1.99; v5.3 EOL; v6.1 current stable

**CONFIRMED, with a precision correction on where rev v3.0 first appears.**

- Installed tree `/cache/esp/esp-idf-v5.3.6` (`git describe` v5.3.6, commit 2026-09-10):
  `components/esp_hw_support/port/esp32p4/Kconfig.hw_support` offers `Rev v0.0 / v0.1 / v1.0`,
  `comment "Maximum Supported ESP32-P4 Revision (Rev v1.99)"`, `ESP32P4_REV_MAX_FULL default 199`.
  `release/v5.3` branch head: 0 matches for `Rev v3.0` (no backport pending).
- `curl raw.githubusercontent.com/espressif/esp-idf/<tag>/.../Kconfig.hw_support`, count of `Rev v3.0` / `Rev v3.1`:

```
v5.4, v5.4.1, v5.4.2, v5.4.3: v3.0=0      v5.4.4: v3.0 present (released 2026-04-17)
v5.5, v5.5.1: v3.0=0 v3.1=0               v5.5.2: v3.0=1 v3.1=0 (2025-12-25)
v5.5.3, v5.5.4, v5.5.5: v3.0=1 v3.1=1      v6.0, v6.0.1, v6.0.2, v6.0.3, v6.1: v3.0=1 v3.1=1
```

  Correction: rev v3.0 needs **v5.4.4+ or v5.5.2+** (not "v5.4.x" generally; v5.4.0-v5.4.3 lack it);
  rev v3.1 needs **v5.5.3+ or v6.0+**. The research's "at least v5.4" is too loose; say v5.5.3+ or v6.x.
- EOL: Espressif's own support table `https://dl.espressif.com/dl/esp-idf/idf_versions.js` (read
  2026-09-29): `"v5.3": start 2024-07-25, end_service 2025-07-25, end_date 2027-01-25`;
  v5.4 EOL 2027-07-05; v5.5 2028-01-21; v6.0 2028-09-20; `"v6.1": start 2026-08-25, end_service
  2027-08-25, end_date 2029-02-25`. The research computed these from the 30-month policy and they
  match to within days (v6.1 start is 2026-08-25 in Espressif's table vs 2026-08-27 GitHub publish).
- Current stable: `gh api repos/espressif/esp-idf/releases/latest` -> `v6.1 2026-08-27T01:56:12Z`;
  WebFetch of `docs.espressif.com/projects/esp-idf/en/stable/esp32/versions.html` -> "ESP-IDF
  Programming Guide v6.1". Latest v5.3.x tag: v5.3.6 (`git ls-remote --tags`), published 2026-09-15.

## Claim 10: control page embedded with include_str! at control.rs:213-217; Dockerfile lacks COPY docs

**CONFIRMED, and it has a consequence the research did not state.** `crates/server/src/control.rs`
lines 213-217 are five `include_str!`: `ui/index.html`, `ui/tokens.css`, `ui/chorus.css`,
`ui/chorus.js`, and `"../../../docs/control-page.md"` (served at `GET /docs/control-page.md`, line 601).
`deploy/Dockerfile` build stage copies only `Cargo.toml Cargo.lock crates fixtures audio-path.conf`
(lines 23-27); no `COPY docs`. So `docker build` of `deploy/Dockerfile` at origin/main cannot compile
chorus-server (the include resolves to `/src/docs/control-page.md`, absent). Also the build stage is
`FROM rust:1.74-slim-bookworm` (line 20) while claim 3 shows 1.74-1.82 cannot compile the workspace
(`--workspace --bins` includes client-linux): **the committed Dockerfile is broken twice.**

## Claim 3: smallest Rust that compiles the workspace is 1.83.0 (1.82 fails E0369)

**CONFIRMED.** Toolchains still present in `~/.rustup/toolchains` (1.74.0, 1.80-1.85, 1.90, 1.98);
mise has only `rust@1.74.0` and `rust@1.98.0` registered, so I used the rustup proxy
`/cache/cargo/bin/cargo +<v>` (same toolchains `mise exec rust@<v>` would resolve to).
`scratch/verify-msrv.sh` under the lock, reusing `scratch/target-msrv-<v>`:

```
1.74.0  check --locked --workspace               rc=101 5.2 s  2x E0369 `==` on ExitCode (chorus-client-linux)
1.82.0  check --locked --workspace               rc=101 2.4 s  2x E0369
1.82.0  check --locked --workspace --all-targets rc=101 4.9 s  --> crates/client-linux/src/main.rs:179:25 and :180:29
1.83.0  check --locked --workspace               rc=0   3.3 s
1.83.0  check --locked --workspace --all-targets rc=0   9.1 s
1.83.0  build --locked --workspace --all-targets rc=0  60.2 s  (extra: build, not just check, also passes)
```

Cause: `outcome.code == ExitCode::from(EXIT_CONFIG)` (client-linux main.rs:179-180); `PartialEq for
ExitCode` is only available from 1.83. The workspace declares `rust-version = "1.74"` (Cargo.toml:21):
false, as the research says. Not independently re-run: 1.80/1.81/1.84/1.85/1.90 rows (bracketing is
already decided by 1.82 fail / 1.83 pass).

## Claim 4: `cargo fmt --check` ~325 hunks in 83 files; clippy ~56 distinct warnings

**CONFIRMED exactly** (rustfmt 1.9.0-stable, clippy 0.1.98, toolchain 1.98.0).

```
cargo +1.98.0 fmt --all -- --check          rc=1   325 "Diff in" hunks, 83 distinct files
cargo +1.98.0 clippy --locked --workspace --all-targets --message-format=json   rc=0 10.4 s
  warnings with spans: 100 summed, 56 distinct (code|file:line:col)
  29 doc_overindented_list_items, 7 assertions_on_constants, 6 doc_lazy_continuation,
  5 neg_cmp_op_on_partial_ord, 2 unusual_byte_groupings, 2 single_match, 1 each: while_let_loop,
  ptr_arg, manual_range_contains, manual_contains, io_other_error
```

Caveat for the brief: both counts are specific to clippy/rustfmt 1.98. If the program pins an older
toolchain (e.g. 1.83 as MSRV), the clippy set will differ (newer lints like `manual_contains`,
`io_other_error`, `doc_overindented_list_items` do not exist in older clippy). Quote the counts with
the toolchain version.

## Claim 7: gitleaks finds 0 findings on chorus history and tree

**CONFIRMED.** `mise exec aqua:gitleaks/gitleaks@8.30.1 -- gitleaks {git,dir} . --redact=100 --no-banner
--report-format json` in `verify-wt` (reports deleted after counting):

```
gitleaks git: "23 commits scanned." "scanned ~3587240 bytes (3.59 MB) in 1.08s" "no leaks found"  rc=0 1.6 s  findings=0
gitleaks dir: "scanned ~3761318 bytes (3.76 MB) in 760ms" "no leaks found"                        rc=0 1.7 s  findings=0
```

Nuance: `gitleaks git` scans all refs (23 commits via `git rev-list --all`; HEAD alone has 20). Default
ruleset only; it says nothing about the private-term list (K27), which does not exist in this container.

## Claim 5: daemonless OCI path (static musl chorus-server + crane + umoci) gives a runnable OCI tarball

**CONFIRMED, and stronger than the research showed.** Re-checked the existing artifact
`scratch/oci/chorus-server-oci.tar` (1,658,880 B) independently in my scratchpad (did not reuse
their `bundle/`):

```
tar xf chorus-server-oci.tar ; all 15 blobs: sha256 matches file name (0 mismatches)
index.json: one manifest, ref.name "chorus-server", linux/amd64
umoci 0.6.0 unpack --rootless --image layout:chorus-server bundle     rc=0
config.json process: args ["/usr/local/bin/chorus-server","--listen","0.0.0.0:4010","--source","tone","--serve-forever"], uid/gid 65532
image config: User 65532:65532, ExposedPorts 4010/tcp, Entrypoint /usr/local/bin/chorus-server
ldd rootfs/usr/local/bin/chorus-server -> "statically linked" (1,610,336 B; chorus-rt-spin 591,864 B)
crane digest gcr.io/distroless/static-debian12:nonroot -> sha256:afa5c872...ff57f7ab (unchanged)
```

Run test of the unpacked binary: `--help` -> "configuration refused: unknown argument '--help'" rc 2
(the research's evidence; it only proves the binary execs). I went further:
`chorus-server --listen 127.0.0.1:47123 --control-listen 127.0.0.1:47124 --source tone --serve-forever
--allow-unlocked-memory --allow-non-realtime` -> "control listening on=127.0.0.1:47124 workers=8",
`GET /` 200 1103 B, `GET /docs/control-page.md` 200 9847 B, killed by timeout (rc 124). So the image's
binary really serves.

Caveats to carry into the brief: (a) the image's default CMD exits rc 3 without
`--ulimit memlock=...` and `--ulimit rtprio=...` ("host contract refused: locking audio-path memory was
denied ... 67108864 bytes were wanted"), by design, same as `deploy/Dockerfile`; (b) not
byte-reproducible yet: config `history[].created` for the two umoci steps is wall time
(`2026-09-29T20:41:36Z`), as the research says; (c) not re-built by me (artifact reuse), and the
binary's source commit is not recorded in the image (no revision label).

## Claim 6: Playwright Chromium runs headless with LD_LIBRARY_PATH=/cache/opt/chromium-libs/lib

**PARTLY (the browser launches, but it renders no text: the research's screenshots are blank).**

```
ldd headless_shell: 14 "not found" without the env, 0 with it              (matches)
headless_shell ... 'data:text/html,<h1>x</h1>' (no env)  -> libglib-2.0.so.0: cannot open shared object file  rc=127
LD_LIBRARY_PATH=... headless_shell --headless --no-sandbox --disable-gpu --screenshot=hs1.png 'data:text/html,<h1>chorus</h1>'
   -> "2726 bytes written"  rc=0 0.7 s
PLAYWRIGHT_BROWSERS_PATH=/cache/playwright LD_LIBRARY_PATH=... uv run --with playwright==1.56.0 python pwv.py
   -> version 141.0.7390.37, rc=0 8.5 s, 4253-byte PNG
```

But I opened the PNGs: `hs1.png` (2726 B), my `shot-verify.png` (4253 B) and the research's own
`scratch/shot-pw.png` (4253 B) are **all plain white; the `<h1>` text is not drawn**. Debug page with a
red 200x100 div plus `<h1 id=h>verify</h1>`: the red box renders, the h1 has `textContent 'verify'`
but `innerText ''` and bounding rect `height 0`. Cause: **the container has no fonts and no
fontconfig config** (`/etc/fonts` and `/usr/share/fonts` absent; the conda env has no fonts). Any
Playwright test that uses `toBeVisible`, `innerText`, text layout, or screenshots of text would fail
or be meaningless.

Fix verified: `micromamba create -p scratch/verify-fonts -c conda-forge fontconfig
fonts-conda-ecosystem` (rc 0, 1.2 s from the micromamba cache, 17 MB), then add
`FONTCONFIG_FILE=<p>/etc/fonts/fonts.conf FONTCONFIG_PATH=<p>/etc/fonts` -> `innerText 'verify'`, h1
height 38, and the screenshot shows the text. The brief's Chromium recipe must include the fonts
env (or install fonts into `/cache/opt/chromium-libs`) and should never accept a PNG's byte count as
proof of rendering. Not tried: full `chrome` binary (needs cups/cairo/pango too, as the research says).

## Claim 8: shopkit `make ci` passes in ~3 min with SHOPKIT_WORKERS=2 given a home sibling; identity passes vacuously

Code reading (`scratch/shopkit` at `fb1d536 release: v1.18.0`):

- `Makefile:48-52` `ci:` = `lock-check sync lint typecheck imports root-tests rules identity
  test-affected`, under `SHELL := bash`, `.SHELLFLAGS := -eu -o pipefail -c`, `.NOTPARALLEL:` (a failing
  step does stop the gate; the trailing "PASS" echo cannot mask a failure). `WORKERS` from
  `shopkit env --workers`; `packages/core/src/shopkit_core/workers.py:104` reads `SHOPKIT_WORKERS`
  first. `identity:` = `uv run --frozen --all-packages shopkit records identity-scan`.
- `packages/records/src/shopkit_records/identity.py`: `find_terms` -> `home_repo` -> `RepoConfig.sibling`
  (`shopkit_core/repo.py:113-128`): `SHOPKIT_REPO_HOME` overrides `[repos] home = "../home"`; a declared
  but absent directory raises `SiblingMissing` (non-zero). `load_terms` on a missing
  `private/scrub-strings.txt` returns `Terms(path=None, reason="no terms file at ...")`, and the scan
  returns `{"ok": True, "status": "skipped: no terms"}`; the CLI exits `0 if result["ok"] else 1`.
  Module docstring line 49: "Without terms the whole scan, images included, is skipped", so the built-in
  `lan-ipv4` (RFC 1918) check is skipped too, not only the private terms.
- Consequence the research missed: the private NSchatz/home clone is **not needed**. Any existing
  directory works: `SHOPKIT_REPO_HOME=<empty dir>` satisfies the sibling check and the scan skips.
  That is exactly how vacuous the step is here.

Re-run (time allowed): `SHOPKIT_REPO_HOME=<empty scratchpad dir> SHOPKIT_WORKERS=2 flock ... timeout 1500
make ci` in `scratch/shopkit`:

```
All checks passed! / 1117 files already formatted
0 errors, 578 warnings, 0 informations            (pyright)
Contracts: 3 kept, 0 broken.
188 passed in 21.30s
25 generated skill(s) current
identity-scan: skipped: no terms (no terms file at .../scratchpad/emptyhome/private/scrub-strings.txt)
affected packages: (none)
make ci: PASS in 171 s (workers 2, base origin/main)      ### rc=0 wall=171.4s
```

**CONFIRMED** (171 s vs the research's 175.6 s; warm venv). Corrections: (1) no private home clone is
needed, any existing dir passes; (2) "vacuous" also covers the built-in LAN-address check; (3) the
clone is stale: it is `fb1d536` (v1.18.0), while NSchatz/shopkit `main` is now `64740a2` "release:
v1.19.0 ..." (committed 2026-09-29T23:25Z, after the research). The ~3 min figure is for a PR that
changes no package (`affected packages: (none)`); a PR touching a package adds its `test-pkg`.

## Claim 9: Svelte 5 + Vite 8 + vitest + happy-dom ~78 packages, builds < 3 s; vite-plugin-pwa ~348

**PARTLY: the counts reproduce under the research's definition, but that definition undercounts the
lockfile, and the 348 figure was measured on the Vite 7 stack.**

Directory entries in `node_modules/.pnpm` (research's method) for `scratch/pwa/*`:

```
svelte8-min  (svelte@5 vite@8 plugin-svelte@7)                         39
svelte8-test (+ vitest@5 happy-dom @testing-library/svelte)             78
svelte-pwa   (svelte@5 vite@7 plugin-svelte@6 vite-plugin-pwa)         348
```

Packages listed in each `pnpm-lock.yaml` (what a supply-chain review has to cover): **64 / 103 / 398**.
For svelte8-test the extra 25 are other-platform native binaries (14 `@rolldown/binding-*`, 10
`lightningcss-*`, `fsevents`) that are locked but only installed on their platform. Quote "78 installed
on linux-x64, 103 locked". The 348 PWA variant is Vite 7 (vite-plugin-pwa 1.3.0 does accept
`vite ^8.0.0` per its registry peerDependencies), so "348 vs 39" compares a Vite 7 PWA stack with a
Vite 8 base; the Workbox delta is about 311 installed packages either way (94 of them `@babel/*`,
16 `workbox-*`; workbox-build 7.4.1 has 37 direct deps). The conclusion (skip Workbox) stands.

Build and test in a copy of `svelte8-test` plus the research's App/Zone/state sources and Zone.test.js
(`scratch/pwa/verify-s8`), node 22.23.3, pnpm 10.34.5, under the lock:

```
vite build (via mise exec + pnpm exec)   "built in 387ms"  wall 1.6 s ; again "806ms" wall 1.9 s
./node_modules/.bin/vite build           "built in 511ms"  wall 1.4 s
dist/assets/index-Dmo3qk3k.js   35.68 kB | gzip: 14.10 kB   (same hash as the research's svelte8-min build)
vitest run (vitest 5.0.2)  "Test Files 1 passed (1)" "Tests 1 passed (1)" Duration 2.32s  wall 4.7 s
```

Build < 3 s: **CONFIRMED**. Two small corrections: the gzip size is 14.10 kB as Vite prints it (the
research wrote 13.9 KB); and the research's component test ran only on the Vite 7 / vitest 3 stack.
I ran the same test on the recommended Vite 8 / vitest 5 stack: it passes, so that gap is now closed.
Bonus evidence for the research's ASSUMED "Vite builds are deterministic": a build in a different
directory produced the byte-identical hashed name `index-Dmo3qk3k.js`.

## Other findings on the way

- `find` is also a broken interactive shell function (prints "claude native binary not installed"),
  not only `grep` as the research says. Use `command find` / `command grep` / `rg` in agent shells.
- The research's claim-1 "incremental one is seconds" and my 0.2-1.6 s both exclude link and
  `elf2image`, which no one has run yet.
- `firmware` compiles with `-std=gnu17` under ESP-IDF (host build is strict C11): the source of the
  `finite` clash, as the research says.
- The research-built OCI image carries no `org.opencontainers.image.revision`; add it so an image names
  the chorus commit it was built from.

## Overall: what the brief should DROP or CORRECT

DROP: nothing outright. Every measured number the research gives reproduced within noise.

CORRECT:
1. **Chromium (claim 6):** "works with caveat" is wrong as stated. It launches but draws no text (no
   fonts, no fontconfig). The recipe must add conda-forge `fontconfig fonts-conda-ecosystem` and set
   `FONTCONFIG_FILE`/`FONTCONFIG_PATH` (verified: text renders, `innerText` correct). A browser smoke
   test must assert rendered text, not a PNG byte count.
2. **ESP32-P4 rev v3 (claim 2):** replace "rev v3.0 first appears in v5.4.x" with "rev v3.0 needs v5.4.4+
   or v5.5.2+; rev v3.1 needs v5.5.3+ or v6.0+". A P4 choice forces the K51 IDF upgrade (the research's
   conclusion stands). v5.3 EOL 2027-01-25 and v6.1 stable are confirmed from Espressif's own
   `idf_versions.js`.
3. **npm package counts (claim 9):** say "78 installed on linux-x64 / 103 in the lockfile" (39/64 for
   build-only), and "vite-plugin-pwa adds about 311 packages (348 total measured on the Vite 7 stack,
   398 locked)". The Vite 8 + vitest 5 component test is now verified (it was only run on vitest 3).
   Gzip skeleton is 14.10 kB, not 13.9 KB.
4. **shopkit gate (claim 8):** the identity step needs only `SHOPKIT_REPO_HOME=<any existing dir>`,
   not a clone of the private home repo; the step then skips entirely (terms and LAN-IP checks). Record
   that the gate was measured at shopkit `fb1d536` and upstream is now `64740a2` (v1.19.0).
5. **Dockerfile (claim 10):** add to the brief that `deploy/Dockerfile` at origin/main cannot build
   chorus-server at all: no `COPY docs` for `include_str!("../../../docs/control-page.md")`, and the
   build stage is `rust:1.74` while the workspace needs 1.83+. Any goal that touches the image or
   the MSRV has to fix both.
6. **Clippy/fmt counts (claim 4):** keep, but tag them "on rustfmt 1.9.0 / clippy 0.1.98 (1.98.0)";
   the clippy set changes with the pinned toolchain.

## Housekeeping

- `git -C /workspace fetch -q origin` was run once at the start (remote-tracking refs only;
  origin/main stayed `1ea9f2a`); no tracked file, branch or index in `/workspace` was changed.
- Worktree `scratch/verify-wt` removed with `git worktree remove --force` + `git worktree prune`.
- Removed `scratch/verify-idf-build` (126 MB, bound to the removed worktree path).
- Left in place: `scratch/verify-fonts` (17 MB, the working fonts env for claim 6), `scratch/pwa/verify-s8`
  (Vite 8 + vitest 5 test copy), logs `scratch/logs/v-*`, scripts `scratch/verify-{idf,msrv,lint}.sh`.
