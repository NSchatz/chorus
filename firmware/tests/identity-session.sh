#!/usr/bin/env bash
# Goal 14, the firmware half of adoption, over a real socket against a real
# server process: a C endpoint that KEEPS its identity.
#
# Before goal 14 a board presented the id `chorus-endpoint` and a key made
# fresh at every boot, so the server adopted its first boot and refused its
# second with key_changed. firmware/tests/test_identity.c grades the unit that
# ends that over a fake store; this grades the whole path the board runs
# (chorus/identity.h through firmware/src/session.c) with the store as a
# directory of files, the real `chorus-server` at HEAD on loopback, and a
# "reboot" that is the endpoint process started again over the same store:
#
#   1. A first boot with an empty store makes an id of the form
#      `chorus-<12 hex>` and a key, the server adopts it, and it plays.
#   2. A reboot presents the SAME id and the SAME key: the server knows it
#      (verdict=known), nothing is adopted twice, nothing is refused.
#   3. A second board (a second empty store) is a second id.
#   4. With no address given at all, the endpoint goes back to the server it
#      last shook hands with (the store's `server_addr`), which is discovery's
#      first fallback.
#   5. A server that comes back with ANOTHER KEY is still refused with the pin
#      kept, exactly as firmware/tests/session-outage.sh grades it for the file
#      paths: the store changes where the pin lives, not what it means.
#
# No multicast here (that is `make verify-endpoint-mdns`), no audio device, no
# privilege, loopback only.
#
#   bash firmware/tests/identity-session.sh      # or: make firmware-check

set -euo pipefail

FW="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../../tools/lib.sh
source "$FW/../tools/lib.sh"

BUILD="${BUILD:-$FW/build}"
ENDPOINT="$BUILD/chorus-endpoint-session"

build_once

if [ ! -x "$ENDPOINT" ]; then
    say "FAIL $ENDPOINT has not been built"
    exit 1
fi
SERVER="$BIN_DIR/chorus-server"
if [ ! -x "$SERVER" ]; then
    say "FAIL $SERVER has not been built"
    exit 1
fi

STATE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/chorus-endpoint-identity.XXXXXX")"
SERVER_PID=""
SERVER_LOG="$STATE_ROOT/server.log"
IDENTITY_DIR="$STATE_ROOT/server-identity"
mkdir -p "$IDENTITY_DIR"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"

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

start_server() {
    "$SERVER" \
        --listen "127.0.0.1:$1" \
        --identity-dir "$IDENTITY_DIR" \
        --source tone \
        --serve-forever \
        --rttime-us "$(conf rttime_us)" \
        --rt-priority "$(conf rt_priority)" \
        --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
        "${CONTRACT_ARGS[@]}" >>"$SERVER_LOG" 2>&1 &
    SERVER_PID=$!
}

kill_server_hard() {
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill -9 "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    SERVER_PID=""
}
trap 'kill_server_hard; rm -rf "$STATE_ROOT"' EXIT

field() {
    # One `key=value` field out of the endpoint's summary lines.
    sed -n "s/.*[[:space:]]$1=\\([^ ]*\\).*/\\1/p" "$2" | tail -n 1
}

# One run of the endpoint over a store: run <name> <store> <more arguments>.
# Leaves $STATUS, $STATE_ROOT/<name>.log (events) and .summary (stdout+stderr).
STATUS=0
run() {
    local name="$1"
    local store="$2"
    shift 2
    set +e
    "$ENDPOINT" --store "$store" --run-seconds 3 --log "$STATE_ROOT/$name.log" "$@" \
        >"$STATE_ROOT/$name.summary" 2>&1
    STATUS=$?
    set -e
    sed 's/^/    /' "$STATE_ROOT/$name.summary"
}

identity_of() {
    # The `id=` and `key=` of a run's start line.
    sed -n 's/.*event=start detail="identity id=\([^ ]*\) key=\([^ ]*\) store=store".*/\1 \2/p' \
        "$STATE_ROOT/$1.log" | head -n 1
}

PORT="$(free_port)"
STORE_A="$STATE_ROOT/board-a"
STORE_B="$STATE_ROOT/board-b"
start_server "$PORT"
sleep 1

# --- 1. the first boot ---------------------------------------------------------

