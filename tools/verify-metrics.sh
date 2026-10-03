#!/usr/bin/env bash
# Goal 15, line A's lint: a REAL scrape of chorus-server's /metrics is clean
# under `promtool check metrics`.
#
# The scrape is not a fixture. It is taken by the line-A test
# (crates/server/tests/metrics_scrape.rs: the real server, the real C endpoint
# and a Linux-client session) and written out by that test, so what promtool
# reads is what a Prometheus would. promtool parses the text exposition format
# (a syntax error fails) and applies its lint rules: HELP on every metric,
# `_total` on counters and only on them, base units, no abbreviated units.
#
# promtool is pinned in mise.toml (the Prometheus release that carries it) and
# is not something `make gate` needs: the format's rules are held in the gate
# by the server's own tests. So this is its own target, and where promtool is
# absent, or is another version, it refuses BY NAME and exits non-zero. It is
# never reported as passed (tools/unrun-checks-are-visibly-unrun.sh holds it
# to that).
#
#   ./tools/verify-metrics.sh          # or: make verify-metrics

source "$(dirname "$0")/lib.sh"

CRITERION="a real scrape of chorus-server's /metrics is clean under promtool check metrics (docs/telemetry.md)"

say "chorus: the exporter's scrape, linted by promtool"
say "  criterion: $CRITERION"

require_promtool "$CRITERION"
say "  promtool:  $PROMTOOL ($("$PROMTOOL" --version 2>&1 | head -n 1))"

OUT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/chorus-verify-metrics.XXXXXX")"
trap 'rm -rf "$OUT_DIR"' EXIT
SCRAPE="$OUT_DIR/scrape.prom"

# The scrape, from the line-A test. The test itself fails by name where the C
# endpoint cannot be built (make, a C compiler, the pinned ESP-IDF tree).
if ! (cd "$REPO_ROOT" && CHORUS_METRICS_SCRAPE_OUT="$SCRAPE" \
    cargo test --quiet --locked -p chorus-server --test metrics_scrape \
    > "$OUT_DIR/test.log" 2>&1); then
    cat "$OUT_DIR/test.log"
    say "FAIL the line-A test did not pass, so there is no scrape to lint"
    exit 1
fi
if [ ! -s "$SCRAPE" ]; then
    cat "$OUT_DIR/test.log"
    say "FAIL the line-A test passed and wrote no scrape to $SCRAPE"
    exit 1
fi

say "  scrape:    $(grep -vc '^#' "$SCRAPE") samples in $(grep -c '^# TYPE ' "$SCRAPE") families"
if ! "$PROMTOOL" check metrics < "$SCRAPE" > "$OUT_DIR/lint.log" 2>&1; then
    cat "$OUT_DIR/lint.log"
    say "FAIL promtool check metrics found problems in the scrape:"
    sed 's/^/    /' "$SCRAPE"
    exit 1
fi
if [ -s "$OUT_DIR/lint.log" ]; then
    # promtool prints lint findings and still exits zero for some of them.
    cat "$OUT_DIR/lint.log"
    say "FAIL promtool check metrics had something to say about the scrape"
    exit 1
fi
say "pass promtool check metrics is clean on the scrape"
say "--- the scrape promtool read"
cat "$SCRAPE"
