#!/usr/bin/env bash
# The device-backed measurement run: play the chirp, capture both endpoint line
# outputs, and analyse the capture.
#
# NEEDS AN ENVIRONMENT. Two endpoints playing one grouped stream, both line
# outputs wired into the L and R inputs of one audio interface, and that
# interface visible to ALSA as a capture device. On a machine without it this
# script exits non-zero naming the missing prerequisite and the criterion it was
# verifying, and reports nothing as passed or skipped-green.
#
# The chirp leaves through a real amplifier into a real loudspeaker. The
# amplitude ceiling in config/measure.conf is checked before any device is
# opened, so a run asking for more than it permits refuses having emitted
# nothing. Do not raise that ceiling to make a quiet capture louder; move the
# interface's input gain instead.
#
#   CHORUS_CAPTURE_DEVICE=hw:1,0 CHORUS_CLIENT_DEVICE=hw:0,0 \
#       ./tools/measure/capture-run.sh [seconds]

source "$(dirname "$0")/../lib.sh"

CRITERION="two endpoint line outputs captured together, cross-correlated into median, p95 and maximum inter-device lag at 10 us or better"

build_once

SECONDS_TO_CAPTURE="${1:-10}"
CAPTURE_DEVICE="$(capture_device)"
OUT_DIR="${CHORUS_MEASURE_OUT:-${TMPDIR:-/tmp}}"
CAPTURE_FILE="$OUT_DIR/chorus-measure-capture.wav"

say "chorus: the device-backed measurement run"
require_capture_device "$CRITERION"

say "chorus: capturing $SECONDS_TO_CAPTURE s into $CAPTURE_FILE"
"$BIN_DIR/chorus-measure-capture" \
    --capture-device "$CAPTURE_DEVICE" \
    --seconds "$SECONDS_TO_CAPTURE" \
    --out "$CAPTURE_FILE"

say "chorus: analysing the capture"
"$BIN_DIR/chorus-measure" lag "$CAPTURE_FILE" --label "device-run"

say "chorus: the run completed and its report is in docs/measurements/"
