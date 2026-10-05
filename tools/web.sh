#!/usr/bin/env bash
# `make web-test`, `make web-build` and `make web-smoke`: the app under web/ (Lit 3 elements
# bundled by esbuild; docs/decisions/0181-the-web-app-stack.md).
#
#   tools/web.sh test    the unit tests: `node --test` with happy-dom as the DOM, so components
#                        render and update in node. No browser is installed or started.
#   tools/web.sh build   web/src into web/dist, the committed output chorus-server embeds. The
#                        build is deterministic; the gate step `web-build` runs it and fails when
#                        web/dist then differs from what is committed.
#   tools/web.sh smoke   the one browser test (web/smoke/; docs/decisions/0183-the-one-browser-smoke-test.md):
#                        Playwright's headless Chromium loads the app from the chorus-server
#                        CHORUS_SERVER_BIN names, through a fake login, and asserts rendered
#                        text. Without CHORUS_SERVER_BIN it ends `web-smoke: SKIPPED` and exits
#                        0; under CI=true that is a failure, and the gate refuses the line.
#   tools/web.sh smoke-install [--with-deps]
#                        download the Chromium build the pinned @playwright/test names, once.
#                        --with-deps also installs its system libraries and fonts with the
#                        system's package manager (root; CI's runner). Never run by the other
#                        three: a run downloads no browser.
#
# The browser's environment (smoke and smoke-install):
#   PLAYWRIGHT_BROWSERS_PATH  where the Chromium build is kept. Default: /cache/chorus-playwright
#                             where /cache is writable, else Playwright's own
#                             (~/.cache/ms-playwright). A directory of chorus's own, because a
#                             Playwright install deletes the builds in its directory that no
#                             installed Playwright it knows of uses: one shared with another
#                             project loses that project's browsers.
#   CHORUS_CHROMIUM_LIBS      a prefix whose lib/ holds the shared libraries Chromium needs,
#                             put on LD_LIBRARY_PATH when it exists (default
#                             /cache/opt/chromium-libs): for a host with no root, where they
#                             come from conda-forge (web/README.md has the command). A host
#                             that has them installed (CI, after --with-deps) has no such
#                             prefix and none is used.
#   CHORUS_CHROMIUM_FONTS     a prefix with etc/fonts/fonts.conf and fonts, named to
#                             fontconfig by FONTCONFIG_FILE when it exists (default
#                             /cache/opt/chromium-fonts). With no font at all Chromium's page
#                             crashes as soon as it lays out text (seen here 2026-10-05).
#
# All of them first install the locked packages (`pnpm install --frozen-lockfile`, install scripts off)
# into web/node_modules, which is ignored, and hold every installed package's licence to
# web/licences.txt. The first install needs the network once, to fill pnpm's store; after that
# it runs offline.
#
# node and pnpm are pinned in mise.toml and this script runs no other version: where the pinned
# one is absent it fails naming it, under CI as anywhere else. A step that did not run is red,
# never a green SKIPPED (docs/decisions/0142-the-home-assistant-gate-steps-run-or-fail.md), and
# the gate holds each step to its own `web-test: PASS`, `web-build: PASS` or `web-smoke: PASS`
# line.

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
T0=$(date +%s.%N)
wall() { awk -v a="$T0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.1fs", b-a}'; }

what="${1:-}"
with_deps=
case "$what" in
    test | build | smoke) ;;
    smoke-install)
        case "${2:-}" in
            "") ;;
            --with-deps) with_deps=--with-deps ;;
            *)
                echo "usage: tools/web.sh smoke-install [--with-deps]"
                exit 2
                ;;
        esac
        ;;
    *)
        echo "usage: tools/web.sh test|build|smoke|smoke-install [--with-deps]"
        exit 2
        ;;
esac
# The step's name in its lines: web-test, web-build, web-smoke.
name="web-${what%-install}"

