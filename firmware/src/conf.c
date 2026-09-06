#include "chorus/conf.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void detail_set(char *detail, size_t detail_len, const char *fmt, ...);

#include <stdarg.h>

static void detail_set(char *detail, size_t detail_len, const char *fmt, ...)
{
    if (detail == NULL || detail_len == 0) {
        return;
    }
    va_list args;
    va_start(args, fmt);
    vsnprintf(detail, detail_len, fmt, args);
    va_end(args);
}

const char *chorus_conf_status_name(chorus_conf_status_t status)
{
    switch (status) {
    case CHORUS_CONF_OK:
        return "ok";
    case CHORUS_CONF_ERR_UNREADABLE:
        return "unreadable";
    case CHORUS_CONF_ERR_MALFORMED_LINE:
        return "malformed-line";
    case CHORUS_CONF_ERR_DUPLICATE_KEY:
        return "duplicate-key";
    case CHORUS_CONF_ERR_TOO_MANY_PAIRS:
        return "too-many-pairs";
    case CHORUS_CONF_ERR_LINE_TOO_LONG:
        return "line-too-long";
    case CHORUS_CONF_ERR_MISSING_KEY:
        return "missing-key";
    case CHORUS_CONF_ERR_BAD_VALUE:
        return "bad-value";
    }
    return "unknown-status";
}

static const char *skip_space(const char *p)
{
    while (*p == ' ' || *p == '\t') {
        p++;
    }
    return p;
}

/* Copy `len` bytes of `src` into `dst`, trimming trailing blanks. Refuses a
 * value that does not fit rather than truncating it. */
static int copy_trimmed(char *dst, size_t dst_len, const char *src, size_t len)
{
    while (len > 0 && (src[len - 1] == ' ' || src[len - 1] == '\t' || src[len - 1] == '\r')) {
        len--;
    }
    if (len + 1 > dst_len) {
        return -1;
    }
    memcpy(dst, src, len);
    dst[len] = '\0';
    return 0;
}

chorus_conf_status_t chorus_conf_parse(chorus_conf_t *out, const char *path, const char *text,
                                       char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    if (path != NULL) {
        snprintf(out->path, sizeof(out->path), "%s", path);
    }

    size_t line_no = 0;
    const char *cursor = text;
    while (*cursor != '\0') {
        line_no++;
        const char *eol = strchr(cursor, '\n');
        size_t raw_len = (eol == NULL) ? strlen(cursor) : (size_t)(eol - cursor);
        const char *line = cursor;
        cursor = (eol == NULL) ? cursor + raw_len : eol + 1;

        /* A comment starts at the first '#'. No configuration value in this
         * tree carries one, so this needs no string tracking. */
        const char *hash = memchr(line, '#', raw_len);
        size_t len = (hash == NULL) ? raw_len : (size_t)(hash - line);

        const char *start = line;
        while (len > 0 && (*start == ' ' || *start == '\t')) {
            start++;
            len--;
        }
        while (len > 0 && (start[len - 1] == ' ' || start[len - 1] == '\t' || start[len - 1] == '\r')) {
            len--;
        }
        if (len == 0) {
            continue;
        }

        const char *eq = memchr(start, '=', len);
        if (eq == NULL || eq == start) {
            detail_set(detail, detail_len, "%s line %zu: not `key = value`", out->path, line_no);
            return CHORUS_CONF_ERR_MALFORMED_LINE;
        }

        if (out->count == CHORUS_CONF_MAX_PAIRS) {
            detail_set(detail, detail_len, "%s carries more than %d pairs", out->path,
                       CHORUS_CONF_MAX_PAIRS);
            return CHORUS_CONF_ERR_TOO_MANY_PAIRS;
        }

        chorus_conf_pair_t *pair = &out->pairs[out->count];
        if (copy_trimmed(pair->key, sizeof(pair->key), start, (size_t)(eq - start)) != 0) {
            detail_set(detail, detail_len, "%s line %zu: key is longer than %d bytes", out->path,
                       line_no, CHORUS_CONF_MAX_TEXT - 1);
            return CHORUS_CONF_ERR_LINE_TOO_LONG;
        }
        const char *value_start = skip_space(eq + 1);
        size_t value_len = len - (size_t)(value_start - start);
        if (copy_trimmed(pair->value, sizeof(pair->value), value_start, value_len) != 0) {
            detail_set(detail, detail_len, "%s line %zu: value is longer than %d bytes", out->path,
                       line_no, CHORUS_CONF_MAX_TEXT - 1);
            return CHORUS_CONF_ERR_LINE_TOO_LONG;
        }

        for (size_t i = 0; i < out->count; i++) {
            if (strcmp(out->pairs[i].key, pair->key) == 0) {
                detail_set(detail, detail_len, "%s line %zu: key %s appears twice", out->path,
                           line_no, pair->key);
                return CHORUS_CONF_ERR_DUPLICATE_KEY;
            }
        }
        out->count++;
    }
    return CHORUS_CONF_OK;
}

