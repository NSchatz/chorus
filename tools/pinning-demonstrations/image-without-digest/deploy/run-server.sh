#!/usr/bin/env bash
# WRONG ON PURPOSE. This is deploy/run-server.sh with one thing changed: the
# CHORUS_IMAGE default is a registry reference instead of the locally built
# chorus-server:dev tag.
#
# That is the whole of the exemption in the real file. A locally built image has
# no publisher and no digest to resolve, so P1 has nothing to pin it to; a
# ghcr.io reference has both, and takes a digest like any other image.
# tools/pinning-check.sh requires this to go red naming P1.

set -euo pipefail

IMAGE="${CHORUS_IMAGE:-ghcr.io/example/chorus-server:1.2.3}"
NAME="${CHORUS_CONTAINER:-chorus-server}"

exec docker run --rm --name "$NAME" "$IMAGE"
