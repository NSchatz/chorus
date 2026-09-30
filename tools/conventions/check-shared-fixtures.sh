#!/usr/bin/env bash
# Rule "Shared fixtures" (BRIEF.md section 7, brief section 4.1): protocol, sync and DSP
# behaviour is specified by files that both implementations read, so the Rust and C cores cannot
# drift apart. Each shared directory below is read by a Rust test and by a C test (both walk the
# directory), and every file in it has an extension those tests read: a file of any other kind
# would be read by neither. fixtures/control, fixtures/discovery and fixtures/measure are
# Rust-only by declaration (the endpoint does not speak them yet; goal 6 moves control here).
# fixtures/protocol/v2 is read by firmware/tests/test_protocol_v2.c (every vector, both
# assertions) and its noise/ directory by firmware/tests/test_noise.c (the published vector in
# both roles), since the endpoint moved to protocol v2 (goal 6). fixtures/controls is the
# controller role's and the status LED's (goal 9): the C controls must produce its .hex bytes and
# the server decodes and applies them (crates/server/tests/controller_role.rs).
. "$(dirname "$0")/lib.sh"
# directory | extensions both sides read | where Rust reads it | where C reads it
shared=(
    "fixtures/protocol|hex fields|crates/protocol/tests|firmware/tests/test_protocol.c"
    "fixtures/sync|cfg|crates/sync/tests crates/sync/src/crosscheck.rs|firmware/tests/test_sync.c"
    "fixtures/sync/crosscheck|expected|crates/sync/tests crates/sync/src/crosscheck.rs|firmware/tests/test_sync.c"
    "fixtures/dsp|*|crates/dsp/tests|firmware/tests/test_dsp.c"
    "fixtures/protocol/v2|hex fields|crates/protocol/tests|firmware/tests/test_protocol_v2.c"
    "fixtures/protocol/v2/noise|fields|crates/protocol/tests|firmware/tests/test_noise.c"
    "fixtures/codec|fields chunks pcm|crates/client-linux/tests|firmware/tests/test_codec.c"
    "fixtures/controls|hex led|crates/server/tests|firmware/tests/test_controls.c"
)
rc=0
bad() { fail "Shared fixtures" "$1"; rc=1; }
for row in "${shared[@]}"; do
    IFS='|' read -r dir exts rust c <<< "$row"
    files="$(git ls-files "$dir" | awk -F/ -v d="$dir" 'index($0, d "/") == 1 && split(substr($0, length(d) + 2), p, "/") == 1')"
    if [ -z "$files" ]; then
        [ "$dir" = fixtures/dsp ] && { echo "$dir: none yet (goal 12)"; continue; }
        bad "$dir holds no fixtures"
        continue
    fi
    leaf="${dir#fixtures/}"
    # Rust and C both name the directory (C through chorus_repo_path or a "%s/<sub>/" format).
    # shellcheck disable=SC2086 # $rust and $c are space-separated path lists, split on purpose
    git grep -q -F -e "fixtures/$leaf\"" -e "\"$leaf\")" -e "fixtures/$leaf/" -e "$(basename "$leaf")\")" -- $rust || bad "$dir is not read by a Rust test under $rust"
    # shellcheck disable=SC2086 # as above
    git grep -q -F -e "\"fixtures/$leaf\"" -e "\"fixtures/$leaf/" -e "/$(basename "$leaf")/" -- $c || bad "$dir is not read by the C test $c"
    count=0
    while IFS= read -r f; do
        count=$((count + 1))
        [ "$exts" = "*" ] && continue
        ok=0
        for e in $exts; do [[ "$f" == *."$e" ]] && ok=1; done
        [ "$ok" = 1 ] || bad "$f: a .${f##*.} file in $dir, which the Rust and C tests do not read ($exts)"
    done <<< "$files"
    echo "$dir: $count files ($exts), read by $rust and $c"
done
exit "$rc"
