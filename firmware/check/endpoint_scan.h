/* The endpoint tree's safety scans.
 *
 * Textual, and that is a deliberate limit worth stating: this scans source, so
 * a forbidden call reached through a macro this scanner cannot see is not
 * caught. What it does catch is every ordinary spelling, and every way of
 * quietly moving one off the list, which is what the check is for. The Rust
 * side says the same thing about itself in crates/audio-path/src/scan.rs, and
 * the two are deliberately the same shape.
 *
 * This lives in firmware/check/ and NOT in firmware/src/, because it is host
 * tooling and because it necessarily contains the very names it forbids. A
 * scanner inside the tree it scans would be its own first finding. */

#ifndef CHORUS_ENDPOINT_SCAN_H
#define CHORUS_ENDPOINT_SCAN_H

#include <stddef.h>

#define CHORUS_SCAN_MAX_UNITS 128
#define CHORUS_SCAN_MAX_FINDINGS 256
#define CHORUS_SCAN_PATH 256
#define CHORUS_SCAN_TEXT 512

typedef struct {
    char unit[CHORUS_SCAN_PATH];
    char reason[CHORUS_SCAN_TEXT];
} chorus_scan_excluded_t;

typedef struct {
    char on_path[CHORUS_SCAN_MAX_UNITS][CHORUS_SCAN_PATH];
    size_t on_path_count;
    chorus_scan_excluded_t excluded[CHORUS_SCAN_MAX_UNITS];
    size_t excluded_count;
} chorus_unit_list_t;

/* Parse firmware/endpoint-units.conf. Returns 0 on success. */
int chorus_unit_list_parse(chorus_unit_list_t *out, const char *text, char *detail,
                           size_t detail_len);
int chorus_unit_list_load(chorus_unit_list_t *out, const char *path, char *detail,
                          size_t detail_len);

typedef struct {
    /* The rule that fired, as a stable name. */
    char rule[64];
    char unit[CHORUS_SCAN_PATH];
    size_t line;
    /* The name that matched, or the file that is unaccounted for. */
    char name[128];
    /* The line itself, trimmed. */
    char text[CHORUS_SCAN_TEXT];
} chorus_scan_finding_t;

typedef struct {
    chorus_scan_finding_t findings[CHORUS_SCAN_MAX_FINDINGS];
    size_t count;
    size_t units_scanned;
    /* Set when the tree could not be walked at all, which is a failure and not
     * a clean scan. */
    int walk_failed;
    char detail[CHORUS_SCAN_TEXT];
} chorus_scan_result_t;

/* Run every rule against the endpoint tree rooted at `root` (a repository
 * root, or a scratch copy of one). */
void chorus_endpoint_scan(const char *root, const chorus_unit_list_t *list,
                          chorus_scan_result_t *out);

/* Whether the tree passes. */
int chorus_scan_ok(const chorus_scan_result_t *result);

/* Print every finding, or the one pass line. */
void chorus_scan_report(const chorus_scan_result_t *result, void *stream);

/* Exposed for the scanner's own unit tests: a line with its comment removed,
 * and a line with its comment removed and its string and character literals
 * blanked. Both write into `out`. */
void chorus_scan_code_only(const char *line, char *out, size_t out_len);
void chorus_scan_code_outside_literals(const char *line, char *out, size_t out_len);

#endif /* CHORUS_ENDPOINT_SCAN_H */