pin() { sed -n "s|^\"$1\" = \"\(.*\)\"\$|\1|p" mise.toml; }
NODE_PIN="$(pin core:node)"
PNPM_PIN="$(pin aqua:pnpm/pnpm)"
if [ -z "$NODE_PIN" ] || [ -z "$PNPM_PIN" ]; then
    echo "FAIL: mise.toml pins no core:node or no aqua:pnpm/pnpm; web/ is built and tested with them"
    exit 1
fi
have() { [ "$(node --version 2> /dev/null)" = "v$NODE_PIN" ] && [ "$(pnpm --version 2> /dev/null)" = "$PNPM_PIN" ]; }
if ! have && command -v mise > /dev/null 2>&1; then
    eval "$(MISE_TRUSTED_CONFIG_PATHS="$ROOT" mise env -s bash 2> /dev/null)"
fi
if ! have; then
    echo "FAIL: node v$NODE_PIN and pnpm $PNPM_PIN are not on PATH (found node $(node --version 2> /dev/null || echo none), pnpm $(pnpm --version 2> /dev/null || echo none)); install the pinned tools rootless with \`mise install\` (mise.toml)"
    exit 1
fi

# The smoke test runs only against a built chorus-server, so the step says which of the two
# happened: PASS (it ran) or SKIPPED (it did not). Under CI not running is a failure.
if [ "$what" = smoke ]; then
    if [ -z "${CHORUS_SERVER_BIN:-}" ]; then
        if [ "${CI:-}" = true ]; then
            echo "FAIL: CHORUS_SERVER_BIN is not set; under CI the browser smoke test runs against a built chorus-server or the step is red"
            exit 1
        fi
        echo "web-smoke: SKIPPED: CHORUS_SERVER_BIN is not set, so the browser smoke test (web/smoke/app.spec.js) did not run; build one with \`cargo build -p chorus-server\`"
        exit 0
    fi
    if [ ! -x "$CHORUS_SERVER_BIN" ] || [ -d "$CHORUS_SERVER_BIN" ]; then
        echo "FAIL: CHORUS_SERVER_BIN ($CHORUS_SERVER_BIN) is not an executable file; build it with \`cargo build -p chorus-server\`"
        exit 1
    fi
    CHORUS_SERVER_BIN="$(realpath "$CHORUS_SERVER_BIN")"
    export CHORUS_SERVER_BIN
fi

cd web
echo "$name: node $(node --version), pnpm $(pnpm --version)"
pnpm install --frozen-lockfile --ignore-scripts
node licences.mjs

if [ "$what" = smoke ] || [ "$what" = smoke-install ]; then
    # The browser's environment (the header says what each is).
    if [ -z "${PLAYWRIGHT_BROWSERS_PATH:-}" ] && [ -d /cache ] && [ -w /cache ]; then
        export PLAYWRIGHT_BROWSERS_PATH=/cache/chorus-playwright
    fi
    libs="${CHORUS_CHROMIUM_LIBS:-/cache/opt/chromium-libs}"
    if [ -d "$libs/lib" ]; then
        export LD_LIBRARY_PATH="$libs/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    else
        libs="the system's"
    fi
    fonts="${CHORUS_CHROMIUM_FONTS:-/cache/opt/chromium-fonts}"
    if [ -f "$fonts/etc/fonts/fonts.conf" ]; then
        export FONTCONFIG_FILE="$fonts/etc/fonts/fonts.conf" FONTCONFIG_PATH="$fonts/etc/fonts"
    else
        fonts="the system's"
    fi
    version="$(node -p 'require("@playwright/test/package.json").version')"
    # The build this version of Playwright names (its browsers.json) and where it keeps it:
    # the headless shell alone, which is what a headless run starts. The pin in package.json
    # and the lockfile's digest so fix the browser too.
    plan="$(pnpm exec playwright install --dry-run chromium-headless-shell)"
    build="$(printf '%s\n' "$plan" | head -n 1)"
    browser="$(printf '%s\n' "$plan" | sed -n 's/^ *Install location: *//p' | head -n 1)"
    echo "$name: @playwright/test $version, $build in ${browser:-no directory}, libraries $libs, fonts $fonts"
