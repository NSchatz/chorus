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

# The claim that this list is EVERY environment-dependent entry point is
# checked at the end against the tools directory itself, not left as prose: a
# new entry point added without a line here would make this meta-check quietly
# incomplete, which is the exact failure it exists to prevent.

source "$(dirname "$0")/lib.sh"

build_once

FAILURES=0
CHECKED=()

# Run one entry point in an environment where its prerequisite is genuinely
# absent, and check what it says.
expect_missing_prerequisite() {
    local name="$1"
    shift
    CHECKED+=("$name")
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

expect_missing_prerequisite "start-fill-and-log-shape.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/start-fill-and-log-shape.sh"

expect_missing_prerequisite "overflow-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/overflow-run.sh"

# This one needs a device that opens, not one that paces, so the ALSA `null`
# device is enough for it. A name that does not exist is absent for it too.
expect_missing_prerequisite "stream-end-and-loss.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/stream-end-and-loss.sh"

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

# No capture device. Genuinely absent: the name does not exist, and a machine
# with no sound card has none under any name.
expect_missing_prerequisite "capture-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CAPTURE_DEVICE=chorus-no-such-capture-device \
    bash "$REPO_ROOT/tools/measure/capture-run.sh"

# No second endpoint, no capture device and no playback device that paces.
# Genuinely absent: this machine has one of itself and no sound card at all,
# and nothing has told this script where a second endpoint would be. This is
# the entry point for AC-1, the one criterion of SYNC-4 that needs hardware.
expect_missing_prerequisite "sync-hour-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_SECOND_ENDPOINT= \
    CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    CHORUS_CAPTURE_DEVICE=chorus-no-such-capture-device \
    bash "$REPO_ROOT/tools/sync-hour-run.sh"

# --- and the list above is the whole list ------------------------------------
#
# An entry point is environment-dependent exactly when it calls one of lib.sh's
# require_* guards. Deriving the set from the scripts rather than restating it
# is what keeps "every" true after the next one is written.
#
# Searched recursively, so a subdirectory of tools/ cannot be a place an entry
# point hides from this check. tools/measure/ is the first one.
say ""
say "--- the list of entry points is complete"
FOUND="$(grep -rlE '^[[:space:]]*require_[a-z_]+ ' --include='*.sh' "$REPO_ROOT/tools" \
    | xargs -n1 basename | sort)"
DECLARED="$(printf '%s\n' "${CHECKED[@]}" | sort)"
MISSED="$(comm -23 <(printf '%s\n' "$FOUND") <(printf '%s\n' "$DECLARED") || true)"
if [ -n "$MISSED" ]; then
    say "FAIL these entry points guard on an environment and are not checked here:"
    printf '%s\n' "$MISSED" | sed 's/^/    /'
    FAILURES=$(( FAILURES + 1 ))
else
    say "pass all $(printf '%s\n' "$FOUND" | wc -l | tr -d ' ') environment-dependent entry points in tools/ are checked above"
fi

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: every environment-dependent entry point refuses visibly"
    exit 0
fi
say "chorus: $FAILURES checks did not hold"
exit 1
