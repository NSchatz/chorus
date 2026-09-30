#!/usr/bin/env bash
# EMBEDDED-5's bring-up on the ESP32-S3 itself (BRIEF.md section 8 item 5:
# "I2S out, TAS5825M alive, network playback ... survives disconnect/reconnect
# abuse unattended"; chorus goal 9's EMBEDDED-5 packet, docs/bench-packet.md
# session S7).
#
# Over the endpoint's serial console (firmware/src/console.c) this records:
#   - the amplifier's bring-up as the endpoint reports it (`status`: amp=...,
#     amp_fault_bits=...), and the boot log the owner saved while flashing, if
#     CHORUS_EMBEDDED5_BOOT_LOG names it;
#   - `resources` before the stream, while it plays and after the abuse: free
#     heap (internal and PSRAM, now and the least since boot), each named task's
#     least free stack, the FIFO after the writer (frames handed to the I2S
#     driver and not yet at the pins) and the GPIO marker's counts;
#   - a session to a server this script starts on this machine (a quiet 440 Hz
#     tone, `--source tone`), then CHORUS_EMBEDDED5_CYCLES rounds of abuse:
#     the server is killed, left dead for an outage that grows from 1 s to
#     twice the endpoint's longest reconnect backoff, and started again on the
#     same port, and the endpoint has to be playing again (audio=running with
#     new chunks) within the bound below without anyone touching it.
#
# PASS: the amplifier came up (amp=ok, no fault bits), every round recovered
# within its bound, and the stream plays at the end. FAIL otherwise. The heap,
# stack and FIFO figures are MEASURED records for the playout and DSP budgets
# and grade nothing: a stack figure near zero is a finding for the ADR that
# chose the stack, not a failed run. Nothing here is a timing claim about sync:
# that is tools/endpoint-rig-run.sh's, against a Linux client, on the rig.
#
# NEEDS AN ENVIRONMENT.
#   - an ESP32-S3 endpoint flashed with an image built from this checkout
#     (tools/firmware-flash.sh, the owner's act), its serial console on
#     CHORUS_ESP32S3_PORT, its amplifier wired to a loudspeaker and its link up
#   - the TAS5825M register map in firmware/config/endpoint.conf (goal 9)
#   - this machine reachable from the endpoint (CHORUS_SERVER_HOST names this
#     machine as the endpoint should dial it; default: hostname)
#
# THE BENCH REPORT (K45). The run writes docs/measurements/
# embedded5-bringup-<date>.md with every console reply and the server's log
# hashed, and with CHORUS_BENCH_PR=1 commits it on bench/<date>-embedded5-bringup
# and opens the PR. `--report-from <run directory>` runs only that half.
#
#   CHORUS_ESP32S3_PORT=/dev/ttyACM0 CHORUS_BENCH_PR=1 ./tools/embedded5-bringup-run.sh
#   ./tools/embedded5-bringup-run.sh --report-from <run directory>

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"

CRITERION="the ESP32-S3 endpoint brings its TAS5825M up, plays a network stream, and survives disconnect/reconnect abuse unattended; its heap, stacks and FIFO after the writer are recorded"
TOPIC=embedded5-bringup

# One key=value word of a console reply line.
reply_value() {
    printf '%s\n' "$1" | tr ' ' '\n' | sed -n "s/^$2=//p" | head -n 1
}

# The least free stack across the `stack_free.<task>=<bytes>` words of a
# `resources` reply, as `<bytes> (<task>)`, ignoring tasks reported absent.
least_stack() {
    printf '%s\n' "$1" | tr ' ' '\n' | sed -n 's/^stack_free\.\([^=]*\)=\([0-9][0-9]*\)$/\2 \1/p' \
        | sort -n | head -n 1 | awk '{ printf "%s (%s)", $1, $2 }'
}

