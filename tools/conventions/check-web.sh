#!/usr/bin/env bash
# Rule "Dependencies and licences", the JavaScript of web/ (P5, docs/decisions/0181-the-web-app-stack.md):
# cheap, with no install and no node. What it holds, each a way the app's supply chain could
# quietly widen:
#   1. Install scripts are off where the pinned pnpm reads it: web/pnpm-workspace.yaml says
#      `ignoreScripts: true` once, every entry of its `allowBuilds` is `false`, no other key
#      there allows a build, and package.json declares no scripts of its own. There is no
#      web/.npmrc: pnpm 12 takes no such setting from one, so a line there would switch
#      nothing off and read as if it did.
#   2. The direct dependencies are exactly the settled stack: lit, esbuild, happy-dom,
#      @happy-dom/global-registrator and @playwright/test (the one browser smoke test).
#      Another package comes with an ADR and a change here.
#   3. Every package of pnpm-lock.yaml comes from the registry with an integrity digest, and
#      the lockfile is the project's alone (no package manager locked beside it).
#   4. Every locked package has a line in web/licences.txt at its version, no line is left
#      over, and every licence is on the allowlist of rule 13.
# (Exact versions beside a committed lockfile are check-pins.sh's; that an installed package
# carries the licence its line records is tools/web.sh's, in the gate steps `web-test` and
# `web-build`.)
# The self-test at the end runs the same function over damaged copies: scripts switched on in
# each of the three places, the setting moved to an .npmrc, a licence off the allowlist, a locked package without a line, a
# package beyond the stack and a locked package without a digest must each fail.
. "$(dirname "$0")/lib.sh"
dir=web
allowed=" MIT Apache-2.0 BSD-2-Clause BSD-3-Clause 0BSD ISC Zlib Unlicense CC0-1.0 "
stack="@happy-dom/global-registrator @playwright/test esbuild happy-dom lit"
files="package.json pnpm-lock.yaml pnpm-workspace.yaml licences.txt"

# The name@version keys of the lockfile's `packages:` section, one per line.
locked() {
    awk '/^packages:/ { on = 1; next } on && /^[^ ]/ { exit }
        on && /^  [^ ].*:$/ { s = $0; gsub(/^  |:$|\047/, "", s); print s }' "$1"
}

# check_dir <directory>: prints one line per problem; returns 1 when there is any.
check_dir() {
    local d="$1" bad=0 f deps keys pv name version licence n
    for f in $files; do
        [ -f "$d/$f" ] || { echo "$d/$f is missing"; bad=1; }
    done
    [ "$bad" = 0 ] || return 1

    # 1. install scripts off
    # Exactly one line names the setting, and it is the one that switches scripts off.
    if [ "$(command grep -c -E '^ignoreScripts[[:space:]]*:' "$d/pnpm-workspace.yaml")" != 1 ] ||
        ! command grep -q -x 'ignoreScripts: true' "$d/pnpm-workspace.yaml"; then
        echo "$d/pnpm-workspace.yaml does not say exactly ignoreScripts: true: install scripts are enabled"
        bad=1
    fi
    if [ -e "$d/.npmrc" ]; then
        echo "$d/.npmrc exists; pnpm's settings are in pnpm-workspace.yaml, and the pinned pnpm reads no script setting from an .npmrc"
        bad=1
    fi
    if awk '/^allowBuilds:/ { on = 1; next } on && /^[^ #]/ { on = 0 } on && /^ +[^ #]/ && !/: false$/ { found = 1 } END { exit !found }' "$d/pnpm-workspace.yaml"; then
        echo "$d/pnpm-workspace.yaml: an allowBuilds entry is not false: a package's install script is enabled"
        bad=1
    fi
    if command grep -n -E '^(onlyBuiltDependencies|onlyBuiltDependenciesFile|dangerouslyAllowAllBuilds|enablePrePostScripts)[[:space:]]*:' "$d/pnpm-workspace.yaml"; then
        echo "$d/pnpm-workspace.yaml: a key that allows install scripts; ignoreScripts: true and allowBuilds with every entry false are the only ones"
        bad=1
    fi
    [ "$(jq -c '[.scripts, .pnpm] | map(select(. != null)) | length' "$d/package.json")" = 0 ] ||
        { echo "$d/package.json declares scripts or a pnpm section; the Makefile runs the app's tools and pnpm-workspace.yaml holds pnpm's settings"; bad=1; }

    # 2. the settled stack and nothing else
    deps="$(jq -r '[.dependencies, .devDependencies, .optionalDependencies, .peerDependencies] | map(. // {} | keys) | add | sort | join(" ")' "$d/package.json")"
    [ "$deps" = "$stack" ] ||
        { echo "$d/package.json depends on '$deps'; the settled stack is '$stack' (another package comes with an ADR)"; bad=1; }

    # 3. the lockfile: one document, every package from the registry with a digest
    if [ "$(command grep -c -x 'lockfileVersion: .*' "$d/pnpm-lock.yaml")" != 1 ] || command grep -q 'packageManagerDependencies' "$d/pnpm-lock.yaml"; then
        echo "$d/pnpm-lock.yaml is more than the project's own lockfile (a package manager is locked beside it)"
        bad=1
    fi
    keys="$(locked "$d/pnpm-lock.yaml")"
    n="$(printf '%s\n' "$keys" | command grep -c .)"
    [ "$n" -gt 0 ] || { echo "$d/pnpm-lock.yaml locks no package"; bad=1; }
    [ "$(awk '/^packages:/ { on = 1; next } on && /^[^ ]/ { exit } on && /^    resolution: \{integrity: sha512-[A-Za-z0-9+\/]+=*\}$/' "$d/pnpm-lock.yaml" | wc -l)" = "$n" ] ||
        { echo "$d/pnpm-lock.yaml: a locked package has no registry integrity digest (resolution: {integrity: sha512-...})"; bad=1; }
    if command grep -n -E '(tarball|repo|commit|directory):' "$d/pnpm-lock.yaml"; then
        echo "$d/pnpm-lock.yaml: a package from a tarball, a repository or a directory, not the registry"
        bad=1
    fi

    # 4. the licence list is the lockfile's packages, each on the allowlist
    while IFS= read -r pv; do
        [ -n "$pv" ] || continue
        command grep -q -E "^${pv%@*} ${pv##*@} [^ ]+\$" "$d/licences.txt" ||
            { echo "$d/licences.txt has no line for the locked package $pv"; bad=1; }
    done <<< "$keys"
    while read -r name version licence rest; do
        [ -n "$licence" ] && [ -z "$rest" ] || { echo "$d/licences.txt: a line is not 'name version licence': $name $version $licence $rest"; bad=1; continue; }
        printf '%s\n' "$keys" | command grep -q -x -F "$name@$version" ||
            { echo "$d/licences.txt lists $name $version, which the lockfile does not lock"; bad=1; }
        [[ "$allowed" == *" $licence "* ]] ||
            { echo "$d/licences.txt: $name $version is licensed $licence, which is off the allowlist"; bad=1; }
    done < <(command grep -v -E '^(#|[[:space:]]*$)' "$d/licences.txt")
    command grep -q -E '^# .*Read [0-9]{4}-[0-9]{2}-[0-9]{2} from https://registry\.npmjs\.org/' "$d/licences.txt" ||
        { echo "$d/licences.txt does not say when and where its licences were read"; bad=1; }
    return "$bad"
}

