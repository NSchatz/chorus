#!/usr/bin/env bash
# Rule "The Home Assistant integration", the pinned install copy (docs/home-assistant.md,
# "Installing: the pinned copy"): tools/ha-export.sh is what an installation's vendored copy
# is made and checked with, so it is held here, cheaply and with no network:
#   - two exports of one commit are the same bytes, the lock included, and so are the modes;
#   - the lock names the commit, lists exactly the commit's files, each with the sha256 of
#     its blob, and plain `sha256sum --check --strict` accepts it;
#   - the working tree cannot reach an export (it is read from the commit's objects);
#   - an export over an earlier one replaces it whole, and a directory that is not an
#     export is refused and left alone;
#   - --verify passes on a fresh export and fails on each way a copy can go wrong: a changed
#     file, a missing file, an extra file, a lock naming another commit, an edited hash.
. "$(dirname "$0")/lib.sh"
rule="The Home Assistant integration"
rc=0
bad() { fail "$rule" "$1"; rc=1; }
T=tools/ha-export.sh
SRC=integrations/homeassistant/custom_components/chorus
W="$(mktemp -d "${TMPDIR:-/tmp}/chorus-check-ha-export.XXXXXX")"
trap 'rm -rf "$W"' EXIT
head="$(git rev-parse HEAD)"
mkdir "$W/a" "$W/b"

# --- two exports are the same bytes -------------------------------------------------
bash "$T" "$W/a" > "$W/a.out" || { bad "$T <dir> failed: $(cat "$W/a.out")"; exit 1; }
bash "$T" "$W/b" HEAD > "$W/b.out" || { bad "$T <dir> HEAD failed: $(cat "$W/b.out")"; exit 1; }
diff -r "$W/a" "$W/b" > "$W/diff" || bad "two exports of $head differ: $(head -n 5 "$W/diff")"
listing() { (cd "$1" && find . -printf '%p %y %m\n' | LC_ALL=C sort); }
[ "$(listing "$W/a")" = "$(listing "$W/b")" ] || bad "two exports of $head differ in a file's name, type or mode"
listing "$W/a" | command grep -v -E ' (d 755|f 644)$' | command grep -q . &&
    bad "an export holds something that is not a 0644 file or a 0755 directory"
tree() { (cd "$1" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1); }
[ "$(tree "$W/a")" = "$(tree "$W/b")" ] || bad "two exports of $head hash differently"

# --- the lock is the commit's files -------------------------------------------------
L="$W/a/chorus.lock"
[ "$(sed -n 's/^# commit = //p' "$L")" = "$head" ] || bad "the lock does not name HEAD ($head) as its commit"
want="$(git ls-tree -r "$head" -- "$SRC" | while read -r _ _ object path; do
    printf '%s  chorus/%s\n' "$(git cat-file blob "$object" | sha256sum | cut -d' ' -f1)" "${path#"$SRC"/}"
done | LC_ALL=C sort -k 2)"
[ "$(command grep -v '^#' "$L")" = "$want" ] || bad "the lock's hash lines are not the sha256 of each blob under $SRC at HEAD, sorted"
[ "$(sed -n 's/^# files = //p' "$L")" = "$(printf '%s\n' "$want" | wc -l)" ] || bad "the lock's file count is not the number of files under $SRC at HEAD"
[ "$(sed -n 's/^# version = //p' "$L")" = "$(git cat-file blob "$head:$SRC/manifest.json" | jq -r .version)" ] ||
    bad "the lock's version is not manifest.json's at HEAD"
(cd "$W/a" && sha256sum --check --strict --quiet chorus.lock) || bad "plain sha256sum --check --strict refuses the lock"
command grep -q -E '__pycache__|\.pyc$' "$L" && bad "the lock lists byte code"

# --- the working tree cannot reach an export ------------------------------------------
# A scratch clone with an edited file, an untracked file and byte code beside the sources.
if git clone --quiet --no-local --depth 1 "file://$PWD" "$W/clone" 2> "$W/clone.err" ||
    git clone --quiet "$PWD" "$W/clone" 2>> "$W/clone.err"; then
    c="$(git -C "$W/clone" rev-parse HEAD)"
    cp "$T" "$W/clone/$T"
    echo '# edited, not committed' >> "$W/clone/$SRC/const.py"
    echo 'stray' > "$W/clone/$SRC/stray.py"
    mkdir -p "$W/clone/$SRC/__pycache__" && echo x > "$W/clone/$SRC/__pycache__/const.cpython-314.pyc"
    mkdir "$W/dirty"
    bash "$W/clone/$T" "$W/dirty" > /dev/null || bad "an export from a dirty working tree failed"
    if [ -e "$W/dirty/chorus/stray.py" ] || [ -e "$W/dirty/chorus/__pycache__" ]; then bad "an untracked file reached the export"; fi
    cmp -s "$W/dirty/chorus/const.py" <(git -C "$W/clone" cat-file blob "$c:$SRC/const.py") ||
        bad "an uncommitted edit reached the export"
    [ "$c" != "$head" ] || diff -r "$W/dirty" "$W/a" > /dev/null || bad "an export from a dirty working tree differs from the clean one"
