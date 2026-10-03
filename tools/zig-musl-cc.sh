#!/usr/bin/env bash
# zig cc as the C compiler of the x86_64 musl image build (tools/image.sh sets
# CC_x86_64_unknown_linux_musl to this file, which is how the `cc` crate finds a
# compiler for that target). It compiles C only: the link stays rustc's own, over
# its self-contained musl, so the server remains a static PIE
# (docs/decisions/0122-the-server-decoders.md).
#
# The `cc` crate passes rustc's triple as --target=x86_64-unknown-linux-musl, a
# spelling zig does not take; it is dropped, and zig's own goes in its place.
# zig 0.16.0 ships the headers of musl 1.2.5, the musl rustc 1.98.1 links.
#
# CHORUS_ZIG_VERSION is the `core:zig` pin of mise.toml, exported by the caller.
set -euo pipefail
args=()
for a in "$@"; do
    case "$a" in
        --target=*) ;;
        *) args+=("$a") ;;
    esac
done
exec mise exec "core:zig@${CHORUS_ZIG_VERSION:?set by tools/image.sh from mise.toml}" -- \
    zig cc -target x86_64-linux-musl "${args[@]}"
