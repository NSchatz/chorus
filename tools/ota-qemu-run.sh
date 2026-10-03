#!/usr/bin/env bash
# A firmware update under the emulator: a good image installs and confirms, a
# bad image boots, never confirms, and the bootloader rolls back to the good
# one (goal 14, line B; docs/decisions/0111-*).
#
# Everything that decides is the code a speaker runs: the server's staging,
# verification and its explicit `firmware_install` (ADR 0110), the session's
# firmware messages and the update unit (ADR 0108), ESP-IDF's app_update and
# its second-stage bootloader, which reads otadata and rolls back an image that
# rebooted without being marked valid. The board is the emulator's
# (firmware/boards/qemu-s3-openeth.conf, ADR 0109), reached over the
# emulator's user network.
#
# The order, and what is graded:
#   1. three images of the emulated profile: A (the one first in the flash),
#      GOOD (another version) and BAD (another version again, built to never
#      confirm: CHORUS_OTA_NEVER_CONFIRM, which the build refuses for any
#      profile but the emulated one); GOOD and BAD staged in the server's
#      firmware directory, each verified by the server before it is listed;
#   2. one emulator, one flash file made from A, a fresh eFuse file; a real
#      chorus-server on loopback; the board is adopted, reports A on slot 0,
#      and nothing is offered to it while nobody asks (the state says
#      `update_available` and the server's log carries no offer);
#   3. `firmware_install` GOOD: the board writes slot 1, reboots, the
#      bootloader boots slot 1 on trial (the board says `state=pending-verify`
#      as it reads otadata), the session confirms it (`state=valid` at the next
#      boot report is not needed: the server's state says `confirmed` and the
#      board runs GOOD on slot 1);
#   4. `firmware_install` BAD: the board writes slot 0, reboots into BAD on
#      trial, does not confirm, marks it invalid and reboots; the bootloader
#      boots GOOD on slot 1 again; the server's state says `rolled_back`,
#      reason `not_confirmed`, running GOOD, the image tried BAD;
#   5. the eFuse file is byte for byte what it was.
#
# Source: simulation. Nothing here is timing evidence; the hardware rollback
# is the owner's bench session.
#
# NEEDS AN ENVIRONMENT, and refuses by name without it (never a green skip):
# the pinned emulator (bash tools/qemu-env.sh install), ESP-IDF at the pinned
# version, the committed server_address's port free on loopback.
#
#   make ota-qemu

source "$(dirname "$0")/lib.sh"
# shellcheck source=qemu/lib.sh
source "$REPO_ROOT/tools/qemu/lib.sh"

CRITERION="under the emulator, an explicit install of a good image is written, booted on trial and confirmed, and an explicit install of a bad image boots, does not confirm, and is rolled back by the bootloader to the good one"
PROFILE=qemu-s3-openeth
IMAGE_A="${CHORUS_IMAGE_OUT:-$REPO_ROOT/firmware/build/$PROFILE}"
IMAGE_GOOD="$REPO_ROOT/firmware/build/$PROFILE-ota-good"
IMAGE_BAD="$REPO_ROOT/firmware/build/$PROFILE-ota-bad"
GOOD_VERSION=ota-qemu-good
BAD_VERSION=ota-qemu-bad
OUT="${CHORUS_QEMU_OUT:-$TARGET_DIR/ota-qemu}"
# Bounds on a run, not measurements (ASSUMED): the whole emulator, one wait
# for a line or a state, and one wait for a transfer of an image.
EMULATOR_SECONDS=900
WAIT_SECONDS=60
TRANSFER_SECONDS=300

say "chorus: a firmware update under the emulator (Source: simulation; not timing evidence)"
say "  criterion: $CRITERION"

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
mkdir -p "$OUT/firmware" "$OUT/identity"

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

# --- 1. three images, two of them staged ----------------------------------------