chorus_conf_status_t chorus_conf_load(chorus_conf_t *out, const char *path, char *detail,
                                      size_t detail_len)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        detail_set(detail, detail_len, "%s could not be opened: %s", path, strerror(errno));
        memset(out, 0, sizeof(*out));
        return CHORUS_CONF_ERR_UNREADABLE;
    }
    if (fseek(file, 0, SEEK_END) != 0) {
        fclose(file);
        detail_set(detail, detail_len, "%s is not seekable", path);
        return CHORUS_CONF_ERR_UNREADABLE;
    }
    long size = ftell(file);
    if (size < 0) {
        fclose(file);
        detail_set(detail, detail_len, "%s has no length", path);
        return CHORUS_CONF_ERR_UNREADABLE;
    }
    rewind(file);
    char *text = malloc((size_t)size + 1);
    if (text == NULL) {
        fclose(file);
        detail_set(detail, detail_len, "%s does not fit in memory", path);
        return CHORUS_CONF_ERR_UNREADABLE;
    }
    size_t read = fread(text, 1, (size_t)size, file);
    fclose(file);
    text[read] = '\0';

    chorus_conf_status_t status = chorus_conf_parse(out, path, text, detail, detail_len);
    free(text);
    return status;
}

const char *chorus_conf_get(const chorus_conf_t *conf, const char *key)
{
    for (size_t i = 0; i < conf->count; i++) {
        if (strcmp(conf->pairs[i].key, key) == 0) {
            return conf->pairs[i].value;
        }
    }
    return NULL;
}

int chorus_conf_is_unknown(const chorus_conf_t *conf, const char *key)
{
    const char *value = chorus_conf_get(conf, key);
    return value != NULL && strcmp(value, CHORUS_CONF_UNKNOWN) == 0;
}

static const char *require(const chorus_conf_t *conf, const char *key, char *detail,
                           size_t detail_len, chorus_conf_status_t *status)
{
    const char *value = chorus_conf_get(conf, key);
    if (value == NULL) {
        detail_set(detail, detail_len, "%s has no %s", conf->path, key);
        *status = CHORUS_CONF_ERR_MISSING_KEY;
        return NULL;
    }
    *status = CHORUS_CONF_OK;
    return value;
}

chorus_conf_status_t chorus_conf_u32(const chorus_conf_t *conf, const char *key, uint32_t *out,
                                     char *detail, size_t detail_len)
{
    chorus_conf_status_t status;
    const char *value = require(conf, key, detail, detail_len, &status);
    if (value == NULL) {
        return status;
    }
    char *end = NULL;
    errno = 0;
    unsigned long parsed = strtoul(value, &end, 10);
    if (errno != 0 || end == value || *end != '\0' || parsed > 0xFFFFFFFFUL) {
        detail_set(detail, detail_len, "%s: %s = %s is not a 32-bit unsigned number", conf->path,
                   key, value);
        return CHORUS_CONF_ERR_BAD_VALUE;
    }
    *out = (uint32_t)parsed;
    return CHORUS_CONF_OK;
}

chorus_conf_status_t chorus_conf_u8(const chorus_conf_t *conf, const char *key, uint8_t *out,
                                    char *detail, size_t detail_len)
{
    uint32_t wide = 0;
    chorus_conf_status_t status = chorus_conf_u32(conf, key, &wide, detail, detail_len);
    if (status != CHORUS_CONF_OK) {
        return status;
    }
    if (wide > 0xFF) {
        detail_set(detail, detail_len, "%s: %s = %u does not fit in a byte", conf->path, key, wide);
        return CHORUS_CONF_ERR_BAD_VALUE;
    }
    *out = (uint8_t)wide;
    return CHORUS_CONF_OK;
}

chorus_conf_status_t chorus_conf_f64(const chorus_conf_t *conf, const char *key, double *out,
                                     char *detail, size_t detail_len)
{
    chorus_conf_status_t status;
    const char *value = require(conf, key, detail, detail_len, &status);
    if (value == NULL) {
        return status;
    }
    char *end = NULL;
    errno = 0;
    double parsed = strtod(value, &end);
    if (errno != 0 || end == value || *end != '\0') {
        detail_set(detail, detail_len, "%s: %s = %s is not a number", conf->path, key, value);
        return CHORUS_CONF_ERR_BAD_VALUE;
    }
    *out = parsed;
    return CHORUS_CONF_OK;
}

chorus_conf_status_t chorus_conf_bool(const chorus_conf_t *conf, const char *key, int *out,
                                      char *detail, size_t detail_len)
{
    chorus_conf_status_t status;
    const char *value = require(conf, key, detail, detail_len, &status);
    if (value == NULL) {
        return status;
    }
    if (strcmp(value, "yes") == 0) {
        *out = 1;
        return CHORUS_CONF_OK;
    }
    if (strcmp(value, "no") == 0) {
        *out = 0;
        return CHORUS_CONF_OK;
    }
    detail_set(detail, detail_len, "%s: %s = %s is not `yes` or `no`", conf->path, key, value);
    return CHORUS_CONF_ERR_BAD_VALUE;
}

chorus_conf_status_t chorus_conf_string(const chorus_conf_t *conf, const char *key, char *out,
                                        size_t out_len, char *detail, size_t detail_len)
{
    chorus_conf_status_t status;
    const char *value = require(conf, key, detail, detail_len, &status);
    if (value == NULL) {
        return status;
    }
    if (strlen(value) + 1 > out_len) {
        detail_set(detail, detail_len, "%s: %s is longer than %zu bytes", conf->path, key,
                   out_len - 1);
        return CHORUS_CONF_ERR_BAD_VALUE;
    }
    snprintf(out, out_len, "%s", value);
    return CHORUS_CONF_OK;
}
