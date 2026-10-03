#!/usr/bin/env bash
# Rule "Shell scripts": every tracked shell script (a .sh file, or a file whose first line is a
# sh or bash shebang) is clean under the pinned shellcheck with the repository's .shellcheckrc,
# starts with #!/usr/bin/env bash and is committed executable (a sourced library instead has no
# shebang and a `# shellcheck shell=bash` line).
. "$(dirname "$0")/lib.sh"
need shellcheck || exit 1
mapfile -t files < <(
    git ls-files -s | while read -r mode _ _ path; do
        case "$path" in tools/conventions/fixtures/*) continue ;; esac
        if [[ "$path" == *.sh ]] || head -n 1 "$path" 2> /dev/null | command grep -q -E '^#!.*\b(ba)?sh\b'; then
            printf '%s %s\n' "$mode" "$path"
        fi
    done
)
rc=0
paths=()
for f in "${files[@]}"; do
    mode="${f%% *}"
    p="${f#* }"
    paths+=("$p")
    # A sourced library has no shebang and says `# shellcheck shell=bash` instead.
    if [ "$(head -c 2 "$p")" != '#!' ] && command grep -q '^# shellcheck shell=bash' "$p"; then
        continue
    fi
    [ "$mode" = 100755 ] || { fail "Shell scripts" "$p is committed $mode, not executable (git update-index --chmod=+x)"; rc=1; }
    [ "$(head -n 1 "$p")" = '#!/usr/bin/env bash' ] || { fail "Shell scripts" "$p does not start with #!/usr/bin/env bash"; rc=1; }
done
# One shellcheck per script, eight at a time. shellcheck analyses every script it is given on its
# own (a sourced file is read again for each script that sources it), so splitting the list
# changes how long it takes and not what is found; xargs exits non-zero when any one of them did.
if ! out="$(printf '%s\0' "${paths[@]}" | xargs -0 -n 1 -P 8 shellcheck -f gcc 2>&1)"; then
    printf '%s\n' "$out" | head -n 30
    fail "Shell scripts" "shellcheck found $(printf '%s\n' "$out" | wc -l) note(s)"
    rc=1
fi
[ "$rc" = 0 ] && echo "$(shellcheck --version | sed -n 2p): ${#paths[@]} scripts clean"
exit "$rc"
