#!/usr/bin/env bash
# AC-4, the three-day soak. NOT PASSED IN THIS REPOSITORY.
#
# "WHEN the system runs unattended for at least three days THE SYSTEM SHALL
# still meet its sync bound and SHALL report no unexplained resync."
#
# There is no way to shorten this and no way to model it into existence. It
# needs THREE DAYS OF WALL CLOCK and it needs the RIG-3 capture rig to measure
# the bound it is held to, because the bound is `chorus#SYNC-4`'s own first
# criterion - median inter-device error below 0.5 ms as measured by that rig -
# and that criterion is ITSELF blocked on two wired endpoints and an audio
# interface. This machine is one container with no sound card, no capture
# device, no second endpoint and no three days.
#
# So this exits non-zero naming both prerequisites, and reports nothing as
# passed, skipped-green or satisfied. `docs/verification-record.md` quotes the
# refusal verbatim.
#
# What stands beside it, and what it is NOT:
#
#   cargo test -p chorus-client-linux --test soak_72h -- --nocapture
#
# 72 MODELLED hours in four modelled sessions, driving the real sync loop, the
# real playout corrector, the real control catalog, the real zone state, its
# real persisted form and the real bounded fanout, against modelled clocks, a
# modelled network and a modelled DAC. Every hard resync in it carries a named
# cause and any that carries none is counted and reported as unexplained. It is
# a MODELLED RESULT. It is not a measurement, it is not this criterion, and the
# verification record says so where it reports the figures.
#
# What an operator with the environment runs:
#
#   CHORUS_SOAK_SECONDS=259200 CHORUS_SECOND_ENDPOINT=user@endpoint-b \
#   CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
#       ./tools/soak-run.sh          # or: make verify-soak
#   ./tools/soak-run.sh --report-from <run directory>
#
# THE BENCH REPORT (K45). The run ends by writing
# docs/measurements/product6-soak-<date>.md with both delay logs hashed (a
# three-day log is larger than the committed limit, so it is hashed and stays
# on the owner's machine), and with CHORUS_BENCH_PR=1 it commits that on
# bench/<date>-product6-soak and opens the PR. The soak takes no RIG-3 capture,
# so the best its report can say about the sync bound is INCOMPLETE: the logs
# graded clean and the bound left to a capture run.

source "$(dirname "$0")/lib.sh"
source "$REPO_ROOT/tools/bench/lib.sh"
bench_args "$@"

build_once

# The report and PR half: the delay logs in, the bench report out.
report_and_publish() {
    local seconds=259200 local_log remote_log grade=1 resyncs_local resyncs_remote="none"
    local_log="$BENCH_RUN_DIR/raw/local.log"
    remote_log="$BENCH_RUN_DIR/raw/remote.log"
    bench_analysis delaylog-local "$BIN_DIR/chorus-delaylog-check" "$local_log" \
        --min-graded-seconds "$seconds" --require-zero-underruns --require-no-rate-change \
        && grade=0
    resyncs_local="$(bench_hard_resyncs_after "$local_log" 0)"
    if [ -f "$remote_log" ]; then
        resyncs_remote="$(bench_hard_resyncs_after "$remote_log" 0)"
    fi
    bench_field soak_seconds "$seconds"
    bench_field delaylog_local "$([ "$grade" = 0 ] && echo graded clean || echo failed its grading)"
    bench_field graded_seconds_local "$(bench_delaylog_value "$local_log" graded_span_us \
        | awk '/^[0-9]+$/ { printf "%.0f", $1 / 1e6; next } { print "none" }')"
    bench_field hard_resyncs "local $resyncs_local, remote $resyncs_remote (each one is in the log with its cause)"
    bench_field sync_bound "not graded: the soak takes no RIG-3 capture"
    local summary
    summary="the local delay log $([ "$grade" = 0 ] && echo graded clean || echo failed its grading) over ${seconds}s; hard resyncs: local $resyncs_local, remote $resyncs_remote; the sync bound needs a RIG-3 capture this run does not take"
    [ "$grade" = 0 ] && bench_finish INCOMPLETE "$summary"
    bench_finish FAIL "$summary"
}