fi

if [ "$what" = smoke-install ]; then
    # shellcheck disable=SC2086  # $with_deps is one flag or nothing
    pnpm exec playwright install $with_deps chromium-headless-shell
    echo "web-smoke: $build is installed in $browser"
    exit 0
fi

if [ "$what" = smoke ]; then
    if [ -z "$browser" ] || [ ! -f "$browser/INSTALLATION_COMPLETE" ]; then
        echo "FAIL: the Chromium build of @playwright/test $version ($build) is not installed in ${browser:-any directory}; install it once with \`bash tools/web.sh smoke-install\`"
        exit 1
    fi
    # Nothing a run writes lands in the checkout.
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/chorus-web-smoke.XXXXXX")"
    trap 'rm -rf "$scratch"' EXIT
    # What there is to run, before running it: the gate has one browser test file.
    listed="$(pnpm exec playwright test --list 2>&1 | tail -n 1)"
    total="$(printf '%s\n' "$listed" | sed -n 's/^Total: \([0-9][0-9]*\) tests\{0,1\} in 1 file$/\1/p')"
    if [ -z "$total" ] || [ "$total" -eq 0 ]; then
        echo "FAIL: Playwright lists '$listed'; the browser smoke test is the tests of exactly one file, web/smoke/app.spec.js"
        exit 1
    fi
    rc=0
    pnpm exec playwright test --output "$scratch/results" > "$scratch/out" 2>&1 || rc=$?
    cat "$scratch/out"
    if [ "$rc" -ne 0 ]; then
        echo "FAIL: the browser smoke test failed"
        exit 1
    fi
    # Playwright's summary: every listed test passed, and none was skipped, flaky or left out.
    passed="$(sed -n 's/^  \([0-9][0-9]*\) passed (.*)$/\1/p' "$scratch/out" | tail -n 1)"
    if [ "${passed:-0}" != "$total" ] || grep -q -E '^  [0-9]+ (skipped|flaky|failed|interrupted|did not run)' "$scratch/out"; then
        echo "FAIL: Playwright passed ${passed:-0} of the $total tests it listed; every test of web/smoke/app.spec.js runs and passes"
        exit 1
    fi
    echo "web-smoke: PASS, 1 test file, $passed of $total tests passed, wall-clock $(wall)"
    exit 0
fi

if [ "$what" = build ]; then
    node build.mjs
    echo "web-build: PASS, wall-clock $(wall)"
    exit 0
fi

# The tests. happy-dom is registered by a preload so it is there before Lit is imported. The
# run is the whole of test/: a summary that counts no test, or a skipped one, is not a pass.
out="$(mktemp "${TMPDIR:-/tmp}/chorus-web-test.XXXXXX")"
trap 'rm -f "$out"' EXIT
rc=0
node --import ./test/setup.js --test --test-reporter=spec 'test/*.test.js' > "$out" 2>&1 || rc=$?
cat "$out"
count() { sed -n "s/^ℹ $1 \([0-9][0-9]*\)$/\1/p" "$out" | tail -n 1; }
tests="$(count tests)"
if [ "$rc" -ne 0 ]; then
    echo "FAIL: the web unit tests failed ($(count fail) of ${tests:-0})"
    exit 1
fi
if [ "${tests:-0}" -eq 0 ] || [ "$(count pass)" != "$tests" ]; then
    echo "FAIL: the run passed $(count pass) of ${tests:-0} tests ($(count skipped) skipped, $(count todo) todo); every test of web/test runs and passes"
    exit 1
fi
echo "web-test: PASS, $tests tests, wall-clock $(wall)"
