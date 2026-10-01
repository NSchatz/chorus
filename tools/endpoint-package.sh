#!/usr/bin/env bash
# `make endpoint-packages`: the Linux endpoint package, one .deb per
# architecture, built rootless, and its checks.
#
#   tools/endpoint-package.sh              # arm64 and amd64
#   tools/endpoint-package.sh amd64        # one architecture
#
# For each architecture:
#   1. cross-build chorus-client, chorus-wakeup-probe, and the two probes the
#      host contract and the spin test run (chorus-server, chorus-rt-spin) for
#      <triple>.<glibc floor> with cargo-zigbuild and zig (pinned in mise.toml,
#      digests in mise.lock; the choice and the floor: the package's ADR);
#   2. check every binary with readelf: its architecture, its interpreter, the
#      shared libraries it names (libasound is NOT one: it is dlopen()ed at run
#      time, ADR 0008) and its highest GLIBC_ symbol version, at or under the
#      floor;
#   3. stage the tree (deploy/endpoint/), write DEBIAN/control, conffiles and
#      md5sums, fix every mtime at the commit's time, and build with
#      `dpkg-deb --root-owner-group --build` (no root, no fakeroot);
#   4. check the package: dpkg-deb --info and --contents against the expected
#      file list, the conffile, a second build byte-identical to the first,
#      `systemd-analyze verify` of the unit as installed in the unpacked tree,
#      and, where this machine runs the architecture, the unpacked binaries
#      themselves (the client opening the ALSA `null` device through the
#      libasound it dlopen()s, and chorus-verify-host reaching its checks).
#
# Output: CHORUS_PACKAGE_OUT (default target/endpoint-packages/), one
# chorus-endpoint_<version>_<arch>.deb per architecture. Installs nothing,
# publishes nothing, touches no device.

set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
# shellcheck source=tools/lib.sh
. tools/lib.sh

# The glibc floor: Debian 12 (bookworm) and Raspberry Pi OS Legacy ship 2.36,
# Debian 13 (trixie) and Raspberry Pi OS 2.41 (packages.debian.org/{bookworm,trixie}/libc6
# and archive.raspberrypi.com's bookworm and trixie arm64 Packages, read 2026-09-30).
GLIBC_FLOOR=2.36
PACKAGE=chorus-endpoint
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
OUT="${CHORUS_PACKAGE_OUT:-$TD/endpoint-packages}"
WORK="$TD/endpoint-packages-work"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
EPOCH="$(git log -1 --format=%ct)"
export SOURCE_DATE_EPOCH="$EPOCH"
# The zig compiler's cache (glibc stubs, libunwind) beside the build, not in $HOME.
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$TD/zig-cache}"

pin() { sed -n "s|^\"$1\" = \"\\(.*\\)\"$|\\1|p" mise.toml; }
ZIG_VERSION="$(pin core:zig)"
ZIGBUILD_VERSION="$(pin aqua:rust-cross/cargo-zigbuild)"
[ -n "$ZIG_VERSION" ] && [ -n "$ZIGBUILD_VERSION" ] ||
    { echo "endpoint-package: mise.toml pins no core:zig or aqua:rust-cross/cargo-zigbuild"; exit 2; }
ZIGBUILD=(mise exec "core:zig@$ZIG_VERSION" "aqua:rust-cross/cargo-zigbuild@$ZIGBUILD_VERSION" -- cargo zigbuild)

# The binaries, where they go, and their mode.
BINS=(chorus-client chorus-wakeup-probe chorus-server chorus-rt-spin)
declare -A DEST=(
    [chorus-client]=usr/bin
    [chorus-wakeup-probe]=usr/bin
    [chorus-server]=usr/lib/chorus/probe
    [chorus-rt-spin]=usr/lib/chorus/probe
)

# Every path the package holds, in dpkg-deb --contents order; the check below
# holds the built package to exactly this.
expected_contents() {
    cat <<'EOF'
./
./etc/
./etc/chorus/
./etc/chorus/client.conf
./usr/
./usr/bin/
./usr/bin/chorus-client
./usr/bin/chorus-verify-host
./usr/bin/chorus-wakeup-probe
./usr/lib/
./usr/lib/chorus/
./usr/lib/chorus/probe/
./usr/lib/chorus/probe/chorus-rt-spin
./usr/lib/chorus/probe/chorus-server
./usr/lib/chorus/verify-host/
./usr/lib/chorus/verify-host/config/
./usr/lib/chorus/verify-host/config/verification.conf
./usr/lib/chorus/verify-host/tools/
./usr/lib/chorus/verify-host/tools/host-contract.sh
./usr/lib/chorus/verify-host/tools/lib.sh
./usr/lib/chorus/verify-host/tools/spin-test.sh
./usr/lib/systemd/
./usr/lib/systemd/system/
./usr/lib/systemd/system/chorus-client.service
./usr/lib/udev/
./usr/lib/udev/rules.d/
./usr/lib/udev/rules.d/70-chorus-leds.rules
./usr/share/
./usr/share/doc/
./usr/share/doc/chorus-endpoint/
./usr/share/doc/chorus-endpoint/README
./usr/share/doc/chorus-endpoint/copyright
./usr/share/doc/chorus-endpoint/examples/
./usr/share/doc/chorus-endpoint/examples/front-panel-rack-amp.conf
./usr/share/doc/chorus-endpoint/examples/front-panel.conf
EOF
}

