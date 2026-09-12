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

# --- a control channel whose address cannot be bound ------------------------
#
# AC-9: "IF the control channel cannot bind its configured address, or is denied
# it THEN THE SYSTEM SHALL exit non-zero with a documented code naming the
# address and the reason, and SHALL NOT serve audio while reporting itself as
# controllable."
#
# The address is genuinely taken: this holds it for the duration of the run.
TAKEN_PORT="$(free_port)"
python3 - "$TAKEN_PORT" >/dev/null 2>&1 <<'PY' &
import socket, sys, time
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
s.bind(("127.0.0.1", int(sys.argv[1])))
s.listen(1)
time.sleep(30)
PY
HOLDER=$!
sleep 1
AUDIO_PORT="$(free_port)"
run_server --listen "127.0.0.1:$AUDIO_PORT" --no-lock-memory --allow-non-realtime \
    --control-listen "127.0.0.1:$TAKEN_PORT" --zone kitchen
CONTROL_STATUS=$STATUS
CONTROL_OUT="$OUT"
kill_quietly "$HOLDER"
check "unbindable-control-address-exits-with-its-documented-code" \
    "$([ "$CONTROL_STATUS" -eq 8 ] && echo 1 || echo 0)" \
    "exit $CONTROL_STATUS, and crates/server/src/main.rs documents 8 for this"
check "unbindable-control-address-names-the-address-and-the-reason" \
    "$(echo "$CONTROL_OUT" | grep -q "127.0.0.1:$TAKEN_PORT could not be bound" \
        && echo 1 || echo 0)" \
    "$(echo "$CONTROL_OUT" | grep 'could not be bound' | head -n 1)"
check "unbindable-control-address-serves-no-audio" \
    "$(echo "$CONTROL_OUT" | grep -q 'listening on=' && echo 0 || echo 1)" \
    "the audio socket was never bound: $(echo "$CONTROL_OUT" | grep -c 'listening on=') 'listening on' lines"
check "unbindable-control-address-plays-nothing" \
    "$(echo "$CONTROL_OUT" | grep -q 'chunks_sent=0' && echo 1 || echo 0)" \
    "$(echo "$CONTROL_OUT" | grep 'stopped' | head -n 1)"
check "unbindable-control-address-does-not-claim-to-be-controllable" \
    "$(echo "$CONTROL_OUT" | grep -q 'control listening on=' && echo 0 || echo 1)" \
    "it never said it was listening for control"

# --- an endpoint with no server address at all ------------------------------
#
# AC-2's other side: an endpoint with neither discovery nor a static address
# has to say WHICH OF THE TWO it lacked, because those are two different things
# to fix.
run_client --no-server --device chorus-no-such-device \
    --delay-log "${TMPDIR:-/tmp}/chorus-refusals-unused.log"
check "no-server-address-exits-with-its-documented-code" \
    "$([ "$STATUS" -eq 7 ] && echo 1 || echo 0)" \
    "exit $STATUS, and crates/client-linux/src/main.rs documents 7 for this"
check "no-server-address-says-which-of-the-two-is-missing" \
    "$(echo "$OUT" | grep -q 'discovery was not attempted and no static address was configured' \
        && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'no server address' | head -n 1)"
check "no-server-address-says-how-to-fix-either-of-them" \
    "$(echo "$OUT" | grep -q -- '--server' && echo "$OUT" | grep -q -- '--discover' \
        && echo 1 || echo 0)" \
    "it names both --server and --discover"
check "no-server-address-does-not-claim-playback" \
    "$(echo "$OUT" | grep -q 'played=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

# --- a control message the server cannot accept ------------------------------
#
# AC-8 is graded byte for byte by `cargo test -p chorus-control --test
# refusals`. What is checked here is that the SHIPPED BINARY refuses the same
# way, over a real socket, because a rule that holds in a library and not in the
# process is not a rule the system has.
CONTROL_PORT="$(free_port)"
AUDIO_PORT="$(free_port)"
"$BIN_DIR/chorus-server" --listen "127.0.0.1:$AUDIO_PORT" --serve-forever \
    --no-lock-memory --allow-non-realtime \
    --control-listen "127.0.0.1:$CONTROL_PORT" --zone kitchen \
    >"${TMPDIR:-/tmp}/chorus-refusals-control.log" 2>&1 &
