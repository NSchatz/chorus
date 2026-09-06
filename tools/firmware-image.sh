#!/usr/bin/env bash
# Build the ESP32-S3 endpoint image.
#
# AC-17: "WHEN the image build is invoked without the ESP-IDF toolchain present
# THE SYSTEM SHALL exit non-zero naming the toolchain and the version it
# expects, and SHALL NOT emit a partial image."
#
# The refusal comes FIRST, before any output directory is created and before
# any object is compiled, so there is nothing partial to leave behind. That
# ordering is the assertion, not the message: a build that refused after
# writing half an image would have satisfied the words and broken the point.
#
# The version it expects is firmware/config/endpoint.conf's espidf_version, and
# the refusal names it. A DIFFERENT installed version is refused too: an image
# built by a toolchain nobody chose looks exactly like an image that was.
#
# THIS SHIPS NOTHING. It builds an image and stops. OTA, rollback and every
# form of image activation belong to chorus#FLEET-10, and
# firmware/check/endpoint-scan.c fails the suite if a line of the endpoint tree
# so much as names one.
#
# NEEDS AN ENVIRONMENT.
#   - ESP-IDF at the version firmware/config/endpoint.conf declares, exported
#     (IDF_PATH set, idf.py on PATH)
#
#   . $IDF_PATH/export.sh && ./tools/firmware-image.sh    # or: make firmware-image

source "$(dirname "$0")/lib.sh"

CRITERION="the image build, invoked without the ESP-IDF toolchain present, exits non-zero naming the toolchain and the version it expects and emits no partial image"

OUT_DIR="${CHORUS_IMAGE_OUT:-$REPO_ROOT/firmware/build/image}"

say "chorus: the ESP32-S3 endpoint image"
say "  criterion:  AC-17"
say "  toolchain:  esp-idf $(endpoint_conf espidf_version)"
say "  IDF_PATH:   ${IDF_PATH:-<unset>}"
say "  output:     $OUT_DIR"

# Every prerequisite is checked before anything is created, so a build that
# cannot run leaves the tree exactly as it found it.
require_espidf "$CRITERION"

# The configuration gates the image for the same reason it gates the host
# build: a pin map that claims a reserved GPIO, or a 24-bit slot width with an
# MCLK multiple not divisible by three, refuses rather than builds.
make -f "$REPO_ROOT/firmware/Makefile" config-check

mkdir -p "$OUT_DIR"
say "chorus: building"
(cd "$REPO_ROOT/firmware" && idf.py -B "$OUT_DIR" build)

say "chorus: the image is in $OUT_DIR and has NOT been flashed or shipped"
say "chorus: flashing is an operator act; OTA is chorus#FLEET-10 and is not in this phase"
