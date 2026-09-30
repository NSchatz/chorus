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
#
# THE BENCH REPORT (K45). On a real device (CHORUS_REMOVABLE_DEVICE is not the
# ALSA `null` device, which cannot be removed anyway) the run ends by writing
# docs/measurements/sound2-device-loss-<date>.md with the client's output, its
# delay log and the run's status hashed (tools/bench/lib.sh), and with
# CHORUS_BENCH_PR=1 commits that on bench/<date>-sound2-device-loss and opens
# the PR. A failing run writes its report too (Result: FAIL) and still exits
# non-zero. The report half alone, from a run directory:
#   ./tools/device-loss-run.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"
set -- "${BENCH_ARGS[@]+"${BENCH_ARGS[@]}"}"

CRITERION="a device that becomes unusable during a run is reported with its reason, the client exits non-zero, and it never reports itself as playing while producing no audio"
TOPIC=sound2-device-loss

# One key of a run's status file.
run_status() {
    sed -n "s/^$2 = //p" "$1" 2>/dev/null | head -n 1
}

# Grade one run from its directory: client.out (what the client printed) and
# run-status.txt (its exit status and the device). Prints what it found and
# returns non-zero on the first failure, as the inline grading always did.
grade() {
    local dir="$1" client_status device
    client_status="$(run_status "$dir/run-status.txt" client_exit)"
    device="$(run_status "$dir/run-status.txt" device)"
    say "chorus: client exited ${client_status:-without a recorded status}"
    if [ "${client_status:-0}" -eq 0 ]; then
        say "chorus: FAIL the client exited zero after its device was removed"
        return 1
    fi
    if [ -z "$device" ] || ! grep -qF "$device" "$dir/client.out"; then
        say "chorus: FAIL the report does not name the device"
        return 1
    fi
    # `delay-refused` is the third accepted reason and is usually the one a
    # removed device produces: the first thing the playout loop asks a device
    # is how far it is from its DAC, so that is the call that fails first. All
    # three say the same thing about this check - the client stopped, named the
    # device, and did not claim to be playing.
    if ! grep -qE 'reason=device-failed|reason=device-unusable|reason=delay-refused' "$dir/client.out"; then
        say "chorus: FAIL the final report does not say the device failed"
        return 1
    fi
    say "chorus: pass the client reported the device and the reason and exited $client_status"
}

# The report and PR half: the raw files in, the bench report out.
report_and_publish() {
    local raw="$BENCH_RUN_DIR/raw" ok=1
    bench_analysis grade grade "$raw" && ok=0
    bench_field client_exit "$(run_status "$raw/run-status.txt" client_exit)"
    bench_field reason "$(sed -n 's/.*stopped reason=\([a-z-]*\).*/\1/p' "$raw/client.out" 2>/dev/null | tail -n 1)"
    bench_field device_named "$(grep -qF "$(run_status "$raw/run-status.txt" device)" "$raw/client.out" 2>/dev/null && echo yes || echo no)"
    local summary
    summary="$([ "$ok" = 0 ] && echo "the removed device was reported with its reason and the client exited non-zero" || echo "the device-loss grading failed") (exit $(run_status "$raw/run-status.txt" client_exit))"
    [ "$ok" = 0 ] && bench_finish PASS "$summary"
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" "$TOPIC"
    report_and_publish
fi

build_once
require_removable_device "$CRITERION"

DEVICE="$CHORUS_REMOVABLE_DEVICE"
BENCH_ON=0
if [ "$DEVICE" != null ]; then
    # A device that is not ALSA `null` is real hardware: this run is a bench run.
    BENCH_ON=1
    bench_begin "$TOPIC" "$CRITERION"
    bench_device "playback: ALSA $DEVICE, made unusable mid-run by the owner's CHORUS_REMOVE_COMMAND"
    bench_reproduce "CHORUS_REMOVABLE_DEVICE=$DEVICE CHORUS_REMOVE_COMMAND=<the owner's removal command> tools/device-loss-run.sh"
    RUN_DIR="$BENCH_RUN_DIR/raw"
else
    RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/chorus-device-loss-XXXXXX")"
fi
LOG="$RUN_DIR/delay.log"
PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

say "chorus: device-loss run"
say "  device: $DEVICE"
say "  remove: $CHORUS_REMOVE_COMMAND"

"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "127.0.0.1:$PORT" \
    --source tone \
    --chunk-us "$(conf chunk_us)" \
    "${CONTRACT_ARGS[@]}" "${SERVER_EXTRA_ARGS[@]}" &
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
    --ephemeral-identity \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$LOG" \
    --run-seconds 30 2>&1)"
CLIENT_STATUS=$?
set -e
wait "$REMOVER_PID" 2>/dev/null || true

printf '%s\n' "$OUT"

printf '%s\n' "$OUT" > "$RUN_DIR/client.out"
{
    printf 'client_exit = %s\n' "$CLIENT_STATUS"
    printf 'device = %s\n' "$DEVICE"
} > "$RUN_DIR/run-status.txt"
if [ "$BENCH_ON" = 1 ]; then
    report_and_publish
fi
grade "$RUN_DIR"
