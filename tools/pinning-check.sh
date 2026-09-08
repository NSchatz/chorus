#!/usr/bin/env bash
# The pinning gate: this repository, then the demonstrations that prove the
# gate can go red.
#
# Judged against the umbrella's pinning conventions, documentation/pinning-conventions.md,
# clauses P1 to P8. tools/pinning-scan.sh is the scanner; this is what
# `make verify` and CI run.
#
#   ./tools/pinning-check.sh      # or: make verify-pinning
#
# WHY THE DEMONSTRATIONS ARE COMMITTED. A check that has never been seen to
# fail is not evidence, it is a formality: every text scanner in this tree got
# that way by being shown the thing it was supposed to catch. So each of the
# four shapes the conventions name is committed under
# tools/pinning-demonstrations/ as a small tree, and each is scanned here and
# required to go red naming the right clause. If fewer than four go red, this
# exits non-zero, which is the only way "the check works" stays true after
# someone edits the scanner.
#
# A fifth demonstration covers the empty-category refusal: a tree in which a
# category the scanner examines finds nothing at all has to be refused, because
# a pattern that stopped matching and a compliant tree are the same green
# otherwise.
#
# NO NETWORK. Nothing here resolves a tag, opens a socket or asks a registry
# anything, so it reaches the same verdict on a machine with no route out. That
# is P8's side of the bargain and the reason this repository adds no scheduled
# liveness workflow: rot shows up when a build fails, and P7 is what makes the
# failure legible when it does.
#
# EXIT CODES, distinct per failure mode (P7):
#
#   0   the tree is pinned and every demonstration went red as required
#   2   an unpinned reference in this repository
#   3   a category of this repository examined no references at all
#   4   a demonstration did not go red, or fewer than four of them did
#   5   a capability this check needs is missing

source "$(dirname "$0")/lib.sh"

# Deliberately NOT build_once: this check reads files and asks git what is
# tracked. It compiles nothing, so it stays runnable and fast on a tree that
# does not build, which is when a bad pin is most likely to be the reason.

SCAN="$REPO_ROOT/tools/pinning-scan.sh"
DEMOS="$REPO_ROOT/tools/pinning-demonstrations"
CONVENTIONS='documentation/pinning-conventions.md'

FAILURES=0
REDS=0
CHECKED=()

fail() {
    say "FAIL $*"
    FAILURES=$(( FAILURES + 1 ))
}

say ""
say "=== chorus: this repository, every pinnable reference ========================"
say ""

set +e
REPO_OUT="$(bash "$SCAN" "$REPO_ROOT" 2>&1)"
REPO_STATUS=$?
set -e
printf '%s\n' "$REPO_OUT" | sed 's/^/    /'

case "$REPO_STATUS" in
    0) say "pass this repository is pinned in every category" ;;
    2) fail "this repository carries an unpinned reference (exit 2); see the report above and $CONVENTIONS" ;;
    3) fail "a category of this repository examined nothing at all (exit 3); the scanner has stopped looking" ;;
    *) fail "the scanner could not run over this repository (exit $REPO_STATUS)" ;;
esac

# --- the lock is the resolution, not a note about it -------------------------
#
# P4's first clause is about what a build resolves FROM. --locked makes a
# resolution that would have to change Cargo.lock a failure instead of a silent
# difference, which is the property the committed lock exists to buy; the same
# flag is on the RUN line in deploy/Dockerfile so the image build reads the same
# committed lock rather than resolving inside the image.
say ""
say "=== chorus: the Rust workspace resolves only from the committed lock ========="
say ""
set +e
LOCKED_OUT="$( (cd "$REPO_ROOT" && cargo metadata --locked --offline --format-version 1) 2>&1 )"
LOCKED_STATUS=$?
set -e
if [ "$LOCKED_STATUS" -eq 0 ]; then
    say "pass cargo metadata --locked resolved with no change to Cargo.lock"
else
    say "$LOCKED_OUT" | sed 's/^/    /'
    fail "cargo metadata --locked exited $LOCKED_STATUS: the workspace does not resolve from the committed lock"
fi

# --- the demonstrations ------------------------------------------------------

