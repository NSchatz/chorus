#!/usr/bin/env bash
# The endpoint image, booted in an emulator: it gets an address, finds the
# server, is adopted under the identity it keeps, and is the same speaker after
# a reboot (goal 14, docs/decisions/0109-*).
#
# What runs is the image as built for the emulator's board profile
# (firmware/boards/qemu-s3-openeth.conf), started by ESP-IDF's own second-stage
# bootloader from a flash file laid out the way a board's flash is. The store is
# NVS in that flash, the identity is the one firmware/src/identity.c makes and
# keeps there, the session is firmware/src/session.c over lwIP, and the server
# is a real `chorus-server` on loopback, reached through the emulator's user
# network. What the emulator does not have, the image does not bring up: the
# amplifier, I2S, the W5500, the radio.
#
# The order, and what is graded:
#   1. the profile's image is built (tools/firmware-image.sh, with its safety
#      scans), and a flash file and a fresh eFuse file are made from it;
#   2. first boot: the link comes up with an address, the board makes an id
#      (`chorus-` and 12 hex digits) and a key and says so on its console, and
#      the server adopts that id with that key: its log and its /api/state;
#   3. the emulator is stopped (the power goes away) and the server sees the
#      speaker leave;
#   4. second boot, the same flash file: the board reads the same id and the
#      same key back from NVS, making neither, and the server knows it
#      (`verdict=known`, never a changed key);
#   5. the eFuse file is byte for byte what it was before the first boot.
#
# Source: simulation. Nothing here is timing evidence, and nothing here says a
# board works: the bench packet's sessions do that.
#
# NEEDS AN ENVIRONMENT, and refuses by name without it (never a green skip):
#   - the pinned emulator and its libraries (bash tools/qemu-env.sh install)
#   - ESP-IDF at the version firmware/config/endpoint.conf declares
#   - the server's audio port on loopback, free: the image reaches the server at
#     the committed server_address's port, so the port is not this run's choice
#
#   make qemu-boot

source "$(dirname "$0")/lib.sh"
# shellcheck source=qemu/lib.sh
source "$REPO_ROOT/tools/qemu/lib.sh"

CRITERION="the endpoint image boots in the emulator, is adopted by a real server under its stored identity, and keeps that identity across a reboot"
PROFILE=qemu-s3-openeth
IMAGE="${CHORUS_IMAGE_OUT:-$REPO_ROOT/firmware/build/$PROFILE}"
OUT="${CHORUS_QEMU_OUT:-$TARGET_DIR/qemu-boot}"
BOOT_SECONDS=120
WAIT_SECONDS=60

say "chorus: the endpoint image in the emulator (Source: simulation; not timing evidence)"
say "  criterion: $CRITERION"

# Every prerequisite before anything is built or started.
qemu_require "$CRITERION"
qemu_idf_env
require_espidf "$CRITERION"
PORT="$(endpoint_conf server_address)"
PORT="${PORT##*:}"
if ! python3 - "$PORT" <<'PY'
import socket, sys
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    s.bind(("127.0.0.1", int(sys.argv[1])))
except OSError:
    sys.exit(1)
s.close()
PY
then
    missing_prerequisite \
        "$CRITERION" \
        "loopback TCP port $PORT, free: something is listening on it" \
        "stop whatever holds 127.0.0.1:$PORT (the image reaches its server at the port of firmware/config/endpoint.conf's server_address)"
fi

STARTED=$(date +%s)
rm -rf "$OUT"
mkdir -p "$OUT"

FAILURES=0
check() {
    local name="$1" ok="$2" detail="$3"
    if [ "$ok" = "1" ]; then
        say "pass $name: $detail"
    else
        say "FAIL $name: $detail"
        FAILURES=$((FAILURES + 1))
    fi
}

QEMU_PID=""
SERVER_PID=""
stop_everything() {
    qemu_stop
    kill_quietly "$SERVER_PID"
    SERVER_PID=""
}
trap stop_everything EXIT

# --- 1. the image, the flash file, the eFuse file ----------------------------

say ""
say "chorus: building the $PROFILE image (log: $OUT/build.log)"
if ! CHORUS_BOARD_PROFILE="$PROFILE" CHORUS_IMAGE_OUT="$IMAGE" bash "$REPO_ROOT/tools/firmware-image.sh" \
    > "$OUT/build.log" 2>&1; then
    tail -n 30 "$OUT/build.log"
    say "qemu-boot: the $PROFILE image did not build: FAIL"
    exit 1
fi
command grep -E '^(chorus: image built|firmware image guard: )' "$OUT/build.log" | sed 's/^/  /' || true
build_once
BUILT=$(date +%s)

