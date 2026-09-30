#!/usr/bin/env bash
# AC-2 and AC-3 of chorus#WIFI-7: the wireless characterization, and THE TWO
# CRITERIA IT GRADES ARE NOT PASSED.
#
# AC-2. WHEN a Wi-Fi endpoint runs with modem sleep disabled THE SYSTEM SHALL
# meet the 5 ms multiroom inter-device bound.
#
# AC-3. WHEN a Wi-Fi endpoint is measured with the platform default left in
# place THE SYSTEM SHALL record the resulting jitter in docs/measurements/
# rather than tune the servo against it.
#
# Neither is claimed anywhere in this repository. Both need an ESP32-S3 endpoint
# on this house's Wi-Fi playing a grouped stream beside a second endpoint in
# another room, with both line outputs captured by the RIG-3 rig, and AC-3 needs
# the same rig run TWICE, once with the platform default left in place. A bound
# is a measured distribution over a real radio and no committed test can produce
# one; no test can make a radio sleep either. On a machine without them this
# script exits non-zero naming the missing prerequisite and the criterion it was
# verifying, and reports nothing as passed, skipped-green or satisfied.
# docs/verification-record.md quotes the refusal and says plainly that the
# criteria are unmet.
#
# What IS committed beside this, and is not it: the report shape and the
# arithmetic, graded against the committed MODELLED series
# `fixtures/measure/14-wireless-jitter-ps-none.offsets` and
# `fixtures/measure/15-wireless-jitter-ps-min-modem.offsets` by
# `cargo test -p chorus-measure --test report_shape`, and the two saved reports
# `make measure-fixture-reports` writes from them. Those series are generated
# from committed parameters. They are NOT measurements of any radio and every
# report written from them says so in its own words.
#
# THE ONE THING THIS RUN MUST NOT LEAD TO. BRIEF.md section 9: "Wi-Fi jitter
# defeats the servo -> bigger buffers or wired-only for that zone; never chase
# Wi-Fi with servo aggression." Whatever this measures, no constant in
# config/sync.conf moves because of it. The answer to a bad number here is a
# deeper buffer in config/transport.conf or a zone declared wired.
#
# NEEDS AN ENVIRONMENT, and code that does not exist yet.
#   - an endpoint serial console that sets the power-save mode and reads it
#     back. IT DOES NOT EXIST (audit A-13; goal 8), so this script refuses by
#     name before it writes to a serial port or fetches a file. The other half
#     of A-13 does exist: the second endpoint writes the `.offsets` series this
#     run fetches with `chorus-client --offsets-out` (goal 7), in the format
#     crates/measure/src/freerun.rs reads
#   - a wireless network the endpoint may join, in firmware/config/endpoint.conf,
#     which this repository declares UNKNOWN and will go on declaring unknown
#   - the access point the run is taken against, named (CHORUS_WIRELESS_AP)
#   - an ESP32-S3 endpoint flashed with an image built from that configuration,
#     on that network (CHORUS_ESP32S3_PORT)
#   - a second endpoint in ANOTHER ROOM playing the same grouped stream
#     (CHORUS_SECOND_ENDPOINT)
#   - both endpoints' line outputs wired into the L and R inputs of one audio
#     interface, visible to ALSA as a capture device (CHORUS_CAPTURE_DEVICE)
#
# The grouped stream IS the rig's chirp: the server runs `--source chirp`,
# built from config/measure.conf, because the lag analyser refuses anything
# without the chirp in it (audit A-7), and the capture tool records with
# `--record-only` and emits nothing of its own. The chirp leaves through a real
# amplifier into a real loudspeaker. The server holds it to the amplitude
# ceiling in config/measure.conf before it binds a socket; do not raise that
# ceiling to make a quiet capture louder, move the interface's input gain
# instead.
#
# THE BENCH REPORT (K45). The run ends by writing
# docs/measurements/wifi7-wireless-<date>.md with each mode's capture and
# offsets series hashed (tools/bench/lib.sh), and with CHORUS_BENCH_PR=1 it
# commits that on bench/<date>-wifi7-wireless and opens the PR.
# `--report-from <run directory>` runs only that half, which needs no radio.
#
#   CHORUS_WIRELESS_AP='ubiquiti-u6-lite 5GHz' CHORUS_ESP32S3_PORT=/dev/ttyACM0 \
#   CHORUS_SECOND_ENDPOINT=user@endpoint-b CHORUS_CAPTURE_DEVICE=hw:1,0 \
#       ./tools/wireless-characterization-run.sh   # or: make verify-wireless
#   ./tools/wireless-characterization-run.sh --report-from <run directory>

# The run below require_endpoint_console is unreachable on purpose until that
# prerequisite exists (audit A-13), so it stays unrun visibly.
# shellcheck disable=SC2317

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"

