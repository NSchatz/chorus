#!/usr/bin/env bash
# The interface-craft gate: this repository's committed identity and the C2
# blocklist over the control page's sources, then the demonstrations that prove
# the gate can go red on every way it can fail.
#
# Judged against the umbrella's .sdd/conventions/interface-craft.md, clauses C1
# and C2. tools/ui/interface-craft-scan.js is the scanner; this is what
# `make verify` and CI run.
#
#   ./tools/interface-craft-check.sh      # or: make verify-interface-craft
#
# WHAT THIS IS AND IS NOT. It is a check of SOURCE TEXT, over the files that
# declare the control page's identity and the record that says what that identity
# is. C2 asks for exactly that - "a mechanical check over the stylesheet and
# template sources against this list" - and everything about what the page PAINTS
# is graded in a real browser engine by tools/ui-render-run.sh. Nothing here
# reports on a rendered colour, a rendered length or a rendered face. C3 to C8 of
# that convention are about the rendered document and are not graded here.
#
# WHY THE PRINTED LISTS ARE ASSERTED HERE. A sweep that read no file exits zero,
# and so does a sweep that read every file: the two are indistinguishable from
# the exit status alone. So the scan PRINTS what it resolved and what it tested,
# and this script holds those lines to a listing of the source directory it takes
# itself and to its own copy of the blocklist below. That copy is deliberately a
# second one: if it and the scan's ever disagree, the disagreement is the finding.
#
# WHY THE DEMONSTRATIONS ARE COMMITTED. A check that has never been seen to fail
# is a formality rather than evidence. Each tree under
# tools/interface-craft-demonstrations/ breaks exactly one thing and is scanned
# here and required to go red with the exit code that fault is assigned. The
# `base` tree beside them breaks nothing and is required to PASS, because a
# scanner that refuses everything discriminates nothing, and each broken tree
# commits only the file it breaks and inherits the rest of base, so the
# difference is one file against one file.
#
# THOSE TREES ARE FIXTURES AND NOT THE SURFACE. They are deliberately broken
# copies, which is why the repository sweep resolves crates/server/src/ui and
# nothing else: a blocklist check that walked tools/ would be grading a fixture.
#
# EXIT CODES, distinct per failure mode. The repository's own verdict wins when
# there is one, because a demonstration that also failed is the less actionable
# of the two:
#
#   0   the record holds, this repository's sources carry no unnamed C2 entry,
#       and every committed demonstration produced what it demonstrates
#   2   an identity source carries a C2 blocklist entry the record does not name
#       as an exception
#   3   a capability this check needs is missing, refused in tools/lib.sh's shape
#   4   a category has stopped matching: the identity sources resolved to an
#       empty set, or a blocklist entry was tested against no source at all
#   5   the design record names an exception and gives it no reason
#   6   the design record names an exception for an entry no identity source
#       carries
#   7   the design record is absent
#   8   the design record is there and cannot be read
#   9   the design record does not parse in the shape this check expects
#   10  the record's five identity entries are wrong: missing, named twice, with
#       no reason, or disagreeing with the token file
#   11  a committed demonstration did not produce what it demonstrates, the base
#       tree did not pass, or the scan did not print what it measured

source "$(dirname "$0")/lib.sh"

# Deliberately NOT build_once: this check reads files. It compiles nothing, so
# it stays runnable on a tree that does not build.

CRITERION="the control page's committed identity holds interface-craft C1 and C2: docs/interface-craft-record.md names a display face, a text face, an accent, a radius signature and a shadow signature with one sentence each and agrees with the token file, and no identity source carries an entry of the C2 blocklist the record does not name as an exception with its reason"
require_node "$CRITERION"

SCAN="$REPO_ROOT/tools/ui/interface-craft-scan.js"
DEMOS="$REPO_ROOT/tools/interface-craft-demonstrations"
UI="$REPO_ROOT/crates/server/src/ui"
CONVENTIONS='.sdd/conventions/interface-craft.md'

# The C2 blocklist, verbatim from that file, held here as well as in the scan.
# One entry per distinct thing that can be matched rather than per clause of C2's
# sentence: a grouped entry would report less than it knows, and a tree carrying
# one half of the "rounded-2xl + shadow-lg" pairing would otherwise pass.
BLOCKLIST=(
    "Inter"
    "Roboto"
    "Helvetica"
    "Arial"
    "Space Grotesk"
    "a bare system stack alone"
    "indigo-500"
    "blue-600"
    "zinc"
    "slate"
    "a purple-to-blue gradient"
    "bg-clip-text"
    "glassmorphism"
    "rounded-2xl"
    "shadow-lg"
)

