#!/usr/bin/env bash
# Every pinnable reference in one tree, judged against the umbrella's pinning
# conventions (documentation/pinning-conventions.md, clauses P1 to P8).
#
# This is the scanner. tools/pinning-check.sh is the entry point that runs it
# over this repository and then over the committed demonstrations; run that one
# unless you are debugging this one.
#
#   ./tools/pinning-scan.sh <root>              # judge a tree as a repository
#   ./tools/pinning-scan.sh --fixture <root>    # judge a tree as a fixture
#
# It reads files and asks git what is tracked. It opens no socket, resolves no
# tag and contacts no registry, which is P8's bargain: rot is discovered when a
# build fails, so this check must reach the same verdict on a machine with no
# route to any registry as on one with a network. A check that phoned a registry
# would red every unrelated pull request the day that registry had a bad
# afternoon.
#
# THE FIVE CATEGORIES, and what one reference is in each:
#
#   dockerfile-from       one FROM instruction
#   workflow-action       one `uses:` line under a .github/workflows directory
#   image-reference       one container image named outside a Dockerfile
#   dependency-manifest   one Cargo.toml or package.json
#   node-install-config   one directory holding a package.json, whose install
#                         configuration governs whether lifecycle scripts run
#
# A category that finds ZERO references in repository mode is a failure, not a
# pass. A pattern here can stop matching because a file moved, and a check that
# has quietly stopped looking is indistinguishable from a compliant tree unless
# it says so. Fixture mode turns that off, because a fixture populates one
# category on purpose.
#
# EXIT CODES, distinct per failure mode (P7), because P8 makes a red build the
# only place rot is discovered and a build has to say which rot it found:
#
#   0   every reference in every category is pinned
#   2   at least one unpinned reference; each names its file, its line, the
#       reference and the clause it broke
#   3   at least one category found no references at all; each is named
#   5   a capability this scanner needs is missing
#
# It writes nothing and leaves no partial artifact behind: the only output is on
# stdout.

set -uo pipefail

CONVENTIONS='documentation/pinning-conventions.md'

# Images this repository builds itself. A locally built image has no publisher,
# so there is no digest to resolve and P1 has nothing to pin it to; that is the
# whole of the exemption and it is not a general escape hatch. Anything NOT on
# this list is treated as a published reference and takes a digest, so changing
# deploy/run-server.sh's default to a registry reference makes this go red.
LOCAL_IMAGES=(
    "chorus-server:dev"     # built by deploy/Dockerfile, tagged by the operator
)

MODE=repository
ROOT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --fixture) MODE=fixture ;;
        --repository) MODE=repository ;;
        -*)
            printf 'pinning-scan: unknown option %s\n' "$1" >&2
            exit 5
            ;;
        *) ROOT="$1" ;;
    esac
    shift
done

if [ -z "$ROOT" ]; then
    printf 'pinning-scan: usage: %s [--fixture] <root>\n' "$0" >&2
    exit 5
fi
if [ ! -d "$ROOT" ]; then
    printf 'MISSING CAPABILITY\n  wanted: a directory to scan\n  got:    %s\n' "$ROOT" >&2
    exit 5
fi
ROOT="$(cd "$ROOT" && pwd)"

for tool in find grep sed git; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'MISSING CAPABILITY\n' >&2
        printf '  wanted: %s, which this scanner reads the tree with\n' "$tool" >&2
        printf '  this check is NOT passed, NOT skipped-green and NOT satisfied.\n' >&2
        exit 5
    fi
done

say() { printf '%s\n' "$*"; }

clause() {
    case "$1" in
        P1) printf 'P1 a container image is pinned by tag AND digest' ;;
        P2) printf 'P2 base images follow the same rule: a FROM line is an image reference like any other' ;;
        P3) printf 'P3 actions are pinned to a commit SHA, with the human-readable version in a trailing comment' ;;
        P4) printf 'P4 dependency manifests are locked: the lockfile is committed, no manifest carries a floating version, and node installs run with lifecycle scripts disabled unless the repo opts back in with a committed reason' ;;
        *)  printf '%s' "$1" ;;
    esac
}

VIOLATIONS=0

