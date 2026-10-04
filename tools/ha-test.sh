#!/usr/bin/env bash
# `make ha-test`, gate step `ha-test`: the Home Assistant integration's own checks.
#
#   uv sync --locked     the pinned Python and the hash-locked harness (uv.lock), into a
#                        virtual environment OUTSIDE the repository
#   ruff                 lint and format check
#   mypy --strict        over the integration and its vendored client
#   pytest               under the pinned Home Assistant test harness, with coverage held
#                        above 95 % overall and at 100 % for the config flow
#
# A narrowed run (any argument: `make ha-test HA_TEST_ARGS="-k announce"`, or a test file) is
# the inner loop: pytest alone with those arguments, passing or failing on the selected tests.
# It runs no ruff, no mypy and holds no coverage threshold (a part of the tests cannot cover
# the whole integration), and it ends with `ha-test: NARROWED PASS`, never `ha-test: PASS`:
# only the whole run is gate evidence. `--live` (`make ha-live`) is the live-server test alone
# and ends with `ha-live: PASS` when the test ran, `ha-live: SKIPPED` when it did not.
#
# Needs no device, no sound card and no Home Assistant: the tests' fake chorus server
# listens on loopback only, and the harness blocks every other address. The first run
# needs the network once, to fetch the pinned Python and the locked packages into uv's
# cache; after that it runs offline.
#
# Environment:
#   UV_PROJECT_ENVIRONMENT  where the virtual environment lives (default
#                           /cache/venvs/chorus-ha where /cache exists, else under
#                           $XDG_CACHE_HOME or ~/.cache); never inside the repository
#   CHORUS_SERVER_BIN       a built chorus-server: tests/test_live_server.py then drives the
#                           real server instead of being skipped by name
#
# uv is pinned in mise.toml. Where it is absent the step fails naming it, under CI as
# anywhere else: a gate step that did not run is red, never a green SKIPPED
# (docs/decisions/0000-the-home-assistant-gate-steps-run-or-fail.md). For the same reason
# `--live` under CI (CI=true) fails when CHORUS_SERVER_BIN is unset.

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
T0=$(date +%s.%N)

if ! command -v uv > /dev/null 2>&1 && command -v mise > /dev/null 2>&1; then
    eval "$(MISE_TRUSTED_CONFIG_PATHS="$ROOT" mise env -s bash 2> /dev/null)"
fi
if ! command -v uv > /dev/null 2>&1; then
    echo "FAIL: uv is not on PATH; the Home Assistant integration's lint, types and tests need the pinned uv of mise.toml: install the pinned tools rootless with \`mise install\`"
    exit 1
fi

if [ -z "${UV_PROJECT_ENVIRONMENT:-}" ]; then
    if [ -d /cache ] && [ -w /cache ]; then
        UV_PROJECT_ENVIRONMENT=/cache/venvs/chorus-ha
    else
        UV_PROJECT_ENVIRONMENT="${XDG_CACHE_HOME:-$HOME/.cache}/chorus/venvs/chorus-ha"
    fi
fi
case "$(realpath -m "$UV_PROJECT_ENVIRONMENT")/" in
    "$ROOT"/*)
        echo "FAIL: UV_PROJECT_ENVIRONMENT ($UV_PROJECT_ENVIRONMENT) is inside the repository; the virtual environment lives outside it"
        exit 1
        ;;
esac
export UV_PROJECT_ENVIRONMENT
# Nothing a run writes lands in the checkout: no byte code, no coverage file.
export PYTHONDONTWRITEBYTECODE=1
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/chorus-ha-test.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT
export COVERAGE_FILE="$SCRATCH/coverage"

cd integrations/homeassistant
echo "ha-test: uv $(uv --version | cut -d' ' -f2), environment $UV_PROJECT_ENVIRONMENT"
uv sync --locked --all-groups --quiet
run() { uv run --locked --all-groups --no-sync "$@"; }
echo "ha-test: $(run python --version), $(run python -c 'import homeassistant.const as c; print("Home Assistant", c.__version__)')"

wall() { awk -v a="$T0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.1fs", b-a}'; }

if [ "${1:-}" = --live ]; then
    # `make ha-live`: the live-server test alone. It runs only against a built chorus-server,
    # so the step says which of the two happened: PASS (it ran) or SKIPPED (it did not, with
    # pytest's reason shown). Under CI not running is a failure.
    if [ -z "${CHORUS_SERVER_BIN:-}" ]; then
        if [ "${CI:-}" = true ]; then
            echo "FAIL: CHORUS_SERVER_BIN is not set; under CI the live-server test runs against a built chorus-server or the step is red"
            exit 1
        fi
        run pytest -rs -p no:cov tests/test_live_server.py
        echo "ha-live: SKIPPED: CHORUS_SERVER_BIN is not set, so the live-server test did not run"
        exit 0
    fi
    if [ ! -x "$CHORUS_SERVER_BIN" ] || [ -d "$CHORUS_SERVER_BIN" ]; then
        echo "FAIL: CHORUS_SERVER_BIN ($CHORUS_SERVER_BIN) is not an executable file; build it with \`cargo build -p chorus-server\`"
        exit 1
    fi
    run pytest -rs -p no:cov tests/test_live_server.py
    echo "ha-live: PASS, wall-clock $(wall)"
    exit 0
fi

if [ "$#" -gt 0 ]; then
    # A narrowed run: the selected tests alone decide it.
    echo "ha-test: pytest $* (narrowed: no ruff, no mypy, no coverage threshold)"
    run pytest "$@"
    echo "ha-test: NARROWED PASS (pytest $*), wall-clock $(wall); the whole run (\`make ha-test\`) holds ruff, mypy and the coverage thresholds"
    exit 0
fi

echo "ha-test: ruff check"
run ruff check --no-cache custom_components tests
echo "ha-test: ruff format --check"
run ruff format --no-cache --check custom_components tests
echo "ha-test: mypy --strict"
run mypy
echo "ha-test: pytest"
run pytest --cov --cov-report=term-missing:skip-covered --cov-fail-under=95
echo "ha-test: the config flow's coverage"
run coverage report --include='*/config_flow.py' --fail-under=100

echo "ha-test: PASS, wall-clock $(wall)"
