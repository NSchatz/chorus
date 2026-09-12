#!/usr/bin/env bash
# The saved reports over the committed fixtures.
#
# Needs no device and no privilege. This is what puts a file in
# docs/measurements/, and it is how a reader who did not take a capture
# reproduces every figure in one: the inputs are in the tree, the parameters
# that made them are beside them, and this script is the command.
#
# The free-run report runs first, because it is what records the baseline the
# lag report then cites.
#
#   ./tools/measure/fixture-reports.sh

source "$(dirname "$0")/../lib.sh"

build_once

FIXTURES="$REPO_ROOT/fixtures/measure"

say "chorus: the free-run baseline, from the noiseless committed series"
"$BIN_DIR/chorus-measure" free-run "$FIXTURES/10-free-run-noiseless.offsets" \
    --label "noiseless-fixture" --source fixture

say ""
say "chorus: inter-device lag, over the committed reference capture"
"$BIN_DIR/chorus-measure" lag "$FIXTURES/01-chirp-pair-a.wav" --label "fixture-reference-capture"

# chorus#WIFI-7: one report per power-save mode. The series are MODELLED and
# every report says so in its own words; the measured version of this run needs
# an ESP32-S3 on a wireless link and the capture rig, and
# tools/wireless-characterization-run.sh refuses by name without them.
say ""
say "chorus: wireless jitter with modem sleep DISABLED, over the committed series"
"$BIN_DIR/chorus-measure" jitter "$FIXTURES/14-wireless-jitter-ps-none.offsets" \
    --label "wireless-ps-none" --mode none --transport wireless

say ""
say "chorus: wireless jitter with the PLATFORM DEFAULT left in place"
"$BIN_DIR/chorus-measure" jitter "$FIXTURES/15-wireless-jitter-ps-min-modem.offsets" \
    --label "wireless-ps-min-modem" --mode min-modem --transport wireless

say ""
say "chorus: all four reports are in docs/measurements/"