report_and_publish() {
    local raw="$BENCH_RUN_DIR/raw" first playing last start mid end cycles total recovered
    first="$(sed -n 's/^\(status .*\)$/\1/p' "$raw/status-before.txt" 2>/dev/null | head -n 1)"
    playing="$(sed -n 's/^\(status .*\)$/\1/p' "$raw/status-playing.txt" 2>/dev/null | head -n 1)"
    last="$(sed -n 's/^\(status .*\)$/\1/p' "$raw/status-end.txt" 2>/dev/null | head -n 1)"
    start="$(sed -n 's/^\(resources .*\)$/\1/p' "$raw/resources-before.txt" 2>/dev/null | head -n 1)"
    mid="$(sed -n 's/^\(resources .*\)$/\1/p' "$raw/resources-playing.txt" 2>/dev/null | head -n 1)"
    end="$(sed -n 's/^\(resources .*\)$/\1/p' "$raw/resources-end.txt" 2>/dev/null | head -n 1)"
    total="$(command grep -c '^cycle ' "$raw/cycles.txt" 2>/dev/null || true)"
    recovered="$(command grep -c '^cycle .* recovered=yes' "$raw/cycles.txt" 2>/dev/null || true)"
    total="${total:-0}"
    recovered="${recovered:-0}"

    local amp fault
    amp="$(reply_value "${playing:-$first}" amp)"
    fault="$(reply_value "${playing:-$first}" amp_fault_bits)"
    bench_field amp_status "${amp:-no status reply} (amp_fault_bits=${fault:-none}; firmware/src/amp.c)"
    bench_field link "$(reply_value "${playing:-$first}" link) ($(reply_value "${playing:-$first}" transport))"
    bench_field cycles_recovered "$recovered of $total (outage from 1 s to twice reconnect_max_backoff_ms; bound per round in raw/cycles.txt)"
    bench_field heap_internal_min_free "$(reply_value "$end" heap_internal_min_free) bytes least since boot (at start $(reply_value "$start" heap_internal_free) free, at end $(reply_value "$end" heap_internal_free) free)"
    bench_field heap_psram_min_free "$(reply_value "$end" heap_psram_min_free)"
    bench_field stack_min_free "$(least_stack "$end") bytes, the least of the named tasks (raw/resources-end.txt lists each)"
    bench_field fifo_us "$(reply_value "$mid" fifo_us) us, $(reply_value "$mid" fifo_frames) frames after the writer while playing (the amplifier's own latency after the pins is not in it)"
    bench_field sync_error_ns "$(reply_value "$last" sync_error_ns) at the end (the endpoint's own servo error, not a rig measurement)"
    bench_field marker "$(reply_value "$end" marker_pin) (edges $(reply_value "$end" marker_edges), missed $(reply_value "$end" marker_missed))"

    if [ -z "$first" ] || [ -z "$start" ]; then
        bench_finish FAIL "the console gave no status or resources reply: $(head -n 1 "$raw/status-before.txt" 2>/dev/null)"
    fi
    # amp=ok is the sequencer's own verdict: it only reports ok after the
    # fault registers read their clear values (firmware/src/amp.c).
    if [ "$amp" != "ok" ]; then
        bench_finish FAIL "the amplifier did not come up: amp=${amp:-none} amp_fault_bits=${fault:-none}"
    fi
    if [ -z "$playing" ] || [ "$(reply_value "$playing" audio)" != "running" ]; then
        bench_finish FAIL "the endpoint never played the stream: $(printf '%s' "${playing:-no status}" | cut -c1-160)"
    fi
    if [ "$total" = 0 ] || [ "$recovered" != "$total" ]; then
        bench_finish FAIL "$recovered of $total abuse rounds recovered unattended"
    fi
    if [ "$(reply_value "$last" audio)" != "running" ]; then
        bench_finish FAIL "all $total rounds recovered but the stream is not playing at the end: audio=$(reply_value "$last" audio)"
    fi
    bench_finish PASS "amp=ok, playing, $recovered of $total disconnect/reconnect rounds recovered unattended; least internal heap $(reply_value "$end" heap_internal_min_free) bytes, least stack $(least_stack "$end") bytes"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" "$TOPIC"
    report_and_publish
fi

say "chorus: EMBEDDED-5 bring-up on the ESP32-S3, over its serial console"
say "  endpoint: ${CHORUS_ESP32S3_PORT:-<unset>}"

require_esp32s3_endpoint "$CRITERION"
require_endpoint_console "$CRITERION"
require_amplifier_registers "$CRITERION"

build_once

CYCLES="${CHORUS_EMBEDDED5_CYCLES:-10}"
MAX_BACKOFF_MS="$(endpoint_conf reconnect_max_backoff_ms)"
# A round recovers when the endpoint plays again within the outage's end plus
# its longest backoff plus 10 s for the session to resume (connect, adopt the
# stream, fill, acquire). The 10 s is a choice for this bench run, not a
# specified bound: the brief asks that it survive, not how fast.
RECOVERY_SLACK_S=10

bench_begin "$TOPIC" "$CRITERION"
bench_device "ESP32-S3 endpoint on its serial console, board profile $(basename "$(board_profile_path)" .conf) ($(board_value board_model), $(board_value board_model_status)); amplifier to a loudspeaker"
bench_device "server: chorus-server on the bench machine, --source tone (quiet 440 Hz), address not recorded (K27)"
bench_reproduce "CHORUS_ESP32S3_PORT=<port> CHORUS_EMBEDDED5_CYCLES=$CYCLES tools/embedded5-bringup-run.sh"
OUT_DIR="$BENCH_RUN_DIR/raw"
mkdir -p "$OUT_DIR"

if [ -n "${CHORUS_EMBEDDED5_BOOT_LOG:-}" ] && [ -f "$CHORUS_EMBEDDED5_BOOT_LOG" ]; then
    # The boot log from the flash step, less any line carrying a MAC address
    # (K27).
    command grep -v -i -E '([0-9a-f]{2}:){5}[0-9a-f]{2}' "$CHORUS_EMBEDDED5_BOOT_LOG" \
        > "$OUT_DIR/boot.log" || true
fi

console_to() {
    local file="$1" command="$2"
    endpoint_console "$command" 15 > "$file" || printf 'no reply\n' > "$file"
}

console_to "$OUT_DIR/status-before.txt" status
console_to "$OUT_DIR/resources-before.txt" resources

PORT="$(free_port)"
SERVER_HOST="${CHORUS_SERVER_HOST:-$(hostname)}"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
SERVER_PID=""
start_server() {
    "$BIN_DIR/chorus-server" \
        --ephemeral-identity \
        --listen "0.0.0.0:$PORT" \
        --source tone \
        --rate "$(endpoint_conf i2s_sample_rate_hz)" \
        --channels "$(conf channels)" \
        --format "$(conf sample_format)" \
        --chunk-us "$(conf chunk_us)" \
        --rttime-us "$(conf rttime_us)" \
        --rt-priority "$(conf rt_priority)" \
        --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
        "${CONTRACT_ARGS[@]}" >> "$OUT_DIR/server.log" 2>&1 &
    SERVER_PID=$!
}
trap 'kill_quietly "$SERVER_PID"' EXIT

# Wait up to $1 seconds for the endpoint to report audio=running with more
# chunks than $2. Prints the status line it ended on.
wait_playing() {
    local limit="$1" chunks_before="$2" waited=0 line
    while [ "$waited" -le "$limit" ]; do
        line="$(endpoint_console status 5 || true)"
        if [ "$(reply_value "$line" audio)" = running ] \
            && [ "$(reply_value "$line" chunks)" -gt "$chunks_before" ] 2> /dev/null; then
            printf '%s\n' "$line"
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    printf '%s\n' "${line:-no reply}"
    return 1
}

say "chorus: starting the server and pointing the endpoint at it"
start_server
sleep 1
SERVER_REPLY="$(endpoint_console "server $SERVER_HOST:$PORT" || true)"
case "$SERVER_REPLY" in
    "server set=$SERVER_HOST:$PORT "*) ;;
    *)
        say "chorus: the endpoint did not take the server address: ${SERVER_REPLY:-no reply}"
        printf 'no reply\n' > "$OUT_DIR/status-playing.txt"
        : > "$OUT_DIR/cycles.txt"
        console_to "$OUT_DIR/status-end.txt" status
        console_to "$OUT_DIR/resources-end.txt" resources
        report_and_publish
        ;;
