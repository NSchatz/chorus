# Toolchain and environment research, chorus program (2026-09-29)

Hands-on, in this container: 2 CPUs (cgroup `cpu.max` 200000/100000), host `nproc` 56, no sudo, no
Docker daemon, `/cache` shared ext4. Every CPU-heavy command ran under
`flock /cache/locks/chorus-heavy.lock` with `-j2` / `CARGO_BUILD_JOBS=2` / `SHOPKIT_WORKERS=2`. Builds
ran in a detached worktree of `origin/main` = `1ea9f2a` at
`/cache/tmp/plan-2026-09-chorus/scratch/chorus-wt` (removed at the end); `/workspace` was not touched
beyond `git worktree add/remove/prune`. Timings are wall-clock from a `date +%s%N` wrapper
(`scratch/timeit.sh`); logs are under `scratch/logs/`. Web facts cite URL + "read 2026-09-29";
memory is marked ASSUMED.

## Container gotchas found on the way (affect every goal)

- **The interactive `grep` is a broken shell function** (it execs a missing claude binary and
  prints "claude native binary not installed"). Use `command grep` or `rg` in agent shells. `make`
  recipes and scripts are not affected (they do not load the snapshot function).
- **`~/.rustup` is on the container overlay, not `/cache`** (`df` shows `overlay`; only
  `/workspace`, `/cache` and `~/.claude` are mounts). mise's `rust@X` entries are symlinks to
  `/cache/cargo/bin` (the rustup proxies), but the toolchains themselves live in
  `/home/claude/.rustup/toolchains` and are lost with the container. 1.98.0 appears baked into the
  image (its symlink dates from Aug 22; ASSUMED). Goals that need another toolchain or the musl
  target should set `RUSTUP_HOME=/cache/rustup` (or reinstall: 18-52 s per minimal toolchain).
- `python3` on PATH is Debian's 3.13.5 (not 3.11 as the global notes say); mise has 3.12.13,
  3.13.15, 3.14.7.
- User namespaces are blocked: `unshare --user --map-root-user id` -> `unshare failed: Operation
  not permitted`; `/proc/self/status`: `CapEff 0`, `NoNewPrivs 1`, `Seccomp 2`. No
  `newuidmap`/`newgidmap`.
- Another agent's processes (a vite build, pyright) share this cgroup's 2 CPUs; the timing-sensitive
  chorus tests flaked twice under that kind of load (item 4).

---

## 1. ESP-IDF v5.3 (pin, install, build of `firmware/`)

**The pin.** `firmware/config/endpoint.conf:197`: `espidf_version = v5.3`. `tools/lib.sh:233-257`
(`require_espidf`) accepts any `idf.py --version` whose first line contains `v5.3`, so every v5.3.x
passes. Latest v5.3.x tag: **v5.3.6** (`git ls-remote --tags` of espressif/esp-idf; GitHub API:
published 2026-09-15). Installed that one.

**Commands (all rootless; reusable):**

```
git clone --depth 1 --branch v5.3.6 --recursive --shallow-submodules \
    https://github.com/espressif/esp-idf.git /cache/esp/esp-idf-v5.3.6        # rc 0, 90.7 s, 621 MB
export IDF_TOOLS_PATH=/cache/esp/tools
export PATH="$(mise where python@3.12.13)/bin:$PATH"
cd /cache/esp/esp-idf-v5.3.6 && ./install.sh esp32s3                          # rc 1, 60.6 s (see below)
python3 tools/idf_tools.py install cmake ninja                                # rc 0, 9.4 s (cmake 3.30.2, ninja 1.12.1)
python3 tools/idf_tools.py install xtensa-esp-elf-gdb xtensa-esp-elf riscv32-esp-elf esp32ulp-elf esp-rom-elfs   # rc 0, 0.5 s
python3 tools/idf_tools.py install-python-env                                 # rc 0, 43.0 s
# openocd needs libusb-1.0 (no apt): conda-forge copy, one symlink on LD_LIBRARY_PATH
MAMBA_ROOT_PREFIX=/cache/opt/micromamba mise exec github:mamba-org/micromamba-releases@2.9.0-0 -- \
    micromamba create -y -q -p /cache/opt/esp-libusb -c conda-forge libusb    # rc 0, 1.1 s
ln -s /cache/opt/esp-libusb/lib/libusb-1.0.so.0 /cache/esp/extra-libs/
LD_LIBRARY_PATH=/cache/esp/extra-libs python3 tools/idf_tools.py install openocd-esp32   # rc 0, 0.6 s
```