if [ -n "$BENCH_REPORT_FROM" ]; then
    bench_load "$BENCH_REPORT_FROM" product6-soak
    report_and_publish
fi

# Three days, in seconds. Committed here rather than passed in, because the
# criterion says "at least three days" and a run shorter than that is a
# different claim.
SOAK_SECONDS=259200

CRITERION="a system left unattended for at least three days still meets its sync bound and reports no unexplained resync"

say "chorus: the three-day soak"
say "  criterion:       AC-4, the one criterion of this phase that needs hardware AND time"
say "  window:          ${SOAK_SECONDS}s of wall clock, which is three days"
say "  bound:           chorus#SYNC-4's own first criterion, as measured by the RIG-3 rig"
say "  soak window:     ${CHORUS_SOAK_SECONDS:-<unset>}"
say "  second endpoint: ${CHORUS_SECOND_ENDPOINT:-<unset>}"
say "  capture:         $(capture_device)"
say "  local device:    $(audio_device)"
say "  modelled beside: cargo test -p chorus-client-linux --test soak_72h"

require_soak_window "$CRITERION" "$SOAK_SECONDS"
require_second_endpoint "$CRITERION"
require_capture_device "$CRITERION"
require_pacing_audio_device "$CRITERION"

bench_begin product6-soak "$CRITERION"
bench_device "local endpoint: chorus-client on the bench machine, ALSA $(audio_device)"
bench_device "remote endpoint: chorus-client over ssh (address not recorded, K27)"
bench_reproduce "CHORUS_SOAK_SECONDS=$SOAK_SECONDS tools/soak-run.sh"

# Nothing below this line has ever run in this repository. It is what an
# operator with three days, two endpoints and an interface would run, and it is
# committed so that they do not have to invent it.
say ""
say "chorus: starting a ${SOAK_SECONDS}s unattended run"

OUT_DIR="$BENCH_RUN_DIR"
mkdir -p "$OUT_DIR/raw"
AUDIO="$(free_port)"
CONTROL="$(free_port)"
read -r -a CONTRACT_ARGS <<< "$(server_contract_args)"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$AUDIO" \
    --source tone \
    --serve-forever \
    --chunk-us "$(conf chunk_us)" \
    --control-listen "127.0.0.1:$CONTROL" \
    --state-file "$OUT_DIR/zones.state" \
    --zone local --zone remote \
    "${CONTRACT_ARGS[@]}" >"$OUT_DIR/server.log" 2>&1 &
SERVER=$!
trap 'kill_quietly "$SERVER"' EXIT
sleep 2

"$BIN_DIR/chorus-client" \
    --ephemeral-identity \
    --server "127.0.0.1:$AUDIO" \
    --control "127.0.0.1:$CONTROL" \
    --zone local --endpoint endpoint-local --rejoin \
    --device "$(audio_device)" \
    --run-seconds "$SOAK_SECONDS" \
    --delay-log "$OUT_DIR/raw/local.log" \
    --sync-interval-ms "$(sync_conf sync_interval_ms)" \
    --playout-latency-us "$(sync_conf playout_latency_us)" &
LOCAL=$!

# shellcheck disable=SC2029 # the remote command line is built here on purpose, from this checkout's config
ssh "$CHORUS_SECOND_ENDPOINT" \
    "chorus-client --ephemeral-identity --server $(hostname):$AUDIO --control $(hostname):$CONTROL \
     --zone remote --endpoint endpoint-remote --rejoin \
     --device \$CHORUS_CLIENT_DEVICE --run-seconds $SOAK_SECONDS \
     --delay-log chorus-soak-remote.log" &
REMOTE=$!

set +e
wait "$LOCAL" "$REMOTE"
set -e
kill_quietly "$SERVER"
scp -q "$CHORUS_SECOND_ENDPOINT:chorus-soak-remote.log" "$OUT_DIR/raw/remote.log" \
    || say "chorus: the remote delay log could not be fetched; the report says so"

# The two delay logs ARE the evidence; the report hashes them and grades the
# local one, and a capture run grades the bound.
say "chorus: grading the local delay log and writing the bench report"
report_and_publish
