#!/usr/bin/env bash
# The per-test limit of the workspace tests (.config/nextest.toml,
# docs/decisions/0000-a-test-past-its-limit-is-ended.md): a test past its slow-timeout period is
# ended and fails, named. This runs a throwaway crate whose one test sleeps 5 s under a copy of
# the repository's configuration with the period at 1 s, and reads the configuration itself.
# Builds nothing of the workspace; the throwaway crate goes where tools/build-dir.sh says.
#
#   bash tools/nextest-timeout-test.sh
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
CONFIG=.config/nextest.toml
# shellcheck source=tools/build-dir.sh
. tools/build-dir.sh

NEXTEST_VERSION="$(sed -n 's|^"aqua:nextest-rs/nextest/cargo-nextest" = "\(.*\)"$|\1|p' mise.toml)"
[ -n "$NEXTEST_VERSION" ] || { echo "mise.toml pins no aqua:nextest-rs/nextest/cargo-nextest" >&2; exit 2; }

fails=0
pass() { echo "ok   $1"; }
fail() {
    echo "FAIL $1: $2" >&2
    fails=$((fails + 1))
}

work="$(mktemp -d "${TMPDIR:-/tmp}/nextest-timeout.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# default_slow_timeout_ok <file>: whether the file's profile.default.slow-timeout is exactly
# { period = "60s", terminate-after = 1 }.
default_slow_timeout_ok() {
    python3 - "$1" << 'EOF'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    st = tomllib.load(f).get("profile", {}).get("default", {}).get("slow-timeout")
if st != {"period": "60s", "terminate-after": 1}:
    print(f"profile.default.slow-timeout is {st!r}", file=sys.stderr)
    sys.exit(1)
EOF
}

# 1. A test past the period is ended: nextest exits non-zero and reports it TIMEOUT by name.
test_nextest_ends_a_sleeping_test_naming_it() {
    local crate="$work/sleeper" out rc=0
    mkdir -p "$crate/src" "$crate/.config"
    cat > "$crate/Cargo.toml" << 'EOF'
[package]
name = "nextest-timeout-sleeper"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]
EOF
    cat > "$crate/src/lib.rs" << 'EOF'
#[test]
fn a_test_that_sleeps_five_seconds() {
    std::thread::sleep(std::time::Duration::from_secs(5));
}
EOF
    # The repository's configuration, the default period at 1 s, up to its overrides (their
    # filters name the workspace's packages, which nextest refuses in another workspace).
    awk '/^\[\[profile\.default\.overrides\]\]$/ { exit } { print }' "$CONFIG" |
        sed 's|^slow-timeout = { period = "60s", terminate-after = 1 }$|slow-timeout = { period = "1s", terminate-after = 1 }|' \
            > "$crate/.config/nextest.toml"
    if ! grep -qx 'slow-timeout = { period = "1s", terminate-after = 1 }' "$crate/.config/nextest.toml"; then
        fail "${FUNCNAME[0]}" "$CONFIG has no default slow-timeout line to shorten"
        return
    fi
    # nextest reads the configuration under the throwaway crate's own workspace root.
    out="$(MISE_TRUSTED_CONFIG_PATHS="$ROOT" CARGO_TARGET_DIR="$(throwaway_build_dir nextest-timeout)" \
        mise exec "aqua:nextest-rs/nextest/cargo-nextest@$NEXTEST_VERSION" -- \
        cargo nextest run --manifest-path "$crate/Cargo.toml" 2>&1)" || rc=$?
    if [ "$rc" -eq 0 ]; then
        fail "${FUNCNAME[0]}" "cargo nextest run exited 0 with a 5 s test under a 1 s limit: $out"
    elif ! grep -qE 'TIMEOUT.*a_test_that_sleeps_five_seconds' <<< "$out"; then
        fail "${FUNCNAME[0]}" "cargo nextest run exited $rc without reporting the test TIMEOUT by name: $out"
    else
        pass "${FUNCNAME[0]}"
    fi
}

# 2. The default profile ends a test at 60 s, and a copy without terminate-after is refused.
test_default_profile_enforces_60s() {
    local copy="$work/no-terminate.toml"
    sed 's|^slow-timeout = { period = "60s", terminate-after = 1 }$|slow-timeout = { period = "60s" }|' \
        "$CONFIG" > "$copy"
    if ! default_slow_timeout_ok "$CONFIG"; then
        fail "${FUNCNAME[0]}" "$CONFIG does not end a test at 60 s"
    elif default_slow_timeout_ok "$copy" 2> /dev/null; then
        fail "${FUNCNAME[0]}" "a copy of $CONFIG without terminate-after was accepted"
    else
        pass "${FUNCNAME[0]}"
    fi
}

# 3. Every per-test override ends its test too, later than the default, and sets nothing else,
# so the test-group overrides keep applying. (That the measured tests pass inside their limits is
# the full gate's own test step, `make gate-full`.)
test_full_gate_passes_with_overrides() {
    if python3 - "$CONFIG" << 'EOF'
import re, sys, tomllib
with open(sys.argv[1], "rb") as f:
    overrides = tomllib.load(f)["profile"]["default"].get("overrides", [])
bad = 0
n = 0
for o in overrides:
    st = o.get("slow-timeout")
    if st is None:
        continue
    n += 1
    m = re.fullmatch(r"(\d+)s", str(st.get("period", "")))
    if set(o) != {"filter", "slow-timeout"} or st.get("terminate-after") != 1 or not m or int(m[1]) <= 60:
        print(f"override {o.get('filter')!r}: {o!r}", file=sys.stderr)
        bad = 1
if n == 0:
    print("no per-test slow-timeout override", file=sys.stderr)
    bad = 1
sys.exit(bad)
EOF
    then
        pass "${FUNCNAME[0]}"
    else
        fail "${FUNCNAME[0]}" "a slow-timeout override in $CONFIG sets more than its own terminating limit over 60 s"
    fi
}

test_nextest_ends_a_sleeping_test_naming_it
test_default_profile_enforces_60s
test_full_gate_passes_with_overrides

if [ "$fails" -ne 0 ]; then
    echo "nextest timeout: $fails failed" >&2
    exit 1
fi