triple_of() {
    case "$1" in
        arm64) echo aarch64-unknown-linux-gnu ;;
        amd64) echo x86_64-unknown-linux-gnu ;;
        *) echo "endpoint-package: no architecture '$1' (arm64 or amd64)" >&2; return 2 ;;
    esac
}

# What readelf -h calls each architecture's machine, and its dynamic loader
# (the glibc ABI's path for each: /lib/ld-linux-aarch64.so.1, /lib64/ld-linux-x86-64.so.2).
machine_of() { case "$1" in arm64) echo AArch64 ;; amd64) echo "Advanced Micro Devices X86-64" ;; esac; }
interp_of() { case "$1" in arm64) echo /lib/ld-linux-aarch64.so.1 ;; amd64) echo /lib64/ld-linux-x86-64.so.2 ;; esac; }

fail() { echo "endpoint-package: FAIL $*"; exit 1; }

build_binaries() {
    local triple="$1"
    echo "endpoint-package: building ${BINS[*]} for $triple.$GLIBC_FLOOR (zig $ZIG_VERSION, cargo-zigbuild $ZIGBUILD_VERSION)"
    "${ZIGBUILD[@]}" --release --locked --target "$triple.$GLIBC_FLOOR" \
        -p chorus-client-linux -p chorus-hostprobe -p chorus-server \
        --bin chorus-client --bin chorus-wakeup-probe --bin chorus-server --bin chorus-rt-spin
}

check_binary() {
    local arch="$1" bin="$2" machine interp needed highest
    machine="$(readelf -h "$bin" | sed -n 's/^ *Machine: *//p')"
    interp="$(readelf -l "$bin" | sed -n 's/.*Requesting program interpreter: \(.*\)]$/\1/p')"
    needed="$(readelf -d "$bin" | sed -n 's/.*(NEEDED).*\[\(.*\)\]$/\1/p' | sort | tr '\n' ' ')"
    highest="$(readelf -V "$bin" | command grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -n 1)"
    printf 'endpoint-package: %-22s %s, interpreter %s, needs %s, highest GLIBC_%s\n' \
        "$(basename "$bin")" "$machine" "$interp" "$needed" "$highest"
    [ "$machine" = "$(machine_of "$arch")" ] || fail "$bin is $machine, not $(machine_of "$arch")"
    [ "$interp" = "$(interp_of "$arch")" ] || fail "$bin asks for interpreter '$interp', not $(interp_of "$arch")"
    local lib
    for lib in $needed; do
        case "$lib" in
            # all three are libc6's (the x86_64 linker names the loader too)
            libc.so.6 | libm.so.6 | ld-linux-x86-64.so.2 | ld-linux-aarch64.so.1) ;;
            *) fail "$bin needs $lib; the package depends on libc6 alone (libasound is dlopen()ed)" ;;
        esac
    done
    [ -n "$highest" ] || fail "$bin has no GLIBC_ symbol versions: not a glibc binary"
    [ "$(printf '%s\n%s\n' "$highest" "$GLIBC_FLOOR" | sort -V | tail -n 1)" = "$GLIBC_FLOOR" ] ||
        fail "$bin needs GLIBC_$highest, above the floor $GLIBC_FLOOR"
}

