/* Reading the committed fixture files: `.hex` (whitespace separated hex bytes,
 * `#` starts a comment) and `.fields` (one `key = value` per line, `#` starts
 * a comment, a value is the rest of the line trimmed, a byte string is hex
 * with no separators). fixtures/README.md gives the format.
 *
 * Test-only, and READ-only: nothing here writes a fixture. Every function is
 * marked used because not every suite needs every one. */

#ifndef CHORUS_FIXTURE_TEXT_H
#define CHORUS_FIXTURE_TEXT_H

#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

/* Read a whole file into `out` and NUL-terminate it. Returns its length, or
 * -1 when it cannot be read or does not fit. */
__attribute__((unused)) static long fixture_read(const char *path, char *out, size_t cap)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return -1;
    }
    size_t got = fread(out, 1, cap - 1, file);
    int more = fgetc(file) != EOF;
    fclose(file);
    if (more) {
        return -1;
    }
    out[got] = '\0';
    return (long)got;
}

/* Parse a `.hex` file's text. Returns the byte count, or -1. */
__attribute__((unused)) static long fixture_parse_hex(const char *text, uint8_t *out, size_t cap)
{
    size_t count = 0;
    const char *cursor = text;
    while (*cursor != '\0') {
        if (*cursor == '#') {
            while (*cursor != '\0' && *cursor != '\n') {
                cursor++;
            }
            continue;
        }
        if (isspace((unsigned char)*cursor)) {
            cursor++;
            continue;
        }
        if (!isxdigit((unsigned char)cursor[0]) || !isxdigit((unsigned char)cursor[1]) ||
            count >= cap) {
            return -1;
        }
        unsigned value = 0;
        if (sscanf(cursor, "%2x", &value) != 1) {
            return -1;
        }
        out[count++] = (uint8_t)value;
        cursor += 2;
    }
    return (long)count;
}

/* The value of `key` in a `.fields` text, copied into `out` and trimmed.
 * Returns `out`, or NULL when the key is absent or the value does not fit. */
__attribute__((unused)) static const char *fixture_field(const char *text, const char *key,
                                                         char *out, size_t cap)
{
    size_t key_len = strlen(key);
    const char *line = text;
    while (*line != '\0') {
        const char *eol = strchr(line, '\n');
        size_t len = (eol == NULL) ? strlen(line) : (size_t)(eol - line);
        const char *next = (eol == NULL) ? line + len : eol + 1;
        const char *hash = memchr(line, '#', len);
        if (hash != NULL) {
            len = (size_t)(hash - line);
        }
        const char *start = line;
        const char *end = line + len;
        while (start < end && (*start == ' ' || *start == '\t')) {
            start++;
        }
        if ((size_t)(end - start) > key_len && strncmp(start, key, key_len) == 0) {
            const char *after = start + key_len;
            while (after < end && (*after == ' ' || *after == '\t')) {
                after++;
            }
            if (after < end && *after == '=') {
                after++;
                while (after < end && (*after == ' ' || *after == '\t')) {
                    after++;
                }
                while (end > after && (end[-1] == ' ' || end[-1] == '\t' || end[-1] == '\r')) {
                    end--;
                }
                size_t value_len = (size_t)(end - after);
                if (value_len + 1 > cap) {
                    return NULL;
                }
                memcpy(out, after, value_len);
                out[value_len] = '\0';
                return out;
            }
        }
        line = next;
    }
    return NULL;
}

/* Unseparated hex to bytes. Returns the byte count, or -1. */
__attribute__((unused)) static long fixture_unhex(const char *hex, uint8_t *out, size_t cap)
{
    size_t len = strlen(hex);
    if (len % 2 != 0 || len / 2 > cap) {
        return -1;
    }
    for (size_t i = 0; i < len / 2; i++) {
        unsigned byte = 0;
        if (!isxdigit((unsigned char)hex[2 * i]) || !isxdigit((unsigned char)hex[2 * i + 1]) ||
            sscanf(hex + 2 * i, "%2x", &byte) != 1) {
            return -1;
        }
        out[i] = (uint8_t)byte;
    }
    return (long)(len / 2);
}

/* Bytes as unseparated lowercase hex, for a failure message. */
__attribute__((unused)) static void fixture_hex(const uint8_t *bytes, size_t len, char *out,
                                                size_t cap)
{
    size_t at = 0;
    out[0] = '\0';
    for (size_t i = 0; i < len && at + 3 < cap; i++) {
        at += (size_t)snprintf(out + at, cap - at, "%02x", bytes[i]);
    }
}

#endif /* CHORUS_FIXTURE_TEXT_H */