`install.sh` failure (why the steps above are split):

```
Installing openocd-esp32@v0.12.0-esp32-20260703
ERROR: tool openocd-esp32 version v0.12.0-esp32-20260703 is installed, but getting error: non-zero exit code (127) with message: .../openocd-esp32/bin/openocd: error while loading shared libraries: libusb-1.0.so.0: cannot open shared object file: No such file or directory
ERROR: Failed to check the tool while installed. Removing directory /cache/esp/tools/tools/openocd-esp32/v0.12.0-esp32-20260703
### idf-install-sh: rc=1 wall=60.6s
```

Without openocd, `export.sh` also refuses: `ERROR: tool openocd-esp32 has no installed versions.`
The env file that makes it work: **`/cache/esp/chorus-idf-env.sh`** (sets `IDF_TOOLS_PATH`, puts
mise python 3.12.13 first, adds `/cache/esp/extra-libs` to `LD_LIBRARY_PATH`), then
`. /cache/esp/esp-idf-v5.3.6/export.sh` (rc 0, 2.2 s; `idf.py --version` -> `ESP-IDF v5.3.6`).
Note: sourcing must happen in the calling shell, not through a wrapper script.

Installed toolchain: xtensa-esp-elf esp-13.2.0_20250707 (GCC 13.2.0), riscv32-esp-elf, esp32ulp-elf
2.38_20240113, xtensa gdb 17.1_20260402, openocd v0.12.0-esp32-20260703, esp-rom-elfs 20240305.
Total install wall about 3.5 min (clone 91 s + tools 61+9+0.5+1 s + python env 43 s).
**Disk: `/cache/esp` 3.0 GB** (source 621 MB, `tools/` 2.4 GB of which `dist/` archives 361 MB
are deletable, python env 90 MB).

**Build of `firmware/` (cold, under the lock):**

```
. /cache/esp/chorus-idf-env.sh && . /cache/esp/esp-idf-v5.3.6/export.sh
cd <wt>/firmware
idf.py -B <scratch>/idf-build set-target esp32s3     # rc 0, 8.1 s
idf.py -B <scratch>/idf-build build                  # rc 2, 144.3 s (1027 of 1044 steps)
ninja -C <scratch>/idf-build -k 0 -j2                # rc 1, 7.7 s: to list every error, not only the first
```

**Verdict: fails, with exactly two source errors; everything else compiles.** With `-k 0` the only
failed objects are `src/sync_sim.c.obj` and `src/monotonic.c.obj` (3 error lines total). The
ESP-IDF glue that nobody had compiled, **`main/app_main.c` and `main/esp_hal.c`, compiles clean
under `-Werror`**, as do conf, protocol, sync_rng, sync_servo, sync_scenario, i2s, amp, telemetry,
session, wifi, endpoint_config and the `endpoint.conf.S` blob; the bootloader builds
(`bootloader.bin`). Link was not reached. Output tail:

```
firmware/src/sync_sim.c:47:12: error: static declaration of 'finite' follows non-static declaration
   47 | static int finite(double value)
.../xtensa-esp-elf/include/math.h:118:12: note: previous declaration of 'finite' with type 'int(double)'
  118 | extern int finite (double);
firmware/src/monotonic.c:16:5: error: implicit declaration of function 'vTaskDelay' [-Werror=implicit-function-declaration]
   16 |     vTaskDelay(pdMS_TO_TICKS(ms));
firmware/src/monotonic.c:16:16: error: implicit declaration of function 'pdMS_TO_TICKS' [-Werror=implicit-function-declaration]
cc1: some warnings being treated as errors
ninja: build stopped: subcommand failed.
```

Causes (read, not fixed): newlib's `math.h` declares the BSD `finite()`, which the host build's
strict C11 hides; `monotonic.c`'s `CHORUS_TARGET_ESP32S3` branch includes only `esp_timer.h`, not
`freertos/FreeRTOS.h` / `freertos/task.h`. Both look like one-line fixes; the next build will show
whether link errors follow. Build dir: 126 MB. For the gate budget: a cold firmware build is about
2.5 min on 2 CPUs; an incremental one is seconds.

