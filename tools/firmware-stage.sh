#!/usr/bin/env bash
# Stage a firmware image built by `make firmware-image` (or tools/firmware-image.sh) in a
# server's firmware directory: the image as <name>.bin and the manifest the server verifies it
# against (version, board, size, sha256), written by the server's own helper so what is staged
# is what the server will check (docs/firmware-updates.md, goal 14).
#
#   tools/firmware-stage.sh <build-dir> <firmware-dir> [name]
#
# The board is the profile the build recorded (board_profile.conf in the build directory); the
# version is the one inside the image's application description; the name defaults to the
# board and the version. Local files only: this opens no socket and writes to no device.
# Staging is not installing: a speaker is offered an image only when somebody sends the
# control plane's firmware_install command.
#
# CHORUS_SERVER_BIN names a chorus-server binary to use; otherwise the workspace's is built
# (cargo build --locked -p chorus-server) and used.
. "$(dirname "$0")/lib.sh"

usage() {
    note "usage: tools/firmware-stage.sh <build-dir> <firmware-dir> [name]"
    exit 2
}
[ $# -ge 2 ] && [ $# -le 3 ] || usage
build="$1"
dir="$2"
name="${3:-}"

image="$build/chorus-endpoint.bin"
[ -f "$image" ] || {
    note "REFUSED: $image is not there; build the image first (make firmware-image)"
    exit 2
}
board=""
for profile in "$build/board_profile.conf" "$build/esp-idf/main/board_profile.conf"; do
    if [ -f "$profile" ]; then
        board="$(sed -n 's/^board_profile *= *//p' "$profile" | head -n 1)"
        [ -n "$board" ] && break
    fi
done
[ -n "$board" ] || {
    note "REFUSED: $build holds no board_profile.conf naming a board_profile; rebuild with make firmware-image"
    exit 2
}

server="${CHORUS_SERVER_BIN:-}"
if [ -z "$server" ]; then
    command -v cargo > /dev/null 2>&1 ||
        missing_prerequisite "stage a firmware image" "cargo (the pinned Rust toolchain)" \
            "install it with mise (rust-toolchain.toml), or set CHORUS_SERVER_BIN to a chorus-server binary"
    (cd "$REPO_ROOT" && cargo build --locked -q -p chorus-server) || exit 1
    server="$TARGET_DIR/debug/chorus-server"
fi

args=(stage-firmware --image "$image" --board "$board" --firmware-dir "$dir")
[ -n "$name" ] && args+=(--name "$name")
"$server" "${args[@]}"