# The report for one unpinned reference: the file, the line number, the
# reference itself, and the clause it broke. All four, because a pin failure
# that does not name itself costs days (P7).
violation() {
    local file="$1" line="$2" cl="$3" ref="$4" wanted="$5"
    printf 'UNPINNED %s:%s\n' "${file#"$ROOT"/}" "$line"
    printf '  reference: %s\n' "$ref"
    printf '  broke:     %s\n' "$(clause "$cl")"
    printf '  stated in: %s (the umbrella)\n' "$CONVENTIONS"
    printf '  wanted:    %s\n' "$wanted"
    VIOLATIONS=$(( VIOLATIONS + 1 ))
}

ok() { printf 'pinned   %s:%s  %s\n' "${1#"$ROOT"/}" "$2" "$3"; }
exempt() { printf 'exempt   %s:%s  %s\n' "${1#"$ROOT"/}" "$2" "$3"; }
skipped() { printf 'skipped  %s:%s  %s\n' "${1#"$ROOT"/}" "$2" "$3"; }

CATEGORIES=(dockerfile-from workflow-action image-reference dependency-manifest node-install-config)
declare -A FOUND
for c in "${CATEGORIES[@]}"; do FOUND["$c"]=0; done
seen() { FOUND["$1"]=$(( ${FOUND["$1"]} + 1 )); }

DIGEST='@sha256:[0-9a-f]{64}'

# --- the files this tree offers ----------------------------------------------
#
# Build outputs, installed dependencies and the git directory are not source and
# are excluded. In repository mode the committed demonstrations are excluded too:
# they are deliberately unpinned trees, and scanning them here would make this
# repository permanently red for exactly the reason it exists to prevent.
mapfile -d '' ALL_FILES < <(
    find "$ROOT" \
        \( -name .git -o -name target -o -name node_modules -o -name build \
           -o -name .playwright -o -name test-results \
           -o -path "$ROOT/tools/pinning-demonstrations" \) -prune -o \
        -type f -print0
)

strip_quotes() {
    local v="$1"
    v="${v%"${v##*[![:space:]]}"}"
    v="${v#"${v%%[![:space:]]*}"}"
    case "$v" in
        \"*\") v="${v#\"}"; v="${v%\"}" ;;
        \'*\') v="${v#\'}"; v="${v%\'}" ;;
    esac
    printf '%s' "$v"
}

is_local_image() {
    local ref="$1" known
    for known in "${LOCAL_IMAGES[@]}"; do
        [ "$ref" = "$known" ] && return 0
    done
    return 1
}

# One container image reference, wherever it was named. Counted under
# image-reference; FROM lines are counted under dockerfile-from and pass their
# own clause in.
judge_image() {
    local file="$1" line="$2" ref="$3" cl="$4"
    if is_local_image "$ref"; then
        exempt "$file" "$line" "$ref is built by this repository, so it has no publisher and no digest to resolve"
        return
    fi
    if [[ "$ref" =~ ^[^[:space:]@]+:[^[:space:]@:]+${DIGEST}$ ]]; then
        ok "$file" "$line" "$ref"
        return
    fi
    violation "$file" "$line" "$cl" "$ref" \
        'both halves, written name:tag@sha256:<64 lowercase hex>. The tag stays readable to a human; the digest is what actually resolves'
}

# --- dockerfile-from ---------------------------------------------------------
#
# Every FROM takes a tag and a digest, including a FROM that names an earlier
# build stage: this file has none, and a stage reference that slipped in would
# be indistinguishable from an unpinned image to anything reading the file, so
# it is refused rather than guessed at.
scan_dockerfiles() {
    local f entry n line ref i
    local -a toks
    for f in "${ALL_FILES[@]}"; do
        case "${f##*/}" in
            Dockerfile|Dockerfile.*|*.Dockerfile) ;;
            *) continue ;;
        esac
        while IFS= read -r entry; do
            [ -n "$entry" ] || continue
            n="${entry%%:*}"
            line="${entry#*:}"
            seen dockerfile-from
            read -r -a toks <<<"$line"
            i=1
            while [ "$i" -lt "${#toks[@]}" ] && [[ "${toks[$i]}" == --* ]]; do
                i=$(( i + 1 ))
            done
            ref="$(strip_quotes "${toks[$i]:-}")"
            judge_image "$f" "$n" "$ref" P2
        done < <(grep -n -E '^[[:space:]]*FROM[[:space:]]' "$f" 2>/dev/null)
    done
}