FAILURES=0
CHECKED=()
REDDED=()
REPO_STATUS=0
REPO_OUT=""

fail() {
    say "FAIL $*"
    FAILURES=$(( FAILURES + 1 ))
}

# --- this repository ----------------------------------------------------------

say ""
say "=== chorus: the control page's committed identity, and the C2 blocklist ======="
say ""

set +e
REPO_OUT="$("$CHORUS_NODE" "$SCAN" 2>&1)"
REPO_STATUS=$?
set -e
printf '%s\n' "$REPO_OUT" | sed 's/^/    /'

case "$REPO_STATUS" in
    0) say "pass the record holds and no identity source carries an entry the record does not name" ;;
    2) fail "an identity source carries a C2 blocklist entry the record does not except (exit 2); see the report above and $CONVENTIONS" ;;
    4) fail "a category has stopped matching (exit 4); the sweep read no file, or an entry was tested against none" ;;
    5) fail "the record excepts an entry and gives no reason (exit 5); an exception is not bought without one" ;;
    6) fail "the record excepts an entry no identity source carries (exit 6); the permission is withdrawn rather than left standing" ;;
    7) fail "the design record is absent (exit 7); interface-craft C1 asks for one to be committed" ;;
    8) fail "the design record is there and cannot be read (exit 8)" ;;
    9) fail "the design record does not parse in the shape this check expects (exit 9)" ;;
    10) fail "the record's five identity entries are wrong (exit 10); see the report above" ;;
    *) fail "the scan could not run over this repository (exit $REPO_STATUS)" ;;
esac

# --- what the scan says it measured, against what is actually there -----------
#
# AC-4's half of the criterion is decided by reading output, so it is read here
# rather than left to a person. Three assertions, and each one is against a fact
# this script establishes for itself.

say ""
say "--- the scan printed the identity sources it read, and they are the ones that are there"

line_after() {
    printf '%s\n' "$REPO_OUT" | sed -n "s/^$1: //p" | head -n 1
}

READ_LINE="$(line_after read)"
if [ -z "$READ_LINE" ]; then
    fail "the scan printed no identity sources, so a reader cannot tell a complete sweep from an empty one"
else
    FOUND_SOURCES="$(find "$UI" -maxdepth 1 -type f \
        \( -name '*.css' -o -name '*.html' -o -name '*.js' \) -printf '%f\n' | sort)"
    READ_SOURCES="$(printf '%s\n' "$READ_LINE" | tr ',' '\n' | xargs -n1 basename | sort)"
    if [ "$FOUND_SOURCES" != "$READ_SOURCES" ]; then
        fail "the scan says it read [$(printf '%s' "$READ_SOURCES" | tr '\n' ' ')] and crates/server/src/ui holds [$(printf '%s' "$FOUND_SOURCES" | tr '\n' ' ')]"
    else
        say "pass the scan read all $(printf '%s\n' "$FOUND_SOURCES" | wc -l | tr -d ' ') files under crates/server/src/ui and nothing outside it"
    fi
fi

say ""
say "--- the scan printed the blocklist entries it tested, and they are the whole list"

TESTED_LINE="$(line_after tested)"
if [ -z "$TESTED_LINE" ]; then
    fail "the scan printed no blocklist entries, so a zero exit says nothing about what was tested"
else
    WANT_ENTRIES="$(printf '%s\n' "${BLOCKLIST[@]}" | sort)"
    GOT_ENTRIES="$(printf '%s\n' "$TESTED_LINE" | tr ',' '\n' | sort)"
    if [ "$WANT_ENTRIES" != "$GOT_ENTRIES" ]; then
        fail "the scan tested [$(printf '%s' "$GOT_ENTRIES" | tr '\n' ' ')] and C2's list is [$(printf '%s' "$WANT_ENTRIES" | tr '\n' ' ')]"
    else
        say "pass all ${#BLOCKLIST[@]} entries of C2's blocklist were tested, against this script's own copy of it"
    fi
fi

say ""
say "--- no entry was tested against nothing"

LEAST="$(line_after least-tested-against)"
SOURCES_READ="$(line_after sources-read)"
ENTRIES_TESTED="$(line_after entries-tested)"
if [ "${SOURCES_READ:-0}" -lt 1 ] || [ "${ENTRIES_TESTED:-0}" -lt 1 ] || [ "${LEAST:-0}" -lt 1 ]; then
    fail "the scan read ${SOURCES_READ:-0} source(s) and tested ${ENTRIES_TESTED:-0} entry(s), the least of them against ${LEAST:-0}; a green run over nothing is the failure this asserts against"
