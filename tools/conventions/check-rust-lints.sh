#!/usr/bin/env bash
# Rule "Rust lints and unsafe": the lint set lives once in the root Cargo.toml and every crate
# opts in; unsafe code is denied except in the places listed here, each with its reason (adding
# one needs an ADR). clippy -D warnings (gate step `clippy`) enforces the lints themselves.
. "$(dirname "$0")/lib.sh"
# file | count of allow(unsafe_code) or expect(unsafe_code) attributes, alone or combined | why
allowed=(
    "crates/alsa/src/lib.rs|1|the crate is the dlopen binding to libasound; every function crosses FFI"
    "crates/hostctl/src/lib.rs|6|libc wrappers for getrlimit, setrlimit, sched_*, mlockall and gettid, one function each"
    "crates/server/tests/regress_0031_f6.rs|1|a test sets SCHED_BATCH through raw libc to show a spawned thread inherits it"
    "crates/hostprobe/src/sys.rs|1|the wakeup probe's libc FFI: clock_gettime, clock_nanosleep and prctl timer slack (docs/decisions/0047-host-probes.md)"
    "crates/hostprobe/src/net.rs|1|the receive-stamp check's socket FFI: setsockopt, recvmsg and a ping socket (docs/decisions/0047-host-probes.md)"
    "crates/opus-sys/src/lib.rs|1|the crate is the FFI binding to the vendored libopus (docs/decisions/0044-the-vendored-decoders.md)"
)
rc=0
bad() { fail "Rust lints and unsafe" "$1"; rc=1; }
lints="$(awk '/^\[/ { on = ($0 ~ /^\[workspace\.lints\./) } on' Cargo.toml)"
for need_line in 'unsafe_code = "deny"' 'undocumented_unsafe_blocks = "warn"' 'dbg_macro = "warn"' 'todo = "warn"'; do
    [[ "$lints" == *"$need_line"* ]] || bad "the root Cargo.toml's [workspace.lints] lacks $need_line"
done
for m in $(sed -n '/^members = \[/,/^\]/p' Cargo.toml | command grep -o '"crates/[^"]*"' | tr -d '"'); do
    awk '/^\[lints\]/ { on = 1; next } /^\[/ { on = 0 } on && /^workspace = true/ { found = 1 } END { exit !found }' "$m/Cargo.toml" ||
        bad "$m/Cargo.toml does not opt in with [lints] workspace = true"
done
# Every attribute that lets unsafe code through, per file: `allow` or `expect`, inner or outer,
# alone or beside other lints (`allow(dead_code, unsafe_code)`), with or without a `reason`, and
# spread over several lines as rustfmt writes a long list. Prints `file:count` for each file with
# at least one.
# shellcheck disable=SC2016 # a perl program: its $ variables are perl's, not the shell's
count_pl='my $n = () = /\b(?:allow|expect)\s*\([^)]*?\b(?:unsafe_code|unsafe_op_in_unsafe_fn)\b/g;'
unsafe_allows() {
    # shellcheck disable=SC2016 # perl's $ARGV and $n
    git ls-files -z '*.rs' | xargs -0 perl -0777 -ne "$count_pl"' print "$ARGV:$n\n" if $n;'
}
# Self-test: each form below is one attribute the count must see; forbid and deny are not.
selftest="$(printf '%s\n' '#![allow(unsafe_code)]' '#[expect(unsafe_code)]' '#[allow(dead_code, unsafe_code)]' \
    '#[expect(clippy::x, unsafe_code, reason = "ffi")]' '#[allow(' '    dead_code,' '    unsafe_op_in_unsafe_fn' ')]' \
    '#[forbid(unsafe_code)]' '#[deny(unsafe_code)]' | perl -0777 -ne "$count_pl"' print $n;')"
[ "$selftest" = 5 ] || bad "self-test: the unsafe-allow count saw $selftest of the 5 forms"
declare -A approved
for a in "${allowed[@]}"; do IFS='|' read -r f c _ <<< "$a"; approved[$f]=$c; done
while IFS= read -r hit; do
    f="${hit%%:*}"
    c="${hit##*:}"
    [ "${approved[$f]:-0}" = "$c" ] || bad "$f allows or expects unsafe code $c time(s); the approved count is ${approved[$f]:-0}"
    unset "approved[$f]"
done < <(unsafe_allows)
for f in "${!approved[@]}"; do bad "$f is approved for unsafe code but allows none; remove it from the list"; done
[ "$rc" = 0 ] && echo "rust lints: workspace set present, every crate opts in, unsafe allowed only in ${#allowed[@]} approved places"
exit "$rc"