# --- workflow-action ---------------------------------------------------------
scan_workflow_actions() {
    local f entry n line val ref comment
    for f in "${ALL_FILES[@]}"; do
        case "$f" in
            */.github/workflows/*.yml|*/.github/workflows/*.yaml) ;;
            *) continue ;;
        esac
        while IFS= read -r entry; do
            [ -n "$entry" ] || continue
            n="${entry%%:*}"
            line="${entry#*:}"
            val="${line#*uses:}"
            read -r ref comment <<<"$val"
            ref="$(strip_quotes "$ref")"

            case "$ref" in
                ./*|.\\*)
                    seen workflow-action
                    exempt "$f" "$n" "$ref is an action in this repository, which has no publisher to pin against"
                    continue
                    ;;
                docker://*)
                    seen image-reference
                    judge_image "$f" "$n" "${ref#docker://}" P1
                    continue
                    ;;
            esac

            seen workflow-action
            if ! [[ "$ref" =~ @[0-9a-f]{40}$ ]]; then
                violation "$f" "$n" P3 "$ref" \
                    'owner/action@<40 lowercase hex commit sha>. A tag like v4 is mutable and is moved by its publisher, straight into every build that wrote it'
                continue
            fi
            if ! [[ "$line" =~ @[0-9a-f]{40}[[:space:]]+#[[:space:]]*v ]]; then
                violation "$f" "$n" P3 "$ref" \
                    'the human-readable version in a trailing comment, as `uses: owner/action@<sha> # v4`, so a reader can tell which release the sha is'
                continue
            fi
            ok "$f" "$n" "$ref $comment"
        done < <(grep -n -E '^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*' "$f" 2>/dev/null)
    done
}

# --- image-reference ---------------------------------------------------------
#
# Images named outside a Dockerfile: a shell variable whose name says it holds
# one, or a yaml `image:` key. A value this cannot resolve statically is said
# out loud rather than passed over, because the one thing this file must never
# do is fall silent.
scan_image_references() {
    local f entry n line val inner
    for f in "${ALL_FILES[@]}"; do
        case "$f" in
            *.sh|*.bash|*.yml|*.yaml) ;;
            *) continue ;;
        esac
        while IFS= read -r entry; do
            [ -n "$entry" ] || continue
            n="${entry%%:*}"
            line="${entry#*:}"
            case "$line" in
                *image:*) val="${line#*image:}" ;;
                *) val="${line#*=}" ;;
            esac
            val="$(strip_quotes "$val")"
            val="${val%%#*}"
            val="$(strip_quotes "$val")"

            # ${CHORUS_IMAGE:-chorus-server:dev} names its default here.
            if [[ "$val" =~ ^\$\{[A-Za-z_][A-Za-z0-9_]*:-(.*)\}$ ]]; then
                inner="${BASH_REMATCH[1]}"
                val="$(strip_quotes "$inner")"
            fi

            case "$val" in
                *'$'*)
                    seen image-reference
                    skipped "$f" "$n" "$val is decided at run time, so nothing here can pin it; the reference it is given still has to be"
                    continue
                    ;;
            esac
            case "$val" in
                *:*) ;;
                *)
                    skipped "$f" "$n" "$val carries no tag, so it is not a container image reference"
                    continue
                    ;;
            esac
            seen image-reference
            judge_image "$f" "$n" "$val" P1
            # The value must start with something: a bare `=` or a `(` is an
            # empty assignment or a bash array, neither of which is an image.
        done < <(grep -n -E -e '^[[:space:]]*(export[[:space:]]+)?[A-Za-z0-9_]*IMAGE[A-Za-z0-9_]*=[^([:space:]]' -e '^[[:space:]]*image:[[:space:]]*[^[:space:]]' "$f" 2>/dev/null)
    done
}

# --- dependency-manifest -----------------------------------------------------

# Tracked by git and not excluded by a .gitignore. A lockfile that is present
# but ignored is not committed, and "the lockfile is committed" is the half of
# P4 that a future accidental registry dependency runs into.
lock_is_committed() {
    local lock="$1" dir
    dir="$(dirname "$lock")"
    git -C "$dir" rev-parse --is-inside-work-tree >/dev/null 2>&1 || return 2
    git -C "$dir" ls-files --error-unmatch "$lock" >/dev/null 2>&1 || return 1
    git -C "$dir" check-ignore -q "$lock" >/dev/null 2>&1 && return 1
    return 0
}

check_lock() {
    local manifest="$1" dir="$2" label="$3"
    shift 3
    local candidate found=""
    for candidate in "$@"; do
        if [ -f "$dir/$candidate" ]; then found="$dir/$candidate"; break; fi
    done
    if [ -z "$found" ]; then
        violation "$manifest" 1 P4 "$label" \
            "a committed lockfile beside it, one of: $*"
        return
    fi
    lock_is_committed "$found"
    case "$?" in
        0) ok "$found" 1 "committed lockfile for ${manifest#"$ROOT"/}" ;;
        2) skipped "$found" 1 "not inside a git work tree, so whether it is committed cannot be read here" ;;
        *) violation "$found" 1 P4 "${found#"$ROOT"/}" \
               'to be tracked by git and not excluded by a .gitignore. A lockfile that is present but ignored is not a committed lockfile' ;;
    esac
}

# A Cargo.toml needs a lock of its own only when no Cargo.toml above it owns
# one: a workspace member resolves through its workspace root.
is_topmost_cargo_toml() {
    local dir="$1"
    while [ "$dir" != "$ROOT" ] && [ "$dir" != "/" ]; do
        dir="$(dirname "$dir")"
        [ -f "$dir/Cargo.toml" ] && return 1
        [ "$dir" = "$ROOT" ] && break
    done
    return 0
}

cargo_spec_is_pinned() {
    local spec="$1"
    [[ "$spec" =~ ^=[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]
}

npm_spec_is_pinned() {
    local spec="$1"
    [[ "$spec" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]
}

# One dependency entry's right-hand side, whatever shape it was written in.
judge_cargo_entry() {
    local file="$1" n="$2" name="$3" body="$4" version
    case "$body" in
        *path*=*|*workspace*=*true*)
            exempt "$file" "$n" "$name resolves inside this workspace, so it has no published version to float"
            return
            ;;
    esac
    if [[ "$body" =~ git[[:space:]]*= ]]; then
        if [[ "$body" =~ rev[[:space:]]*=[[:space:]]*\"[0-9a-f]{40}\" ]]; then
            ok "$file" "$n" "$name at a git rev"
        else
            violation "$file" "$n" P4 "$name = $body" \
                'a git dependency pinned with rev = "<40 lowercase hex commit sha>". A branch or a tag is moved by its publisher'
        fi
        return
    fi
    if [[ "$body" =~ version[[:space:]]*=[[:space:]]*\"([^\"]*)\" ]]; then
        version="${BASH_REMATCH[1]}"
    elif [[ "$body" =~ ^\"([^\"]*)\"$ ]]; then
        version="${BASH_REMATCH[1]}"
    else
        violation "$file" "$n" P4 "$name = $body" \
            'a version this scanner can read, written `name = "=X.Y.Z"` or as a table carrying `version = "=X.Y.Z"`'
        return
    fi
    if cargo_spec_is_pinned "$version"; then
        ok "$file" "$n" "$name = \"$version\""
    else
        violation "$file" "$n" P4 "$name = \"$version\"" \
            'an exact version requirement, written =X.Y.Z. A bare "1.2.3" is a caret range and floats, and `*` and `latest` never resolve twice the same way'
    fi
}

scan_cargo_manifest() {
    local f="$1" dir section n line lhs rhs subtable_name subtable_body subtable_line
    dir="$(dirname "$f")"
    section=""
    subtable_name=""
    subtable_body=""
    subtable_line=0
    n=0
    while IFS= read -r line || [ -n "$line" ]; do
        n=$(( n + 1 ))
        line="${line%%#*}"
        case "$line" in
            \[*\]*)
                if [ -n "$subtable_name" ]; then
                    judge_cargo_entry "$f" "$subtable_line" "$subtable_name" "$subtable_body"
                    subtable_name=""
                    subtable_body=""
                fi
                section="${line#*[}"
                section="${section%%]*}"
                if [[ "$section" =~ (^|\.)((dev-|build-)?dependencies)\.([A-Za-z0-9_-]+)$ ]]; then
                    subtable_name="${BASH_REMATCH[4]}"
                    subtable_body=""
                    subtable_line="$n"
                fi
                continue
                ;;
        esac
        if [ -n "$subtable_name" ]; then
            subtable_body="$subtable_body $line"
            continue
        fi
        [[ "$section" =~ (^|\.)((dev-|build-)?dependencies)$ ]] || continue
        [[ "$line" =~ ^[[:space:]]*([A-Za-z0-9_-]+)[[:space:]]*=[[:space:]]*(.*)$ ]] || continue
        lhs="${BASH_REMATCH[1]}"
        rhs="${BASH_REMATCH[2]}"
        rhs="${rhs%"${rhs##*[![:space:]]}"}"
        [ -n "$rhs" ] || continue
        judge_cargo_entry "$f" "$n" "$lhs" "$rhs"
    done < "$f"
    if [ -n "$subtable_name" ]; then
        judge_cargo_entry "$f" "$subtable_line" "$subtable_name" "$subtable_body"
    fi

    if is_topmost_cargo_toml "$dir"; then
        check_lock "$f" "$dir" "the Rust workspace rooted at ${dir#"$ROOT"/}" Cargo.lock
    else
        exempt "$f" 1 "a workspace member, which resolves through the lock at its workspace root"
    fi
}

scan_package_json() {
    local f="$1" dir n line in_deps name spec
    dir="$(dirname "$f")"
    in_deps=0
    n=0
    while IFS= read -r line || [ -n "$line" ]; do
        n=$(( n + 1 ))
        if [[ "$line" =~ \"(dependencies|devDependencies|optionalDependencies|peerDependencies)\"[[:space:]]*:[[:space:]]*\{ ]]; then
            in_deps=1
            continue
        fi
        if [ "$in_deps" -eq 1 ]; then
            case "$line" in
                *\}*) in_deps=0; continue ;;
            esac
            [[ "$line" =~ \"([^\"]+)\"[[:space:]]*:[[:space:]]*\"([^\"]*)\" ]] || continue
            name="${BASH_REMATCH[1]}"
            spec="${BASH_REMATCH[2]}"
            case "$spec" in
                file:*|link:*|workspace:*)
                    exempt "$f" "$n" "$name resolves from this tree, so it has no published version to float"
                    continue
                    ;;
            esac
            if npm_spec_is_pinned "$spec"; then
                ok "$f" "$n" "\"$name\": \"$spec\""
            else
                violation "$f" "$n" P4 "\"$name\": \"$spec\"" \
                    'an exact version with no range, wildcard or `latest`, written X.Y.Z'
            fi
        fi
    done < "$f"

    check_lock "$f" "$dir" "the node package at ${dir#"$ROOT"/}" \
        pnpm-lock.yaml package-lock.json npm-shrinkwrap.json yarn.lock bun.lockb
}

scan_dependency_manifests() {
    local f
    for f in "${ALL_FILES[@]}"; do
        case "${f##*/}" in
            Cargo.toml)
                seen dependency-manifest
                scan_cargo_manifest "$f"
                ;;
            package.json)
                seen dependency-manifest
                scan_package_json "$f"
                ;;
        esac
    done
}

