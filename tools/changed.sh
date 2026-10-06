#!/usr/bin/env bash
# What a change touches, for the changed-only gate (`make gate-changed`, tools/gate.sh changed):
# the files that differ from BASE (committed, staged, unstaged and untracked), mapped to what
# has to be checked. Prints one line per area:
#   crate <package> <lib|bin>  a workspace crate the change touches, or one that depends on such
#                              a crate (any dependency kind, from `cargo metadata`); `lib` when
#                              it has a library target (and so documentation tests)
#   web | ha | firmware        the app (web/, which also touches chorus-server, the crate that
#                              embeds web/dist), the Home Assistant integration (integrations/),
#                              the ESP32-S3 endpoint (firmware/)
# A file under fixtures/, config/ or third_party/, or a root .conf, touches every crate and tree
# that names its path. The workspace manifest, the lock file, the toolchain pin and the nextest
# configuration touch every crate. Anything else (docs, tools, workflows) prints nothing: the
# conventions checks, which the changed-only gate always runs, are what hold those.
#
#   tools/changed.sh BASE
set -u
cd "$(dirname "$0")/.." || exit 2
base="${1:?usage: tools/changed.sh BASE}"

mapfile -t files < <({
    git diff --name-only "$base"...HEAD
    git diff --name-only HEAD
    git ls-files --others --exclude-standard
} | sort -u)

dirs=()
all=0
web=0
ha=0
firmware=0
for f in "${files[@]}"; do
    case "$f" in
        Cargo.toml | Cargo.lock | rust-toolchain.toml | .config/nextest.toml) all=1 ;;
        crates/*/*) d="${f#crates/}"; dirs+=("crates/${d%%/*}") ;;
        # chorus-server embeds web/dist (crates/server/build.rs) and its tests read it.
        web/*) web=1; dirs+=("crates/server") ;;
        integrations/*) ha=1 ;;
        firmware/*) firmware=1 ;;
        fixtures/* | config/* | third_party/* | *.conf)
            # The path's first two components (fixtures/dsp, config/sync.conf), as the
            # code names them.
            key="$(printf '%s' "$f" | cut -d/ -f1-2)"
            while IFS= read -r hit; do
                case "$hit" in
                    crates/*) d="${hit#crates/}"; dirs+=("crates/${d%%/*}") ;;
                    firmware/*) firmware=1 ;;
                    web/*) web=1 ;;
                    integrations/*) ha=1 ;;
                esac
            done < <(git grep -l -F -- "$key" -- crates firmware web integrations 2> /dev/null)
            ;;
    esac
done

[ "$web" = 1 ] && echo web
[ "$ha" = 1 ] && echo ha
[ "$firmware" = 1 ] && echo firmware
[ "$all" = 0 ] && [ "${#dirs[@]}" -eq 0 ] && exit 0

cargo metadata --no-deps --format-version 1 --locked |
    CHANGED_ALL="$all" CHANGED_DIRS="${dirs[*]:-}" ROOT="$(pwd)" python3 -c '
import json, os, sys
meta = json.load(sys.stdin)
root = os.environ["ROOT"]
pkgs = {p["name"]: p for p in meta["packages"]}
def rel(p):
    return os.path.relpath(os.path.dirname(p["manifest_path"]), root)
touched = set(os.environ["CHANGED_DIRS"].split())
want = set(pkgs) if os.environ["CHANGED_ALL"] == "1" else {n for n, p in pkgs.items() if rel(p) in touched}
users = {n: set() for n in pkgs}
for n, p in pkgs.items():
    for d in p["dependencies"]:
        if d["name"] in pkgs:
            users[d["name"]].add(n)
todo = list(want)
while todo:
    for u in users[todo.pop()]:
        if u not in want:
            want.add(u)
            todo.append(u)
for n in sorted(want):
    lib = any("lib" in t["kind"] or "rlib" in t["kind"] for t in pkgs[n]["targets"])
    print("crate", n, "lib" if lib else "bin")
'