qemu_flash_image "$IMAGE" "$OUT/flash.bin" | sed 's/^/  /'
qemu_fresh_efuses "$OUT/efuse.bin"
EFUSES_BEFORE="$(sha256sum "$OUT/efuse.bin" | cut -d' ' -f1)"
say "  eFuse file: $(wc -c < "$OUT/efuse.bin" | tr -d ' ') bytes, the chip's defaults, sha256 $EFUSES_BEFORE"

# --- the server ---------------------------------------------------------------

CONTROL="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
mkdir -p "$OUT/identity"
"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --control-listen "127.0.0.1:$CONTROL" \
    --identity-dir "$OUT/identity" \
    --state-file "$OUT/zones.state" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rttime-us "$(conf rttime_us)" \
    --rt-priority "$(conf rt_priority)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
    --serve-forever \
    "${CONTRACT_ARGS[@]}" > "$OUT/server.log" 2>&1 &
SERVER_PID=$!
wait_for_line "$OUT/server.log" 10 'control listening on=' || true
check "the-server-is-up" \
    "$(kill -0 "$SERVER_PID" 2> /dev/null && command grep -q 'control listening on=' "$OUT/server.log" && echo 1 || echo 0)" \
    "a real chorus-server on 127.0.0.1:$PORT, control on 127.0.0.1:$CONTROL"

# speaker <id>: "<present> <key>" for that speaker in the server's /api/state,
# or nothing when the state lists no such speaker.
speaker() {
    python3 - "$CONTROL" "$1" <<'PY' 2> /dev/null || true
import json, socket, sys
port, wanted = int(sys.argv[1]), sys.argv[2]
s = socket.create_connection(("127.0.0.1", port), timeout=5)
s.sendall(b"GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n")
answer = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    answer += chunk
state = json.loads(answer.split(b"\r\n\r\n", 1)[1].decode("utf-8"))
for one in state.get("speakers", []):
    if one.get("id") == wanted:
        print("%s %s" % ("present" if one.get("present") else "absent", one.get("key", "")))
PY
}

# wait_for_speaker <id> <present|absent> <seconds>
wait_for_speaker() {
    local id="$1" want="$2" seconds="$3" i
    for ((i = 0; i < seconds * 2; i++)); do
        case "$(speaker "$id")" in "$want "*) return 0 ;; esac
        sleep 0.5
    done
    return 1
}

# The identity line the board prints once the store has given or made one
# (firmware/main/esp_identity.c).
IDENTITY_LINE='identity id=chorus-[0-9a-f]{12} key=[^ ]+ id_made_this_boot=[01] key_made_this_boot=[01]'
identity_field() { # identity_field <serial log> <field>
    { command grep -a -o -E "$IDENTITY_LINE" "$1" | head -n 1 | tr ' ' '\n' | sed -n "s/^$2=//p"; } || true
}
# A panic, an abort or a reset the image did not ask for.
crashed() {
    command grep -a -c -E 'Guru Meditation|abort\(\) was called|Backtrace:|Rebooting\.\.\.' "$1" || true
}

# boot <n>: start the emulator on the one flash file and wait for the session.
boot() {
    local n="$1" serial="$OUT/serial-$1.log"
    qemu_boot "$OUT/flash.bin" "$OUT/efuse.bin" "$serial" "$BOOT_SECONDS"
    wait_for_line "$serial" "$WAIT_SECONDS" 'link=emulated phy=openeth status=' || true
    check "boot-$n-the-link-came-up-with-an-address" \
        "$(command grep -a -q 'link=emulated phy=openeth status=up' "$serial" && echo 1 || echo 0)" \
        "$(command grep -a -o -E 'link=emulated phy=openeth status=[a-z-]+' "$serial" | head -n 1)"
    wait_for_line "$serial" "$WAIT_SECONDS" "$IDENTITY_LINE" || true
    wait_for_line "$serial" "$WAIT_SECONDS" 'event=link-up' || true
    check "boot-$n-the-board-reached-the-server" \
        "$(command grep -a -q 'event=link-up' "$serial" && echo 1 || echo 0)" \
        "$(command grep -a -o -E 'emulated link: the server is the gateway' "$serial" | head -n 1), session event=link-up on the console"
}

# --- 2. first boot: adopted ----------------------------------------------------

say ""
say "chorus: first boot (serial: $OUT/serial-1.log)"
boot 1
ID="$(identity_field "$OUT/serial-1.log" id)"
KEY="$(identity_field "$OUT/serial-1.log" key)"
check "boot-1-the-board-made-an-identity-and-kept-it" \
    "$([ -n "$ID" ] && [ "$(identity_field "$OUT/serial-1.log" id_made_this_boot)" = 1 ] &&
        [ "$(identity_field "$OUT/serial-1.log" key_made_this_boot)" = 1 ] && echo 1 || echo 0)" \
    "$(command grep -a -o -E "$IDENTITY_LINE" "$OUT/serial-1.log" | head -n 1)"