else
    say "pass every one of $ENTRIES_TESTED entries was tested against at least $LEAST of $SOURCES_READ sources"
fi

# --- the committed trees ------------------------------------------------------

run_fixture() {
    local name="$1"
    CHECKED+=("$name")
    local dir="$DEMOS/$name"
    if [ ! -d "$dir" ]; then
        fail "$name: no such demonstration under tools/interface-craft-demonstrations"
        return 1
    fi
    set +e
    FIXTURE_OUT="$("$CHORUS_NODE" "$SCAN" --fixture "$dir" 2>&1)"
    FIXTURE_STATUS=$?
    set -e
    return 0
}

# One committed tree that is RIGHT on purpose. Without it, "refuses a tree
# carrying Roboto" and "refuses every tree" leave identical evidence.
expect_green() {
    local name="$1"
    run_fixture "$name" || return
    say ""
    say "--- $name (exit $FIXTURE_STATUS, wanted 0)"
    printf '%s\n' "$FIXTURE_OUT" | sed 's/^/    /'
    if [ "$FIXTURE_STATUS" -ne 0 ]; then
        fail "$name exited $FIXTURE_STATUS and 0 was wanted; the scan refuses a tree that holds every rule"
        return
    fi
    say "pass $name is accepted, so the scan discriminates rather than refusing every tree"
}

# One committed tree carrying exactly one entry of the blocklist and nothing else
# on it. It has to go red on that entry, naming it and no other: a tree that went
# red for two entries would leave the second unproved and the first unlocated.
expect_entry() {
    local name="$1" entry="$2"
    run_fixture "$name" || return
    say ""
    say "--- $name (exit $FIXTURE_STATUS, wanted 2, naming $entry)"
    printf '%s\n' "$FIXTURE_OUT" | sed 's/^/    /'

    local red=1
    if [ "$FIXTURE_STATUS" -ne 2 ]; then
        fail "$name exited $FIXTURE_STATUS and 2 was wanted; a demonstration that does not go red is not a demonstration"
        red=0
    fi
    if ! printf '%s\n' "$FIXTURE_OUT" | grep -qF "FAIL blocklist-hit [$entry] "; then
        fail "$name did not go red naming $entry, so a reader is not told which entry of $CONVENTIONS was carried"
        red=0
    fi
    local others
    others="$(printf '%s\n' "$FIXTURE_OUT" | grep -E '^FAIL ' \
        | sed -n 's/^FAIL [a-z-]* \[\([^]]*\)\].*/\1/p' | sort -u \
        | grep -vxF "$entry" || true)"
    if [ -n "$others" ]; then
        fail "$name went red on more than the one entry it carries: $(printf '%s' "$others" | tr '\n' ' ')"
        red=0
    fi
    if [ "$red" -eq 1 ]; then
        say "pass $name went red naming $entry and nothing else"
        REDDED+=("$entry")
    fi
}

# One committed tree per remaining way this gate can fail. Each has to exit with
# the code that fault is assigned and to say which fault it was, because a red
# build that only says "there was a fault" is the thing distinct codes exist to
# replace.
expect_mode() {
    local name="$1" want_code="$2" want_finding="$3" want_said="$4"
    run_fixture "$name" || return
    say ""
    say "--- $name (exit $FIXTURE_STATUS, wanted $want_code, saying $want_finding)"
    printf '%s\n' "$FIXTURE_OUT" | sed 's/^/    /'

    local red=1
    if [ "$FIXTURE_STATUS" -ne "$want_code" ]; then
        fail "$name exited $FIXTURE_STATUS and $want_code was wanted; the code is what tells a red build which kind of fault it was"
        red=0
    fi
    if ! printf '%s\n' "$FIXTURE_OUT" | grep -qF "FAIL $want_finding "; then
        fail "$name did not report $want_finding, so a reader is told the exit code and nothing else"
        red=0
    fi
    if [ -n "$want_said" ] && ! printf '%s\n' "$FIXTURE_OUT" | grep -qF "$want_said"; then
        fail "$name did not say '$want_said', which is the half of this criterion that is about what it reports"
        red=0
    fi
    if [ "$red" -eq 1 ]; then
        say "pass $name exited $want_code reporting $want_finding"
    fi
}

say ""
say "=== chorus: one committed tree per entry of the C2 blocklist =================="

