#!/usr/bin/env bash
# The control-plane thread-population checks, run over and over on one build.
#
# Those checks grade AC-12 against a control plane with a FIXED worker pool, and
# they have to take workers from that pool to do it. A check like that is not
# graded by one green run of itself: the run that passes and the run that fails
# are the same check on the same build, and which of the two happens is decided
# by a scheduler. So this runs them the number of consecutive times
# config/verification.conf commits to, on ONE build, at the test binary's own
# parallelism, and passes only if every one of those runs passed.
#
# A repeated run that had quietly stopped exercising the pool would be green for
# the wrong reason, so this also runs the same checks in two configurations
# where the control plane CANNOT serve every attachment and command they make,
# and requires each of those to go RED naming the busy-worker refusal, the
# worker ceiling in force and the attachments that were being held. A build
# where a starved control plane stopped being visible to these checks fails
# here, however green the repetitions were.
#
# Needs no device, no privilege and no network: one loopback server per check,
# started and killed by the checks themselves. It guards on no environment and
# calls no require_* guard, which is why it is not in the list
# tools/unrun-checks-are-visibly-unrun.sh derives.
#
#   ./tools/control-determinism.sh
#   CHORUS_DETERMINISM_REPETITIONS=500 ./tools/control-determinism.sh

source "$(dirname "$0")/lib.sh"

# The floor the repetition count may never go under, committed or overridden.
#
# Fifty is not a measurement of anything; it is the smallest number that makes
# the claim "this is deterministic" cost more than one lucky scheduling. It is a
# FLOOR and not a default: lowering the committed constant under it fails this
# check by name rather than quietly buying a faster build, and so does an
# override that grants fewer, because the number of repetitions IS the claim and
# a shorter run is a different one. `require_soak_window` in tools/lib.sh holds
# CHORUS_SOAK_SECONDS to the same rule for the same reason. Running the checks
# fewer times than that is running the checks, not making this claim: the binary
# itself is there for that, and `make one` runs it.
MINIMUM_REPETITIONS=50

# The checks this runs, and the package they live in.
TEST_TARGET=control_thread_population
TEST_PACKAGE=chorus-server

# How many CPUs the repetitions are given.
#
# The failure this grades is a scheduling race: a control worker that has
# answered a connection and has not yet handed its slot back. On a machine with
# many idle cores that interleaving almost never happens, so fifty green
# repetitions there would be fifty runs of the easy case. Two CPUs is what the
# runner that reported the failure has, and it is where the race is real.
REPETITION_CPUS=2

refuse() {
    printf 'REFUSED\n' >&2
    while [ "$#" -gt 0 ]; do
        printf '  %s\n' "$1" >&2
        shift
    done
    printf '  this check is NOT passed, NOT skipped-green and NOT satisfied.\n' >&2
    exit 2
}

# Here-strings rather than a pipe into grep, here and below: `grep -q` stops
# reading at the first match, and under `pipefail` the writer it left holding a
# closed pipe is a non-zero pipeline status, which would be this check failing
# because it found what it was looking for.
positive_integer() {
    grep -qE '^[1-9][0-9]*$' <<<"${1:-}"
}

# --- the repetition count ----------------------------------------------------
#
# Committed, because a check and the thing it checks must not be able to drift
# apart, and an environment override for running MORE repetitions than the
# committed number. The floor binds the override too: the count is what the claim
# is made of, so an override that granted fewer would be a way to pass this check
# without making its claim.

if ! COMMITTED="$(conf control_determinism_repetitions 2>/dev/null)"; then
    refuse \
        "config/verification.conf has no control_determinism_repetitions" \
        "that file is where a constant a check rests on lives, and this check rests on how many consecutive repetitions the determinism claim is made out of" \
        "there is no fallback count: a repetition count this check invented would be a number nobody committed to" \
        "add to config/verification.conf: control_determinism_repetitions = $MINIMUM_REPETITIONS"
fi

if ! positive_integer "$COMMITTED"; then
    refuse \
        "config/verification.conf has control_determinism_repetitions = '$COMMITTED', which is not a positive integer" \
        "set it to a whole number of repetitions, no fewer than $MINIMUM_REPETITIONS"
fi

