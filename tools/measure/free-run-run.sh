#!/usr/bin/env bash
# RIG-3's free-run drift baseline from two real clients (audit A-4): both
# endpoints play one grouped stream with correction DISABLED, each writes its
# playout offset against the server timeline (`chorus-client --free-run
# --offsets-out`), and `chorus-measure pair` lines the two series up on the
# server timeline and fits the slope of their difference: the relative rate of
# the two DACs, with its 95% confidence half-width, recorded as the baseline in
# docs/measurements/free-run-baseline.conf with `source = hardware`.
#
# NEEDS AN ENVIRONMENT.
#   - a second wired Linux endpoint running chorus-client (CHORUS_SECOND_ENDPOINT)
#   - a local ALSA playback device that reports a delay (CHORUS_CLIENT_DEVICE)
# No capture interface: the series is the clients' own view of their error,
# which is what a free run is about. On a machine without them this script exits
# non-zero naming the missing prerequisite and the criterion, and reports nothing
# as passed, skipped-green or satisfied.
#
# With correction disabled the two endpoints walk apart at their relative rate
# (BRIEF.md section 6 expects tens of ppm, a few ms over ten minutes), which is
# the point: this run is a baseline, not playout anybody should listen to.
#
# THE BENCH REPORT (K45). The run ends by writing
# docs/measurements/rig3-free-run-<date>.md with both client series and the
# paired series hashed, and the new baseline beside it (tools/bench/lib.sh);
# with CHORUS_BENCH_PR=1 it commits both on bench/<date>-rig3-free-run and opens
# the PR. `--report-from <run directory>` runs only that half.
#
#   CHORUS_SECOND_ENDPOINT=user@endpoint-b CHORUS_CLIENT_DEVICE=hw:0,0 \
#       ./tools/measure/free-run-run.sh [seconds]      # default 600
#   ./tools/measure/free-run-run.sh --report-from <run directory>

source "$(dirname "$0")/../lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"
set -- "${BENCH_ARGS[@]+"${BENCH_ARGS[@]}"}"

CRITERION="a free-run drift baseline between two clients running with correction disabled, measured by the rig rather than borrowed"

build_once

