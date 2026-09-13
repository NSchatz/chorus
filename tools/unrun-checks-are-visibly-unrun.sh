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

# No ESP-IDF toolchain. Genuinely absent: IDF_PATH is unset here and idf.py is
# not on PATH, which is what a machine with no embedded toolchain looks like.
# This is the entry point for AC-17 of the EMBEDDED-5 phase.
expect_missing_prerequisite "firmware-image.sh" \
    env -u IDF_PATH CHORUS_SKIP_BUILD=1 \
    bash "$REPO_ROOT/tools/firmware-image.sh"

# No ESP32-S3, no amplifier, no second endpoint, no capture device and no
# playback device that paces. Genuinely absent: this machine is one container
# with no serial port and no sound card of any kind. This is the entry point
# for AC-1 and AC-3, the two criteria of EMBEDDED-5 that need hardware, and
# neither is passed anywhere in this repository.
expect_missing_prerequisite "endpoint-rig-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_ESP32S3_PORT= CHORUS_SECOND_ENDPOINT= \
    CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    CHORUS_CAPTURE_DEVICE=chorus-no-such-capture-device \
    bash "$REPO_ROOT/tools/endpoint-rig-run.sh"

# No playback device. Genuinely absent: the name does not exist. Both of these
# are PRODUCT-6's entry points for AC-1 and AC-3, and both run real endpoints,
# which need a device that opens.
expect_missing_prerequisite "control-plane-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/control-plane-run.sh"

expect_missing_prerequisite "restart-storm-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/restart-storm-run.sh"

# The same, for AC-2's fallback half run all the way to playing. It needs a
# device that opens and no multicast at all, so the device is the prerequisite
# that is made absent here.
expect_missing_prerequisite "discovery-fallback-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    bash "$REPO_ROOT/tools/discovery-fallback-run.sh"

# No browser engine. Genuinely absent: the path does not exist. This is the
# entry point for AC-5, AC-6 and AC-10, and it must refuse rather than fall back
# to reading the stylesheet, which is the failure it exists to prevent.
expect_missing_prerequisite "ui-render-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_BROWSER=/chorus-no-such-browser \
    bash "$REPO_ROOT/tools/ui-render-run.sh"

# No interpreter for the styling scan. Genuinely absent: the name does not
# exist. The styling check reads source text and has no engine to lose, but it
# is still a check with a prerequisite, and a check that quietly reported green
# because its interpreter was missing would be the same fault as one that read
# the stylesheet because the browser was.
expect_missing_prerequisite "styling-check.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_NODE=chorus-no-such-node \
    bash "$REPO_ROOT/tools/styling-check.sh"

# No three days. Genuinely absent: no run in this pipeline has three days, and
# no amount of configuration makes one. This is AC-4, which is NOT PASSED
# anywhere in this repository.
expect_missing_prerequisite "soak-run.sh" \
    env CHORUS_SKIP_BUILD=1 CHORUS_SOAK_SECONDS= CHORUS_SECOND_ENDPOINT= \
    CHORUS_CLIENT_DEVICE=chorus-no-such-device \
    CHORUS_CAPTURE_DEVICE=chorus-no-such-capture-device \
    bash "$REPO_ROOT/tools/soak-run.sh"

# No usable multicast. Genuinely absent, and it has to be made so on purpose:
# this container DOES carry multicast and tools/mdns-live-run.sh passes here, so
# the only honest way to see its refusal is to make UDP 5353 genuinely
# unavailable, which is what a host already running a responder looks like.
# tools/with-mdns-port-taken.sh binds it for the duration.
expect_missing_prerequisite "mdns-live-run.sh" \
    env CHORUS_SKIP_BUILD=1 \
    bash "$REPO_ROOT/tools/with-mdns-port-taken.sh" \
    bash "$REPO_ROOT/tools/mdns-live-run.sh"

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
