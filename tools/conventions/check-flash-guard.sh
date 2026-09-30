#!/usr/bin/env bash
# Rule "The flash guard" (brief section 0.7, K4, K93): every tool that can write to a device
# refuses unless the owner, at the bench, set the owner-at-bench variable to 1, and nothing in
# this repository sets it. Outside docs/, every occurrence of the variable's name must be an
# approved read form, used the one way its language allows (a whitelist per file kind, so a
# form nobody thought of fails rather than passes):
#     shell (*.sh, *.bash, a sh/bash shebang)  "${NAME:-}" inside a test against 1 on the same
#                                              line: [ "${NAME:-}" != 1 ] or = / == , [[ ]] too
#     Rust (*.rs)                              matches!(env::var("NAME").as_deref(), Ok("1")) or
#                                              env::var("NAME").as_deref() == / != Ok("1")
#     C (*.c, *.h)                             const char *v = getenv("NAME"); alone on its line,
#                                              the next line comparing v with strcmp(v, "1")
#     Python (*.py)                            os.getenv("NAME") == / != "1"
#     Markdown (*.md)                          the bare name in backticks (prose that names it)
# and on none of those lines a default or a go-ahead that is not the value 1 (unwrap_or,
# map_or, or_else, is_err, is_ok, .ok(), .or(, ?:, a Python `or`, :=, :-<value>). Any other
# kind of file may not hold the name at all. A read assigned to a variable in shell fails,
# which is what stops a two-line default (x="${NAME:-}" then : "${x:=1}").
#
# The name built from pieces fails anywhere outside docs/: once the whole name is removed from
# a line, any of CHORUS_OWNER, OWNER_AT or AT_BENCH left on it, or OWNER and BENCH both, is a
# finding. What a lexical scan cannot see (a name decoded from hex escapes at run time) is why
# tools/conventions/check-flash-tools-refuse.sh also runs every flashing tool without the value.
#
# Not scanned: docs/ (the owner's command lines), the program's plan (.claude/goals/) and this
# check's fixtures, which quote the forbidden forms, and exactly two scripts that must build the
# name from pieces to do their job: this file and check-flash-tools-refuse.sh (which sets it to
# values other than 1). check-flash-guard-fixtures.sh fails if that list ever grows.
#
#   check-flash-guard.sh           scan the tracked files of this repository
#   check-flash-guard.sh DIR       scan every file under DIR, paths taken relative to DIR
#                                  (the fixture test, check-flash-guard-fixtures.sh)
set -u
if [ $# -ge 1 ]; then
    root="$1"
    cd "$root" || exit 2
    files() { command find . -type f -printf '%P\0'; }
else
    . "$(dirname "$0")/lib.sh"
    files() { git ls-files -z; }
fi

# Built from pieces so this file holds no occurrence of the name itself.
name="CHORUS_OWNER_AT_""BENCH"
excluded='^(docs/|\.claude/goals/|tools/conventions/fixtures/flash-guard/|tools/conventions/check-flash-guard\.sh$|tools/conventions/check-flash-tools-refuse\.sh$)'

kind_of() {
    case "$1" in
        *.md) echo md ;;
        *.rs) echo rust ;;
        *.c | *.h) echo c ;;
        *.py) echo python ;;
        *.sh | *.bash) echo shell ;;
        *)
            if head -n 1 "$1" 2> /dev/null | command grep -q -E '^#!.*\b(ba)?sh\b'; then
                echo shell
            else
                echo other
            fi
            ;;
    esac
}