# The report and PR half: the two client series in, the paired series, the fit,
# the baseline and the bench report out.
report_and_publish() {
    local stem branch label figures a_count b_count
    read -r stem branch <<< "$(bench_stem rig3-free-run "$(bench_manifest date)")"
    # chorus-measure names its report rig3-free-run-<label>.md and the baseline
    # cites that path, so the label is the stem's date part and the baseline
    # points at this bench report.
    label="${stem#rig3-free-run-}"
    a_count="$(sed -n '/^\[observations\]/,$p' "$BENCH_RUN_DIR/raw/client-a.offsets" 2>/dev/null | grep -c '^-\{0,1\}[0-9]' || true)"
    b_count="$(sed -n '/^\[observations\]/,$p' "$BENCH_RUN_DIR/raw/client-b.offsets" 2>/dev/null | grep -c '^-\{0,1\}[0-9]' || true)"
    bench_field client_a_observations "$a_count"
    bench_field client_b_observations "$b_count"
    if bench_analysis pair "$BIN_DIR/chorus-measure" pair \
        "$BENCH_RUN_DIR/raw/client-a.offsets" "$BENCH_RUN_DIR/raw/client-b.offsets" \
        --label "$label" --source hardware \
        --series-out "$BENCH_RUN_DIR/raw/paired.offsets" \
        --out "$BENCH_RUN_DIR/analysis" \
        --baseline-out "$BENCH_RUN_DIR/analysis/free-run-baseline.conf"; then
        read -r -a figures <<< "$(sed -n 's/^chorus-measure: \([-+0-9.]*\) ppm (+\/-\([0-9.]*\) ppm) over \([0-9]*\) observations spanning \([0-9.]*\) s$/\1 \2 \3 \4/p' \
            "$BENCH_RUN_DIR/analysis/pair.txt" | tail -n 1)"
        bench_field relative_rate_ppm "${figures[0]}"
        bench_field half_width_ppm "${figures[1]}"
        bench_field observations "${figures[2]}"
        bench_field span_s "${figures[3]}"
        bench_extra_file "$BENCH_RUN_DIR/analysis/free-run-baseline.conf" \
            docs/measurements/free-run-baseline.conf
        bench_finish MEASURED "relative rate ${figures[0]} ppm (+/-${figures[1]} ppm) over ${figures[2]} paired observations spanning ${figures[3]} s, recorded as the free-run baseline"
    fi
    bench_field relative_rate_ppm none
    bench_field half_width_ppm none
    bench_field observations none
    bench_field span_s none
    bench_finish FAIL "the two series did not pair or fit into a publishable slope (the analysis names the condition); the baseline is unchanged"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" rig3-free-run
    report_and_publish
fi

RUN_SECONDS="${1:-600}"
DEVICE="$(audio_device)"

say "chorus: the free-run drift baseline, two clients with correction disabled"
say "  run:             ${RUN_SECONDS}s"
say "  local device:    $DEVICE"
say "  second endpoint: ${CHORUS_SECOND_ENDPOINT:-<unset>}"

require_second_endpoint "$CRITERION"
require_pacing_audio_device "$CRITERION"

bench_begin rig3-free-run "$CRITERION"
bench_device "client A: chorus-client --free-run on the bench machine, ALSA $DEVICE"
bench_device "client B: chorus-client --free-run on the second wired Linux endpoint over ssh, ALSA ${CHORUS_SECOND_DEVICE:-default} (address not recorded, K27)"
bench_reproduce "tools/measure/free-run-run.sh $RUN_SECONDS"
RAW="$BENCH_RUN_DIR/raw"

PORT="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"
"$BIN_DIR/chorus-server" \
    --ephemeral-identity \
    --listen "0.0.0.0:$PORT" \
    --source tone \
    --rate "$(conf sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    "${CONTRACT_ARGS[@]}" &
SERVER_PID=$!
trap 'kill_quietly "$SERVER_PID"' EXIT
sleep 1

# The same committed constants every rig run passes, plus --free-run: the loop
# still exchanges and forms its error at the committed cadence, and applies
# nothing.
client_args() {
    printf '%s' "--min-us $(conf min_us) --max-us $(conf max_us) \
--start-fill-us $(conf start_fill_us) --device-target-us $(conf device_target_us) \
--sync-interval-ms $(sync_conf sync_interval_ms) \
--filter-window $(sync_conf filter_window) \
--smoothing-alpha $(sync_conf smoothing_alpha) \
--max-rtt-us $(sync_conf max_rtt_us) \
--staleness-limit-ms $(sync_conf staleness_limit_ms) \
--playout-latency-us $(sync_conf playout_latency_us) \
--run-seconds $RUN_SECONDS --free-run"
}

read -r -a CLIENT_ARGS <<< "$(client_args)"
"$BIN_DIR/chorus-client" \
    --ephemeral-identity --endpoint-id free-run-client-a \
    --server "127.0.0.1:$PORT" \
    --device "$DEVICE" \
    --delay-log "$RAW/client-a.log" \
    --offsets-out "$RAW/client-a.offsets" \
    "${CLIENT_ARGS[@]}" &
CLIENT_A_PID=$!

SERVER_HOST="${CHORUS_SERVER_HOST:-$(hostname)}"
# shellcheck disable=SC2029 # the remote command line is built here on purpose, from this checkout's config
ssh "$CHORUS_SECOND_ENDPOINT" "chorus-client --ephemeral-identity --endpoint-id free-run-client-b \
--server ${SERVER_HOST}:${PORT} --device ${CHORUS_SECOND_DEVICE:-default} \
--delay-log free-run-client-b.log --offsets-out free-run-client-b.offsets $(client_args)" &
CLIENT_B_PID=$!
trap 'kill_quietly "$CLIENT_B_PID"; kill_quietly "$CLIENT_A_PID"; kill_quietly "$SERVER_PID"' EXIT

set +e
wait "$CLIENT_A_PID"
wait "$CLIENT_B_PID"
set -e
kill_quietly "$SERVER_PID"
scp -q "$CHORUS_SECOND_ENDPOINT:free-run-client-b.offsets" "$RAW/client-b.offsets" \
    || say "chorus: client B's series could not be fetched; the report says so"
scp -q "$CHORUS_SECOND_ENDPOINT:free-run-client-b.log" "$RAW/client-b.log" || true

say "chorus: pairing the two series, fitting the baseline and writing the bench report"
report_and_publish