say ""
say "chorus: 1 of 5, a first boot with an empty store"
run first "$STORE_A" --server "127.0.0.1:$PORT"
read -r ID_FIRST KEY_FIRST <<< "$(identity_of first)"
check "first-boot-ran" \
    "$([ "$STATUS" -eq 0 ] && echo 1 || echo 0)" \
    "exit $STATUS"
check "first-boot-made-an-id-of-the-form" \
    "$(printf '%s' "${ID_FIRST:-}" | grep -Eq '^chorus-[0-9a-f]{12}$' && echo 1 || echo 0)" \
    "the id is ${ID_FIRST:-none}: chorus- and twelve hex digits, from the random source"
check "first-boot-kept-it" \
    "$([ "$(cat "$STORE_A/id" 2>/dev/null)" = "${ID_FIRST:-x}" ] \
        && [ "$(wc -c < "$STORE_A/noise_key" 2>/dev/null)" = "32" ] && echo 1 || echo 0)" \
    "the store holds the id and a 32-byte key"
check "first-boot-was-adopted" \
    "$(grep -c "endpoint adopted id=${ID_FIRST:-x} key=${KEY_FIRST:-x}" "$SERVER_LOG" \
        | awk '{print ($1 == 1) ? 1 : 0}')" \
    "$(grep -o "endpoint adopted id=${ID_FIRST:-x} key=[^ ]*" "$SERVER_LOG" | head -n 1)"
check "first-boot-pinned-the-server" \
    "$([ "$(field servers_pinned "$STATE_ROOT/first.summary")" = "1" ] \
        && grep -q '^pinned ' "$STORE_A/server_pins" && echo 1 || echo 0)" \
    "the server's pin is in the store"
check "first-boot-played" \
    "$([ "$(field chunks "$STATE_ROOT/first.summary")" -gt 0 ] && echo 1 || echo 0)" \
    "$(field chunks "$STATE_ROOT/first.summary") chunks arrived"
check "first-boot-kept-the-servers-address" \
    "$([ "$(cat "$STORE_A/server_addr" 2>/dev/null)" = "127.0.0.1:$PORT" ] && echo 1 || echo 0)" \
    "the store's last server is $(cat "$STORE_A/server_addr" 2>/dev/null || echo none)"

# --- 2. the reboot --------------------------------------------------------------

say ""
say "chorus: 2 of 5, a reboot: the same store, a new process"
PINS_FIRST="$(cat "$STORE_A/server_pins")"
run second "$STORE_A" --server "127.0.0.1:$PORT"
read -r ID_SECOND KEY_SECOND <<< "$(identity_of second)"
check "reboot-same-id" \
    "$([ -n "${ID_SECOND:-}" ] && [ "${ID_SECOND:-}" = "${ID_FIRST:-x}" ] && echo 1 || echo 0)" \
    "the id is ${ID_SECOND:-none} again"
check "reboot-same-key" \
    "$([ -n "${KEY_SECOND:-}" ] && [ "${KEY_SECOND:-}" = "${KEY_FIRST:-x}" ] && echo 1 || echo 0)" \
    "the key's fingerprint is ${KEY_SECOND:-none} again"
check "reboot-known-not-readopted" \
    "$([ "$(grep -c "endpoint adopted id=${ID_FIRST:-x} " "$SERVER_LOG")" = "1" ] \
        && grep -q "id=${ID_FIRST:-x} key=${KEY_FIRST:-x} verdict=known" "$SERVER_LOG" \
        && echo 1 || echo 0)" \
    "the server adopted it once and knew it the second time: $(grep -o "id=${ID_FIRST:-x} key=[^ ]* verdict=known" "$SERVER_LOG" | head -n 1)"
check "reboot-not-refused" \
    "$([ "$STATUS" -eq 0 ] && [ "$(field refusals "$STATE_ROOT/second.summary")" = "0" ] \
        && ! grep -q 'endpoint key changed' "$SERVER_LOG" && echo 1 || echo 0)" \
    "exit $STATUS, no refusal either way: the second boot is the speaker the first was"
check "reboot-played" \
    "$([ "$(field chunks "$STATE_ROOT/second.summary")" -gt 0 ] && echo 1 || echo 0)" \
    "$(field chunks "$STATE_ROOT/second.summary") chunks arrived"
check "reboot-wrote-no-new-pin" \
    "$([ "$(field servers_pinned "$STATE_ROOT/second.summary")" = "0" ] \
        && [ "$(cat "$STORE_A/server_pins")" = "$PINS_FIRST" ] && echo 1 || echo 0)" \
    "the server was known by its pin, and the pin is unchanged"

