#!/usr/bin/env bash
# `make gate`: the one check a change has to pass before it merges.
#
# The cheap steps (the conventions checks, fmt and clippy) all run, in order, even
# after one fails, so a red run lists every broken one. If any of them failed the
# gate stops there, names them, and lists the steps it did not run: a change that
# fails a lint is red whatever its tests say, and the tests and builds after it
# are most of the gate's time. Once the cheap steps pass, every later step runs,
# in order, even after one fails. Each step prints its exit code and its
# wall-clock; the run ends with the total and PASS or FAIL. A step's full output
# goes to its own log under CHORUS_GATE_LOG (default target/gate/), and the last
# lines of a failing step are printed beside it.
#
# The workspace tests run under cargo-nextest (pinned in mise.toml, configured in
# .config/nextest.toml): every test in its own process, several at once, the
# wall-clock-sensitive ones one at a time, no retries. The documentation tests,
# which nextest does not run, run after them with `cargo test --doc`.
# docs/decisions/0112-a-faster-gate-with-the-same-checks.md says why.
#
# The gate never takes chorus-heavy.lock itself; callers do (the merge rule in the retired
# program's brief, section 0.3: https://github.com/NSchatz/chorus/blob/535ed2816b5735153ac81c52a235024aa806f2b5/.claude/goals/2026-09-chorus.md).
#
#   make gate             # everything
#   make gate-fast        # the conventions checks only, for docs-only changes
#   make gate-changed     # a pull request's gate: the conventions checks, then only the
#                         # crates, trees and tests the change touches (tools/changed.sh),
#                         # two minutes the target; `make gate` runs nightly on main
#   make tier-fast        # the conventions checks, fmt, clippy, the Home Assistant
#                         # integration's tests, the web app's tests and rebuild
#                         # and the workspace tests: the fast
#                         # tier run on every PR (the goals program's gate tiers, W15, W36);
#                         # `make tier-full` is `make gate`, run at goal end and nightly on main
#
# Environment:
#   CHORUS_IDF_ENV            a script that puts ESP-IDF on PATH and sets IDF_PATH,
#                             sourced when IDF_PATH is unset (Makefile default:
#                             the rootless install this repository is built with)
#   CHORUS_CCACHE_DIR         where ccache keeps its cache (default /cache/ccache/chorus)
#   (the image step pulls its digest-pinned base once, then reads it from a cache)
#   CHORUS_SOLOIST_IMAGE_CACHE  where the soloist-image step keeps its base and the pinned
#                             Debian packages (default /cache/chorus-soloist-image); warm, the
#                             step needs no network
#   CHORUS_GATE_OUTAGE_SECONDS  the outage-of-minutes run's length in the gate
#                             (default 30; `make firmware-check` runs the committed 130)
#   IDF_PY_BUILD_JOBS         idf.py's ninja job count; passed through explicitly
#   CHORUS_ALSA_PREFIX        a rootless alsa-lib for the alsa-null step when the
#                             system has no libasound.so.2 (default /cache/opt/chorus-alsa;
#                             SKIPPED under CI when neither exists)
#   UV_PROJECT_ENVIRONMENT    where the ha-test and ha-hassfest steps keep the Home
#                             Assistant integration's virtual environment (default
#                             /cache/venvs/chorus-ha; never inside the repository); they fetch
#                             the pinned Python and the locked packages once, then need no
#                             network (a missing uv fails the step, under CI too)
#   CHORUS_HA_CORE            where the ha-hassfest step keeps its checkout of Home Assistant
#                             core at the pinned tag (default /cache/chorus-ha-core); cloned
#                             once, then no network
#   (the web-test and web-build steps install web/'s locked packages into web/node_modules
#   with the pinned node and pnpm of mise.toml; the first run fetches them once, into pnpm's
#   store; a missing tool fails the step, under CI too)
#   PLAYWRIGHT_BROWSERS_PATH, CHORUS_CHROMIUM_LIBS, CHORUS_CHROMIUM_FONTS
#                             where the web-smoke step finds the pinned Chromium build and,
#                             on a host with no root, its libraries and fonts (tools/web.sh
#                             says each default; `bash tools/web.sh smoke-install` downloads
#                             the build once, and a missing one fails the step)
#   NEXTEST_TEST_THREADS     how many tests run at once (default: .config/nextest.toml)
#   CHORUS_FIRMWARE_JOBS      make's job count for the firmware host build (default 8)