# One committed tree that is RIGHT on purpose. P4 leaves the node opt-back-in
# deliberately open, and without a tree that takes it and passes, "refuses a
# `false` with no reason" and "refuses every `false`" leave identical evidence.
expect_green() {
    local name="$1"
    CHECKED+=("$name")
    local dir="$DEMOS/$name"
    if [ ! -d "$dir" ]; then
        fail "$name: no such demonstration under tools/pinning-demonstrations"
        return
    fi

    local out status
    set +e
    out="$(bash "$SCAN" --fixture "$dir" 2>&1)"
    status=$?
    set -e

    say ""
    say "--- $name (exit $status, wanted 0)"
    printf '%s\n' "$out" | sed 's/^/    /'

    if [ "$status" -ne 0 ]; then
        fail "$name exited $status and 0 was wanted; the escape hatch P4 leaves open has been shut by accident"
        return
    fi
    say "pass $name is accepted, so the check discriminates rather than refusing every case"
}

# One committed tree that is unpinned on purpose. It has to go red, with the
# right exit code, naming the clause it broke.
expect_red() {
    local name="$1" mode="$2" want_status="$3" want_clause="$4"
    CHECKED+=("$name")
    local dir="$DEMOS/$name"
    if [ ! -d "$dir" ]; then
        fail "$name: no such demonstration under tools/pinning-demonstrations"
        return
    fi

    local out status
    set +e
    if [ "$mode" = fixture ]; then
        out="$(bash "$SCAN" --fixture "$dir" 2>&1)"
    else
        out="$(bash "$SCAN" "$dir" 2>&1)"
    fi
    status=$?
    set -e

    say ""
    say "--- $name (exit $status, wanted $want_status)"
    printf '%s\n' "$out" | sed 's/^/    /'

    local red=1 named="$want_clause"
    if [ "$status" -ne "$want_status" ]; then
        fail "$name exited $status and $want_status was wanted; a demonstration that does not go red is not a demonstration"
        red=0
    fi
    if ! printf '%s' "$out" | grep -qF "$want_clause"; then
        fail "$name did not name $want_clause, so a reader is not told which clause of $CONVENTIONS was broken"
        red=0
    fi
    if [ "$want_status" -eq 2 ]; then
        if ! printf '%s' "$out" | grep -qE '^UNPINNED [^:]+:[0-9]+$'; then
            fail "$name did not name a file and a line number"
            red=0
        fi
        named="$want_clause, its file and its line"
    fi
    if [ "$red" -eq 1 ]; then
        say "pass $name went red naming $named"
        REDS=$(( REDS + 1 ))
    fi
}

say ""
say "=== chorus: the four shapes, committed, each shown going red ================="

expect_red from-without-digest         fixture    2 P2
expect_red action-at-a-mutable-tag     fixture    2 P3
expect_red manifest-with-a-range       fixture    2 P4
expect_red lifecycle-scripts-back-on   fixture    2 P4

# The fifth shape: an image named outside a Dockerfile. This is the other half
# of the locally built exemption - deploy/run-server.sh's chorus-server:dev
# default is exempt because this repository builds it, and the moment that
# default becomes a registry reference it takes a digest like anything else.
# This tree is that moment.
expect_red image-without-digest        fixture    2 P1

say ""
say "=== chorus: and a category that examined nothing at all ======================"

expect_red a-category-went-empty       repository 3 "STOPPED LOOKING"

say ""
say "=== chorus: and the one case that is supposed to pass ========================"

expect_green lifecycle-scripts-back-on-with-a-reason

# --- the four are four, and the list is the whole list -----------------------

say ""
say "--- at least four shapes went red"
REQUIRED=4
if [ "$REDS" -lt "$REQUIRED" ]; then
    fail "$REDS demonstration(s) went red and $REQUIRED were required, so this check is a formality rather than evidence"
else
    say "pass $REDS demonstrations went red, and $REQUIRED were required"
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
    say "chorus: every image, action and dependency manifest is pinned, and the check that says so was shown going red on all $REDS of them"
    exit 0
fi

say "chorus: $FAILURES pinning checks did not hold"
case "$REPO_STATUS" in
    2) exit 2 ;;
    3) exit 3 ;;
esac
exit 4