else
    bad "could not clone the repository into a scratch directory: $(cat "$W/clone.err")"
fi

# --- replacing and refusing ---------------------------------------------------------
cp -r "$W/a" "$W/again"
echo stale > "$W/again/chorus/dropped.py"
bash "$T" "$W/again" > /dev/null || bad "an export over an earlier export failed"
diff -r "$W/a" "$W/again" > /dev/null || bad "an export over an earlier one left something of it behind"
mkdir -p "$W/foreign/chorus" && echo mine > "$W/foreign/chorus/keep.txt"
if bash "$T" "$W/foreign" > /dev/null 2>&1; then bad "a chorus directory with no lock beside it was replaced"; fi
[ "$(cat "$W/foreign/chorus/keep.txt" 2> /dev/null)" = mine ] || bad "a refused directory was changed"
[ -z "$(find "$W/foreign" -name '.chorus-export.*')" ] || bad "a refused export left its work directory behind"
if bash "$T" "$W/absent" > /dev/null 2>&1; then bad "an export into a directory that does not exist passed"; fi
if bash "$T" "$W/a" not-a-commit > /dev/null 2>&1; then bad "an export of a name that is no commit passed"; fi

# --- --verify: the good copy, then each bad one -----------------------------------------
bash "$T" --verify "$W/a" > "$W/verify.out" 2>&1 || bad "--verify refuses a fresh export: $(cat "$W/verify.out")"
command grep -q "VERIFIED .* $head " "$W/verify.out" || bad "--verify did not print its VERIFIED line naming the commit"
must_refuse() { # <copy> <what was done to it> <what the refusal must name>
    if bash "$T" --verify "$W/$1" > "$W/$1.out" 2>&1; then
        bad "--verify passed a copy with $2"
    elif ! command grep -q -F "$3" "$W/$1.out"; then
        bad "--verify refused a copy with $2 without naming it ($3): $(tail -n 3 "$W/$1.out")"
    fi
}
cp -r "$W/a" "$W/changed" && echo '# one line more' >> "$W/changed/chorus/const.py"
must_refuse changed "a changed file" "chorus/const.py"
cp -r "$W/a" "$W/missing" && rm "$W/missing/chorus/_aiochorus/py.typed"
must_refuse missing "a missing file" "chorus/_aiochorus/py.typed"
cp -r "$W/a" "$W/extra" && echo x > "$W/extra/chorus/extra.py"
must_refuse extra "an extra file" "chorus/extra.py"
cp -r "$W/a" "$W/link" && ln -s const.py "$W/link/chorus/link.py"
must_refuse link "a symbolic link" "chorus/link.py"
cp -r "$W/a" "$W/commit" && sed -i "s/^# commit = .*/# commit = $(printf '0%.0s' {1..40})/" "$W/commit/chorus.lock"
must_refuse commit "a lock naming a commit this repository does not have" "is not in this repository"
# A copy that is consistent with its own lock and is still not the commit's: the file and its
# hash line edited together.
cp -r "$W/a" "$W/forged" && echo '# forged' >> "$W/forged/chorus/const.py"
new="$(sha256sum "$W/forged/chorus/const.py" | cut -d' ' -f1)"
sed -i "s|^[0-9a-f]\{64\}  chorus/const.py\$|$new  chorus/const.py|" "$W/forged/chorus.lock"
(cd "$W/forged" && sha256sum --check --strict --quiet chorus.lock) || bad "the forged copy is not self-consistent (the check's own setup)"
must_refuse forged "a file and its hash line edited together" "is not the lock of $head"

[ "$rc" = 0 ] && echo "ha-export: two exports of $head are the same bytes ($(sed -n 's/^# files = //p' "$L") files, tree $(tree "$W/a" | cut -c1-16)); the lock is the commit's blobs; --verify refuses 6 bad copies by name"
exit "$rc"