**esp32p4 in v5.3.** `idf.py --list-targets` (v5.3.6) lists `esp32 esp32s2 esp32c3 esp32s3 esp32c2
esp32c6 esp32h2 esp32p4` (`--preview` adds `linux esp32c5 esp32c61`). **But v5.3.x supports only P4
silicon up to rev v1.99**: `components/esp_hw_support/port/esp32p4/Kconfig.hw_support` in the local
v5.3.6 clone offers `ESP32P4_REV_MIN` choices v0.0/v0.1/v1.0 and `ESP32P4_REV_MAX_FULL` default 199.
Rev v3.0 first appears in v5.4.x (v5.4.4 adds `ESP32P4_SELECTS_REV_LESS_V3` and "Rev v3.0"), v3.1 in
v5.5.x (v5.5.5, v6.0.3, v6.1)
[https://raw.githubusercontent.com/espressif/esp-idf/{v5.4.4,v5.5.5,v6.0.3,v6.1}/components/esp_hw_support/port/esp32p4/Kconfig.hw_support,
read 2026-09-29]. COMPATIBILITY.md: "ESP32-P4 v1.0, v1.3: Supported since ESP-IDF v5.3"
[https://github.com/espressif/esp-idf/blob/master/COMPATIBILITY.md, read 2026-09-29]. The research
note's P4 PTP/PPS feature is "from silicon rev 3" (research-endpoint-hardware.md item 4), so **a P4
choice under K37 forces the K51 upgrade proposal** (at least v5.4, realistically v5.5+ or v6.x).

**Support period.** Each minor release: 30 months = 12 months Service + 18 months Maintenance
("only bugfixes for high severity issues or security issues"), then EOL
[https://github.com/espressif/esp-idf/blob/master/SUPPORT_POLICY.md, read 2026-09-29]. Release dates
from the GitHub API (`gh api repos/espressif/esp-idf/releases/tags/<t>`, read 2026-09-29): v5.3
2024-07-25, v5.4 2025-01-04, v5.5 2025-07-21, v6.0 2026-03-20, **v6.1 2026-08-27 = `releases/latest`**
(the `/en/stable/` docs are titled "ESP-IDF Programming Guide v6.1" [https://docs.espressif.com/projects/esp-idf/en/stable/esp32p4/api-reference/system/chip_revision.html, search-result title read 2026-09-29]). Computed from the policy: **v5.3 has been in
Maintenance since about 2025-07-25 and reaches EOL about 2027-01-25** (4 months from today); v5.4 EOL
about 2027-07; v5.5 about 2028-01; v6.0 about 2028-09; v6.1 in Service until about 2027-08, EOL about
2029-02. v5.3.6 (2026-09-15) is the current maintenance bugfix.

Espressif now recommends EIM (`eim install -i v5.3.6`) over `install.sh`
[https://github.com/espressif/esp-idf/releases/tag/v5.3.6, read 2026-09-29]; the legacy path above
works rootless and was not compared with EIM here.

---

## 2. Chromium / Playwright

`/cache/playwright` holds `chromium-1194`, `chromium_headless_shell-1194`, `ffmpeg-1011` (597 + 323
+ 5 MB), i.e. Playwright 1.56 (confirmed: Python `playwright==1.56.0` launched it without a download,
`browser.version` = 141.0.7390.37). chorus's `tools/ui` pins `@playwright/test` 1.56.1.

**Out of the box: fails, missing system libraries.** `ldd headless_shell`: 14 not found (libglib-2.0,
libgobject-2.0, libnspr4, libnss3, libnssutil3, libdbus-1, libgio-2.0, libatk-1.0, libatk-bridge-2.0,
libatspi, libXcomposite, libXdamage, libXrandr, libasound.so.2); full `chrome`: 18 (adds libsmime3,
libcups, libcairo, libpango).

```
headless_shell --headless --no-sandbox --screenshot=... 'data:text/html,<h1>hi</h1>'
.../headless_shell: error while loading shared libraries: libglib-2.0.so.0: cannot open shared object file
### hs-run: rc=127
uv run --python 3.12 --with playwright==1.56.0 python pw.py   -> rc 1, "<process did exit: exitCode=127>"
```

**Rootless fix, no apt: works.** conda-forge via mise's micromamba:

```
MAMBA_ROOT_PREFIX=/cache/opt/micromamba mise exec github:mamba-org/micromamba-releases@2.9.0-0 -- \
  micromamba create -y -q -p /cache/opt/chromium-libs -c conda-forge nss nspr glib dbus atk-1.0 \
  at-spi2-atk at-spi2-core xorg-libxcomposite xorg-libxdamage xorg-libxrandr alsa-lib
                                                  # rc 0, 40.9 s incl. micromamba download; 323 MB
LD_LIBRARY_PATH=/cache/opt/chromium-libs/lib headless_shell --headless --no-sandbox --disable-gpu \
  --screenshot=shot.png 'data:text/html,<h1>chorus</h1>'   # rc 0, 7.7 s, "2726 bytes written"
PLAYWRIGHT_BROWSERS_PATH=/cache/playwright LD_LIBRARY_PATH=/cache/opt/chromium-libs/lib \
  uv run -q --python 3.12 --with playwright==1.56.0 python pw.py   # rc 0, 3.8 s, 4253-byte PNG
```

With the env, `ldd headless_shell` has 0 missing. Verdict: **works with caveat** (needs the
conda-forge env on `LD_LIBRARY_PATH`; the full `chrome` binary would also need cups/cairo/pango, not
tried; the headless shell is what Playwright uses for headless). The conda env also provides
`libasound.so.2`, which survey §2 found missing for the ALSA `null` runs (not tried here).
`make verify-ui` itself (Node `@playwright/test`, pnpm) was not run; K18 retires it anyway.
(The `frontend-debugging` skill mentions a "browser image variant" of this container; ASSUMED it
bakes these libs, not checked.)

---

## 3. gitleaks and cargo-deny

```
mise exec aqua:gitleaks/gitleaks@8.30.1 -- gitleaks version            # 8.30.1 (latest in aqua), 1.0 s
gitleaks git . --redact=100 --no-banner --report-format json --report-path <tmp>   # rc 0, 1.9 s
  "23 commits scanned." "scanned ~3587240 bytes (3.59 MB) in 1.33s" "no leaks found"
gitleaks dir . --redact=100 ...                                            # rc 0, 2.0 s, "no leaks found"
```

**Findings: 0 (history) and 0 (tree).** Reports were redacted and deleted. Verdict: works, 2 s each;
cheap enough for every `make gate`. The private-term list for K27 can be a gitleaks custom rule file
kept outside the repo.

**cargo-deny:** did not compile it; the prebuilt is faster and pinned:
`mise exec aqua:EmbarkStudios/cargo-deny@0.20.2 -- cargo-deny --version` -> `cargo-deny 0.20.2`
(rc 0, 0.8 s; 0.20.2 = crates.io `max_stable_version`, read 2026-09-29 via
https://crates.io/api/v1/crates/cargo-deny). `cargo install --locked cargo-deny` was **not run**
(time went to items 1 and 6; expect a few minutes at -j2, ASSUMED). A licence check with
`deny.toml` `[licenses] allow = ["MIT", "Apache-2.0"]` and `private.ignore = false`:

```
cargo-deny check licenses        # rc 4, 0.7 s
error[unlicensed]: chorus-server = 0.1.0 is unlicensed      (x12, one per workspace crate)
warning[no-license-field]: license expression was not specified in manifest for crate ...
licenses FAILED
```

Verdict: works; it correctly fails today because no crate has a `license` field, and will pass once
K26 adds `license = "MIT OR Apache-2.0"`. Note: 0.20 has no `--config` on `check`; it reads
`deny.toml` from the workspace root.

---

## 4. Rust

mise offers 100 Rust versions (`mise ls-remote rust`), newest 1.98.1; installed here: 1.74.0 and
1.98.0 (`rustc 1.98.0 (88d9e12ae 2026-08-18)`).

| command (1.98.0, scratch worktree) | result | wall |
|---|---|---|
| `cargo fmt --all -- --check` | rc 1: **325 hunks in 83 files** (same as survey) | 0.7 s |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc 101 after the first crate; with `--keep-going`: 6 lint errors in 5 failing crates (dependents unchecked, so a lower bound) | 0.7 s / 3.3 s |
| `cargo clippy --workspace --all-targets` (count) | rc 0, **56 distinct warnings** (100 summed per target): 29 doc list item overindented, 7 constant assertion, 6 doc list item without indentation, 5 neg-cmp on partial ord, 2 single-pattern match, 2 unequal digit groups, ... | 12.1 s |
| `cargo build --workspace` (fresh `CARGO_TARGET_DIR`) | rc 0 | **23.3 s** |
| `cargo test --workspace` (run 1, while ESP-IDF pip ran) | rc 101, fail-fast: 52 binaries, 458 passed, **1 failed**: `server/regress_0043_f1` "the first server never came up" | 134.0 s |
| `cargo test --workspace --no-fail-fast` (run 2) | rc 101: 73 binaries, **542 passed, 1 failed**: `server/control_stalled_peer` "the stalled subscriber was never dropped after 108 commands ... HTTP/1.1 503"; regress_0043_f1 passed | 81.0 s |
| `cargo +1.80.0 check --locked --workspace --all-targets` | rc 101, 2x E0369 `ExitCode ==` (`client-linux`) | 4.2 s |
| `+1.81.0`, `+1.82.0` | rc 101, same 2x E0369 | 5.2 / 4.2 s |
| **`+1.83.0`** | **rc 0** | 9.1 s |
| `+1.84.0`, `+1.85.0`, `+1.90.0` | rc 0 | 11.7 / 8.0 / 8.8 s |

**Smallest compiling Rust: 1.83.0** (`cargo check`; the declared MSRV 1.74 is false). Toolchain
downloads: 18-52 s each (`rustup toolchain install <v> --profile minimal`, 464-567 MB each in
`~/.rustup`, ephemeral).

**Flakiness finding:** two different timing tests failed in two runs, each while other work shared
the 2 CPUs (run 1: the ESP-IDF pip install; run 2: another agent's pyright/vite and a `du` over
`/cache/venv`). regress_0043_f1 races on `free_port()` (bind :0, drop, reuse) plus a startup wait;
control_stalled_peer hit the busy-worker 503. This is the determinism class the gate already
stresses; under K29's "3 agents, 2 workers" it will recur unless the gate's test step is also
serialized or those tests are hardened. Not investigated further.

---

## 5. Rootless OCI image for chorus-server

**podman / buildah: fail.** mise's `github:podman-container-tools/podman@6.1.2` ships only
`podman-remote-static-linux_amd64` (a client): `podman info` -> `Error: unable to connect to Podman
socket: ... dial unix /scratch/storage-run-1000/podman/podman.sock: connect: no such file or
directory` (rc 125). A local rootless podman/buildah needs a user namespace, and
`unshare --user --map-root-user id` -> `unshare: unshare failed: Operation not permitted` (CapEff 0,
NoNewPrivs 1, seccomp filter; no `newuidmap`, `/etc/subuid` has only `node:100000:65536`). Not
attempted further: the failure is the kernel/seccomp policy, not a missing binary.

**Daemonless path: works end to end.**

```
mise exec rust@1.98.0 -- rustup target add x86_64-unknown-linux-musl          # 12.8 s, 221 MB
cargo build --locked --release -p chorus-server --target x86_64-unknown-linux-musl -j2   # rc 0, 15.0 s
  -> chorus-server 1.6 MB, chorus-rt-spin 0.6 MB, ldd: "statically linked"
crane digest gcr.io/distroless/static-debian12:nonroot
  -> sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab   (index digest, 2026-09-29)
crane pull --format=oci --platform linux/amd64 gcr.io/distroless/static-debian12:nonroot oci/base-static   # 2.4 s
# (add org.opencontainers.image.ref.name=base to index.json so umoci can address it)
umoci insert --rootless --image chorus:base --tag chorus-server stage /      # stage = usr/local/bin/{chorus-server,chorus-rt-spin}, etc/chorus/verification.conf
umoci config --image chorus:chorus-server --config.entrypoint /usr/local/bin/chorus-server \
  --config.cmd --listen --config.cmd 0.0.0.0:4010 --config.cmd --source --config.cmd tone \
  --config.cmd --serve-forever --config.exposedports 4010/tcp --config.user 65532:65532 --config.label ...
umoci rm --image chorus:base && umoci gc --layout chorus
tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -cf chorus-server-oci.tar oci-layout index.json blobs
                                                                           # all umoci steps: rc 0, 1.5 s
```

Result: `chorus-server-oci.tar`, **1.66 MB OCI archive** (distroless static base layers + one
922 kB layer). Checks: `umoci unpack --rootless` (0.6 s) gives the expected rootfs and
`config.json` (args = entrypoint + CMD, uid/gid 65532); the unpacked binary executes
(`chorus-server --help` -> "configuration refused: unknown argument '--help'", i.e. it runs);
`crane registry serve --address 127.0.0.1:35117` (a daemonless local registry) accepted
`crane push chorus 127.0.0.1:35117/chorus-server:dev` (0.1 s) and `crane validate --remote` ->
`PASS`, `crane config` shows Entrypoint, User 65532:65532, ExposedPorts 4010/tcp.
Tools: crane 0.22.1 (`aqua:google/go-containerregistry`), umoci 0.6.0 (`aqua:opencontainers/umoci`).
Caveats: the committed `deploy/Dockerfile` uses a glibc build on `debian:bookworm-slim` with a
created user; this path is a musl static binary on distroless (a K41 design choice to record). The
umoci layer timestamps are not yet reproducible (history `created` is wall time); pin with
`SOURCE_DATE_EPOCH`-style flags if byte-reproducible releases matter. `crane append -o` (docker
tarball) was not needed.

---

## 6. shopkit's gate (`make ci`)

`gh repo clone NSchatz/shopkit scratch/shopkit -- --depth 1` (18.1 s, 27 MB, `fb1d536 release:
v1.18.0 (devices goal 2) (#188)`). Gate per `CLAUDE.md`/`Makefile`: **`make ci`** = `lock-check sync
lint typecheck imports root-tests rules identity test-affected`; workers from `shopkit env --workers`,
overridable by **`SHOPKIT_WORKERS`**.

Run 1: `SHOPKIT_WORKERS=2 flock ... timeout 1800 make ci` -> **rc 2 after 231.6 s**: every step up
to `rules` passed (`uv lock --check`; ruff "All checks passed!"; "1117 files already formatted";
pyright "0 errors, 578 warnings"; import-linter "Contracts: 3 kept, 0 broken"; root tests "188 passed
in 36.60s"; "25 generated skill(s) current"), then:

```
uv run --frozen --all-packages shopkit records identity-scan
shopkit: sibling repo 'home' is declared (.../shopkit.toml [repos] home = '../home') but .../scratch/home does not exist
  fix: clone it there (gh repo clone NSchatz/home .../scratch/home) or point SHOPKIT_REPO_HOME at a checkout
make: *** [Makefile:49: ci] Error 2
```

With a sparse depth-1 clone of NSchatz/home as the sibling (4.2 s; private repo, readable with this
`gh` login), `make identity test-affected` -> rc 0, 1.1 s: `identity-scan: skipped: no terms (no
terms file at .../home/private/scrub-strings.txt)` and `affected packages: (none)`.
`private/` is gitignored in home (`.gitignore:4:/private`) and no `scrub-strings.txt` exists
anywhere under `/cache` (find, depth 6), so **the identity step passes vacuously here**.
Run 2 (full `make ci` with home present): see the addendum at the end.

Needs: uv 0.12.19 and Python 3.13 (uv-managed), node 22.23.3 (pyright runs on it, from shopkit's
`mise.toml`), build123d 0.13 / OCP 8.0.1 as PyPI wheels (OCCT comes inside the wheel; no system
OCCT); `.venv` 1.9 GB per checkout, per-package test venvs under `/cache/venv/shopkit/<hash>`.
Blender (`claude-gpu blender`), KiCad, Java etc. only for marked tests; `CI_MARKS = not net and not
gpu`, and a missing tool skips its tests. **Verdict: works with caveats**: about 4 min on 2 workers
for a no-package-change PR; a `shopkit-acoustics` PR adds `test-pkg P=acoustics` (a kernel-free
package's tests, small); it needs a `../home` (or `SHOPKIT_REPO_HOME`) checkout, and its identity
scan has no terms in this container. A chorus goal can run shopkit's gate here and self-merge under
shopkit's rules, provided the program accepts the vacuous identity scan or asks the owner for the terms
file (the Needs list). Nothing was pushed.

---

## 7. shellcheck / actionlint / yamllint

| tool | how | version | wall | on chorus |
|---|---|---|---|---|
| shellcheck | `mise exec aqua:koalaman/shellcheck@0.11.0 --` (0.10.0 also installed) | 0.11.0 | 3.1 s first use | 33 tracked `.sh`: rc 1, 86 findings (83 note, 3 warning; top SC2086 x29, SC1091 x28, SC2016 x9), 4.3 s |
| actionlint | `mise exec aqua:rhysd/actionlint@1.7.12 aqua:koalaman/shellcheck@0.11.0 --` | 1.7.12 | 0.2 s | rc 0 on `.github/workflows` |
| yamllint | `mise exec pipx:yamllint@1.38.0 --` or `uvx --from yamllint==1.38.0 yamllint` | 1.38.0 | 2.1 s / 2.0 s | `-d relaxed` on workflows: rc 0, 5 line-length warnings |

Gotcha: plain `actionlint` inside mise finds the `shellcheck` shim with no global version and fails
(`mise ERROR No version is set for shim: shellcheck`, actionlint rc 3). Name both tools in one
`mise exec`, or `mise use` them in a repo `mise.toml`. All three are rootless and fine for homelab PR
bodies; gitleaks as in item 3.

---

## Installs left in place (for the program)

| path | size | what |
|---|---|---|
| `/cache/esp/esp-idf-v5.3.6` | 621 MB | ESP-IDF v5.3.6 source, shallow, submodules |
| `/cache/esp/tools` | 2.4 GB | `IDF_TOOLS_PATH`: toolchains, cmake/ninja, openocd, python env (`dist/` 361 MB of archives is deletable) |
| `/cache/esp/extra-libs` | symlink | `libusb-1.0.so.0` for openocd |
| `/cache/esp/chorus-idf-env.sh` | 1 file | source before `export.sh` |
| `/cache/opt/esp-libusb` | 1.6 MB | conda-forge libusb |
| `/cache/opt/chromium-libs` | 323 MB | conda-forge libs for Playwright Chromium |
| `/cache/opt/micromamba` | 192 MB | micromamba package cache |
| mise: `aqua:opencontainers/umoci@0.6.0`, `aqua:EmbarkStudios/cargo-deny@0.20.2`, `github:mamba-org/micromamba-releases@2.9.0-0` | small | under `/cache/mise/installs` |
| `~/.rustup/toolchains/1.80.0, 1.81.0, 1.82.0, 1.83.0, 1.84.0, 1.85.0, 1.90.0` + musl target on 1.98.0 | 0.5 GB each | **ephemeral** (container overlay) |

## Addendum: shopkit `make ci`, run 2 (home clone present)

```
SHOPKIT_WORKERS=2 flock /cache/locks/chorus-heavy.lock timeout 1800 make ci
identity-scan: skipped: no terms (no terms file at .../scratch/home/private/scrub-strings.txt)
affected packages: (none)
make ci: PASS in 175 s (workers 2, base origin/main)
```

rc 0, 175.6 s (warm venv; run 1 was 231.6 s up to identity). pyright "0 errors, 578 warnings";
root tests "188 passed in 20.37s". One kernel-free package's tests as a size proxy for
`shopkit-acoustics`: `make test-pkg P=sizing` -> "178 passed in 1.89s", rc 0, 4.7 s (venv already
synced). So a shopkit PR that adds one small package costs about 3-4 min of gate on 2 workers.

## Cleanup

`git -C /workspace worktree remove --force .../scratch/chorus-wt` and `git -C /workspace worktree
prune`; the sparse NSchatz/home clone was deleted (it held no private terms, but it is a private
repo). The shopkit clone, build dirs and OCI outputs stay under
`/cache/tmp/plan-2026-09-chorus/scratch/` (shopkit `.venv` 1.9 GB, `target-198` 1.2 GB, `idf-build`
126 MB, `oci/` 14 MB with `chorus-server-oci.tar`); delete the directory when the plan is merged.