# --- node-install-config -----------------------------------------------------
#
# P4's last clause: node installs run with lifecycle scripts disabled unless the
# repo opts back in WITH A COMMITTED REASON. A flag a human is expected to
# remember to type is not a control; a committed setting is. Both config readers
# are checked, because a control that holds in only one of the two files pnpm
# consults is not a control either.
#
# `false` is allowed and is meant to be reachable. What is refused is a `false`
# with nothing beside it saying why, because that is the one a later reader
# cannot tell from an accident.
#
# The reason has to be MARKED as one - `# reason: <why>` - and not merely be a
# comment that happens to sit above the setting. Accepting any adjacent comment
# was tried first and is worthless: every one of these files already carries
# explanatory prose, so the rule passed on the strength of its own instructions
# and would have passed on a licence header. A marker is the difference between
# a control and a formality, and it costs the person opting back in six
# characters.
REASON='reason:'
judge_install_config() {
    local file="$1" pattern="$2" comment_marker="$3"
    local entry n line value prev trailing candidate why=""
    entry="$(grep -n -E "$pattern" "$file" 2>/dev/null | head -n 1)"
    [ -n "$entry" ] || return 1
    n="${entry%%:*}"
    line="${entry#*:}"
    value="$(strip_quotes "${line%%"$comment_marker"*}")"
    value="$(strip_quotes "${value#*[=:]}")"
    case "$value" in
        true)
            ok "$file" "$n" "lifecycle scripts are off"
            return 0
            ;;
    esac

    # The reason may ride on the setting's own line or on the line above it.
    trailing=""
    case "$line" in
        *"$comment_marker"*) trailing="${line#*"$comment_marker"}" ;;
    esac
    prev=""
    if [ "$n" -gt 1 ]; then
        prev="$(sed -n "$(( n - 1 ))p" "$file")"
        case "$prev" in
            *"$comment_marker"*) prev="${prev#*"$comment_marker"}" ;;
            *) prev="" ;;
        esac
    fi
    for candidate in "$trailing" "$prev"; do
        candidate="$(strip_quotes "$candidate")"
        if [[ "$candidate" =~ ^${REASON}[[:space:]]*[^[:space:]] ]]; then
            why="$candidate"
            break
        fi
    done

    if [ -n "$why" ]; then
        exempt "$file" "$n" "lifecycle scripts are on, and the setting carries its reason: $why"
    else
        violation "$file" "$n" P4 "$(strip_quotes "$line")" \
            "a reason marked as one, written \`${comment_marker} ${REASON} <why>\`, on the setting's own line or the line directly above it. Turning lifecycle scripts back on is allowed and is meant to be; doing it with nothing saying why is what is refused, and an ordinary comment that happens to sit above the setting is not a reason"
    fi
    return 0
}