# build <directory> <version or empty> <never confirm 0|1> <log>
build() {
    if ! CHORUS_BOARD_PROFILE="$PROFILE" CHORUS_IMAGE_OUT="$1" CHORUS_IMAGE_VERSION="$2" \
        CHORUS_OTA_NEVER_CONFIRM="$3" bash "$REPO_ROOT/tools/firmware-image.sh" > "$4" 2>&1; then
        tail -n 30 "$4"
        say "ota-qemu: the image in $1 did not build: FAIL"
        exit 1
    fi
    command grep -E '^(chorus: image built|  version:|  update:)' "$4" | sed 's/^/  /' || true
}
say ""
say "chorus: building image A (first in the flash), GOOD ($GOOD_VERSION) and BAD ($BAD_VERSION, never confirms)"
build "$IMAGE_A" "" 0 "$OUT/build-a.log"
build "$IMAGE_GOOD" "$GOOD_VERSION" 0 "$OUT/build-good.log"
build "$IMAGE_BAD" "$BAD_VERSION" 1 "$OUT/build-bad.log"
build_once
VERSION_A="$(python3 - "$IMAGE_A/chorus-endpoint.bin" <<'PY'
import sys
# esp_app_desc_t follows the 24-byte image header and the first segment's
# 8-byte header; its version field is 32 bytes at offset 16 of it
# (components/esp_app_format/include/esp_app_desc.h in the pinned v6.1).
data = open(sys.argv[1], "rb").read(24 + 8 + 48)
print(data[24 + 8 + 16 : 24 + 8 + 48].split(b"\0")[0].decode("ascii", "replace"))
PY
)"
say "  image A's version: $VERSION_A"
CHORUS_SERVER_BIN="$BIN_DIR/chorus-server" bash "$REPO_ROOT/tools/firmware-stage.sh" "$IMAGE_GOOD" "$OUT/firmware" good |
    sed 's/^/  /'
CHORUS_SERVER_BIN="$BIN_DIR/chorus-server" bash "$REPO_ROOT/tools/firmware-stage.sh" "$IMAGE_BAD" "$OUT/firmware" bad |
    sed 's/^/  /'
BUILT=$(date +%s)

qemu_flash_image "$IMAGE_A" "$OUT/flash.bin" | sed 's/^/  /'
qemu_fresh_efuses "$OUT/efuse.bin"
EFUSES_BEFORE="$(sha256sum "$OUT/efuse.bin" | cut -d' ' -f1)"
say "  eFuse file: $(wc -c < "$OUT/efuse.bin" | tr -d ' ') bytes, the chip's defaults, sha256 $EFUSES_BEFORE"

# --- 2. the server, the board, adoption, and nothing offered ---------------------

CONTROL="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --control-listen "127.0.0.1:$CONTROL" \
    --identity-dir "$OUT/identity" \
    --state-file "$OUT/zones.state" \
    --firmware-dir "$OUT/firmware" \
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

# state <python expression over `state` and `speaker`>: evaluated against the
# server's /api/state; `speaker` is this run's speaker's record ({} until it is
# listed) and `fw` its firmware member ({} until reported).
state() {
    python3 - "$CONTROL" "${ID:-}" "$1" <<'PY' 2> /dev/null || true
import json, socket, sys
port, wanted, expression = int(sys.argv[1]), sys.argv[2], sys.argv[3]
s = socket.create_connection(("127.0.0.1", port), timeout=5)
s.sendall(b"GET /api/state HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n")
answer = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    answer += chunk
state = json.loads(answer.split(b"\r\n\r\n", 1)[1].decode("utf-8"))
speaker = next((one for one in state.get("speakers", []) if one.get("id") == wanted), {})
fw = speaker.get("firmware") or {}
print(eval(expression))
PY
}

# wait_for_state <python condition> <seconds>
wait_for_state() {
    local condition="$1" seconds="$2" i
    for ((i = 0; i < seconds * 2; i++)); do
        [ "$(state "$condition")" = True ] && return 0
        sleep 0.5
    done
    return 1
}

# command <json>: POST /api/command; prints the status line and the body.
command_post() {
    python3 - "$CONTROL" "$1" <<'PY'
import socket, sys
port, body = int(sys.argv[1]), sys.argv[2].encode()
s = socket.create_connection(("127.0.0.1", port), timeout=10)
s.sendall(
    b"POST /api/command HTTP/1.1\r\nHost: 127.0.0.1:" + str(port).encode()
    + b"\r\nOrigin: http://127.0.0.1:" + str(port).encode()
    + b"\r\nContent-Type: application/json\r\nContent-Length: " + str(len(body)).encode()
    + b"\r\nConnection: close\r\n\r\n" + body
)
answer = b""
while True:
    chunk = s.recv(65536)
    if not chunk:
        break
    answer += chunk
head, _, rest = answer.partition(b"\r\n\r\n")
print(head.split(b"\r\n")[0].decode() + " " + rest.decode("utf-8", "replace").strip())
PY
}