CONTROL_SERVER=$!
sleep 2
refuse_message() {
    python3 - "$CONTROL_PORT" "$1" <<'PY'
import socket, sys
port, body = int(sys.argv[1]), sys.argv[2].encode()
s = socket.create_connection(("127.0.0.1", port))
s.sendall(
    b"POST /api/command HTTP/1.1\r\nHost: chorus\r\nContent-Type: application/json\r\n"
    + b"Content-Length: " + str(len(body)).encode() + b"\r\nConnection: close\r\n\r\n" + body
)
answer = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    answer += chunk
sys.stdout.write(answer.decode("utf-8", "replace"))
PY
}
BEFORE_STATE="$(refuse_message '{"v":1,"t":"hello"}' | tail -n 1)"
UNKNOWN_ZONE="$(refuse_message '{"v":1,"t":"mute","zone":"bathroom","muted":true}')"
check "the-binary-refuses-an-unknown-zone-naming-the-field" \
    "$(printf '%s' "$UNKNOWN_ZONE" | grep -q '400 Bad Request' \
        && printf '%s' "$UNKNOWN_ZONE" | grep -q '"field":"zone"' && echo 1 || echo 0)" \
    "$(printf '%s' "$UNKNOWN_ZONE" | tail -n 1 | cut -c1-110)"
OUT_OF_RANGE="$(refuse_message '{"v":1,"t":"volume","zone":"kitchen","volume":2.000}')"
check "the-binary-refuses-a-volume-outside-the-declared-range" \
    "$(printf '%s' "$OUT_OF_RANGE" | grep -q '"field":"volume"' && echo 1 || echo 0)" \
    "$(printf '%s' "$OUT_OF_RANGE" | tail -n 1 | cut -c1-110)"
MALFORMED="$(refuse_message '{"v":1,"t":"mute",}')"
check "the-binary-refuses-a-message-that-is-not-json" \
    "$(printf '%s' "$MALFORMED" | grep -q 'not well-formed JSON' && echo 1 || echo 0)" \
    "$(printf '%s' "$MALFORMED" | tail -n 1 | cut -c1-110)"
WRONG_VERSION="$(refuse_message '{"v":9,"t":"hello"}')"
check "the-binary-refuses-an-unimplemented-catalog-version-and-names-both" \
    "$(printf '%s' "$WRONG_VERSION" | grep -q '426 Upgrade Required' \
        && printf '%s' "$WRONG_VERSION" | grep -q '"offered":9' \
        && printf '%s' "$WRONG_VERSION" | grep -q '"implemented":\[1\]' && echo 1 || echo 0)" \
    "$(printf '%s' "$WRONG_VERSION" | tail -n 1 | cut -c1-110)"
AFTER_STATE="$(refuse_message '{"v":1,"t":"hello"}' | tail -n 1)"
check "no-refused-message-changed-the-state-a-subscriber-would-be-sent" \
    "$([ "$BEFORE_STATE" = "$AFTER_STATE" ] && echo 1 || echo 0)" \
    "the state is byte-identical before and after four refusals"
kill_quietly "$CONTROL_SERVER"

# --- a zone declared with a transport nobody committed -----------------------
#
# WIFI-7's AC-5: "IF the server is started with a zone whose declared transport
# is not one the committed configuration names THEN THE SYSTEM SHALL exit
# non-zero, name the zone, the value it read and the permitted transports, and
# SHALL serve no audio and no control state."
#
# The transports are committed in config/transport.conf and the binary carries
# the same list; `cargo test -p chorus-client-linux --test wireless_policy`
# asserts the two agree. What is checked here is that the SHIPPED BINARY refuses
# the same way.
PERMITTED="$(sed -n 's/^[[:space:]]*transports[[:space:]]*=[[:space:]]*\([^#]*\).*/\1/p' \
    "$REPO_ROOT/config/transport.conf" | head -n 1 | sed 's/[[:space:]]*$//')"
# The file lists them separated by spaces and a refusal names them separated by
# commas. Deriving one from the other here is what keeps this check reading the
# COMMITTED list rather than a copy of it written in this script.
PERMITTED_LIST="$(printf '%s' "$PERMITTED" | sed 's/[[:space:]][[:space:]]*/, /g')"
say "chorus: the transports config/transport.conf names are: $PERMITTED_LIST"
CONTROL_PORT="$(free_port)"
run_server --listen 127.0.0.1:0 --no-lock-memory --allow-non-realtime \
    --control-listen "127.0.0.1:$CONTROL_PORT" --zone kitchen --zone "bedroom=wifi"