violations=0
checked=0
while IFS= read -r -d '' f; do
    [[ "$f" =~ $excluded ]] && continue
    command grep -q -I -E 'CHORUS_OWNER|OWNER_AT|AT_BENCH|OWNER.*BENCH|BENCH.*OWNER' "$f" 2> /dev/null || continue
    out="$(awk -v name="$name" -v kind="$(kind_of "$f")" -v file="$f" '
        function count(s, t,   n, i) { n = 0; while ((i = index(s, t)) > 0) { n++; s = substr(s, i + length(t)) } return n }
        function strip(s, t,   i) { while ((i = index(s, t)) > 0) s = substr(s, 1, i - 1) " " substr(s, i + length(t)); return s }
        function report(why) { printf "%s:%d: %s: %s\n", file, NR, why, $0; bad++ }
        { line[NR] = $0 }
        {
            rest = strip($0, name)
            if (rest ~ /CHORUS_OWNER|OWNER_AT|AT_BENCH/ || (rest ~ /OWNER/ && rest ~ /BENCH/)) {
                report("the name built from pieces"); next
            }
            n = count($0, name)
            if (n == 0) next
            hits += n
            if (kind == "md") {
                if (count($0, "`" name "`") != n) report("not an approved read form (Markdown names it only in backticks)")
                next
            }
            if ($0 ~ /unwrap_or|map_or|or_else|is_err|is_ok|\.ok\(\)|\.or\(|\?:|:=/ || $0 ~ /[^A-Za-z_]or[^A-Za-z_]/ && kind == "python") {
                report("an approved read that supplies a default or a go-ahead other than 1"); next
            }
            if (kind == "shell") {
                s = $0; ok = 0
                q = "\"\\$\\{" name ":-\\}\""
                re = "\\[\\[? +" q " +(=|==|!=) +(1|\"1\"|'"'"'1'"'"') +\\]\\]?"
                while (match(s, re)) { ok++; s = substr(s, RSTART + RLENGTH) }
                if (ok != n) report("not an approved read form (shell reads it only as \"${NAME:-}\" in a test against 1)")
            } else if (kind == "rust") {
                s = $0; ok = 0
                v = "(std::)?env::var\\(\"" name "\"\\)\\.as_deref\\(\\)"
                re = "(matches!\\( *" v ", *Ok\\(\"1\"\\) *\\)|" v " *(==|!=) *Ok\\(\"1\"\\))"
                while (match(s, re)) { ok++; s = substr(s, RSTART + RLENGTH) }
                if (ok != n) report("not an approved read form (Rust reads it only as env::var(NAME).as_deref() against Ok(\"1\"))")
            } else if (kind == "c") {
                re = "^[ \t]*const char \\*[A-Za-z_][A-Za-z0-9_]* = getenv\\(\"" name "\"\\);[ \t]*$"
                if (n != 1 || $0 !~ re) { report("not an approved read form (C reads it only as const char *v = getenv(NAME); alone on its line)"); next }
                v = $0; sub(/^[ \t]*const char \*/, "", v); sub(/ .*/, "", v)
                pending[NR] = v
            } else if (kind == "python") {
                s = $0; ok = 0
                re = "os\\.getenv\\(\"" name "\"\\) *(==|!=) *[\"'"'"']1[\"'"'"']"
                while (match(s, re)) { ok++; s = substr(s, RSTART + RLENGTH) }
                if (ok != n) report("not an approved read form (Python reads it only as os.getenv(NAME) == \"1\")")
            } else {
                report("not an approved read form (this kind of file may not name it outside docs/)")
            }
        }
        END {
            for (l in pending) {
                v = pending[l]
                nx = line[l + 1]
                want = "strcmp\\(" v ", \"1\"\\) *(==|!=) *0"
                if (nx !~ want) {
                    NR = l; $0 = line[l]
                    report("not an approved read form (the next line must compare " v " with strcmp(" v ", \"1\"))")
                }
            }
            printf "HITS %d\n", hits
            exit (bad > 0)
        }' "$f")"
    rc=$?
    hits="$(printf '%s\n' "$out" | sed -n 's/^HITS //p')"
    checked=$((checked + ${hits:-0}))
    found="$(printf '%s\n' "$out" | sed '/^HITS /d; /^$/d')"
    if [ -n "$found" ]; then
        printf '%s\n' "$found"
        violations=$((violations + $(printf '%s\n' "$found" | wc -l)))
    elif [ "$rc" -ne 0 ]; then
        echo "$f: the scan of this file failed (awk exit $rc)"
        violations=$((violations + 1))
    fi
done < <(files)

if [ "$violations" -gt 0 ]; then
    echo "FAIL: $violations occurrence(s) of the owner-at-bench variable outside docs/ are not approved read forms"
    echo "rule: docs/conventions.md, \"The flash guard\""
    exit 1
fi
echo "flash guard: $checked occurrence(s) outside docs/, all approved read forms"