if [ "$COMMITTED" -lt "$MINIMUM_REPETITIONS" ]; then
    refuse \
        "config/verification.conf commits to $COMMITTED repetitions and the floor is $MINIMUM_REPETITIONS" \
        "the floor is the point of the constant: under it, one lucky scheduling is most of the evidence" \
        "raise control_determinism_repetitions to at least $MINIMUM_REPETITIONS"
fi

REPETITIONS="$COMMITTED"
if [ -n "${CHORUS_DETERMINISM_REPETITIONS+set}" ]; then
    if ! positive_integer "${CHORUS_DETERMINISM_REPETITIONS}"; then
        refuse \
            "CHORUS_DETERMINISM_REPETITIONS is '${CHORUS_DETERMINISM_REPETITIONS}', which is not a positive integer" \
            "the override runs MORE repetitions than the committed count; it is not a way to run this check zero times" \
            "unset it to run the committed $COMMITTED, or set it to a whole number of at least $MINIMUM_REPETITIONS"
    fi
    if [ "$CHORUS_DETERMINISM_REPETITIONS" -lt "$MINIMUM_REPETITIONS" ]; then
        refuse \
            "CHORUS_DETERMINISM_REPETITIONS grants $CHORUS_DETERMINISM_REPETITIONS repetitions and this claim is made out of at least $MINIMUM_REPETITIONS" \
            "the floor binds the override exactly as it binds the committed constant: under it, one lucky scheduling is most of the evidence" \
            "unset it to run the committed $COMMITTED, or raise it to at least $MINIMUM_REPETITIONS. A shorter run is a different claim and this will not make it" \
            "to run the checks themselves a handful of times, run them: make one ARGS='--package $TEST_PACKAGE --test $TEST_TARGET'"
    fi
    REPETITIONS="$CHORUS_DETERMINISM_REPETITIONS"
fi

# --- the record and the constant say the same number -------------------------
#
# docs/verification-record.md states the determinism claim in words, including
# how many repetitions it is made out of. Two places holding one number is two
# places to change, so the check that they agree is here rather than in a
# reader's memory.
RECORD="$REPO_ROOT/docs/verification-record.md"
if ! grep -q "$COMMITTED consecutive repetitions" "$RECORD"; then
    refuse \
        "docs/verification-record.md does not say '$COMMITTED consecutive repetitions'" \
        "config/verification.conf commits to $COMMITTED, and the record is what a reader is told" \
        "update the AC-12 row of docs/verification-record.md to the committed number"
fi

# --- one build, and the binary every repetition runs -------------------------

build_once

BINARY="$( (cd "$REPO_ROOT" && cargo test --package "$TEST_PACKAGE" --test "$TEST_TARGET" \
    --no-run --message-format=json 2>/dev/null) | python3 -c '
import json, sys

target = sys.argv[1]
found = ""
for line in sys.stdin:
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if message.get("executable") and message.get("target", {}).get("name") == target:
        found = message["executable"]
print(found)
' "$TEST_TARGET" )"

if [ -z "$BINARY" ] || [ ! -x "$BINARY" ]; then
    refuse \
        "the $TEST_TARGET test binary could not be resolved" \
        "cargo test --package $TEST_PACKAGE --test $TEST_TARGET --no-run named '${BINARY:-nothing}'" \
        "build the workspace and try again: cargo build --workspace --all-targets"
fi

# Every repetition runs THIS file. Resolving it once is what makes "on one
# build" true rather than approximately true.
say "chorus: the control-plane thread-population checks, repeated"
say "  binary:      $BINARY"
say "  repetitions: $REPETITIONS (config/verification.conf commits to $COMMITTED)"

# The CPUs the repetitions get, taken from the ones this process is actually
# allowed to use rather than assumed to be 0 and 1.
CPU_LIST=""
if command -v taskset >/dev/null 2>&1; then
    CPU_LIST="$(python3 -c '
import os, sys

want = int(sys.argv[1])
allowed = sorted(os.sched_getaffinity(0))
print(",".join(str(cpu) for cpu in allowed[:want]))
' "$REPETITION_CPUS")"
fi
if [ -n "$CPU_LIST" ]; then
    say "  cpus:        $CPU_LIST, because the failure this grades is a scheduling race"
    run_repetition() {
        set +e
        OUT="$(env "$@" taskset -c "$CPU_LIST" "$BINARY" 2>&1)"
        STATUS=$?
        set -e
    }
