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
# NEEDS AN ENVIRONMENT.
#   - a wireless network the endpoint may join, in firmware/config/endpoint.conf,
#     which this repository declares UNKNOWN and will go on declaring unknown
#   - the access point the run is taken against, named (CHORUS_WIRELESS_AP)
#   - an ESP32-S3 endpoint flashed with an image built from that configuration,
#     on that network (CHORUS_ESP32S3_PORT)
#   - a second endpoint in ANOTHER ROOM playing the same grouped stream
#     (CHORUS_SECOND_ENDPOINT)
#   - both endpoints' line outputs wired into the L and R inputs of one audio
#     interface, visible to ALSA as a capture device (CHORUS_CAPTURE_DEVICE)
#   - a local ALSA playback device that reports a delay (CHORUS_CLIENT_DEVICE)
#
# The chirp leaves through a real amplifier into a real loudspeaker. The
# amplitude ceiling in config/measure.conf is checked by the capture tool before
# any device is opened; do not raise it to make a quiet capture louder, move the
# interface's input gain instead.
#
#   CHORUS_WIRELESS_AP='ubiquiti-u6-lite 5GHz' CHORUS_ESP32S3_PORT=/dev/ttyACM0 \
#   CHORUS_SECOND_ENDPOINT=user@endpoint-b CHORUS_CAPTURE_DEVICE=hw:1,0 \
#   CHORUS_CLIENT_DEVICE=hw:0,0 \
#       ./tools/wireless-characterization-run.sh   # or: make verify-wireless

source "$(dirname "$0")/lib.sh"

CRITERION="a Wi-Fi endpoint with modem sleep disabled holds the 5 ms multiroom inter-device bound, and the jitter with the platform power save mode left in place is recorded rather than answered with servo aggression"

build_once

RUN_SECONDS="${CHORUS_WIRELESS_RUN_SECONDS:-$(sync_conf sync_hour_run_seconds)}"
CAPTURE_SECONDS="$(sync_conf sync_hour_capture_seconds)"
SETTLE_SECONDS="$(sync_conf sync_hour_settle_seconds)"
DECLARED_MODE="$(endpoint_conf link_wifi_power_save)"
COEXISTENCE="$(endpoint_conf link_wifi_coexistence)"
WIRELESS_BOUND="$(transport_conf wireless_bound_us)"
WIRELESS_LATENCY="$(transport_conf wireless_playout_latency_us)"
DEVICE="$(audio_device)"
CAPTURE_DEVICE="$(capture_device)"
OUT_DIR="${CHORUS_MEASURE_OUT:-${TMPDIR:-/tmp}}"

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
say "  local device:    $DEVICE"
say "  capture device:  $CAPTURE_DEVICE"

# Every prerequisite is checked before anything is started, so a run that cannot
# be graded emits nothing at all. The wireless one is first because it is the
# one this entry point is about, and because it is absent on every machine: the
# network name and the secret are declared unknown in this repository and will
# go on being declared unknown.
require_wireless_link "$CRITERION"
require_esp32s3_endpoint "$CRITERION"
require_second_endpoint "$CRITERION"
require_pacing_audio_device "$CRITERION"
require_capture_device "$CRITERION"

PORT="$(free_port)"
CONTRACT_ARGS="$(server_contract_args)"

say "chorus: starting the server; the zone the ESP32-S3 plays is declared WIRELESS"
"$BIN_DIR/chorus-server" \
    --listen "0.0.0.0:$PORT" \
    --source tone \
    --rate "$(endpoint_conf i2s_sample_rate_hz)" \
    --channels "$(conf channels)" \
    --format "$(conf sample_format)" \
    --chunk-us "$(conf chunk_us)" \
    --rttime-us "$(conf rttime_us)" \
    --rt-priority "$(conf rt_priority)" \
    --memlock-wanted-bytes "$(conf memlock_wanted_bytes)" \
    --zone "wireless-room=wireless" \
    --zone "second-room=wireless" \
    $CONTRACT_ARGS ${CHORUS_SERVER_EXTRA_ARGS:-} &
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
    REMOTE_COMMAND="chorus-client --server ${SERVER_HOST}:${PORT} \
--device ${CHORUS_SECOND_DEVICE:-default} \
--transport wireless \
--zone second-room \
--delay-log wireless-characterization-${MODE}.log \
--sync-interval-ms $(sync_conf sync_interval_ms) \
--filter-window $(sync_conf filter_window) \
--smoothing-alpha $(sync_conf smoothing_alpha) \
--hard-resync-threshold-us $(sync_conf hard_resync_threshold_us) \
--max-correction-ppm $(sync_conf max_correction_ppm) \
--staleness-limit-ms $(sync_conf staleness_limit_ms) \
--max-rtt-us $(sync_conf max_rtt_us) \
--mute-us $(sync_conf mute_us) \
--run-seconds $RUN_SECONDS"
    ssh "$CHORUS_SECOND_ENDPOINT" "$REMOTE_COMMAND" &
    SECOND_PID=$!
    trap 'kill_quietly "$SECOND_PID"; kill_quietly "$SERVER_PID"' EXIT

    say "chorus: letting the loop acquire the timeline for ${SETTLE_SECONDS}s"
    sleep "$SETTLE_SECONDS"

    WAV="$OUT_DIR/chorus-wireless-$MODE.wav"
    say "chorus: capturing both line outputs for ${CAPTURE_SECONDS}s"
    "$BIN_DIR/chorus-measure-capture" \
        --capture-device "$CAPTURE_DEVICE" \
        --seconds "$CAPTURE_SECONDS" \
        --out "$WAV"

    say "chorus: inter-device lag over that capture, which is what AC-2's bound is about"
    "$BIN_DIR/chorus-measure" lag "$WAV" --label "wireless-$MODE"

    say "chorus: the jitter the second endpoint's own delay log recorded, into docs/measurements/"
    SERIES="$OUT_DIR/chorus-wireless-$MODE.offsets"
    scp "$CHORUS_SECOND_ENDPOINT:wireless-characterization-${MODE}.offsets" "$SERIES"
    "$BIN_DIR/chorus-measure" jitter "$SERIES" \
        --label "wireless-measured-ps-$MODE" \
        --mode "$MODE" \
        --transport wireless \
        --measured

    kill_quietly "$SECOND_PID"
done

kill_quietly "$SERVER_PID"
say ""
say "chorus: the characterization completed and its reports are in docs/measurements/"
say "chorus: one report per power-save mode, each naming the mode that was in force."
say "chorus: whatever they say, no constant in config/sync.conf moves because of it."
