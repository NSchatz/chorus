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

say ""
say "chorus: both reports are in docs/measurements/"