SERIAL="$OUT/serial.log"
FW='"%s %s slot=%s state=%s reason=%s image=%s" % (fw.get("version"), fw.get("board"), fw.get("slot"), fw.get("state"), fw.get("reason"), fw.get("image_version"))'

say ""
say "chorus: the board, image A on slot 0 (serial: $SERIAL)"
qemu_boot "$OUT/flash.bin" "$OUT/efuse.bin" "$SERIAL" "$EMULATOR_SECONDS"
IDENTITY_LINE='identity id=chorus-[0-9a-f]{12} key=[^ ]+ id_made_this_boot=[01] key_made_this_boot=[01]'
wait_for_line "$SERIAL" "$WAIT_SECONDS" "$IDENTITY_LINE" || true
ID="$({ command grep -a -o -E "$IDENTITY_LINE" "$SERIAL" | head -n 1 | sed -n 's/^identity id=\([^ ]*\) .*/\1/p'; } || true)"
ID="${ID:-no-id}"
wait_for_state 'fw.get("version") is not None and speaker.get("present")' "$WAIT_SECONDS" || true
check "the-board-is-adopted-and-reports-image-a" \
    "$([ "$(state 'fw.get("version")')" = "$VERSION_A" ] && [ "$(state 'fw.get("slot")')" = 0 ] && echo 1 || echo 0)" \
    "$ID: $(state "$FW")"
check "the-staged-images-are-verified" \
    "$([ "$(state '",".join(sorted(i["name"] + ":" + i["verdict"] for i in state.get("firmware", {}).get("images", [])))')" = "bad:verified,good:verified" ] && echo 1 || echo 0)" \
    "$(state '[(i["name"], i["version"], i["verdict"]) for i in state.get("firmware", {}).get("images", [])]')"
sleep 5
check "nothing-is-offered-while-nobody-asks" \
    "$([ "$(state 'fw.get("update_available")')" = True ] && ! command grep -q 'firmware offer' "$OUT/server.log" && echo 1 || echo 0)" \
    "update_available=$(state 'fw.get("update_available")'), firmware offers in the server's log: $(command grep -c 'firmware offer' "$OUT/server.log" || true)"

# --- 3. the good image ---------------------------------------------------------

say ""
say "chorus: firmware_install good"
ANSWER="$(command_post "{\"v\":2,\"t\":\"firmware_install\",\"speaker\":\"$ID\",\"image\":\"good\"}")"
check "the-install-of-good-is-accepted" "$([[ "$ANSWER" == *" 200 "* ]] && echo 1 || echo 0)" "$ANSWER"
wait_for_line "$SERIAL" "$TRANSFER_SECONDS" 'rebooting into the new image' || true
wait_for_line "$SERIAL" "$WAIT_SECONDS" "running slot=1 state=pending-verify version=$GOOD_VERSION" || true
check "good-boots-on-slot-1-on-trial" \
    "$(command grep -a -q "running slot=1 state=pending-verify version=$GOOD_VERSION" "$SERIAL" && echo 1 || echo 0)" \
    "$(command grep -a -o -E 'running slot=1 state=[a-z-]+ version=[^ ]+' "$SERIAL" | head -n 1 | tr -d '\r')"
wait_for_state "fw.get(\"state\") == \"confirmed\" and fw.get(\"version\") == \"$GOOD_VERSION\"" "$WAIT_SECONDS" || true
check "good-is-confirmed" \
    "$([ "$(state 'fw.get("state")')" = confirmed ] && [ "$(state 'fw.get("version")')" = "$GOOD_VERSION" ] && [ "$(state 'fw.get("slot")')" = 1 ] && echo 1 || echo 0)" \
    "$(state "$FW"); server: $(command grep -o -E 'firmware confirmed [^"]*' "$OUT/server.log" | head -n 1)"

# --- 4. the bad image, and the bootloader's rollback -------------------------------

