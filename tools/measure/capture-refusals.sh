#!/usr/bin/env bash
# The measurement rig's refusal paths, in the environment they matter most in:
# a machine with no capture device.
#
# Runs anywhere: no audio device, no privilege, no container. Verifies:
#
#   - a run requesting a chirp amplitude above the ceiling config/measure.conf
#     declares refuses to start, names both the requested and the permitted
#     amplitude, and emits nothing. It refuses BEFORE the device probe, because
#     the request is what is dangerous and it will be just as dangerous on the
#     machine that does have the device;
#   - a run at or below the ceiling gets past the amplitude check and then
#     refuses on the device, which is what says the ceiling is a real gate and
#     not a blanket refusal;
#   - the device-backed entry point, where there is no capture device, exits
#     non-zero naming the missing prerequisite and the criterion, and reports
#     nothing as passed or skipped-green.
#
#   ./tools/measure/capture-refusals.sh

source "$(dirname "$0")/../lib.sh"

build_once

FAILURES=0
check() {
    local name="$1"
    local ok="$2"
    local detail="$3"
    if [ "$ok" = "1" ]; then
        say "pass $name: $detail"
    else
        say "FAIL $name: $detail"
        FAILURES=$(( FAILURES + 1 ))
    fi
}

CEILING="$(sed -n 's/^[[:space:]]*chirp_amplitude_ceiling[[:space:]]*=[[:space:]]*\([^#]*\).*/\1/p' \
    "$REPO_ROOT/config/measure.conf" | head -n 1 | tr -d '[:space:]')"
say "chorus: the declared chirp amplitude ceiling is $CEILING full scale"

run_capture() {
    set +e
    OUT="$("$BIN_DIR/chorus-measure-capture" "$@" 2>&1)"
    STATUS=$?
    set -e
}

# --- an amplitude above the declared ceiling ---------------------------------
run_capture --probe-capture-device --capture-device chorus-no-such-capture-device \
    --amplitude 0.9
check "over-level-chirp-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "over-level-chirp-names-the-requested-amplitude" \
    "$(echo "$OUT" | grep -q 'requested:.*0\.9' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'requested:' | head -n 1)"
check "over-level-chirp-names-the-permitted-amplitude" \
    "$(echo "$OUT" | grep -q "permitted:.*$CEILING" && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'permitted:' | head -n 1)"
check "over-level-chirp-emits-nothing" \
    "$(echo "$OUT" | grep -q 'emitted:.*nothing' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'emitted:' | head -n 1)"
check "over-level-chirp-refuses-before-it-looks-for-a-device" \
    "$(echo "$OUT" | grep -q 'capture-device-probe' && echo 0 || echo 1)" \
    "no device probe ran"
check "over-level-chirp-claims-nothing" \
    "$(echo "$OUT" | grep -qiE '^(pass|ok|skipped|green)' && echo 0 || echo 1)" \
    "$(echo "$OUT" | head -n 1)"

# --- exactly the ceiling, which is permitted ---------------------------------
#
# Without this the check above would pass just as well against a tool that
# refused every amplitude.
run_capture --probe-capture-device --capture-device chorus-no-such-capture-device \
    --amplitude "$CEILING"
check "at-the-ceiling-gets-past-the-amplitude-check" \
    "$(echo "$OUT" | grep -q 'MISSING PREREQUISITE' && echo 1 || echo 0)" \
    "refused on the device, not on the level"
check "at-the-ceiling-is-not-reported-as-an-over-level-refusal" \
    "$(echo "$OUT" | grep -q '^REFUSED' && echo 0 || echo 1)" \
    "$(echo "$OUT" | head -n 1)"

# --- no capture device -------------------------------------------------------
run_capture --probe-capture-device --capture-device chorus-no-such-capture-device
check "absent-capture-device-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "absent-capture-device-names-the-prerequisite" \
    "$(echo "$OUT" | grep -q 'prerequisite:.*capture device' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'prerequisite:' | head -n 1)"
check "absent-capture-device-names-the-criterion" \
    "$(echo "$OUT" | grep -q 'criterion:.*inter-device lag' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'criterion:' | head -n 1)"
check "absent-capture-device-does-not-claim-a-capture" \
    "$(echo "$OUT" | grep -q 'usable=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'capture-device-probe' | head -n 1)"
check "absent-capture-device-claims-nothing" \
    "$(echo "$OUT" | grep -qiE '^(pass|ok|skipped|green)' && echo 0 || echo 1)" \
    "$(echo "$OUT" | grep 'NOT passed' | head -n 1)"

# --- the analysis half refuses on the committed degenerate captures ----------
#
# The same refusals the suite grades, run through the real binary, because a
# typed error inside a library is not by itself evidence that the entry point
# exits non-zero on it.
FIXTURES="$REPO_ROOT/fixtures/measure"
REPORTS_BEFORE="$(find "$REPO_ROOT/docs/measurements" -maxdepth 1 -type f | wc -l | tr -d ' ')"
refuses() {
    local name="$1"
    local fixture="$2"
    local expected="$3"
    set +e
    OUT="$("$BIN_DIR/chorus-measure" lag "$FIXTURES/$fixture" --label refusal-check 2>&1)"
    STATUS=$?
    set -e
    check "$name-exits-non-zero" \
        "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
    check "$name-says-which-condition" \
        "$(echo "$OUT" | grep -qi "$expected" && echo 1 || echo 0)" \
        "$(echo "$OUT" | head -n 1)"
}

refuses "silence" "04-silence.wav" "is silent"
refuses "uncorrelated-noise" "05-uncorrelated-noise.wav" "no chirp is present"
refuses "unresolvable-chirps" "06-unresolvable-chirps.wav" "could not be resolved"
refuses "wrong-channel-count" "07-wrong-channel-count.wav" "were required"
refuses "unsupported-format" "08-unsupported-format.wav" "was required"
refuses "truncated-body" "09-truncated-body.wav" "truncated body"

REPORTS_AFTER="$(find "$REPO_ROOT/docs/measurements" -maxdepth 1 -type f | wc -l | tr -d ' ')"
check "a-refused-run-leaves-docs-measurements-unchanged" \
    "$([ "$REPORTS_BEFORE" = "$REPORTS_AFTER" ] && echo 1 || echo 0)" \
    "$REPORTS_BEFORE files before, $REPORTS_AFTER after"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: every measurement refusal path holds"
    exit 0
fi
say "chorus: $FAILURES measurement refusal checks failed"
exit 1
