/* The `key = value` reader the endpoint shares with the rest of the tree.
 *
 * The same format config/sync.conf, config/verification.conf and
 * the `.cfg` scenarios under fixtures/sync already use: one `key = value` per line, `#` starts a
 * comment, whitespace around either side is insignificant. Written here rather
 * than borrowed because the endpoint has to read these files with nothing but
 * a C compiler.
 *
 * Nothing in here reads a clock, opens a socket or allocates after the file is
 * loaded. */

#ifndef CHORUS_CONF_H
#define CHORUS_CONF_H

#include <stddef.h>
#include <stdint.h>

/* Longest key or value this reader accepts. A configuration line longer than
 * this is refused rather than truncated: a truncated value is a value nobody
 * wrote. */
#define CHORUS_CONF_MAX_TEXT 256

/* Most pairs one file may carry. */
#define CHORUS_CONF_MAX_PAIRS 128

/* The literal a value carries when this phase declares it unknown. */
#define CHORUS_CONF_UNKNOWN "unknown"

typedef struct {
    char key[CHORUS_CONF_MAX_TEXT];
    char value[CHORUS_CONF_MAX_TEXT];
} chorus_conf_pair_t;

typedef struct {
    chorus_conf_pair_t pairs[CHORUS_CONF_MAX_PAIRS];
    size_t count;
    /* The path the pairs were read from, for a diagnostic that has to name
     * the file a value came from. */
    char path[CHORUS_CONF_MAX_TEXT];
} chorus_conf_t;

/* Why a file could not be read. */
typedef enum {
    CHORUS_CONF_OK = 0,
    CHORUS_CONF_ERR_UNREADABLE,
    CHORUS_CONF_ERR_MALFORMED_LINE,
    CHORUS_CONF_ERR_DUPLICATE_KEY,
    CHORUS_CONF_ERR_TOO_MANY_PAIRS,
    CHORUS_CONF_ERR_LINE_TOO_LONG,
    CHORUS_CONF_ERR_MISSING_KEY,
    CHORUS_CONF_ERR_BAD_VALUE
} chorus_conf_status_t;

/* A short stable name for a status, for diagnostics. */
const char *chorus_conf_status_name(chorus_conf_status_t status);

/* Parse `text` into `out`. `path` is remembered for diagnostics only. */
chorus_conf_status_t chorus_conf_parse(chorus_conf_t *out, const char *path, const char *text,
                                       char *detail, size_t detail_len);

/* Read and parse the file at `path`. */
chorus_conf_status_t chorus_conf_load(chorus_conf_t *out, const char *path, char *detail,
                                      size_t detail_len);

/* The raw value for `key`, or NULL when the key is absent. */
const char *chorus_conf_get(const chorus_conf_t *conf, const char *key);

/* Whether `key` is present and reads exactly `unknown`. */
int chorus_conf_is_unknown(const chorus_conf_t *conf, const char *key);

/* Typed reads. Each returns CHORUS_CONF_OK, or an error with `detail` filled
 * in naming the key, the value as written and the file. A value of `unknown`
 * is never silently coerced: it is CHORUS_CONF_ERR_BAD_VALUE here, and a
 * caller that wants to tolerate it asks with chorus_conf_is_unknown first. */
chorus_conf_status_t chorus_conf_u32(const chorus_conf_t *conf, const char *key, uint32_t *out,
                                     char *detail, size_t detail_len);
chorus_conf_status_t chorus_conf_u8(const chorus_conf_t *conf, const char *key, uint8_t *out,
                                    char *detail, size_t detail_len);
chorus_conf_status_t chorus_conf_f64(const chorus_conf_t *conf, const char *key, double *out,
                                     char *detail, size_t detail_len);
chorus_conf_status_t chorus_conf_bool(const chorus_conf_t *conf, const char *key, int *out,
                                      char *detail, size_t detail_len);
chorus_conf_status_t chorus_conf_string(const chorus_conf_t *conf, const char *key, char *out,
                                        size_t out_len, char *detail, size_t detail_len);

#endif /* CHORUS_CONF_H */