say ""
say "chorus: firmware_install bad"
ANSWER="$(command_post "{\"v\":2,\"t\":\"firmware_install\",\"speaker\":\"$ID\",\"image\":\"bad\"}")"
check "the-install-of-bad-is-accepted" "$([[ "$ANSWER" == *" 200 "* ]] && echo 1 || echo 0)" "$ANSWER"
wait_for_line "$SERIAL" "$TRANSFER_SECONDS" "running slot=0 state=pending-verify version=$BAD_VERSION" || true
check "bad-boots-on-slot-0-on-trial" \
    "$(command grep -a -q "running slot=0 state=pending-verify version=$BAD_VERSION" "$SERIAL" && echo 1 || echo 0)" \
    "$(command grep -a -o -E "running slot=0 state=[a-z-]+ version=$BAD_VERSION" "$SERIAL" | head -n 1 | tr -d '\r')"
# The trial is ota_confirm_seconds long (firmware/config/endpoint.conf), and
# the unit then marks the image invalid and reboots.
TRIAL="$(endpoint_conf ota_confirm_seconds)"
wait_for_line "$SERIAL" "$((TRIAL + WAIT_SECONDS))" 'did not confirm in time' || true
check "bad-does-not-confirm-and-gives-itself-up" \
    "$(command grep -a -q 'did not confirm in time' "$SERIAL" && echo 1 || echo 0)" \
    "$(command grep -a -o -E 'this image did not confirm in time[^.;]*' "$SERIAL" | head -n 1 | tr -d '\r')"
wait_for_state "fw.get(\"state\") == \"rolled_back\"" "$WAIT_SECONDS" || true
ROLLED="$(command grep -a -n -E "running slot=1 state=valid version=$GOOD_VERSION" "$SERIAL" | tail -n 1 | cut -d: -f1)"
GAVE_UP="$(command grep -a -n 'did not confirm in time' "$SERIAL" | head -n 1 | cut -d: -f1)"
check "the-bootloader-rolled-back-to-good" \
    "$([ -n "$ROLLED" ] && [ -n "$GAVE_UP" ] && [ "$ROLLED" -gt "$GAVE_UP" ] && echo 1 || echo 0)" \
    "after the trial, the board reads otadata: $(command grep -a -o -E "running slot=1 state=valid version=$GOOD_VERSION" "$SERIAL" | tail -n 1 | tr -d '\r')"
check "the-server-says-rolled-back" \
    "$([ "$(state 'fw.get("state")')" = rolled_back ] && [ "$(state 'fw.get("reason")')" = not_confirmed ] &&
        [ "$(state 'fw.get("version")')" = "$GOOD_VERSION" ] && [ "$(state 'fw.get("image_version")')" = "$BAD_VERSION" ] && echo 1 || echo 0)" \
    "$(state "$FW")"
check "nothing-crashed" \
    "$([ "$(command grep -a -c -E 'Guru Meditation|abort\(\) was called|Backtrace:' "$SERIAL" || true)" = 0 ] && echo 1 || echo 0)" \
    "$(command grep -a -c -E 'Guru Meditation|abort\(\) was called|Backtrace:' "$SERIAL" || true) panic or abort lines on the console"

# --- 5. no eFuse was written ------------------------------------------------------

stop_everything
EFUSES_AFTER="$(sha256sum "$OUT/efuse.bin" | cut -d' ' -f1)"
check "the-efuse-file-is-byte-identical" "$([ "$EFUSES_AFTER" = "$EFUSES_BEFORE" ] && echo 1 || echo 0)" \
    "sha256 before $EFUSES_BEFORE, after $EFUSES_AFTER"

ENDED=$(date +%s)
say ""
say "ota-qemu: wall-clock $((ENDED - STARTED)) s (three images and the server $((BUILT - STARTED)) s, the emulator $((ENDED - BUILT)) s); logs in $OUT"
if [ "$FAILURES" -ne 0 ]; then
    for f in "$SERIAL" "$OUT/server.log"; do
        [ -f "$f" ] && { say "--- the last lines of $f"; tail -n 30 "$f" | tr -d '\r'; }
    done
    say "ota-qemu: $FAILURES check(s) failed: FAIL"
    exit 1
fi
say "ota-qemu: good installed, bad rolled back: PASS"
