#!/usr/bin/env bash
# Every place the system is supposed to refuse rather than pretend.
#
# Runs anywhere: no audio device, no privilege, no container. Verifies:
#
#   - an unsupported sample format, and an unsupported rate, fail at start with
#     a typed error naming the format, and put no chunk on the wire;
#   - a granted rtprio ceiling of zero, with the option absent, exits non-zero
#     naming the ceiling it read and the priority it wanted, and plays nothing;
#     with the option set, it starts and every status report says it has no
#     real-time policy;
#   - a locked-memory limit below what the server asks for, with the option
#     absent, exits non-zero naming both numbers and plays nothing; with the
#     option set, it starts and every status report says it is unlocked;
#   - the client cannot open a device that does not exist, reports the device
#     and the reason, exits non-zero, and does not report itself as playing;
#   - the client cannot reach a server that is not there, says which of the two
#     happened, and exits non-zero;
#   - the host-contract entry point, handed a report whose thread inventory did
#     not complete, exits non-zero naming the criterion it was verifying and
#     the reason it could not, and reports nothing as passed or skipped-green.
#
#   ./tools/refusals.sh

source "$(dirname "$0")/lib.sh"

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

run_server() {
    set +e
    OUT="$("$BIN_DIR/chorus-server" "$@" 2>&1)"
    STATUS=$?
    set -e
}

run_client() {
    set +e
    OUT="$("$BIN_DIR/chorus-client" "$@" 2>&1)"
    STATUS=$?
    set -e
}

say "chorus: refusal paths"