CRITERION="a Wi-Fi endpoint with modem sleep disabled holds the 5 ms multiroom inter-device bound, and the jitter with the platform power save mode left in place is recorded rather than answered with servo aggression"

build_once

# The report and PR half: per power-save mode, the capture's inter-device lag
# (AC-2's bound) and the second endpoint's offsets series as a measured jitter
# report (AC-3's record). PASS needs the `none` mode's median lag under the
# wireless bound; without a `none` run the report is MEASURED, a record only.
report_and_publish() {
    local bound series mode modes="" figures jitter none_median=""
    bound="$(transport_conf wireless_bound_us)"
    bench_lag_captures wireless
    for series in "$BENCH_RUN_DIR"/raw/wireless-*.offsets; do
        [ -f "$series" ] || continue
        mode="$(basename "$series" .offsets)"
        mode="${mode#wireless-}"
        modes="$modes $mode"
        if bench_analysis "jitter-$mode" "$BIN_DIR/chorus-measure" jitter "$series" \
            --label "wireless-measured-ps-$mode" --mode "$mode" --transport wireless \
            --measured --out "$BENCH_RUN_DIR/analysis"; then
            jitter="$(sed -n 's/.*: median \([0-9.]*\) us, p95 \([0-9.]*\) us, max \([0-9.]*\) us, peak to peak \([0-9.]*\) us over \([0-9]*\) observations$/median \1, p95 \2, max \3, peak to peak \4 over \5 observations/p' \
                "$BENCH_RUN_DIR/analysis/jitter-$mode.txt" | tail -n 1)"
            bench_field "jitter_us_$mode" "$jitter"
        else
            bench_field "jitter_us_$mode" "the series did not analyse"
        fi
        if [ -f "$BENCH_RUN_DIR/analysis/lag-capture-$mode.txt" ]; then
            read -r -a figures <<< "$(bench_lag_figures "$BENCH_RUN_DIR/analysis/lag-capture-$mode.txt")"
            [ "$mode" = none ] && none_median="${figures[0]:-}"
        fi
    done
    modes="${modes# }"
    bench_field modes "${modes:-none recorded}"
    bench_field bound_us "$bound (config/transport.conf wireless_bound_us)"
    if [ -n "$none_median" ]; then
        local abs="${none_median#[-+]}"
        if awk -v m="$abs" -v b="$bound" 'BEGIN { exit !(m < b) }'; then
            bench_finish PASS "power save none: median inter-device lag $none_median us inside the $bound us bound; jitter recorded for: $modes"
        fi
        bench_finish FAIL "power save none: median inter-device lag $none_median us, outside the $bound us bound; jitter recorded for: $modes"
    fi
    bench_finish MEASURED "jitter recorded for: ${modes:-no mode}; no resolved power-save-none capture, so AC-2's bound is not graded"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" wifi7-wireless
    report_and_publish
fi

RUN_SECONDS="${CHORUS_WIRELESS_RUN_SECONDS:-$(sync_conf sync_hour_run_seconds)}"
CAPTURE_SECONDS="$(sync_conf sync_hour_capture_seconds)"
SETTLE_SECONDS="$(sync_conf sync_hour_settle_seconds)"
DECLARED_MODE="$(endpoint_conf link_wifi_power_save)"
COEXISTENCE="$(endpoint_conf link_wifi_coexistence)"
WIRELESS_BOUND="$(transport_conf wireless_bound_us)"
WIRELESS_LATENCY="$(transport_conf wireless_playout_latency_us)"
CAPTURE_DEVICE="$(capture_device)"

# The two modes this characterization is OF. One report per mode is what AC-12
# asks for, and running both is what makes the difference between them a
# measurement rather than an argument.
MODES="${CHORUS_WIRELESS_MODES:-none min-modem}"

say "chorus: the wireless characterization, one run per power-save mode"
say "  criterion:       AC-2 and AC-3, the two criteria of this phase that need hardware"
say "  modes:           $MODES"
say "  endpoint sets:   $DECLARED_MODE (firmware/config/endpoint.conf)"
say "  coexistence:     $COEXISTENCE"
say "  bound:           ${WIRELESS_BOUND} us between rooms (config/transport.conf)"
say "  playout latency: ${WIRELESS_LATENCY} us, the wireless policy's"
say "  run:             ${RUN_SECONDS}s, of which the first ${SETTLE_SECONDS}s is acquisition"
say "  capture:         ${CAPTURE_SECONDS}s per mode"
say "  access point:    ${CHORUS_WIRELESS_AP:-<unset>}"
say "  endpoint:        ${CHORUS_ESP32S3_PORT:-<unset>}"
say "  second endpoint: ${CHORUS_SECOND_ENDPOINT:-<unset>}"
say "  capture device:  $CAPTURE_DEVICE"

# Every prerequisite is checked before anything is started, so a run that cannot
# be graded emits nothing at all, and nothing is written to a serial port or
# fetched from another machine. The first two are absent on every machine, not
# only this one. The endpoint console this run drives does not exist yet (audit
# A-13; goal 8 builds it), and the network name and
# the secret are declared unknown in this repository and will go on being
# declared unknown.
require_endpoint_console "$CRITERION"
require_wireless_link "$CRITERION"
require_esp32s3_endpoint "$CRITERION"
require_second_endpoint "$CRITERION"
require_capture_device "$CRITERION"

bench_begin wifi7-wireless "$CRITERION"
bench_device "capture interface: ALSA $CAPTURE_DEVICE (L = ESP32-S3 endpoint, R = second endpoint)"
bench_device "ESP32-S3 endpoint on the wireless link, access point: $CHORUS_WIRELESS_AP"
bench_device "second endpoint: chorus-client in another room over ssh, ALSA ${CHORUS_SECOND_DEVICE:-default} (address not recorded, K27)"
bench_reproduce "tools/wireless-characterization-run.sh"
OUT_DIR="$BENCH_RUN_DIR/raw"

PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
read -r -a SERVER_EXTRA_ARGS <<< "${CHORUS_SERVER_EXTRA_ARGS:-}"

say "chorus: starting the server; the zone the ESP32-S3 plays is declared WIRELESS"
"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "0.0.0.0:$PORT" \
    --source chirp \
    --measure-config "$REPO_ROOT/config/measure.conf" \
    --rate "$(endpoint_conf i2s_sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rttime-us "$(conf rttime_us)" \
    --rt-priority "$(conf rt_priority)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
    --zone "wireless-room=wireless" \
    --zone "second-room=wireless" \
    "${CONTRACT_ARGS[@]}" "${SERVER_EXTRA_ARGS[@]}" &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT
sleep 1

SERVER_HOST="${CHORUS_SERVER_HOST:-$(hostname)}"

for MODE in $MODES; do
    say ""
    say "chorus: === power save $MODE ==="
    say "chorus: setting the endpoint's power save mode over its serial console"
    # The endpoint SETS the mode it is told to and reports the mode it set; the
    # line it publishes is what says which mode was actually in force, and the
    # report below carries that word and not this one.
    printf 'power-save %s\n' "$MODE" > "$CHORUS_ESP32S3_PORT"
    printf 'server %s:%s\n' "$SERVER_HOST" "$PORT" > "$CHORUS_ESP32S3_PORT"

    say "chorus: starting the second endpoint on $CHORUS_SECOND_ENDPOINT, in another room"
    REMOTE_COMMAND="chorus-client --ephemeral-identity --endpoint-id wireless-second-room --server ${SERVER_HOST}:${PORT} \
--device ${CHORUS_SECOND_DEVICE:-default} \
--transport wireless \
--zone second-room \
--delay-log wireless-characterization-${MODE}.log \
--offsets-out wireless-characterization-${MODE}.offsets \
--sync-interval-ms $(sync_conf sync_interval_ms) \
--filter-window $(sync_conf filter_window) \
--smoothing-alpha $(sync_conf smoothing_alpha) \
--hard-resync-threshold-us $(sync_conf hard_resync_threshold_us) \
--max-correction-ppm $(sync_conf max_correction_ppm) \
--staleness-limit-ms $(sync_conf staleness_limit_ms) \
--max-rtt-us $(sync_conf max_rtt_us) \
--mute-us $(sync_conf mute_us) \
--run-seconds $RUN_SECONDS"
    # shellcheck disable=SC2029 # the remote command line is built here on purpose, from this checkout's config
    ssh "$CHORUS_SECOND_ENDPOINT" "$REMOTE_COMMAND" &
    SECOND_PID=$!
    trap 'kill_quietly "$SECOND_PID"; kill_quietly "$SERVER_PID"' EXIT

    say "chorus: letting the loop acquire the timeline for ${SETTLE_SECONDS}s"
    sleep "$SETTLE_SECONDS"

    WAV="$OUT_DIR/capture-$MODE.wav"
    say "chorus: capturing both line outputs for ${CAPTURE_SECONDS}s"
    "$BIN_DIR/chorus-measure-capture" \
        --record-only \
        --capture-device "$CAPTURE_DEVICE" \
        --seconds "$CAPTURE_SECONDS" \
        --out "$WAV"

    say "chorus: fetching the second endpoint's offsets series (chorus-client --offsets-out)"
    scp -q "$CHORUS_SECOND_ENDPOINT:wireless-characterization-${MODE}.offsets" \
        "$OUT_DIR/wireless-$MODE.offsets"

    kill_quietly "$SECOND_PID"
done

kill_quietly "$SERVER_PID"
say ""
say "chorus: whatever the reports say, no constant in config/sync.conf moves because of it."
say "chorus: analysing both modes and writing the bench report"
report_and_publish
