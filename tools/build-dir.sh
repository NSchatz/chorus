#!/usr/bin/env bash
# Where a throwaway build keeps its cargo target directory (request #230 from the goals
# program, 2026-10-03: the development container's disk is its bottleneck, so builds whose
# output nobody reads again do not write it there).
#
#   throwaway_build_dir <name>
#
# prints, in this order: CHORUS_BUILD_DIR/<name> when the caller set that variable;
# /scratch/chorus-build/<name> when /scratch is a tmpfs with at least
# CHORUS_BUILD_TMPFS_MIN_KIB free (default 6 GiB: a musl release build of the server with
# the Dockerfile's build context checked beside it took under 3 GiB here on 2026-10-03, and
# /scratch is counted against the container's memory); else
# /cache/wt/chorus/target/shared/<name>, the one directory every worktree of the lane shares.
# Outside the development container neither exists, and it is <repository>/target/<name>.
# CARGO_TARGET_DIR is deliberately not read: a private directory handed in that way is what
# the request is about.
throwaway_build_dir() {
    local name="$1" min="${CHORUS_BUILD_TMPFS_MIN_KIB:-6291456}" free root
    if [ -n "${CHORUS_BUILD_DIR:-}" ]; then
        printf '%s/%s\n' "$CHORUS_BUILD_DIR" "$name"
        return 0
    fi
    if [ -d /scratch ] && [ -w /scratch ] && [ "$(stat -f -c %T /scratch 2>/dev/null)" = tmpfs ]; then
        free="$(df -k --output=avail /scratch 2>/dev/null | tail -n 1 | tr -d ' ')"
        if [ -n "$free" ] && [ "$free" -ge "$min" ]; then
            printf '/scratch/chorus-build/%s\n' "$name"
            return 0
        fi
    fi
    if [ -d /cache/wt/chorus/target ] && [ -w /cache/wt/chorus/target ]; then
        printf '/cache/wt/chorus/target/shared/%s\n' "$name"
        return 0
    fi
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    printf '%s/target/%s\n' "$root" "$name"
}