rule="Dependencies and licences"
rc=0
need jq || exit 1
out="$(check_dir "$dir")" || { printf '%s\n' "$out"; fail "$rule" "web/ is not the settled stack with install scripts off and every licence on the allowlist"; rc=1; }

# Self-test: each damaged copy must fail, and the clean copy must pass.
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
first="$(locked "$dir/pnpm-lock.yaml" | head -n 1)"
# damage <how>: run inside a copy of the directory.
damage() {
    case "$1" in
    scripts-on) sed -i 's/^ignoreScripts: true$/ignoreScripts: false/' pnpm-workspace.yaml ;;
    scripts-unset) sed -i '/^ignoreScripts: true$/d' pnpm-workspace.yaml ;;
    npmrc) echo 'ignore-scripts=true' > .npmrc ;;
    build-allowed) sed -i 's/^  esbuild: false$/  esbuild: true/' pnpm-workspace.yaml ;;
    own-script) jq '.scripts = {postinstall: "node build.mjs"}' package.json > p && mv p package.json ;;
    off-allowlist) sed -i "s|^${first%@*} ${first##*@} .*|${first%@*} ${first##*@} MPL-2.0|" licences.txt ;;
    unlisted) sed -i "\\|^${first%@*} ${first##*@} |d" licences.txt ;;
    beyond-the-stack) jq '.devDependencies.vite = "8.3.1"' package.json > p && mv p package.json ;;
    no-digest) sed -i '0,/^    resolution: {integrity: sha512-.*}$/s//    resolution: {tarball: https:\/\/example.invalid\/x.tgz}/' pnpm-lock.yaml ;;
    clean) ;;
    esac
}
# Each damaged copy fails, and for its own reason: the line it must print is named beside it.
while IFS='|' read -r how reason; do
    copy="$scratch/$how"
    mkdir -p "$copy"
    for f in $files; do cp "$dir/$f" "$copy/"; done
    (cd "$copy" && damage "$how")
    if out="$(check_dir "$copy")"; then
        [ "$how" = clean ] || { fail "$rule" "self-test: a copy damaged as '$how' passed the check"; rc=1; }
    elif [ "$how" = clean ]; then
        printf '%s\n' "$out"
        fail "$rule" "self-test: a clean copy failed the check"
        rc=1
    elif [[ "$out" != *"$reason"* ]]; then
        printf '%s\n' "$out"
        fail "$rule" "self-test: a copy damaged as '$how' failed without saying '$reason'"
        rc=1
    fi
    [ -z "${CHORUS_CHECK_WEB_VERBOSE:-}" ] || printf 'self-test %s: %s\n' "$how" "${out:-pass}"
done << 'CASES'
clean|
scripts-on|install scripts are enabled
scripts-unset|install scripts are enabled
npmrc|the pinned pnpm reads no script setting from an .npmrc
build-allowed|a package's install script is enabled
own-script|declares scripts
off-allowlist|is licensed MPL-2.0, which is off the allowlist
unlisted|has no line for the locked package
beyond-the-stack|the settled stack is
no-digest|has no registry integrity digest
CASES

[ "$rc" = 0 ] && echo "web: install scripts off, the settled stack ($stack), $(locked "$dir/pnpm-lock.yaml" | wc -l) locked packages each with a digest and a licence on the allowlist; 9 damaged copies each fail"
exit "$rc"