scan_node_install_config() {
    local f dir handled candidate
    local -a dirs=()
    for f in "${ALL_FILES[@]}"; do
        [ "${f##*/}" = "package.json" ] || continue
        dirs+=("$(dirname "$f")")
    done
    for dir in "${dirs[@]}"; do
        seen node-install-config
        handled=0
        for candidate in "$dir/.npmrc" "$ROOT/.npmrc"; do
            [ -f "$candidate" ] || continue
            if judge_install_config "$candidate" '^[[:space:]]*ignore-scripts[[:space:]]*=' '#'; then
                handled=1
                break
            fi
        done
        for candidate in "$dir/pnpm-workspace.yaml" "$ROOT/pnpm-workspace.yaml"; do
            [ -f "$candidate" ] || continue
            if judge_install_config "$candidate" '^[[:space:]]*ignoreScripts[[:space:]]*:' '#'; then
                handled=1
                break
            fi
        done
        if [ "$handled" -eq 0 ]; then
            violation "$dir/package.json" 1 P4 "the install under ${dir#"$ROOT"/}" \
                'a committed `ignore-scripts = true` in .npmrc beside it, or `ignoreScripts: true` in pnpm-workspace.yaml, so that `pnpm install` typed with no flags runs no dependency lifecycle script'
        fi
    done
}