expect_entry a-face-named-inter                 "Inter"
expect_entry a-face-named-roboto                "Roboto"
expect_entry a-face-named-helvetica             "Helvetica"
expect_entry a-face-named-arial                 "Arial"
expect_entry a-face-named-space-grotesk         "Space Grotesk"
expect_entry a-bare-system-stack-alone          "a bare system stack alone"
expect_entry an-accent-named-indigo-500         "indigo-500"
expect_entry an-accent-named-blue-600           "blue-600"
expect_entry an-untouched-zinc-ramp             "zinc"
expect_entry an-untouched-slate-ramp            "slate"
expect_entry a-purple-to-blue-gradient          "a purple-to-blue gradient"
expect_entry a-gradient-bg-clip-text-heading    "bg-clip-text"
expect_entry glassmorphism                      "glassmorphism"
expect_entry a-rounded-2xl-container            "rounded-2xl"
expect_entry a-shadow-lg-container              "shadow-lg"

say ""
say "=== chorus: one committed tree per remaining way this gate can fail ==========="

# A sweep that read nothing is the failure mode a green exit hides best. Two
# trees, because "there was no file to read" and "there were files and none could
# be read" are different faults with the same consequence.
expect_mode an-empty-identity-source-set \
    4 stopped-matching "resolved to an EMPTY set"
expect_mode a-blocklist-entry-tested-against-no-source \
    4 stopped-matching "was tested against no source at all"

# An exception is not bought without a stated reason, and the entry it was
# covering is reported as if it had never been claimed.
expect_mode an-exception-carrying-no-reason \
    5 exception-without-reason "FAIL blocklist-hit [Roboto]"

# A permission that has stopped being needed is withdrawn.
expect_mode an-exception-no-source-needs \
    6 exception-nothing-needs "no identity source carries it"

# Absent, unreadable and unparseable are three faults with three remedies, so
# they are three codes and three trees, and each says which one it was.
expect_mode a-record-that-is-absent \
    7 record-absent "the design record is absent"
expect_mode a-record-that-cannot-be-read \
    8 record-unreadable "cannot be read"
expect_mode a-record-that-does-not-parse \
    9 record-unparseable "does not parse in the shape this check expects"

# C1 is graded as "the record exists, names all five, and the token file matches
# it". One tree per half of that.
expect_mode a-record-missing-an-identity-entry \
    10 identity "asks for this entry and the record declares it nowhere"
expect_mode a-record-that-disagrees-with-the-token-file \
    10 identity "declares it as"

say ""
say "=== chorus: and the one tree that is supposed to pass =========================="

expect_green base

# --- the demonstrations are enough of them, and the list is the whole list ----

say ""
say "--- every entry of the blocklist was shown going red"
ENTRIES_RED="$(printf '%s\n' "${REDDED[@]:-}" | sed '/^$/d' | sort -u)"
ENTRIES_ALL="$(printf '%s\n' "${BLOCKLIST[@]}" | sort -u)"
NEVER_RED="$(comm -23 <(printf '%s\n' "$ENTRIES_ALL") <(printf '%s\n' "$ENTRIES_RED") || true)"
if [ -n "$NEVER_RED" ]; then
    say "FAIL these entries are on the blocklist and no committed tree has been seen to carry them:"
    printf '%s\n' "$NEVER_RED" | sed 's/^/    /'
    FAILURES=$(( FAILURES + 1 ))
else
    say "pass all $(printf '%s\n' "$ENTRIES_ALL" | wc -l | tr -d ' ') entries of the blocklist have a committed tree that goes red on them"
fi

# A demonstration added to the directory and never run here would make this
# meta-check quietly incomplete, which is the exact failure it exists to
# prevent. Derived from the directory rather than restated.
say ""
say "--- every committed demonstration is exercised above"
FOUND_DEMOS="$(find "$DEMOS" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)"
DECLARED_DEMOS="$(printf '%s\n' "${CHECKED[@]}" | sort)"
MISSED="$(comm -23 <(printf '%s\n' "$FOUND_DEMOS") <(printf '%s\n' "$DECLARED_DEMOS") || true)"
if [ -n "$MISSED" ]; then
    say "FAIL these demonstrations are committed and are not run here:"
    printf '%s\n' "$MISSED" | sed 's/^/    /'
    FAILURES=$(( FAILURES + 1 ))
else
    say "pass all $(printf '%s\n' "$FOUND_DEMOS" | wc -l | tr -d ' ') committed demonstrations are run above"
fi

say ""
if [ "$FAILURES" -eq 0 ]; then
    say "chorus: the control page's identity is on record, its sources carry no C2 entry the record does not name, and the check that says so went red on all ${#BLOCKLIST[@]} entries and every other way it can fail"
    exit 0
fi

say "chorus: $FAILURES interface-craft checks did not hold"
if [ "$REPO_STATUS" -ne 0 ]; then
    exit "$REPO_STATUS"
fi
exit 11