# Every step function is invoked by name through step(), which shellcheck
# cannot see.
# shellcheck disable=SC2329

set -u
cd "$(dirname "$0")/.." || exit 2
ROOT="$(pwd)"

MODE="${1:-full}"

# Every cargo build in the gate, and every one its steps and tests start, is a
# one-shot build: incremental compilation would write gigabytes of small cache
# files (2.7 GB in 23,730 files in one goal worktree, measured 2026-10-03) for a
# rebuild that never comes. Exported so the nested builds agree with the gate's
# and reuse its artifacts. The code built is the same either way.
export CARGO_INCREMENTAL=0

# The pinned test runner (mise.toml), run through mise as tools/endpoint-package.sh
# runs its pinned tools.
NEXTEST_VERSION="$(sed -n 's|^"aqua:nextest-rs/nextest/cargo-nextest" = "\(.*\)"$|\1|p' mise.toml)"

LOG="${CHORUS_GATE_LOG:-${CARGO_TARGET_DIR:-$ROOT/target}/gate}"
mkdir -p "$LOG"
: > "$LOG/summary.txt"
FAILED=()
T0=$(date +%s.%N)

elapsed() { awk -v a="$1" -v b="$2" 'BEGIN{printf "%.1f", b-a}'; }

step() {
    local name="$1"
    shift
    local s e rc
    s=$(date +%s.%N)
    ( "$@" ) > "$LOG/$name.log" 2>&1
    rc=$?
    e=$(date +%s.%N)
    printf 'gate: %-22s rc=%-3s %7ss\n' "$name" "$rc" "$(elapsed "$s" "$e")" | tee -a "$LOG/summary.txt"
    if [ "$rc" -ne 0 ]; then
        FAILED+=("$name")
        tail -n 15 "$LOG/$name.log" | sed "s/^/  $name| /"
    fi
}

# The workspace tests: every test under nextest, then the documentation tests,
# which nextest does not run. Both run even when the first fails, so a red run
# names every failing test.
workspace_tests() {
    local rc=0
    if [ -z "$NEXTEST_VERSION" ]; then
        echo "mise.toml pins no aqua:nextest-rs/nextest/cargo-nextest; the gate runs the workspace tests with it"
        return 2
    fi
    MISE_TRUSTED_CONFIG_PATHS="$ROOT" \
        mise exec "aqua:nextest-rs/nextest/cargo-nextest@$NEXTEST_VERSION" -- \
        cargo nextest run --workspace --locked || rc=1
    cargo test --doc --workspace --locked || rc=1
    return "$rc"
}

# After the cheap steps: stop here when any of them failed, naming them and the
# steps that will not run.
stop_if_cheap_steps_failed() {
    [ "${#FAILED[@]}" -eq 0 ] && return 0
    local T1
    T1=$(date +%s.%N)
    printf 'gate: STOPPED after the cheap steps: %s failed; not run: %s\n' "${FAILED[*]}" "$*" |
        tee -a "$LOG/summary.txt"
    printf 'gate: FAIL (%s), wall-clock %ss\n' "${FAILED[*]}" "$(elapsed "$T0" "$T1")" | tee -a "$LOG/summary.txt"
    exit 1
}

# --- the conventions checks (also the whole of gate-fast) -----------------------

# docs/conventions.md: every rule names its check, and every check is a
# tools/conventions/check-*.sh script run here as its own timed step (the
# conventions table check holds the two lists to each other).
conventions() {
    local c
    for c in tools/conventions/check-*.sh; do
        c="${c##*/check-}"
        step "${c%.sh}" bash "tools/conventions/check-${c}"
    done
}

# --- the firmware image ------------------------------------------------------

