#!/usr/bin/env bash
# The styling gate: this repository's stylesheet sources, then the
# demonstrations that prove the gate can go red.
#
# Judged against the umbrella's styling conventions, .sdd/conventions/styling.md,
# clauses S1 to S10. tools/ui/styling-scan.js is the scanner; this is what
# `make verify` and CI run.
#
#   ./tools/styling-check.sh      # or: make verify-styling
#
# WHAT THIS IS AND IS NOT. It is a check of SOURCE TEXT, over the files that
# declare the tokens and spend them. Clause S1 sanctions that split by name -
# "a mechanical check over the stylesheet sources, which is a check of source
# text and so does not collide with F2" - and everything about what the page
# PAINTS is graded in a real browser engine by tools/ui-render-run.sh, which
# refuses rather than reading a stylesheet. Nothing here reports on a rendered
# colour, a rendered length or a rendered face. A green run of this says the
# files say the right things; only the engine says the page draws them.
#
# WHY THE DEMONSTRATIONS ARE COMMITTED. A check that has never been seen to fail
# is a formality rather than evidence. Each tree under
# tools/styling-demonstrations/ breaks exactly one rule and is scanned here and
# required to go red naming that rule. The `base` tree beside them breaks
# nothing and is required to PASS, because a scanner that refuses everything
# discriminates nothing, and each broken tree commits only the file it breaks
# and inherits the rest of base, so the difference is one file against one file.
#
# EXIT CODES, distinct per failure mode:
#
#   0   the sources hold every rule and every demonstration went red as required
#   2   a styling rule is broken in this repository
#   3   a category of this repository examined nothing at all
#   4   a demonstration did not go red, or the base tree did not pass
#   5   a capability this check needs is missing

source "$(dirname "$0")/lib.sh"

# Deliberately NOT build_once: this check reads files. It compiles nothing, so
# it stays runnable on a tree that does not build.

CRITERION="the control page's stylesheet sources hold the umbrella's styling conventions S1 to S10: every colour and length resolves to a token, three tiers named for role, the 4px scale, a hand-authored value per theme, a measured contrast ratio recorded beside each pair, separation by border and surface, and one accent hue"
require_node "$CRITERION"

SCAN="$REPO_ROOT/tools/ui/styling-scan.js"
DEMOS="$REPO_ROOT/tools/styling-demonstrations"
CONVENTIONS='.sdd/conventions/styling.md'

FAILURES=0
REDS=0
CHECKED=()
REDDED=()
REPO_STATUS=0

fail() {
    say "FAIL $*"
    FAILURES=$(( FAILURES + 1 ))
}

say ""
say "=== chorus: the control page's stylesheet sources ============================="
say ""

set +e
REPO_OUT="$("$CHORUS_NODE" "$SCAN" 2>&1)"
REPO_STATUS=$?
set -e
printf '%s\n' "$REPO_OUT" | sed 's/^/    /'

case "$REPO_STATUS" in
    0) say "pass every styling rule holds over this repository's stylesheet sources" ;;
    2) fail "a styling rule is broken in this repository (exit 2); see the report above and $CONVENTIONS" ;;
    3) fail "a category of this repository examined nothing at all (exit 3); the scan has stopped looking" ;;
    *) fail "the scan could not run over this repository (exit $REPO_STATUS)" ;;
esac

# One committed tree that is RIGHT on purpose. Without it, "refuses a raw
# colour" and "refuses every stylesheet" leave identical evidence.
expect_green() {
    local name="$1"
    CHECKED+=("$name")
    local dir="$DEMOS/$name"
    if [ ! -d "$dir" ]; then
        fail "$name: no such demonstration under tools/styling-demonstrations"
        return
    fi
    local out status
    set +e
    out="$("$CHORUS_NODE" "$SCAN" --fixture "$dir" 2>&1)"
    status=$?
    set -e
    say ""
    say "--- $name (exit $status, wanted 0)"
    printf '%s\n' "$out" | sed 's/^/    /'
    if [ "$status" -ne 0 ]; then
        fail "$name exited $status and 0 was wanted; the scanner refuses a tree that holds every rule"
        return
    fi
    say "pass $name is accepted, so the scan discriminates rather than refusing every tree"
}

