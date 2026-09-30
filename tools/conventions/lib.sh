# Shared by the tools/conventions/check-*.sh scripts: every one runs from the repository root
# with the pinned tools of mise.toml on PATH, and fails naming its rule in docs/conventions.md.
# shellcheck shell=bash

cd "$(git rev-parse --show-toplevel)" || exit 2

# The pinned tools (mise.toml). CI's mise action puts them on PATH itself; here mise is asked
# for the environment of this checkout's mise.toml (trusted for this call only, never globally).
if command -v mise > /dev/null 2>&1 && [ -z "${CHORUS_CONVENTIONS_TOOLS:-}" ]; then
    eval "$(MISE_TRUSTED_CONFIG_PATHS="$PWD" mise env -s bash 2> /dev/null)"
    export CHORUS_CONVENTIONS_TOOLS=1
fi

# The commit the commit-message rule starts at: origin/main when goal 3 began (the history
# before it keeps the subjects it was written with; history is never rewritten).
CONVENTIONS_BASE=d9a9171a7c48275700f4f5c8fab86a9c2c76601a

# fail <rule> <message>: print why and where the rule lives, return non-zero.
fail() {
    echo "FAIL: $2"
    echo "rule: docs/conventions.md, \"$1\""
    return 1
}

# need <tool>: refuse by name when a pinned tool is missing.
need() {
    command -v "$1" > /dev/null 2>&1 && return 0
    echo "FAIL: $1 is not on PATH; install the pinned tools rootless with \`mise install\` (mise.toml)"
    return 1
}
