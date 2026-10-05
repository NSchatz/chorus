#!/usr/bin/env bash
# `make web-test` and `make web-build`: the app under web/ (Lit 3 elements bundled by esbuild;
# docs/decisions/0000-the-web-app-stack.md).
#
#   tools/web.sh test    the unit tests: `node --test` with happy-dom as the DOM, so components
#                        render and update in node. No browser is installed or started.
#   tools/web.sh build   web/src into web/dist, the committed output chorus-server embeds. The
#                        build is deterministic; the gate step `web-build` runs it and fails when
#                        web/dist then differs from what is committed.
#
# Both first install the locked packages (`pnpm install --frozen-lockfile`, install scripts off)
# into web/node_modules, which is ignored, and hold every installed package's licence to
# web/licences.txt. The first install needs the network once, to fill pnpm's store; after that
# it runs offline.
#
# node and pnpm are pinned in mise.toml and this script runs no other version: where the pinned
# one is absent it fails naming it, under CI as anywhere else. A step that did not run is red,
# never a green SKIPPED (docs/decisions/0142-the-home-assistant-gate-steps-run-or-fail.md), and
# the gate holds each step to its own `web-test: PASS` or `web-build: PASS` line.

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
T0=$(date +%s.%N)
wall() { awk -v a="$T0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.1fs", b-a}'; }

what="${1:-}"
case "$what" in
    test | build) ;;
    *)
        echo "usage: tools/web.sh test|build"
        exit 2
        ;;
esac

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

cd web
echo "web-$what: node $(node --version), pnpm $(pnpm --version)"
pnpm install --frozen-lockfile --ignore-scripts
node licences.mjs

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