# ESP-IDF and ccache, for every step that builds an image.
firmware_env() {
    if [ -z "${IDF_PATH:-}" ] && [ -n "${CHORUS_IDF_ENV:-}" ] && [ -f "$CHORUS_IDF_ENV" ]; then
        # shellcheck disable=SC1090
        . "$CHORUS_IDF_ENV" > "$LOG/idf-env.log" 2>&1
    fi
    # A mise shim can be on PATH without a version behind it, so what is asked is
    # whether ccache runs, not whether the name resolves.
    if ! ccache --version > /dev/null 2>&1 && command -v mise > /dev/null 2>&1; then
        PATH="$(mise where github:ccache/ccache@4.14.1 2>/dev/null):$PATH"
    fi
    if ! ccache --version > /dev/null 2>&1; then
        echo "ccache is not on PATH; the gate builds the firmware through it"
        echo "install it rootless: mise install github:ccache/ccache@4.14.1, and put \$(mise where github:ccache/ccache@4.14.1) on PATH"
        return 1
    fi
    export CCACHE_DIR="${CHORUS_CCACHE_DIR:-/cache/ccache/chorus}"
    # Every checkout builds in its own directory, so absolute paths differ
    # between worktrees and would make every compile a miss. ccache rewrites
    # paths under the checkout as relative ones and leaves the working
    # directory out of the hash, so a new worktree's first build hits the cache.
    export CCACHE_BASEDIR="$ROOT"
    export CCACHE_NOHASHDIR=true
    mkdir -p "$CCACHE_DIR"
    echo "ccache: $(ccache --version | head -n 1), CCACHE_DIR=$CCACHE_DIR"
}

# One image per board profile of the one P1 target (esp32s3): the wired
# classes' W5500 default, the compact speakers' Wi-Fi tier, and the emulator's
# board (goal 14). $1 is the board profile (firmware/boards/), each built in
# its own persistent directory and each held to the same safety scans.
firmware_idf() {
    local profile="$1"
    firmware_env || return 1
    ccache --zero-stats > /dev/null
    CHORUS_IDF_CCACHE=1 \
        CHORUS_BOARD_PROFILE="$profile" \
        CHORUS_IMAGE_OUT="$ROOT/firmware/build/gate-esp32s3-$profile" \
        IDF_PY_BUILD_JOBS="${IDF_PY_BUILD_JOBS:-2}" \
        bash tools/firmware-image.sh || return 1
    ccache --show-stats | head -n 8
    # The last lines of the step's log: what was built, for which board, on
    # which link.
    echo "firmware image: target esp32s3, board profile $profile"
}

# The emulator's image, booted (goal 14): tools/qemu-boot-run.sh on the
# directory the firmware-esp32s3-qemu step just built, so the image is not
# built twice. It refuses by name when the pinned emulator is absent
# (bash tools/qemu-env.sh install); it is never skipped. Source: simulation.
firmware_qemu_boot() {
    firmware_env || return 1
    CHORUS_IDF_CCACHE=1 \
        CHORUS_IMAGE_OUT="$ROOT/firmware/build/gate-esp32s3-qemu-s3-openeth" \
        CHORUS_QEMU_OUT="$LOG/qemu-boot" \
        IDF_PY_BUILD_JOBS="${IDF_PY_BUILD_JOBS:-2}" \
        bash tools/qemu-boot-run.sh
}

# The firmware update under the emulator (goal 14, line B): tools/ota-qemu-run.sh,
# image A from the directory the firmware-esp32s3-qemu step built, GOOD and BAD
# in directories of their own. Refuses by name like qemu-boot. Source: simulation.
firmware_ota_qemu() {
    firmware_env || return 1
    CHORUS_IDF_CCACHE=1 \
        CHORUS_IMAGE_OUT="$ROOT/firmware/build/gate-esp32s3-qemu-s3-openeth" \
        CHORUS_QEMU_OUT="$LOG/ota-qemu" \
        IDF_PY_BUILD_JOBS="${IDF_PY_BUILD_JOBS:-2}" \
        bash tools/ota-qemu-run.sh
}