ID="${ID:-no-id}"
wait_for_line "$OUT/server.log" "$WAIT_SECONDS" "client session .* id=$ID key=$KEY verdict=" || true
check "boot-1-the-server-adopted-it-under-that-id-and-key" \
    "$(command grep -q "client session .* id=$ID key=$KEY verdict=adopted" "$OUT/server.log" && echo 1 || echo 0)" \
    "$(command grep -o -E "id=$ID key=[^ ]+ verdict=[a-z-]+" "$OUT/server.log" | head -n 1)"
wait_for_speaker "$ID" present 20 || true
check "boot-1-the-servers-state-lists-the-speaker-present" \
    "$([ "$(speaker "$ID")" = "present $KEY" ] && echo 1 || echo 0)" \
    "/api/state speakers: $ID $(speaker "$ID")"
check "boot-1-nothing-crashed" "$([ "$(crashed "$OUT/serial-1.log")" = 0 ] && echo 1 || echo 0)" \
    "$(crashed "$OUT/serial-1.log") panic, abort or unasked reset lines on the console"

# --- 3. the power goes away -----------------------------------------------------

qemu_stop
wait_for_speaker "$ID" absent 30 || true
check "the-server-saw-the-speaker-leave" \
    "$([ "$(speaker "$ID")" = "absent $KEY" ] && echo 1 || echo 0)" \
    "/api/state speakers: $ID $(speaker "$ID")"

# --- 4. second boot: the same speaker --------------------------------------------

say ""
say "chorus: second boot, the same flash file (serial: $OUT/serial-2.log)"
boot 2
check "boot-2-the-board-read-the-same-identity-back" \
    "$([ "$(identity_field "$OUT/serial-2.log" id)" = "$ID" ] && [ "$(identity_field "$OUT/serial-2.log" key)" = "$KEY" ] &&
        [ "$(identity_field "$OUT/serial-2.log" id_made_this_boot)" = 0 ] &&
        [ "$(identity_field "$OUT/serial-2.log" key_made_this_boot)" = 0 ] && echo 1 || echo 0)" \
    "$(command grep -a -o -E "$IDENTITY_LINE" "$OUT/serial-2.log" | head -n 1)"
wait_for_line "$OUT/server.log" "$WAIT_SECONDS" "client session .* id=$ID key=$KEY verdict=known" || true
check "boot-2-the-server-knows-it" \
    "$(command grep -q "client session .* id=$ID key=$KEY verdict=known" "$OUT/server.log" && echo 1 || echo 0)" \
    "$(command grep -o -E "id=$ID key=[^ ]+ verdict=[a-z-]+" "$OUT/server.log" | tail -n 1)"
check "no-key-change-was-ever-seen" \
    "$(command grep -q -E 'key changed|key_changed' "$OUT/server.log" "$OUT/serial-2.log" && echo 0 || echo 1)" \
    "neither the server's log nor the board's console names a changed key"
wait_for_speaker "$ID" present 20 || true
check "boot-2-the-servers-state-lists-the-same-speaker-present" \
    "$([ "$(speaker "$ID")" = "present $KEY" ] && echo 1 || echo 0)" \
    "/api/state speakers: $ID $(speaker "$ID")"
check "boot-2-nothing-crashed" "$([ "$(crashed "$OUT/serial-2.log")" = 0 ] && echo 1 || echo 0)" \
    "$(crashed "$OUT/serial-2.log") panic, abort or unasked reset lines on the console"

# --- 5. no eFuse was written ------------------------------------------------------

stop_everything
EFUSES_AFTER="$(sha256sum "$OUT/efuse.bin" | cut -d' ' -f1)"
check "the-efuse-file-is-byte-identical" "$([ "$EFUSES_AFTER" = "$EFUSES_BEFORE" ] && echo 1 || echo 0)" \
    "sha256 before $EFUSES_BEFORE, after $EFUSES_AFTER"

ENDED=$(date +%s)
say ""
say "qemu-boot: wall-clock $((ENDED - STARTED)) s (the image and the server $((BUILT - STARTED)) s, the two boots $((ENDED - BUILT)) s); logs in $OUT"
if [ "$FAILURES" -ne 0 ]; then
    for f in "$OUT/serial-1.log" "$OUT/serial-2.log" "$OUT/server.log"; do
        [ -f "$f" ] && { say "--- the last lines of $f"; tail -n 25 "$f" | tr -d '\r'; }
    done
    say "qemu-boot: $FAILURES check(s) failed: FAIL"
    exit 1
fi
say "qemu-boot: adopted, identity kept across a reboot: PASS"