else
    say "  cpus:        ALL of them, because taskset is not on PATH here. The repetitions"
    say "               below ran, and they ran where the race is least likely to happen."
    run_repetition() {
        set +e
        OUT="$(env "$@" "$BINARY" 2>&1)"
        STATUS=$?
        set -e
    }
fi

elapsed_since() {
    local started="$1"
    local now
    now="$(date +%s%3N)"
    printf '%d.%03d' $(( (now - started) / 1000 )) $(( (now - started) % 1000 ))
}

# --- the repetitions ---------------------------------------------------------

say ""
say "--- $REPETITIONS consecutive repetitions, at the test binary's own parallelism"
STARTED="$(date +%s%3N)"
PASSED=0
REPETITION=0
while [ "$REPETITION" -lt "$REPETITIONS" ]; do
    REPETITION=$(( REPETITION + 1 ))
    run_repetition
    if [ "$STATUS" -ne 0 ]; then
        ELAPSED="$(elapsed_since "$STARTED")"
        printf '\n'
        say "FAIL repetition $REPETITION of $REPETITIONS went red on an unchanged build (exit $STATUS)"
        say "     after ${ELAPSED} s, with $PASSED repetitions before it passing. It said:"
        printf '%s\n' "$OUT" | sed 's/^/    /'
        say ""
        say "chorus: the control-plane thread-population checks are NOT deterministic."
        say "        $PASSED of $REPETITIONS repetitions passed and repetition $REPETITION did not."
        exit 1
    fi
    PASSED=$(( PASSED + 1 ))
    printf '.'
    if [ $(( REPETITION % 50 )) -eq 0 ]; then
        printf ' %d\n' "$REPETITION"
    fi
done
printf '\n'
ELAPSED="$(elapsed_since "$STARTED")"
say "pass $PASSED of $REPETITIONS repetitions passed, in ${ELAPSED} s"

# --- and the starvation is still visible -------------------------------------
#
# Each of these runs the SAME checks against a control plane with fewer workers
# than the attachments and commands they make, so there is no scheduling in
# which they can be served. The checks have to go red saying so. A build where
# they went green here would be a build where the checks had stopped needing the
# control plane to serve them, which is the way a repeated run rots into a
# repeated run of nothing.

FAILURES=0
starved() {
    local workers="$1"
    local what="$2"
    say ""
    say "--- starved: $workers control workers, which cannot serve $what"
    local started
    started="$(date +%s%3N)"
    run_repetition CHORUS_DETERMINISM_WORKERS="$workers"
    local took
    took="$(elapsed_since "$started")"
    local ok=1
    if [ "$STATUS" -eq 0 ]; then
        say "FAIL a starved run passed (exit 0) in ${took} s. These checks require the control"
        say "     plane to serve them, and a control plane that cannot did not stop them."
        ok=0
    fi
    local wanted
    for wanted in '503 Service Unavailable' 'control workers is busy' \
        "control workers in force: $workers" 'attachments held:'; do
        if ! grep -qF -- "$wanted" <<<"$OUT"; then
            say "FAIL the starved run's output never said '$wanted'"
            ok=0
        fi
    done
    if [ "$ok" -eq 1 ]; then
        say "pass it went red in ${took} s naming the refusal, the ceiling and the attachments held:"
        { grep -A6 'refused to serve' <<<"$OUT" || true; } | head -n 12 | sed 's/^/    /'
    else
        printf '%s\n' "$OUT" | sed 's/^/    /'
        FAILURES=$(( FAILURES + 1 ))
    fi
}

# Fewer workers than the event streams the checks hold open: the third
# attachment has no worker to be served by, ever.
starved 2 "the three event streams these checks attach"
# Enough workers for those three streams and not one left over: every request
# the checks make while holding them starves, which is the shape the reported
# failures took.
starved 3 "a request that needs a worker while three event streams are held"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: $PASSED consecutive repetitions of the control-plane thread-population"
    say "        checks passed on one build, in ${ELAPSED} s, and a starved control plane"
    say "        still stops them by name. One green run is not this claim."
    exit 0
fi
say "chorus: $FAILURES starved runs did not hold"
exit 1