# The profiles the image steps built and scanned in THIS run (each step's log
# ends with `firmware image: target esp32s3, board profile <p>` and carries the
# guard's `safety scan: <p>: ... pass` line) against firmware/boards/*.conf.
# Fails naming the profile when one was not built, did not pass its scan, or
# was built without being a committed profile.
profiles_all_built() {
    local committed built scanned p rc=0
    committed="$(for p in firmware/boards/*.conf; do p="${p##*/}"; echo "${p%.conf}"; done | sort)"
    built="$(sed -n 's/^firmware image: target esp32s3, board profile \(.*\)$/\1/p' \
        "$LOG"/firmware-esp32s3-*.log 2>/dev/null | sort -u)"
    scanned="$(sed -n 's/^safety scan: \([^:]*\): no eFuse write, no Secure Boot, no Flash Encryption, no anti-rollback: pass$/\1/p' \
        "$LOG"/firmware-esp32s3-*.log 2>/dev/null | sort -u)"
    for p in $committed; do
        if ! printf '%s\n' "$built" | grep -qx -- "$p"; then
            echo "FAIL board-profile-not-built :: firmware/boards/$p.conf has no image step in tools/gate.sh that built it in this run"
            rc=1
        elif ! printf '%s\n' "$scanned" | grep -qx -- "$p"; then
            echo "FAIL board-profile-not-scanned :: the image for $p did not pass tools/firmware-image-guard.sh in this run"
            rc=1
        else
            echo "safety scan: $p: no eFuse write, no Secure Boot, no Flash Encryption, no anti-rollback: pass"
        fi
    done
    for p in $built; do
        if ! printf '%s\n' "$committed" | grep -qx -- "$p"; then
            echo "FAIL image-built-for-no-committed-profile :: $p is not one of firmware/boards/*.conf"
            rc=1
        fi
    done
    [ "$rc" -eq 0 ] && echo "pass firmware-profiles: the images built and scanned are exactly firmware/boards/*.conf: $(echo "$committed" | tr '\n' ' ')"
    return "$rc"
}

# A Home Assistant step (ha-test, ha-hassfest, ha-live) is green only when it ran: its make
# target exits 0 AND its output carries the step's own `<name>: PASS` line and no SKIPPED
# line. A run that skipped (no uv, no built server, a narrowed test run) is red
# (docs/decisions/0142-the-home-assistant-gate-steps-run-or-fail.md). HA_TEST_ARGS is passed
# empty so a value in the environment cannot narrow the gate's run.
ha_step() {
    local name="$1" out rc=0
    out="$(mktemp "${TMPDIR:-/tmp}/chorus-gate-$name.XXXXXX")"
    make --no-print-directory "$name" HA_TEST_ARGS= > "$out" 2>&1 || rc=$?
    cat "$out"
    if [ "$rc" -eq 0 ]; then
        # (pytest's own `SKIPPED [1] <file>` summary lines are a test's, not the step's.)
        if grep -q -e '^SKIPPED:' -e "^$name: SKIPPED" "$out"; then
            echo "FAIL: the $name step printed SKIPPED; in the gate it runs or it is red"
            rc=1
        elif ! grep -q "^$name: PASS" "$out"; then
            echo "FAIL: the $name step exited 0 without its '$name: PASS' line; it did not run whole"
            rc=1
        fi
    fi
    rm -f "$out"
    return "$rc"
}
# The live-server test against the chorus-server the build step made.
ha_live() {
    CHORUS_SERVER_BIN="${CARGO_TARGET_DIR:-$ROOT/target}/debug/chorus-server" ha_step ha-live
}
# The step's verdict line into the summary.
ha_summary() {
    sed -n -e "s/^$1: \\(PASS.*\\)/gate: $1 \\1/p" -e "s/^\\(FAIL: .*\\)/gate: $1 \\1/p" "$LOG/$1.log" | tee -a "$LOG/summary.txt"
}

# A web step (web-test, web-build, web-live, web-smoke) is green only when it ran, like a Home Assistant
# step: its make target exits 0 and prints its own `<name>: PASS` line and no SKIPPED line
# (tools/web.sh).
web_step() {
    local name="$1" out rc=0
    out="$(mktemp "${TMPDIR:-/tmp}/chorus-gate-$name.XXXXXX")"
    if [ -n "${WEB_STEP_SESSION:-}" ]; then
        # --wait: setsid forks when its caller leads a process group, and the step's exit
        # status is the make's either way. A make ended by a signal has none: setsid says so
        # and exits non-zero.
        setsid --wait make --no-print-directory "$name" > "$out" 2>&1 || rc=$?
    else
        make --no-print-directory "$name" > "$out" 2>&1 || rc=$?
    fi
    cat "$out"
    if [ "$rc" -eq 0 ]; then
        if grep -q "^$name: SKIPPED" "$out"; then
            echo "FAIL: the $name step printed SKIPPED; in the gate it runs or it is red"
            rc=1
        elif ! grep -q "^$name: PASS" "$out"; then
            echo "FAIL: the $name step exited 0 without its '$name: PASS' line; it did not run whole"
            rc=1
        fi
    fi
    rm -f "$out"
    return "$rc"
}
# The app's live test, against the chorus-server the build step made. It runs in a session
# (and so a process group) of its own, WEB_STEP_SESSION: three CI runs (one of them main at
# 4449bf7) ended inside this step with SIGTERM delivered to every process of the job at once,
# make and the runner's worker included, so the job stopped with exit 143, no failed step and
# no logs. A signal sent to the step's process group now ends the step alone: it is red with
# what it had printed, the gate goes on and the logs are kept.
web_live() {
    WEB_STEP_SESSION=1 CHORUS_SERVER_BIN="${CARGO_TARGET_DIR:-$ROOT/target}/debug/chorus-server" web_step web-live
}
# The one browser test, against the chorus-server the build step made.
web_smoke() {
    CHORUS_SERVER_BIN="${CARGO_TARGET_DIR:-$ROOT/target}/debug/chorus-server" web_step web-smoke
}
# The app's output is committed (chorus-server embeds web/dist with no node in the Rust build),
# so it must be what web/src builds: rebuild it, then fail on any difference from the index, a
# changed file or one the build added or removed.
web_build() {
    local changed
    web_step web-build || return 1
    changed="$(git status --porcelain -- web/dist)"
    if [ -n "$changed" ]; then
        printf '%s\n' "$changed"
        git --no-pager diff --stat -- web/dist
        echo "FAIL: web/dist is not what web/src builds; run \`make web-build\` and commit web/dist with the change"
        return 1
    fi
    echo "web-build: web/dist is byte for byte what web/src builds"
}

echo "gate: $MODE, $(git rev-parse --short HEAD 2>/dev/null), $(cargo --version), IDF_PY_BUILD_JOBS=${IDF_PY_BUILD_JOBS:-2}" | tee -a "$LOG/summary.txt"

conventions

if [ "$MODE" = tier-fast ]; then
    # The steps of the full gate that check the Rust code itself, without the long runs (the
    # determinism, firmware, verify and image steps): cargo test builds what it tests, so the
    # separate build step is left to the full tier.
    step fmt              cargo fmt --all --check
    step clippy           cargo clippy --workspace --all-targets --locked -- -D warnings
    stop_if_cheap_steps_failed ha-test web-test web-build test
    # The Home Assistant integration's own lint, types and tests (goal 18): under a minute,
    # so before the workspace tests. (The conventions check of the same rule,
    # check-ha-integration.sh, ran above as step ha-integration.)
    step ha-test          ha_step ha-test
    ha_summary ha-test
    # The web app (web/): its unit tests under node with no browser, then its committed
    # output rebuilt and held to the index. Seconds each.
    step web-test         web_step web-test
    step web-build        web_build
    step test             workspace_tests
fi

if [ "$MODE" = changed ]; then
    # The pull request's gate: the conventions checks above, then only what the change
    # touches (tools/changed.sh): fmt, clippy and the tests of the touched crates and the
    # crates that depend on them, the Home Assistant integration's tests, the app's tests and
    # rebuild, and the firmware's host checks, each only when its tree changed. A docs-only
    # change builds and tests nothing. The full gate runs nightly on main
    # (.github/workflows/nightly.yml). The target is two minutes: over it, a warning, not a
    # failure.
    base="${CHORUS_GATE_BASE:-$(git merge-base origin/main HEAD 2> /dev/null || echo HEAD)}"
    plan="$(bash tools/changed.sh "$base")" || { echo "gate: tools/changed.sh failed"; exit 1; }
    mapfile -t crates < <(printf '%s\n' "$plan" | awk '$1 == "crate" { print $2 }')
    mapfile -t libs < <(printf '%s\n' "$plan" | awk '$1 == "crate" && $3 == "lib" { print $2 }')
    pkgs=()
    for c in "${crates[@]}"; do pkgs+=(-p "$c"); done
    docpkgs=()
    for c in "${libs[@]}"; do docpkgs+=(-p "$c"); done
    printf 'gate: changed since %s: %s\n' "$(git rev-parse --short "$base")" \
        "$(printf '%s\n' "$plan" | awk '{ print ($1 == "crate" ? $2 : $1) }' | paste -sd' ' -)" |
        tee -a "$LOG/summary.txt"
    [ -n "$plan" ] || echo "gate: nothing to build or test (no crate, web/, integrations/ or firmware/ change)" | tee -a "$LOG/summary.txt"
    if [ "${#crates[@]}" -gt 0 ]; then
        step fmt          cargo fmt --all --check
        step clippy       cargo clippy "${pkgs[@]}" --all-targets --locked -- -D warnings
    fi
    stop_if_cheap_steps_failed ha-test web-test web-build firmware-check test
    if printf '%s\n' "$plan" | grep -qx ha; then
        step ha-test      ha_step ha-test
        ha_summary ha-test
    fi
    if printf '%s\n' "$plan" | grep -qx web; then
        step web-test     web_step web-test
        step web-build    web_build
    fi
    if printf '%s\n' "$plan" | grep -qx firmware; then
        step firmware-check env CHORUS_OUTAGE_SECONDS="${CHORUS_GATE_OUTAGE_SECONDS:-30}" \
                              make --no-print-directory firmware-check
    fi
    if [ "${#crates[@]}" -gt 0 ]; then
        changed_tests() {
            local rc=0
            MISE_TRUSTED_CONFIG_PATHS="$ROOT" \
                mise exec "aqua:nextest-rs/nextest/cargo-nextest@$NEXTEST_VERSION" -- \
                cargo nextest run "${pkgs[@]}" --locked --no-tests=pass || rc=1
            if [ "${#docpkgs[@]}" -gt 0 ]; then
                cargo test --doc "${docpkgs[@]}" --locked || rc=1
            fi
            return "$rc"
        }
        step test         changed_tests
    fi
    T1=$(date +%s.%N)
    took="$(elapsed "$T0" "$T1")"
    if awk -v t="$took" 'BEGIN { exit !(t > 120) }'; then
        msg="the changed-only gate took ${took}s, over its two-minute target"
        if [ "${GITHUB_ACTIONS:-}" = true ]; then echo "::warning::$msg"; else echo "gate: WARNING: $msg"; fi
    fi
fi

if [ "$MODE" = full ]; then
    step fmt             cargo fmt --all --check
    step clippy           cargo clippy --workspace --all-targets --locked -- -D warnings
    stop_if_cheap_steps_failed ha-test ha-hassfest web-test web-build build ha-live web-live web-smoke test determinism firmware-check verify alsa-null \
        firmware-esp32s3-wired firmware-esp32s3-wifi firmware-esp32s3-qemu firmware-profiles \
        qemu-boot ota-qemu image soloist-image soloist-lists endpoint-packages
    # The Home Assistant integration (goal 18): its lint, types and tests under the pinned
    # harness, then Home Assistant's own hassfest over it from the pinned core checkout.
    # Both are under a minute warm, so they come before the builds.
    step ha-test          ha_step ha-test
    ha_summary ha-test
    step ha-hassfest      ha_step ha-hassfest
    ha_summary ha-hassfest
    # The web app (web/): its unit tests under node with no browser, then its committed
    # output rebuilt and held to the index. Seconds each, so before the builds.
    step web-test         web_step web-test
    step web-build        web_build
    step build            cargo build --workspace --all-targets --locked
    # The integration against the chorus-server the build step just made, on loopback: join,
    # unjoin, volume, group volume, sources and an announcement through Home Assistant's
    # service calls (goal 18; tests/test_live_server.py).
    step ha-live          ha_live
    ha_summary ha-live
    # The app's live test (web/live/): its elements and state layer in node, with no browser,
    # against the server just built: two rooms and a bonded set rendered, a volume change and
    # a mute through them read back from /api/state, another client's change followed. About 2 s.
    step web-live         web_live
    ha_summary web-live
    # The one browser test (web/smoke/): headless Chromium loads the app from the server
    # just built, through a fake login. About 5 s.
    step web-smoke        web_smoke
    ha_summary web-smoke
    step test             workspace_tests
    step determinism      make --no-print-directory verify-control-determinism
    step firmware-check   env CHORUS_OUTAGE_SECONDS="${CHORUS_GATE_OUTAGE_SECONDS:-30}" \
                              make --no-print-directory firmware-check
    step verify           make --no-print-directory verify
    step alsa-null        make --no-print-directory verify-alsa-null
    step firmware-esp32s3-wired firmware_idf brick-s3-wired
    step firmware-esp32s3-wifi  firmware_idf compact-s3-wifi
    step firmware-esp32s3-qemu  firmware_idf qemu-s3-openeth
    # Every board profile is built and scanned (goal 14): a profile added to
    # firmware/boards without an image step here would ship unbuilt and
    # unguarded, so the two sets are held to each other.
    step firmware-profiles profiles_all_built
    # What each image is, from its own build log, into the summary: the
    # target, the board and its ASSUMED status with the Needs item, the link,
    # and the safety scan's verdict for that target.
    for p in wired wifi qemu; do
        sed -n -e 's/^  board: *\(.*\)/  board \1/p' -e 's/^  needs item: *\(.*\)/  needs item \1/p' \
            -e 's/^chorus: image built: \(.*\)/  built: \1/p' \
            -e 's/^\(safety scan: .*\)/  \1/p' "$LOG/firmware-esp32s3-$p.log" |
            sed "s/^/gate: firmware-esp32s3-$p /" | tee -a "$LOG/summary.txt"
    done
    # The emulator's image, booted against a real server: adopted, and the same
    # speaker after a reboot. The run prints its own wall-clock.
    step qemu-boot        firmware_qemu_boot
    sed -n -e 's/^qemu-boot: \(.*\)/gate: qemu-boot \1/p' "$LOG/qemu-boot.log" | tee -a "$LOG/summary.txt"
    # A good image installed and confirmed, a bad one rolled back by the
    # bootloader, both by the server's explicit install (line B).
    step ota-qemu         firmware_ota_qemu
    sed -n -e 's/^ota-qemu: \(.*\)/gate: ota-qemu \1/p' "$LOG/ota-qemu.log" | tee -a "$LOG/summary.txt"
    step image            make --no-print-directory image
    # The Soloist receiver image (goal 17), built from pinned packages and tested
    # unpacked; then the listings of both images and the release, held to "no
    # Soloist file" (conventions rule 24). The image prints its own wall-clock.
    step soloist-image    make --no-print-directory soloist-image
    sed -n -e 's/^soloist-image: \(manifest digest .*\|wall-clock .*\)/gate: soloist-image \1/p' "$LOG/soloist-image.log" | tee -a "$LOG/summary.txt"
    step soloist-lists    make --no-print-directory soloist-lists
    # Both architectures: the two cross builds and their checks take about 45 s
    # cold and 9 s warm here (measured 2026-09-30), inside the gate's budget.
    step endpoint-packages make --no-print-directory endpoint-packages
fi

T1=$(date +%s.%N)
if [ "${#FAILED[@]}" -eq 0 ]; then
    printf 'gate: PASS, wall-clock %ss\n' "$(elapsed "$T0" "$T1")" | tee -a "$LOG/summary.txt"
    exit 0
fi
printf 'gate: FAIL (%s), wall-clock %ss\n' "${FAILED[*]}" "$(elapsed "$T0" "$T1")" | tee -a "$LOG/summary.txt"
exit 1