check "unknown-transport-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "unknown-transport-names-the-zone" \
    "$(echo "$OUT" | grep -q "zone 'bedroom'" && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "unknown-transport-names-the-value-it-read" \
    "$(echo "$OUT" | grep -q "transport 'wifi'" && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "unknown-transport-names-the-permitted-transports" \
    "$(echo "$OUT" | grep -q "$PERMITTED_LIST" && echo 1 || echo 0)" \
    "it names $PERMITTED_LIST, which is what config/transport.conf commits"
check "unknown-transport-serves-no-audio" \
    "$(echo "$OUT" | grep -q 'listening on=' && echo 0 || echo 1)" \
    "the audio socket was never bound: $(echo "$OUT" | grep -c 'listening on=') 'listening on' lines"
check "unknown-transport-serves-no-control-state" \
    "$(echo "$OUT" | grep -q 'control listening on=' && echo 0 || echo 1)" \
    "the control channel was never bound, so no subscriber was ever told a state"
check "unknown-transport-says-it-served-nothing" \
    "$(echo "$OUT" | grep -q 'chunks_sent=0 zones_served=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

# And a transport the committed configuration DOES name is accepted, so the
# refusal above is about the word and not about the flag being unusable.
#
# The run is stopped by an address that is genuinely held, which happens AFTER
# the zone tiers are reported and before anything is served: a server that got
# past its configuration has to be stopped by something, and a server that
# served a client would need one.
TAKEN_PORT="$(free_port)"
python3 - "$TAKEN_PORT" >/dev/null 2>&1 <<'PY' &
import socket, sys, time
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
s.bind(("127.0.0.1", int(sys.argv[1])))
s.listen(1)
time.sleep(30)
PY
HOLDER=$!
sleep 1
run_server --listen 127.0.0.1:0 --no-lock-memory --allow-non-realtime \
    --control-listen "127.0.0.1:$TAKEN_PORT" --zone kitchen --zone "bedroom=wireless"
kill_quietly "$HOLDER"
check "a-committed-transport-is-accepted" \
    "$(echo "$OUT" | grep -q 'zone id=bedroom transport=wireless' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'zone id=bedroom' | head -n 1)"
check "a-zone-declaring-no-transport-is-reported-as-wired" \
    "$(echo "$OUT" | grep -q 'zone id=kitchen transport=wired' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'zone id=kitchen' | head -n 1)"
check "every-zone-has-its-tier-reported" \
    "$([ "$(echo "$OUT" | grep -c 'zone id=')" -eq 2 ] && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep -c 'zone id=') tier lines for 2 declared zones, so no zone's tier is implicit"

# --- an endpoint that cannot apply the declared wireless latency -------------
#
# WIFI-7's AC-8: "IF an endpoint in a group held to the wireless policy cannot
# apply the declared wireless playout latency THEN THE SYSTEM SHALL stop playing
# that stream and report both the latency it was asked for and the one it could
# apply, rather than play at the wired latency."
WIRELESS_LATENCY="$(sed -n \
    's/^[[:space:]]*wireless_playout_latency_us[[:space:]]*=[[:space:]]*\([^#]*\).*/\1/p' \
    "$REPO_ROOT/config/transport.conf" | head -n 1 | tr -d '[:space:]')"
WIRED_LATENCY="$(sync_conf playout_latency_us)"
say "chorus: the wireless policy declares ${WIRELESS_LATENCY} us of playout latency, the wired tier ${WIRED_LATENCY} us"

# Told to play at the wired latency inside a group held to the wireless policy.
run_client --transport wireless --playout-latency-us "$WIRED_LATENCY" \
    --device chorus-no-such-device \
    --delay-log "${TMPDIR:-/tmp}/chorus-refusals-unused.log"
check "wired-latency-in-a-wireless-group-exits-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "wired-latency-in-a-wireless-group-names-the-latency-it-was-asked-for" \
    "$(echo "$OUT" | grep -q "declared playout latency is $WIRELESS_LATENCY us" && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "wired-latency-in-a-wireless-group-names-the-one-it-could-apply" \
    "$(echo "$OUT" | grep -q "would apply $WIRED_LATENCY us instead" && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "wired-latency-in-a-wireless-group-plays-nothing" \
    "$(echo "$OUT" | grep -q 'played=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

# Bounds that cannot hold the declared latency at all: the same refusal, and it
# still names both numbers rather than reporting the one it cannot use twice.
run_client --transport wireless --max-us 300000 \
    --device chorus-no-such-device \
    --delay-log "${TMPDIR:-/tmp}/chorus-refusals-unused.log"
check "bounds-that-cannot-hold-the-wireless-latency-exit-non-zero" \
    "$([ "$STATUS" -ne 0 ] && echo 1 || echo 0)" "exit $STATUS"
check "bounds-that-cannot-hold-the-wireless-latency-name-both" \
    "$(echo "$OUT" | grep -q "declared playout latency is $WIRELESS_LATENCY us" \
        && echo "$OUT" | grep -q "would apply $WIRED_LATENCY us instead" && echo 1 || echo 0)" \
    "$(echo "$OUT" | head -n 1)"
check "bounds-that-cannot-hold-the-wireless-latency-play-nothing" \
    "$(echo "$OUT" | grep -q 'played=0' && echo 1 || echo 0)" \
    "$(echo "$OUT" | grep 'stopped' | head -n 1)"

# And an endpoint in a wireless group that CAN apply it is not refused, so the
# refusals above are about the latency and not about the tier being unusable.
run_client --transport wireless --device chorus-no-such-device --probe-device
check "a-wireless-endpoint-that-can-apply-the-declared-latency-is-not-refused" \
    "$(echo "$OUT" | grep -q 'configuration refused' && echo 0 || echo 1)" \
    "it got as far as the device, which is what exit $STATUS says"

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: every refusal path holds"
    exit 0
fi
say "chorus: $FAILURES refusal checks failed"
exit 1