write_copyright() {
    local triple="$1" dest="$2"
    {
        echo "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
        echo "Upstream-Name: chorus"
        echo "Comment: Built from commit $(git rev-parse HEAD) by tools/endpoint-package.sh."
        echo " The Rust crates statically linked into the binaries, as Cargo.lock resolves"
        echo " them for $triple (name version: licence), follow. The MPL-2.0 crates' source"
        echo " is at https://crates.io/crates/<name>/<version> and is attached to every"
        echo " chorus release that carries this package (docs/release.md)."
        cargo tree --locked -e normal -p chorus-client-linux -p chorus-hostprobe -p chorus-server \
            --prefix none --format '{p}|{l}' --target "$triple" |
            sed -e 's/ (\*)//' -e 's/ (proc-macro)//' -e 's/ ([^)]*)|/|/' | LC_ALL=C sort -u |
            awk -F'|' '{printf " %s: %s\n", $1, $2}'
        echo
        echo "Files: *"
        echo "Copyright: the chorus authors"
        echo "License: MIT or Apache-2.0"
        echo
        echo "Files: usr/bin/chorus-client"
        echo "Copyright: the chorus authors; libopus: see its licence below"
        echo "License: (MIT or Apache-2.0) and BSD-3-Clause"
        echo "Comment: chorus-client statically links the libopus 1.6.1 decoder (third_party/opus)."
        echo
        echo "License: BSD-3-Clause"
        sed -e 's/^$/./' -e 's/^/ /' third_party/opus/COPYING
        echo
        echo "License: MIT"
        sed -e 's/^$/./' -e 's/^/ /' LICENSE-MIT
        echo
        echo "License: Apache-2.0"
        sed -e 's/^$/./' -e 's/^/ /' LICENSE-APACHE
    } > "$dest"
}

stage_tree() {
    local arch="$1" triple="$2" stage="$3" b
    rm -rf "$stage"
    mkdir -p "$stage/DEBIAN" "$stage/etc/chorus" "$stage/usr/bin" "$stage/usr/lib/chorus/probe" \
        "$stage/usr/lib/chorus/verify-host/tools" "$stage/usr/lib/chorus/verify-host/config" \
        "$stage/usr/lib/systemd/system" "$stage/usr/lib/udev/rules.d" "$stage/usr/share/doc/$PACKAGE/examples"
    for b in "${BINS[@]}"; do
        install -m 0755 "$TD/$triple/release/$b" "$stage/${DEST[$b]}/$b"
    done
    install -m 0755 deploy/endpoint/chorus-verify-host.sh "$stage/usr/bin/chorus-verify-host"
    install -m 0644 tools/lib.sh tools/host-contract.sh tools/spin-test.sh "$stage/usr/lib/chorus/verify-host/tools/"
    install -m 0644 config/verification.conf "$stage/usr/lib/chorus/verify-host/config/"
    install -m 0644 deploy/endpoint/chorus-client.service "$stage/usr/lib/systemd/system/"
    install -m 0644 deploy/endpoint/client.conf "$stage/etc/chorus/client.conf"
    install -m 0644 deploy/endpoint/README "$stage/usr/share/doc/$PACKAGE/README"
    install -m 0644 deploy/endpoint/70-chorus-leds.rules "$stage/usr/lib/udev/rules.d/"
    install -m 0644 deploy/endpoint/front-panel.conf "$stage/usr/share/doc/$PACKAGE/examples/front-panel.conf"
    install -m 0644 config/front-panel/rack-amp.conf "$stage/usr/share/doc/$PACKAGE/examples/front-panel-rack-amp.conf"
    write_copyright "$triple" "$stage/usr/share/doc/$PACKAGE/copyright"
    chmod 0644 "$stage/usr/share/doc/$PACKAGE/copyright"

    local size
    size="$(du -sk --exclude=DEBIAN "$stage" | cut -f1)"
    cat > "$stage/DEBIAN/control" <<EOF
Package: $PACKAGE
Version: $VERSION
Architecture: $arch
Maintainer: chorus <chorus@example.invalid>
Installed-Size: $size
Depends: libc6 (>= $GLIBC_FLOOR), libasound2t64 | libasound2
Recommends: python3
Section: sound
Priority: optional
Description: chorus multiroom audio endpoint (chorus-client and its systemd unit)
 chorus-client receives a chorus server's stream over protocol v2 and plays it
 through ALSA in sync with the other endpoints of its group. The package runs it
 as chorus-client.service (a dynamic unprivileged user in group audio, with
 real-time limits) configured by /etc/chorus/client.conf, and carries
 chorus-verify-host, the host contract and spin test of the repository's
 make verify-host, with the probes they run.
EOF
    echo "/etc/chorus/client.conf" > "$stage/DEBIAN/conffiles"
    (cd "$stage" && command find . -path ./DEBIAN -prune -o -type f -printf '%P\n' | LC_ALL=C sort |
        xargs md5sum > DEBIAN/md5sums)
    chmod 0644 "$stage/DEBIAN/control" "$stage/DEBIAN/conffiles" "$stage/DEBIAN/md5sums"
    command find "$stage" -type d -exec chmod 0755 {} +
    command find "$stage" -exec touch --no-dereference --date="@$EPOCH" {} +
}

