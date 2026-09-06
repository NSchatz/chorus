/* A test harness small enough to read in one sitting.
 *
 * No framework, because a framework would be a dependency and the endpoint's
 * whole verification story is that it needs a C compiler and nothing else.
 * Every check prints its own line, so a CI log carries the measured values and
 * not only the fact that an assertion held. */

#ifndef CHORUS_TEST_HARNESS_H
#define CHORUS_TEST_HARNESS_H

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int chorus_test_failures;
static int chorus_test_checks;

static void chorus_check(int ok, const char *fmt, ...)
{
    chorus_test_checks++;
    char message[1024];
    va_list args;
    va_start(args, fmt);
    vsnprintf(message, sizeof(message), fmt, args);
    va_end(args);
    if (ok) {
        printf("pass %s\n", message);
    } else {
        printf("FAIL %s\n", message);
        chorus_test_failures++;
    }
}

static void chorus_section(const char *name)
{
    printf("\n--- %s\n", name);
}

static int chorus_test_report(const char *suite)
{
    printf("\n%s: %d checks, %d failed\n", suite, chorus_test_checks, chorus_test_failures);
    return chorus_test_failures == 0 ? 0 : 1;
}

/* The repository root, handed in by the Makefile so a test can find the
 * committed fixtures without guessing where it was run from. */
#ifndef CHORUS_REPO_ROOT
#define CHORUS_REPO_ROOT "."
#endif

/* Marked used because not every suite needs a fixture, and an unused static in
 * a header is not a defect. */
__attribute__((unused)) static void chorus_repo_path(char *out, size_t out_len,
                                                     const char *relative)
{
    snprintf(out, out_len, "%s/%s", CHORUS_REPO_ROOT, relative);
}

#endif /* CHORUS_TEST_HARNESS_H */
