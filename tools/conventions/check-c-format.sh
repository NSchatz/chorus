#!/usr/bin/env bash
# Rule "C format": every tracked C source and header under firmware/ is formatted by the pinned
# clang-format with the repository's .clang-format.
. "$(dirname "$0")/lib.sh"
need clang-format || exit 1
mapfile -t files < <(git ls-files 'firmware/*.c' 'firmware/*.h')
out="$(clang-format --dry-run --Werror "${files[@]}" 2>&1)"
if [ -n "$out" ]; then
    printf '%s\n' "$out" | command grep -E '^[^ ].*(error|warning):' | cut -d: -f1 | sort | uniq -c | head -n 20
    fail "C format" "files above differ from .clang-format; run: clang-format -i \$(git ls-files 'firmware/*.c' 'firmware/*.h')"
    exit 1
fi
echo "$(clang-format --version | head -n 1): ${#files[@]} files formatted"