esac

wait_playing 60 0 > "$OUT_DIR/status-playing.txt" || true
sleep 10
console_to "$OUT_DIR/resources-playing.txt" resources

: > "$OUT_DIR/cycles.txt"
for round in $(seq 1 "$CYCLES"); do
    # Outages from 1 s up to twice the longest backoff, so the rounds cover a
    # reconnect inside the first backoff and one after the backoff saturates.
    outage=$(( 1 + (round - 1) * (2 * MAX_BACKOFF_MS / 1000) / (CYCLES > 1 ? CYCLES - 1 : 1) ))
    before="$(endpoint_console status 5 || true)"
    chunks_before="$(reply_value "$before" chunks)"
    say "chorus: round $round of $CYCLES: server down for ${outage}s"
    kill_quietly "$SERVER_PID"
    sleep "$outage"
    start_server
    bound=$(( MAX_BACKOFF_MS / 1000 + RECOVERY_SLACK_S ))
    t0="$(date +%s)"
    if line="$(wait_playing "$bound" "${chunks_before:-0}")"; then
        recovered=yes
    else
        recovered=no
    fi
    printf 'cycle %d outage_s=%d bound_s=%d recovered=%s after_s=%d status: %s\n' \
        "$round" "$outage" "$bound" "$recovered" "$(( $(date +%s) - t0 ))" "$line" >> "$OUT_DIR/cycles.txt"
done

sleep 10
console_to "$OUT_DIR/status-end.txt" status
console_to "$OUT_DIR/resources-end.txt" resources
cat "$OUT_DIR/cycles.txt"
report_and_publish