# --- an unsupported sample format -------------------------------------------
run_server --format pcm_s20le --listen 127.0.0.1:0
check "unsupported-format-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "unsupported-format-is-named" \
    "$(echo "$OUT" | grep -q 'pcm_s20le' && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "unsupported-format-emits-no-chunk" \
    "$(echo "$OUT" | grep -q 'chunks_sent=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

# --- an unsupported sample rate ---------------------------------------------
run_server --rate 4000 --listen 127.0.0.1:0
check "unsupported-rate-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "unsupported-rate-is-named" \
    "$(echo "$OUT" | grep -q '4000' && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "unsupported-rate-emits-no-chunk" \
    "$(echo "$OUT" | grep -q 'chunks_sent=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

# --- a granted rtprio ceiling of zero ---------------------------------------
CEILING="$(ulimit -r 2>/dev/null || echo 0)"
say "chorus: this host's granted rtprio ceiling is $CEILING"

set +e
OUT="$(bash -c 'ulimit -r 0 2>/dev/null; exec "$0" --listen 127.0.0.1:0 --no-lock-memory' \
    "$BIN_DIR/chorus-server" 2>&1)"
STATUS=$?
set -e
check "ceiling-zero-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "ceiling-zero-names-the-ceiling-and-the-priority" \
    "$(echo "$OUT" | grep -q 'rtprio ceiling is 0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "ceiling-zero-plays-nothing" \
    "$(echo "$OUT" | grep -q 'chunks_sent=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

PORT="$(free_port)"
set +e
OUT="$(bash -c 'ulimit -r 0 2>/dev/null; exec "$0" --listen "127.0.0.1:$1" \
    --no-lock-memory --allow-non-realtime' \
    "$BIN_DIR/chorus-server" "$PORT" 2>&1 &
    SERVER=$!
    sleep 1
    kill "$SERVER" 2>/dev/null
    wait "$SERVER" 2>/dev/null)"
set -e
check "ceiling-zero-with-the-option-starts" \
    "$(echo "$OUT" | grep -q 'listening' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'listening' | head -n 1)"
NON_RT_LINES="$(echo "$OUT" | grep -c 'chorus-server: ' || true)"
NON_RT_SAYING="$(echo "$OUT" | grep -c 'scheduling=no-real-time-policy-by-configuration' || true)"
STATUS_LINES="$(echo "$OUT" | grep -c 'scheduling=' || true)"
check "ceiling-zero-with-the-option-says-so-in-every-status-report" \
    "$([ "$STATUS_LINES" -gt 0 ] && [ "$NON_RT_SAYING" -eq "$STATUS_LINES" ] && echo 1 || echo 0)" \
    "$NON_RT_SAYING of $STATUS_LINES status reports say it, out of $NON_RT_LINES lines"

# --- a locked-memory limit below what the server asks for -------------------
WANTED="$(conf memlock_wanted_bytes)"
set +e
OUT="$(bash -c 'ulimit -l 64 2>/dev/null; ulimit -r 0 2>/dev/null; \
    exec "$0" --listen 127.0.0.1:0 --memlock-wanted-bytes "$1" --allow-non-realtime' \
    "$BIN_DIR/chorus-server" "$WANTED" 2>&1)"
STATUS=$?
set -e
check "memlock-denied-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "memlock-denied-names-the-limit-and-the-amount" \
    "$(echo "$OUT" | grep -q "locked-memory limit read is .* and $WANTED bytes were wanted" \
        && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "memlock-denied-plays-nothing" \
    "$(echo "$OUT" | grep -q 'chunks_sent=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

PORT="$(free_port)"
set +e
OUT="$(bash -c 'ulimit -l 64 2>/dev/null; ulimit -r 0 2>/dev/null; \
    exec "$0" --listen "127.0.0.1:$1" --memlock-wanted-bytes "$2" \
    --allow-non-realtime --allow-unlocked-memory' \
    "$BIN_DIR/chorus-server" "$PORT" "$WANTED" 2>&1 &
    SERVER=$!
    sleep 1
    kill "$SERVER" 2>/dev/null
    wait "$SERVER" 2>/dev/null)"
set -e
check "memlock-denied-with-the-option-starts" \
    "$(echo "$OUT" | grep -q 'listening' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'listening' | head -n 1)"
UNLOCKED="$(echo "$OUT" | grep -c 'memory=unlocked' || true)"
MEMORY_LINES="$(echo "$OUT" | grep -c 'memory=' || true)"
check "memlock-denied-with-the-option-says-so-in-every-status-report" \
    "$([ "$MEMORY_LINES" -gt 0 ] && [ "$UNLOCKED" -eq "$MEMORY_LINES" ] && echo 1 || echo 0)" \
    "$UNLOCKED of $MEMORY_LINES status reports say it"

# --- a device that does not exist -------------------------------------------
run_client --device chorus-no-such-device --probe-device
check "absent-device-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "absent-device-names-the-device-and-the-reason" \
    "$(echo "$OUT" | grep -q 'chorus-no-such-device' && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "absent-device-does-not-claim-playback" \
    "$(echo "$OUT" | grep -q 'usable=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'device-probe' | head -n 1)"

# --- a server that is not there ---------------------------------------------
PORT="$(free_port)"
run_client --server "127.0.0.1:$PORT" --device chorus-no-such-device \
    --delay-log "${TMPDIR:-/tmp}/chorus-refusals-unused.log"
check "absent-server-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "absent-server-says-which-of-the-two-happened" \
    "$(echo "$OUT" | grep -q 'could not be reached at start' && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "absent-server-does-not-claim-playback" \
    "$(echo "$OUT" | grep -q 'played=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

# --- a host contract whose thread inventory did not complete ----------------
#
# The scheduling half of the host contract is graded from the server's own
# inventory of its threads. An inventory that did not complete cannot say
# whether a thread is real-time without having been reported, so the entry
# point has to refuse rather than read the report for an answer that is not in
# it. "Zero undeclared real-time threads" out of a list that lost a thread is
# the failure this whole check exists to prevent, and it looks exactly like
# success.
INCOMPLETE_REPORT="${TMPDIR:-/tmp}/chorus-incomplete-inventory.report"
INCOMPLETE_REASON="cannot read the scheduling record of thread 41: Permission denied (errno 13)"
{
    printf 'chorus-server: scheduling=real-time rtprio_ceiling=20 rtprio_obtained=20 rttime_us=200000\n'
    printf 'chorus-server: INCOMPLETE-THREAD-INVENTORY reason=%s\n' "$INCOMPLETE_REASON"
    printf 'chorus-server: scheduling-report inventory=incomplete undeclared_real_time=unknown reason=%s\n' \
        "$INCOMPLETE_REASON"
} > "$INCOMPLETE_REPORT"

set +e
OUT="$(CHORUS_HOST_CONTRACT_REPORT="$INCOMPLETE_REPORT" CHORUS_SKIP_BUILD=1 \
    bash "$REPO_ROOT/tools/host-contract.sh" 2>&1)"
STATUS=$?
set -e
check "incomplete-inventory-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "incomplete-inventory-names-the-criterion" \
    "$(echo "$OUT" | grep -q 'criterion:.*no thread is real-time without being reported' \
        && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'criterion:' | head -n 1)"
check "incomplete-inventory-names-the-reason" \
    "$(echo "$OUT" | grep -q "reason:.*Permission denied" && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'reason:' | head -n 1)"
check "incomplete-inventory-claims-nothing" \
    "$(echo "$OUT" | grep -qiE '^(pass|ok|skipped|green)' && echo 0 || echo 1)" \
    "$(echo "$OUT" | grep 'NOT passed' | head -n 1)"

# And the same entry point still grades a complete report on its merits, so the
# refusal above is about the inventory and not about the grading being broken.
COMPLETE_REPORT="${TMPDIR:-/tmp}/chorus-complete-inventory.report"
{
    printf 'chorus-server: scheduling=real-time rtprio_ceiling=20 rtprio_obtained=20 rttime_us=200000\n'
    printf 'chorus-server: thread role=audio tid=41 declared_real_time=1 declared_priority=20 declared_rttime_us=200000 kernel_policy=SCHED_FIFO kernel_priority=20\n'
    printf 'chorus-server: scheduling-report threads=1 vanished=0 real_time_declared=1 undeclared_real_time=0\n'
} > "$COMPLETE_REPORT"
set +e
OUT="$(CHORUS_HOST_CONTRACT_REPORT="$COMPLETE_REPORT" CHORUS_SKIP_BUILD=1 \
    bash "$REPO_ROOT/tools/host-contract.sh" 2>&1)"
STATUS=$?
set -e
check "complete-inventory-still-grades-clean" \
    "$([ "$STATUS" -eq 0 ] && echo 1 || echo 0)" \
    "exit $STATUS: $(echo "$OUT" | grep '^pass' | head -n 1)"

# --- a configuration whose bounds no run could cross ------------------------
run_client --min-us 60000 --max-us 60000000 --start-fill-us 120000 \
    --device-target-us 120000 --probe-device
check "uncrossable-bounds-are-refused" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "uncrossable-bounds-say-why" \
    "$(echo "$OUT" | grep -q 'not under the 600 s' && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: every refusal path holds"
    exit 0
fi
say "chorus: $FAILURES refusal checks failed"
exit 1