# --- run ---------------------------------------------------------------------

say "chorus: every pinnable reference under $ROOT, against $CONVENTIONS"
# The verdict does not depend on reaching anything, so the proxy this inherited
# is reported rather than used: a run behind a dead proxy and a run with a
# working network reach the same verdict, and that is P8's bargain in one line.
say "        mode=$MODE, network=none, inherited http_proxy=${http_proxy:-unset}"
say ""

scan_dockerfiles
scan_workflow_actions
scan_image_references
scan_dependency_manifests
scan_node_install_config

say ""
EMPTY=()
for c in "${CATEGORIES[@]}"; do
    say "category $c: ${FOUND[$c]} reference(s)"
    [ "${FOUND[$c]}" -eq 0 ] && EMPTY+=("$c")
done

if [ "$MODE" = repository ] && [ "${#EMPTY[@]}" -gt 0 ]; then
    say ""
    say "STOPPED LOOKING"
    for c in "${EMPTY[@]}"; do
        say "  category $c examined no references at all"
    done
    say "  A category goes empty when a file moved and a pattern stopped matching."
    say "  An empty category and a compliant tree look identical from here, so this"
    say "  refuses rather than report green on a check that has stopped looking."
    say "  Fix the pattern in tools/pinning-scan.sh, or delete the category on"
    say "  purpose if the thing it watched really is gone."
    exit 3
fi

if [ "$VIOLATIONS" -gt 0 ]; then
    say ""
    say "chorus: $VIOLATIONS unpinned reference(s); see $CONVENTIONS"
    exit 2
fi

say ""
say "chorus: every reference in every category is pinned"
exit 0