# One committed tree that breaks exactly one rule. It has to go red, naming that
# rule and nothing else: a tree that went red for two reasons would leave the
# second one unproved and the first one unlocated.
expect_red() {
    local name="$1" want_rule="$2"
    CHECKED+=("$name")
    local dir="$DEMOS/$name"
    if [ ! -d "$dir" ]; then
        fail "$name: no such demonstration under tools/styling-demonstrations"
        return
    fi

    local out status
    set +e
    out="$("$CHORUS_NODE" "$SCAN" --fixture "$dir" 2>&1)"
    status=$?
    set -e

    say ""
    say "--- $name (exit $status, wanted 2, naming $want_rule)"
    printf '%s\n' "$out" | sed 's/^/    /'

    local red=1
    if [ "$status" -ne 2 ]; then
        fail "$name exited $status and 2 was wanted; a demonstration that does not go red is not a demonstration"
        red=0
    fi
    if ! printf '%s\n' "$out" | grep -qE "^FAIL $want_rule "; then
        fail "$name did not go red naming $want_rule, so a reader is not told which rule of $CONVENTIONS was broken"
        red=0
    fi
    local others
    others="$(printf '%s\n' "$out" | grep -E '^FAIL ' | awk '{print $2}' | sort -u | grep -v "^$want_rule$" || true)"
    if [ -n "$others" ]; then
        fail "$name went red on more than the one rule it breaks: $(printf '%s' "$others" | tr '\n' ' ')"
        red=0
    fi
    if [ "$red" -eq 1 ]; then
        say "pass $name went red naming $want_rule and nothing else"
        REDS=$(( REDS + 1 ))
        REDDED+=("$want_rule")
    fi
}

say ""
say "=== chorus: every shape the scan refuses, committed and shown going red ======="

expect_red a-token-missing-from-one-theme        role-vocabulary
expect_red a-raw-colour-in-the-page              no-colour-literals
expect_red a-length-off-the-scale                spacing-scale
expect_red a-length-written-inline               spacing-scale
expect_red a-token-nothing-declares              tokens-resolve
expect_red a-theme-derived-from-the-other        hand-authored-themes
expect_red a-token-file-that-does-not-parse      tokens-parse
expect_red a-surface-that-names-a-primitive      three-tiers
expect_red a-second-hue-outside-the-states       one-accent-hue
expect_red a-raised-surface                      border-and-surface-separation
expect_red motion-under-an-exemption             exemptions-stay-true
expect_red a-comment-that-denies-the-annotation  decision-record
expect_red no-decision-on-record                 decision-record
expect_red a-clause-with-two-dispositions        clause-record
expect_red a-clause-mapped-to-nothing            clause-record

say ""
say "=== chorus: and the one tree that is supposed to pass ========================="

expect_green base

# --- the demonstrations are enough of them, and the list is the whole list ----

say ""
say "--- every rule the scan runs was shown going red"
RULES_RUN="$(printf '%s\n' "$REPO_OUT" | sed -n 's/^ran: //p' | tr ',' '\n' | sed '/^$/d' | sort -u)"
RULES_RED="$(printf '%s\n' "${REDDED[@]:-}" | sed '/^$/d' | sort -u)"
NEVER_RED="$(comm -23 <(printf '%s\n' "$RULES_RUN") <(printf '%s\n' "$RULES_RED") || true)"
if [ -n "$NEVER_RED" ]; then
    say "FAIL these rules run and no committed tree has been seen to break them:"
    printf '%s\n' "$NEVER_RED" | sed 's/^/    /'
    FAILURES=$(( FAILURES + 1 ))
else
    say "pass all $(printf '%s\n' "$RULES_RUN" | wc -l | tr -d ' ') rules the scan runs have a committed tree that goes red on them"
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
    say "chorus: the stylesheet sources hold S1 to S10, and the check that says so went red on all $REDS shapes it refuses"
    exit 0
fi

say "chorus: $FAILURES styling checks did not hold"
case "$REPO_STATUS" in
    2) exit 2 ;;
    3) exit 3 ;;
esac
exit 4
