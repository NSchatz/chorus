#!/usr/bin/env bash
# Rule "Rust lints and unsafe": the lint set lives once in the root Cargo.toml and every crate
# opts in; unsafe code is denied except in the places listed here, each with its reason (adding
# one needs an ADR). clippy -D warnings (gate step `clippy`) enforces the lints themselves.
. "$(dirname "$0")/lib.sh"
# file | count of allow(unsafe_code) attributes | why
allowed=(
    "crates/alsa/src/lib.rs|1|the crate is the dlopen binding to libasound; every function crosses FFI"
    "crates/hostctl/src/lib.rs|6|libc wrappers for getrlimit, setrlimit, sched_*, mlockall and gettid, one function each"
    "crates/server/tests/regress_0031_f6.rs|1|a test sets SCHED_BATCH through raw libc to show a spawned thread inherits it"
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
declare -A approved
for a in "${allowed[@]}"; do IFS='|' read -r f c _ <<< "$a"; approved[$f]=$c; done
while IFS= read -r hit; do
    f="${hit%%:*}"
    c="${hit##*:}"
    [ "${approved[$f]:-0}" = "$c" ] || bad "$f allows unsafe_code $c time(s); the approved count is ${approved[$f]:-0}"
    unset "approved[$f]"
done < <(git grep -c -E 'allow\((unsafe_code|unsafe_op_in_unsafe_fn)\)' -- '*.rs')
for f in "${!approved[@]}"; do bad "$f is approved for unsafe code but allows none; remove it from the list"; done
[ "$rc" = 0 ] && echo "rust lints: workspace set present, every crate opts in, unsafe allowed only in ${#allowed[@]} approved places"
exit "$rc"