# --- 3. a second board ----------------------------------------------------------

say ""
say "chorus: 3 of 5, a second board: a second empty store"
run other "$STORE_B" --server "127.0.0.1:$PORT"
read -r ID_OTHER KEY_OTHER <<< "$(identity_of other)"
check "second-board-is-another-id" \
    "$(printf '%s' "${ID_OTHER:-}" | grep -Eq '^chorus-[0-9a-f]{12}$' \
        && [ "${ID_OTHER:-}" != "${ID_FIRST:-}" ] && [ "${KEY_OTHER:-}" != "${KEY_FIRST:-}" ] \
        && echo 1 || echo 0)" \
    "${ID_OTHER:-none} is not ${ID_FIRST:-none}"
check "second-board-adopted-beside-the-first" \
    "$(grep -c "endpoint adopted id=${ID_OTHER:-x} " "$SERVER_LOG" | awk '{print ($1 == 1) ? 1 : 0}')" \
    "the server adopted it as a new speaker"

# --- 4. no address at all: the last server that answered -----------------------

say ""
say "chorus: 4 of 5, no address given: back to the last server it shook hands with"
run lastgood "$STORE_A" --no-server
check "last-good-located" \
    "$(grep -q "server-located how=last-good address=127.0.0.1:$PORT because=discovery was not attempted" \
        "$STATE_ROOT/lastgood.summary" && echo 1 || echo 0)" \
    "$(grep 'server-located' "$STATE_ROOT/lastgood.summary" | head -n 1)"
check "last-good-played" \
    "$([ "$STATUS" -eq 0 ] && [ "$(field chunks "$STATE_ROOT/lastgood.summary")" -gt 0 ] \
        && echo 1 || echo 0)" \
    "exit $STATUS, $(field chunks "$STATE_ROOT/lastgood.summary") chunks arrived from it"
run nowhere "$STATE_ROOT/board-c" --no-server
check "nowhere-is-said-by-name" \
    "$([ "$STATUS" -eq 7 ] && grep -q 'server-located how=nowhere' "$STATE_ROOT/nowhere.summary" \
        && grep -q 'has no server address' "$STATE_ROOT/nowhere.summary" && echo 1 || echo 0)" \
    "a board that has met no server and is given no address exits $STATUS saying so"

# --- 5. a server that comes back with another key ------------------------------

say ""
say "chorus: 5 of 5, a server that comes back with another key"
kill_server_hard
IDENTITY_DIR="$STATE_ROOT/another-server-identity"
mkdir -p "$IDENTITY_DIR"
: >"$SERVER_LOG"
start_server "$PORT"
sleep 1
run changed "$STORE_A" --server "127.0.0.1:$PORT"
check "key-changed-refused" \
    "$(grep -c 'event=server-key-changed' "$STATE_ROOT/changed.log" | awk '{print ($1 == 1) ? 1 : 0}')" \
    "$(grep -o 'event=server-key-changed detail="[^"]*"' "$STATE_ROOT/changed.log" | head -n 1)"
check "key-changed-stopped" \
    "$([ "$STATUS" -eq 4 ] && grep -q 'server-key-changed:' "$STATE_ROOT/changed.summary" \
        && echo 1 || echo 0)" \
    "the endpoint stopped (exit $STATUS) rather than treating another key as an outage"
check "key-changed-pin-kept" \
    "$([ -n "$PINS_FIRST" ] && [ "$(cat "$STORE_A/server_pins")" = "$PINS_FIRST" ] && echo 1 || echo 0)" \
    "the store's pin is what it was before the refusal"
check "key-changed-identity-kept" \
    "$([ "$(cat "$STORE_A/id")" = "${ID_FIRST:-x}" ] && echo 1 || echo 0)" \
    "and the endpoint is still ${ID_FIRST:-none}"
check "key-changed-server-saw-the-refusal" \
    "$(grep -c 'reason=refused-by-peer' "$SERVER_LOG" | awk '{print ($1 >= 1) ? 1 : 0}')" \
    "the new server logged the endpoint's session_refused: $(grep -o 'reason=refused-by-peer detail="[^"]*"' "$SERVER_LOG" | head -n 1)"
kill_server_hard

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: a C endpoint keeps its id, its key and its server's pin across boots, over a real socket"
    exit 0
fi
say "chorus: $FAILURES identity checks did not hold"
exit 1
