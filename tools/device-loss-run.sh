#!/usr/bin/env bash
# A device removed mid-run.
#
# Verifies: when the audio device becomes unusable during a run, the client
# reports the device and the reason, exits non-zero, and does not report itself
# as playing while producing no audio.
#
# Prerequisite: a device that can be removed or made unusable mid-run. Set
# CHORUS_REMOVABLE_DEVICE to the ALSA device name and CHORUS_REMOVE_COMMAND to
# the command that removes it (unplug it, unbind the driver, or `rmmod
# snd_aloop` for a loopback). Nothing here guesses at how to break a device on
# someone else's machine.
#
#   CHORUS_REMOVABLE_DEVICE=hw:Loopback,0 \
#   CHORUS_REMOVE_COMMAND='sudo modprobe -r snd_aloop' \
#     ./tools/device-loss-run.sh

source "$(dirname "$0")/lib.sh"

CRITERION="a device that becomes unusable during a run is reported with its reason, the client exits non-zero, and it never reports itself as playing while producing no audio"

build_once
require_removable_device "$CRITERION"

DEVICE="$CHORUS_REMOVABLE_DEVICE"
LOG="${TMPDIR:-/tmp}/chorus-device-loss.log"
PORT="$(free_port)"
CONTRACT_ARGS="$(server_contract_args)"

say "chorus: device-loss run"
say "  device: $DEVICE"
say "  remove: $CHORUS_REMOVE_COMMAND"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --chunk-us "$(conf chunk_us)" \
    $CONTRACT_ARGS ${CHORUS_SERVER_EXTRA_ARGS:-} &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT

sleep 1

(
    sleep 5
    say "chorus: removing the device"
    eval "$CHORUS_REMOVE_COMMAND"
) &
REMOVER_PID=$!

set +e
OUT="$("$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$LOG" \
    --run-seconds 30 2>&1)"
CLIENT_STATUS=$?
set -e
wait "$REMOVER_PID" 2>/dev/null || true

printf '%s\n' "$OUT"
say "chorus: client exited $CLIENT_STATUS"

if [ "$CLIENT_STATUS" -eq 0 ]; then
    say "chorus: FAIL the client exited zero after its device was removed"
    exit 1
fi
if ! printf '%s' "$OUT" | grep -q "$DEVICE"; then
    say "chorus: FAIL the report does not name the device"
    exit 1
fi
if ! printf '%s' "$OUT" | grep -qE 'reason=device-failed|reason=device-unusable'; then
    say "chorus: FAIL the final report does not say the device failed"
    exit 1
fi
say "chorus: pass the client reported the device and the reason and exited $CLIENT_STATUS"
