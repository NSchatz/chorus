#!/usr/bin/env bash
# The pinned emulator and its libraries: install them when absent, otherwise
# only verify them (goal 14, docs/decisions/0109-*).
#
#   bash tools/qemu-env.sh            # verify; exits non-zero saying what is wrong
#   bash tools/qemu-env.sh install    # install whatever is absent, then verify
#
# What it installs, rootless, with no system package and nothing outside the
# two directories below:
#   - Espressif's QEMU release archive (tools/qemu/pins.conf: version, URL,
#     sha256), extracted under CHORUS_QEMU_HOME (default /cache/opt/chorus-qemu)
#     in a directory named after the version;
#   - micromamba (the same file: version, URL, sha256), beside it;
#   - the conda-forge library environment the emulator's binary loads
#     (tools/qemu/libs.explicit.txt: every package by exact build and sha256),
#     at CHORUS_QEMU_LIBS (default /cache/opt/chorus-qemu-libs).
# Every download is checked against its pinned sha256 before it is used, and a
# mismatch stops the install with the file removed.
#
# What it never does: replace something that is present. An emulator or an
# environment that is installed and is NOT the pinned one is reported and left
# alone; whoever owns the machine removes it. A verification that fails is an
# exit code, never a quiet reinstall.
#
# The emulator is GPL-2.0 software run as an unmodified binary. Nothing here
# reads, builds or links its source (docs/clean-room.md).

source "$(dirname "$0")/lib.sh"
# shellcheck source=qemu/lib.sh
source "$REPO_ROOT/tools/qemu/lib.sh"

MODE="${1:-verify}"
case "$MODE" in
    verify | install) ;;
    *)
        say "usage: tools/qemu-env.sh [verify|install]"
        exit 2
        ;;
esac

# fetch <url> <sha256> <output>: download, check, or stop with nothing left behind.
fetch() {
    local url="$1" want="$2" out="$3" found
    mkdir -p "$(dirname "$out")"
    if [ -f "$out" ] && [ "$(sha256sum "$out" | cut -d' ' -f1)" = "$want" ]; then
        return 0
    fi
    say "chorus: fetching $url"
    if ! curl --fail --silent --show-error --location --max-time 600 --output "$out.fetching" "$url"; then
        rm -f "$out.fetching"
        say "FAIL $url could not be fetched"
        exit 1
    fi
    found="$(sha256sum "$out.fetching" | cut -d' ' -f1)"
    if [ "$found" != "$want" ]; then
        rm -f "$out.fetching"
        say "FAIL $url has sha256 $found, pinned $want; nothing was installed from it"
        exit 1
    fi
    mv "$out.fetching" "$out"
}

install_emulator() {
    local home version archive
    home="$(qemu_home)"
    version="$(qemu_pin qemu_version)"
    if [ -e "$home/$version" ]; then
        return 0
    fi
    archive="$home/dist/$(basename "$(qemu_pin qemu_asset_url)")"
    fetch "$(qemu_pin qemu_asset_url)" "$(qemu_pin qemu_asset_sha256)" "$archive"
    say "chorus: extracting the emulator into $home/$version"
    rm -rf "$home/$version.extracting"
    mkdir -p "$home/$version.extracting"
    tar -xf "$archive" -C "$home/$version.extracting"
    mv "$home/$version.extracting" "$home/$version"
}

install_libs() {
    local home libs mamba
    home="$(qemu_home)"
    libs="$(qemu_libs)"
    if [ -e "$libs" ]; then
        return 0
    fi
    mamba="$home/micromamba-$(qemu_pin micromamba_version)/micromamba"
    fetch "$(qemu_pin micromamba_url)" "$(qemu_pin micromamba_sha256)" "$mamba"
    chmod +x "$mamba"
    say "chorus: installing the emulator's libraries into $libs"
    # The lock's comment lines are for people; micromamba is given the list.
    command grep -E '^(@EXPLICIT|https://)' "$QEMU_LIBS_LOCK" > "$home/libs.explicit.list"
    "$mamba" create --yes --quiet --root-prefix "$home/mamba-root" --prefix "$libs" \
        --file "$home/libs.explicit.list"
}

if [ "$MODE" = install ]; then
    install_emulator
    install_libs
fi

say "chorus: the emulator"
say "  pinned:    QEMU $(qemu_pin qemu_version) (GPL-2.0-only, run as an unmodified binary; its source is never read)"
say "  program:   $(qemu_program)"
say "  libraries: $(qemu_libs) ($(command grep -c '^https://' "$QEMU_LIBS_LOCK") conda-forge packages, tools/qemu/libs.explicit.txt)"
if why="$(qemu_verify)"; then
    say "  verified:  the program's sha256, every library package's sha256, $(qemu_run --version | head -n 1)"
    exit 0
fi
say "  NOT VERIFIED: $why"
if [ "$MODE" = verify ]; then
    say "  install what is absent with: bash tools/qemu-env.sh install"
    say "  (something present that is not the pinned one is never replaced: remove it, then install)"
fi
exit 1
