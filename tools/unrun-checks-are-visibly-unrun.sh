#!/usr/bin/env bash
# Every environment-dependent entry point, run with its prerequisite absent.
#
# Verifies: each one exits non-zero, names the missing prerequisite AND the
# criterion it was verifying, and does not report that criterion as passed,
# skipped-green or otherwise satisfied.
#
# This is what turns the evidence table into a record instead of a promise. An
# entry point that quietly reported success on a machine with no sound card
# would make every row of that table worthless, and the only way to know it
# does not is to run it on a machine with no sound card and look.
#
# Runs anywhere, and it is at its most meaningful exactly where the
# prerequisites are missing.
#
#   ./tools/unrun-checks-are-visibly-unrun.sh

source "$(dirname "$0")/lib.sh"

build_once

FAILURES=0

# Run one entry point in an environment where its prerequisite is genuinely
# absent, and check what it says.
expect_missing_prerequisite() {
    local name="$1"
    shift
    local out status
    set +e
    out="$("$@" 2>&1)"
    status=$?
    set -e

    say "--- $name (exit $status)"
    printf '%s\n' "$out" | sed 's/^/    /'

    local ok=1
    if [ "$status" -eq 0 ]; then
        say "FAIL $name exited zero with its prerequisite absent"
        ok=0
    fi
    if ! printf '%s' "$out" | grep -q 'MISSING PREREQUISITE'; then
        say "FAIL $name did not say a prerequisite was missing"
        ok=0
    fi
    if ! printf '%s' "$out" | grep -q 'criterion:'; then
        say "FAIL $name did not name the criterion it was verifying"
        ok=0
    fi
    if ! printf '%s' "$out" | grep -q 'prerequisite:'; then
        say "FAIL $name did not name the prerequisite"
        ok=0
    fi
    if printf '%s' "$out" | grep -qiE '^(pass|ok|skipped|green)'; then
        say "FAIL $name reported something that reads as satisfied"
        ok=0
    fi
    if [ "$ok" -eq 1 ]; then
        say "pass $name refuses, names both, and claims nothing"
    else
        FAILURES=$(( FAILURES + 1 ))
    fi
}

say "chorus: environment-dependent entry points, with their prerequisites absent"
say ""

# No such audio device. Genuinely absent: the name does not exist.
CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    expect_missing_prerequisite "ten-minute-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/ten-minute-run.sh"

expect_missing_prerequisite "delay-log-shape.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/delay-log-shape.sh"

expect_missing_prerequisite "overflow-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/overflow-run.sh"

# No granted real-time priority. Genuinely absent: a soft limit can always be
# lowered, so this is the real thing rather than a pretend one.
expect_missing_prerequisite "spin-test.sh" \
    bash -c 'ulimit -r 0 2>/dev/null; CHORUS_SKIP_BUILD=1 exec bash "$0"' \
    "$REPO_ROOT/tools/spin-test.sh"

expect_missing_prerequisite "host-contract.sh" \
    bash -c 'ulimit -r 0 2>/dev/null; CHORUS_SKIP_BUILD=1 exec bash "$0"' \
    "$REPO_ROOT/tools/host-contract.sh"

# No way to make a device unusable mid-run. Genuinely absent: nothing has told
# this script how.
expect_missing_prerequisite "device-loss-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_REMOVABLE_DEVICE= CHORUS_REMOVE_COMMAND= \
    bash "$REPO_ROOT/tools/device-loss-run.sh"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: every environment-dependent entry point refuses visibly"
    exit 0
fi
say "chorus: $FAILURES entry points did not refuse properly"
exit 1
