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

source "$(dirname "$0")/lib.sh"

build_once

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

# Nothing below this line has ever run in this repository. It is what an
# operator with three days, two endpoints and an interface would run, and it is
# committed so that they do not have to invent it.
say ""
say "chorus: starting a ${SOAK_SECONDS}s unattended run"

OUT_DIR="${TMPDIR:-/tmp}/chorus-soak"
mkdir -p "$OUT_DIR"
AUDIO="$(free_port)"
CONTROL="$(free_port)"
CONTRACT_ARGS="$(server_contract_args)"

"$BIN_DIR/chorus-server" \
    --listen "127.0.0.1:$AUDIO" \
    --source tone \
    --serve-forever \
    --chunk-us "$(conf chunk_us)" \
    --control-listen "127.0.0.1:$CONTROL" \
    --state-file "$OUT_DIR/zones.state" \
    --zone local --zone remote \
    $CONTRACT_ARGS >"$OUT_DIR/server.log" 2>&1 &
SERVER=$!
trap 'kill_quietly "$SERVER"' EXIT
sleep 2

"$BIN_DIR/chorus-client" \
    --server "127.0.0.1:$AUDIO" \
    --control "127.0.0.1:$CONTROL" \
    --zone local --endpoint endpoint-local --rejoin \
    --device "$(audio_device)" \
    --run-seconds "$SOAK_SECONDS" \
    --delay-log "$OUT_DIR/local.log" \
    --sync-interval-ms "$(sync_conf sync_interval_ms)" \
    --playout-latency-us "$(sync_conf playout_latency_us)" &
LOCAL=$!

ssh "$CHORUS_SECOND_ENDPOINT" \
    "chorus-client --server $(hostname):$AUDIO --control $(hostname):$CONTROL \
     --zone remote --endpoint endpoint-remote --rejoin \
     --device \$CHORUS_CLIENT_DEVICE --run-seconds $SOAK_SECONDS \
     --delay-log chorus-soak-remote.log" &
REMOTE=$!

wait "$LOCAL" "$REMOTE"
kill_quietly "$SERVER"

# The two delay logs and the captures ARE the evidence, and they are graded
# afterwards by someone who did not take them.
"$BIN_DIR/chorus-delaylog-check" "$OUT_DIR/local.log" \
    --min-graded-seconds "$SOAK_SECONDS" \
    --require-zero-underruns \
    --require-no-rate-change
say "chorus: the soak finished; grade the captures with chorus-measure lag"