build_deb() {
    local stage="$1" deb="$2"
    dpkg-deb --root-owner-group -Zxz --build "$stage" "$deb" > /dev/null
}

check_deb() {
    local arch="$1" deb="$2" unpacked="$3"
    echo "endpoint-package: dpkg-deb --info $(basename "$deb")"
    dpkg-deb --info "$deb" | sed 's/^/  /'
    [ "$(dpkg-deb --field "$deb" Architecture)" = "$arch" ] || fail "$deb is not Architecture: $arch"
    dpkg-deb --field "$deb" Depends | command grep -q 'libasound2t64 | libasound2' || fail "$deb does not depend on libasound"
    [ "$(dpkg-deb --ctrl-tarfile "$deb" | tar -xO ./conffiles)" = /etc/chorus/client.conf ] ||
        fail "$deb does not mark /etc/chorus/client.conf as a conffile"

    local contents
    contents="$(dpkg-deb --contents "$deb")"
    printf '%s\n' "$contents" | awk '{print "  " $0}'
    diff <(echo "$contents" | awk '{print $NF}') <(expected_contents) ||
        fail "$deb holds a different file list from expected_contents (diff above)"
    echo "$contents" | awk '$2 != "root/root" {bad = 1} END {exit bad}' || fail "a file in $deb is not root/root"

    rm -rf "$unpacked"
    dpkg-deb --extract "$deb" "$unpacked"
    # systemd-analyze verify with --root= resolves the unit, its ExecStart=
    # binary and its paths inside a tree, with no root and no installation.
    # The tree is the unpacked package plus this machine's own targets and
    # slices (the unit's default dependencies, sysinit.target and the rest,
    # and the targets it orders after), copied beside it, never into the package.
    local vroot="$unpacked-verify-root" unit=usr/lib/systemd/system/chorus-client.service verify
    rm -rf "$vroot"
    cp -a "$unpacked" "$vroot"
    cp -a /usr/lib/systemd/system/*.target /usr/lib/systemd/system/*.slice "$vroot/usr/lib/systemd/system/"
    echo "endpoint-package: systemd-analyze verify --root=<unpacked package + host targets> $unit"
    if ! verify="$(systemd-analyze verify --man=no --root="$vroot" "$vroot/$unit" 2>&1)"; then
        echo "$verify"
        fail "systemd-analyze verify refused the unit"
    fi
    # verify exits 0 over an unknown key or a bad value, and says so: anything
    # it prints is a finding.
    [ -z "$verify" ] || { echo "$verify"; fail "systemd-analyze verify printed findings for the unit"; }
    echo "  systemd-analyze verify: exit 0, no findings ($(systemd-analyze --version | head -n 1))"
    # The check's own teeth: the same unit with one misspelt directive must
    # draw a finding, or an empty output above proves nothing.
    sed 's/^LimitRTTIME=/LimitRTTYME=/' "$vroot/$unit" > "$vroot/usr/lib/systemd/system/chorus-selftest.service"
    systemd-analyze verify --man=no --root="$vroot" "$vroot/usr/lib/systemd/system/chorus-selftest.service" 2>&1 |
        command grep -q "Unknown key 'LimitRTTYME'" || fail "systemd-analyze verify did not flag a misspelt directive"
    echo "  self-test: a misspelt directive in a copy of the unit is flagged"
    # The front-panel drop-in, installed where the owner installs it, verified with the unit.
    install -D -m 0644 "$unpacked/usr/share/doc/$PACKAGE/examples/front-panel.conf" \
        "$vroot/etc/systemd/system/chorus-client.service.d/front-panel.conf"
    if ! verify="$(systemd-analyze verify --man=no --root="$vroot" "$vroot/$unit" 2>&1)" || [ -n "$verify" ]; then
        echo "$verify"
        fail "systemd-analyze verify refused the unit with the front-panel drop-in"
    fi
    echo "  systemd-analyze verify with the front-panel drop-in: exit 0, no findings"
    # udevadm is not installed here, so the LED rule is held to its shape: every
    # rule matches only `chorus-*` LEDs on add and runs only chgrp audio or chmod g+w.
    local rules="$unpacked/usr/lib/udev/rules.d/70-chorus-leds.rules" n
    n="$(command grep -c -v -E '^(#|$)' "$rules")"
    [ "$n" -ge 1 ] || fail "70-chorus-leds.rules holds no rule"
    command grep -v -E '^(#|$)' "$rules" | while IFS= read -r rule; do
        case "$rule" in
            'ACTION=="add", SUBSYSTEM=="leds", KERNEL=="chorus-*", '*) ;;
            *) fail "a rule in 70-chorus-leds.rules is not scoped to chorus-* LEDs on add: $rule" ;;
        esac
        if printf '%s\n' "$rule" | command grep -o 'RUN+="[^"]*"' |
            command grep -v -q -E '^RUN\+="/bin/(chgrp audio|chmod g\+w) /sys%p/(brightness|multi_intensity)"$'; then
            fail "a rule in 70-chorus-leds.rules runs something other than chgrp audio or chmod g+w: $rule"
        fi
    done
    echo "  70-chorus-leds.rules: $n rules, all scoped to chorus-* LEDs, chgrp audio and chmod g+w only"
    echo "endpoint-package: systemd-analyze security --offline=true (exposure, informational)"
    systemd-analyze security --offline=true --root="$vroot" --no-pager "$vroot/$unit" 2>&1 |
        tail -n 1 | tr -cd '[:print:]' | sed 's/^/  /'
    echo
}

# Run the unpacked binaries where this machine can.
run_unpacked() {
    local arch="$1" unpacked="$2" host
    host="$(uname -m)"
    if ! { [ "$arch" = amd64 ] && [ "$host" = x86_64 ]; } && ! { [ "$arch" = arm64 ] && [ "$host" = aarch64 ]; }; then
        echo "endpoint-package: $arch binaries not run: this machine is $host (readelf checked them above)"
        return 0
    fi
    local out
    if use_rootless_alsa; then
        out="$("$unpacked/usr/bin/chorus-client" --device null --probe-device 2>&1)" ||
            { echo "$out"; fail "the packaged chorus-client could not open the ALSA null device"; }
        echo "  $out"
        echo "$out" | command grep -q 'device-probe device=null usable=1' || fail "the probe did not report the null device usable"
    elif [ "${CI:-}" = true ]; then
        # As the gate's alsa-null step: under CI there is no /cache to hold a rootless alsa-lib.
        echo "  SKIPPED: the null-device probe; no libasound.so.2 and no rootless alsa-lib at $(alsa_prefix) under CI"
    else
        fail "no libasound.so.2 on this machine for the client's dlopen: $ALSA_INSTALL_HINT"
    fi
    out="$("$unpacked/usr/bin/chorus-client" --rt-priority 0 2>&1)" && fail "--rt-priority 0 was accepted"
    echo "$out" | command grep -q 'rt-priority is zero' || { echo "$out"; fail "--rt-priority 0 was not refused by name"; }

    # chorus-verify-host against the unpacked tree: with no granted ceiling it
    # must refuse by name (exit 3), and with one it runs both checks.
    local rc=0
    out="$(CHORUS_ENDPOINT_ROOT="$unpacked" "$unpacked/usr/bin/chorus-verify-host" 2>&1)" || rc=$?
    echo "$out" | tail -n 4 | sed 's/^/  /'
    if [ "$(ulimit -r)" = 0 ]; then
        if [ "$rc" != 3 ] || ! echo "$out" | command grep -q 'MISSING PREREQUISITE'; then
            fail "chorus-verify-host did not refuse by name with no rtprio ceiling (exit $rc)"
        fi
    fi
    echo "endpoint-package: chorus-verify-host exit $rc (ulimit -r $(ulimit -r))"
}

ARCHES=("$@")
[ "${#ARCHES[@]}" -gt 0 ] || ARCHES=(arm64 amd64)
mkdir -p "$OUT" "$WORK"
for arch in "${ARCHES[@]}"; do
    triple="$(triple_of "$arch")"
    build_binaries "$triple"
    for b in "${BINS[@]}"; do
        check_binary "$arch" "$TD/$triple/release/$b"
    done
    deb="$OUT/${PACKAGE}_${VERSION}_$arch.deb"
    stage_tree "$arch" "$triple" "$WORK/stage-$arch"
    build_deb "$WORK/stage-$arch" "$deb"
    # A second build from a fresh stage has to be byte-identical.
    stage_tree "$arch" "$triple" "$WORK/stage-$arch-again"
    build_deb "$WORK/stage-$arch-again" "$WORK/again.deb"
    cmp -s "$deb" "$WORK/again.deb" || fail "two builds of $(basename "$deb") differ"
    echo "endpoint-package: two builds of $(basename "$deb") are byte-identical"
    check_deb "$arch" "$deb" "$WORK/unpacked-$arch"
    run_unpacked "$arch" "$WORK/unpacked-$arch"
done
(cd "$OUT" && ls -l ./*.deb && sha256sum ./*.deb)
echo "endpoint-package: PASS ${ARCHES[*]}"
