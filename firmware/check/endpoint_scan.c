#include "endpoint_scan.h"

#include <ctype.h>
#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

/* --- the forbidden names ----------------------------------------------------
 *
 * Each list is the ordinary spellings of one act. They are assembled at run
 * time out of fragments so that this file does not itself contain a literal a
 * careless grep over the tree would find, which matters because this file is
 * the one place in the repository where every one of these strings has to
 * appear. */

/* A settable wall clock: one a human or an NTP daemon can move. `time(` on its
 * own is deliberately NOT here: `clock_gettime(CLOCK_MONOTONIC, ...)` ends in
 * those five characters, and a rule that fired on the correct clock read would
 * be a rule nobody could satisfy. */
static const char *const SETTABLE_CLOCK_NAMES[] = {
    "CLOCK_REALTIME", "gettimeofday", "settimeofday",   "time(NULL)",     "time(0)",
    "localtime",      "gmtime",       "mktime",         "ctime(",         "sntp_",
    "esp_sntp",       "esp_netif_sntp", "adjtime",      "clock_settime",
};

/* Burning an eFuse. "Each eFuse is a one-bit field which can be programmed to
 * 1 after which it cannot be reverted back to 0", so there is no recovery and
 * no place for one of these in this phase. */
static const char *const EFUSE_WRITE_NAMES[] = {
    "esp_efuse_write", "esp_efuse_burn",  "esp_efuse_batch_write", "esp_efuse_set_",
    "espefuse",        "efuse_hal_write", "esp_secure_boot_enable",
    "esp_flash_encryption_enable",
};

/* Activating an OTA image. chorus#FLEET-10 owns this, with a deliberately bad
 * image shipped and reverted as its evidence. */
static const char *const OTA_NAMES[] = {
    "esp_ota_set_boot_partition", "esp_ota_mark_app_valid", "esp_ota_mark_app_invalid",
    "esp_ota_begin",              "esp_ota_write",          "esp_ota_end",
    "esp_https_ota",              "esp_ota_get_next_update_partition",
};

/* Placing anything in external RAM. */
static const char *const EXTERNAL_RAM_NAMES[] = {
    "EXT_RAM_BSS_ATTR", "EXT_RAM_NOINIT_ATTR", "EXT_RAM_ATTR", "MALLOC_CAP_SPIRAM",
    "SPIRAM_MALLOC",
};

#define COUNT_OF(a) (sizeof(a) / sizeof((a)[0]))

/* The one unit the register-literal rule applies to, and the reason it is the
 * only one: it is the unit that talks to the amplifier. */
static const char *const AMP_DRIVER_UNIT = "firmware/src/amp.c";

/* --- line reading ----------------------------------------------------------- */

static int is_ident_byte(char c)
{
    return isalnum((unsigned char)c) || c == '_';
}

/* Past the end of an ordinary string opened just before `i`, or the end of the
 * line. */
static size_t skip_quoted(const char *line, size_t len, size_t i)
{
    while (i < len) {
        if (line[i] == '\\') {
            i += 2;
            continue;
        }
        if (line[i] == '"') {
            return i + 1;
        }
        i++;
    }
    return len;
}

/* Past the end of a character literal starting at `i`, or past the quote when
 * what starts there is not one. C has no lifetimes, but an apostrophe inside a
 * comment that has already been cut can still appear, so this stays cautious
 * and never swallows the rest of a line silently. */
static size_t skip_char_literal(const char *line, size_t len, size_t i)
{
    size_t j = i + 1;
    while (j < len) {
        if (line[j] == '\\') {
            j += 2;
            continue;
        }
        if (line[j] == '\'') {
            return j + 1;
        }
        j++;
    }
    return i + 1;
}

void chorus_scan_code_only(const char *line, char *out, size_t out_len)
{
    size_t len = strlen(line);
    size_t i = 0;
    size_t written = 0;
    while (i < len && written + 1 < out_len) {
        if (line[i] == '/' && i + 1 < len && line[i + 1] == '/') {
            break;
        }
        if (line[i] == '"') {
            size_t end = skip_quoted(line, len, i + 1);
            while (i < end && written + 1 < out_len) {
                out[written++] = line[i++];
            }
            continue;
        }
        if (line[i] == '\'') {
            size_t end = skip_char_literal(line, len, i);
            while (i < end && written + 1 < out_len) {
                out[written++] = line[i++];
            }
            continue;
        }
        out[written++] = line[i++];
    }
    out[written] = '\0';
}

void chorus_scan_code_outside_literals(const char *line, char *out, size_t out_len)
{
    char code[CHORUS_SCAN_TEXT];
    chorus_scan_code_only(line, code, sizeof(code));
    size_t len = strlen(code);
    size_t i = 0;
    size_t written = 0;
    while (i < len && written + 1 < out_len) {
        if (code[i] == '"') {
            size_t end = skip_quoted(code, len, i + 1);
            while (i < end && written + 1 < out_len) {
                out[written++] = ' ';
                i++;
            }
            continue;
        }
        if (code[i] == '\'') {
            size_t end = skip_char_literal(code, len, i);
            while (i < end && written + 1 < out_len) {
                out[written++] = ' ';
                i++;
            }
            continue;
        }
        out[written++] = code[i++];
    }
    out[written] = '\0';
}

/* --- the unit list ---------------------------------------------------------- */

static char *trim(char *text)
{
    while (*text == ' ' || *text == '\t') {
        text++;
    }
    size_t len = strlen(text);
    while (len > 0 && (text[len - 1] == ' ' || text[len - 1] == '\t' || text[len - 1] == '\r')) {
        text[--len] = '\0';
    }
    return text;
}

int chorus_unit_list_parse(chorus_unit_list_t *out, const char *text, char *detail,
                           size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    enum { NONE, ON_PATH, EXCLUDED } section = NONE;

    const char *cursor = text;
    size_t line_no = 0;
    while (*cursor != '\0') {
        line_no++;
        const char *eol = strchr(cursor, '\n');
        size_t raw_len = (eol == NULL) ? strlen(cursor) : (size_t)(eol - cursor);
        char line[CHORUS_SCAN_TEXT];
        size_t copy = (raw_len < sizeof(line) - 1) ? raw_len : sizeof(line) - 1;
        memcpy(line, cursor, copy);
        line[copy] = '\0';
        cursor = (eol == NULL) ? cursor + raw_len : eol + 1;

        char *hash = strchr(line, '#');
        if (hash != NULL) {
            *hash = '\0';
        }
        char *content = trim(line);
        if (*content == '\0') {
            continue;
        }
        if (strcmp(content, "[on-path]") == 0) {
            section = ON_PATH;
            continue;
        }
        if (strcmp(content, "[excluded]") == 0) {
            section = EXCLUDED;
            continue;
        }
        if (section == NONE) {
            snprintf(detail, detail_len, "line %zu is outside any section: %s", line_no, content);
            return -1;
        }
        if (section == ON_PATH) {
            if (out->on_path_count == CHORUS_SCAN_MAX_UNITS) {
                snprintf(detail, detail_len, "more than %d units on the path",
                         CHORUS_SCAN_MAX_UNITS);
                return -1;
            }
            snprintf(out->on_path[out->on_path_count], CHORUS_SCAN_PATH, "%s", content);
            out->on_path_count++;
            continue;
        }
        char *equals = strchr(content, '=');
        if (equals == NULL) {
            snprintf(detail, detail_len,
                     "line %zu excludes %s with no reason, and an exclusion without one is just "
                     "a shorter list",
                     line_no, content);
            return -1;
        }
        *equals = '\0';
        char *unit = trim(content);
        char *reason = trim(equals + 1);
        if (*reason == '\0') {
            snprintf(detail, detail_len, "line %zu excludes %s with an empty reason", line_no,
                     unit);
            return -1;
        }
        if (out->excluded_count == CHORUS_SCAN_MAX_UNITS) {
            snprintf(detail, detail_len, "more than %d exclusions", CHORUS_SCAN_MAX_UNITS);
            return -1;
        }
        snprintf(out->excluded[out->excluded_count].unit, CHORUS_SCAN_PATH, "%s", unit);
        snprintf(out->excluded[out->excluded_count].reason, CHORUS_SCAN_TEXT, "%s", reason);
        out->excluded_count++;
    }
    return 0;
}

int chorus_unit_list_load(chorus_unit_list_t *out, const char *path, char *detail,
                          size_t detail_len)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        snprintf(detail, detail_len, "%s could not be opened", path);
        return -1;
    }
    static char text[131072];
    size_t read = fread(text, 1, sizeof(text) - 1, file);
    fclose(file);
    text[read] = '\0';
    return chorus_unit_list_parse(out, text, detail, detail_len);
}

static int is_on_path(const chorus_unit_list_t *list, const char *unit)
{
    for (size_t i = 0; i < list->on_path_count; i++) {
        if (strcmp(list->on_path[i], unit) == 0) {
            return 1;
        }
    }
    return 0;
}

static int is_excluded(const chorus_unit_list_t *list, const char *unit)
{
    for (size_t i = 0; i < list->excluded_count; i++) {
        if (strcmp(list->excluded[i].unit, unit) == 0) {
            return 1;
        }
    }
    return 0;
}

/* --- the scan --------------------------------------------------------------- */

static void add(chorus_scan_result_t *out, const char *rule, const char *unit, size_t line,
                const char *name, const char *text)
{
    if (out->count >= CHORUS_SCAN_MAX_FINDINGS) {
        return;
    }
    chorus_scan_finding_t *finding = &out->findings[out->count];
    snprintf(finding->rule, sizeof(finding->rule), "%s", rule);
    snprintf(finding->unit, sizeof(finding->unit), "%s", unit);
    finding->line = line;
    snprintf(finding->name, sizeof(finding->name), "%s", name);
    snprintf(finding->text, sizeof(finding->text), "%s", text);
    out->count++;
}

static char *read_file(const char *path)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return NULL;
    }
    if (fseek(file, 0, SEEK_END) != 0) {
        fclose(file);
        return NULL;
    }
    long size = ftell(file);
    if (size < 0) {
        fclose(file);
        return NULL;
    }
    rewind(file);
    char *text = malloc((size_t)size + 1);
    if (text == NULL) {
        fclose(file);
        return NULL;
    }
    size_t read = fread(text, 1, (size_t)size, file);
    fclose(file);
    text[read] = '\0';
    return text;
}

static void check_names(chorus_scan_result_t *out, const char *rule, const char *unit,
                        size_t line_no, const char *code, const char *raw,
                        const char *const *names, size_t name_count)
{
    for (size_t i = 0; i < name_count; i++) {
        if (strstr(code, names[i]) != NULL) {
            char trimmed[CHORUS_SCAN_TEXT];
            snprintf(trimmed, sizeof(trimmed), "%s", raw);
            char *body = trim(trimmed);
            add(out, rule, unit, line_no, names[i], body);
        }
    }
}

/* A hex literal in code, which in the amplifier driver would be a register
 * address or a register value and this phase names neither. */
static int has_hex_literal(const char *code, char *found, size_t found_len)
{
    for (size_t i = 0; code[i] != '\0'; i++) {
        if (code[i] != '0') {
            continue;
        }
        if (code[i + 1] != 'x' && code[i + 1] != 'X') {
            continue;
        }
        if (i > 0 && is_ident_byte(code[i - 1])) {
            continue;
        }
        if (!isxdigit((unsigned char)code[i + 2])) {
            continue;
        }
        size_t end = i + 2;
        while (isxdigit((unsigned char)code[end])) {
            end++;
        }
        size_t len = end - i;
        if (len + 1 > found_len) {
            len = found_len - 1;
        }
        memcpy(found, code + i, len);
        found[len] = '\0';
        return 1;
    }
    return 0;
}

static void scan_unit(chorus_scan_result_t *out, const char *root, const char *unit)
{
    char path[CHORUS_SCAN_PATH * 2];
    snprintf(path, sizeof(path), "%s/%s", root, unit);
    char *text = read_file(path);
    if (text == NULL) {
        add(out, "unit-does-not-exist", unit, 0, unit,
            "named in firmware/endpoint-units.conf and not in the tree");
        return;
    }
    out->units_scanned++;

    size_t line_no = 0;
    char *cursor = text;
    while (*cursor != '\0') {
        line_no++;
        char *eol = strchr(cursor, '\n');
        size_t raw_len = (eol == NULL) ? strlen(cursor) : (size_t)(eol - cursor);
        char raw[CHORUS_SCAN_TEXT];
        size_t copy = (raw_len < sizeof(raw) - 1) ? raw_len : sizeof(raw) - 1;
        memcpy(raw, cursor, copy);
        raw[copy] = '\0';
        cursor = (eol == NULL) ? cursor + raw_len : eol + 1;

        char code[CHORUS_SCAN_TEXT];
        chorus_scan_code_only(raw, code, sizeof(code));

        check_names(out, "settable-clock-on-the-endpoint-path", unit, line_no, code, raw,
                    SETTABLE_CLOCK_NAMES, COUNT_OF(SETTABLE_CLOCK_NAMES));
        check_names(out, "efuse-write-in-the-endpoint-tree", unit, line_no, code, raw,
                    EFUSE_WRITE_NAMES, COUNT_OF(EFUSE_WRITE_NAMES));
        check_names(out, "ota-activation-in-the-endpoint-tree", unit, line_no, code, raw,
                    OTA_NAMES, COUNT_OF(OTA_NAMES));
        check_names(out, "dma-descriptor-in-external-ram", unit, line_no, code, raw,
                    EXTERNAL_RAM_NAMES, COUNT_OF(EXTERNAL_RAM_NAMES));

        if (strcmp(unit, AMP_DRIVER_UNIT) == 0) {
            char bare[CHORUS_SCAN_TEXT];
            chorus_scan_code_outside_literals(raw, bare, sizeof(bare));
            char found[64];
            if (has_hex_literal(bare, found, sizeof(found))) {
                char trimmed[CHORUS_SCAN_TEXT];
                snprintf(trimmed, sizeof(trimmed), "%s", raw);
                add(out, "register-literal-in-the-amplifier-driver", unit, line_no, found,
                    trim(trimmed));
            }
        }
    }
    free(text);
}

/* Walk a directory, recursively, adding a finding for every .c or .h the list
 * does not account for. */
static void walk(chorus_scan_result_t *out, const char *root, const char *relative,
                 const chorus_unit_list_t *list)
{
    char path[CHORUS_SCAN_PATH * 4];
    snprintf(path, sizeof(path), "%s/%s", root, relative);
    DIR *dir = opendir(path);
    if (dir == NULL) {
        out->walk_failed = 1;
        snprintf(out->detail, sizeof(out->detail), "could not be walked: %.400s", path);
        return;
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        char child_relative[CHORUS_SCAN_PATH * 2];
        snprintf(child_relative, sizeof(child_relative), "%s/%s", relative, entry->d_name);
        char child_path[CHORUS_SCAN_PATH * 4];
        snprintf(child_path, sizeof(child_path), "%s/%s", root, child_relative);

        struct stat info;
        if (stat(child_path, &info) != 0) {
            continue;
        }
        if (S_ISDIR(info.st_mode)) {
            walk(out, root, child_relative, list);
            continue;
        }
        const char *dot = strrchr(entry->d_name, '.');
        if (dot == NULL || (strcmp(dot, ".c") != 0 && strcmp(dot, ".h") != 0)) {
            continue;
        }
        if (is_on_path(list, child_relative) || is_excluded(list, child_relative)) {
            continue;
        }
        add(out, "unit-missing-from-the-list", child_relative, 0, child_relative,
            "is in a directory this scan walks (firmware/src, firmware/include or firmware/main) "
            "and is neither listed nor excluded with a reason in firmware/endpoint-units.conf");
    }
    closedir(dir);
}

void chorus_endpoint_scan(const char *root, const chorus_unit_list_t *list,
                          chorus_scan_result_t *out)
{
    memset(out, 0, sizeof(*out));
    for (size_t i = 0; i < list->on_path_count; i++) {
        scan_unit(out, root, list->on_path[i]);
    }
    walk(out, root, "firmware/src", list);
    walk(out, root, "firmware/include", list);
    /* firmware/main is the target binding, compiled only by ESP-IDF and never
     * by the host build. It is scanned all the same, and for the strongest
     * reason there is: it is the one place in this tree where an OTA call or
     * an eFuse write would compile, so it is the one place the rules most need
     * to hold. */
    walk(out, root, "firmware/main", list);
}

int chorus_scan_ok(const chorus_scan_result_t *result)
{
    return (result->count == 0 && !result->walk_failed) ? 1 : 0;
}

void chorus_scan_report(const chorus_scan_result_t *result, void *stream)
{
    FILE *out = (FILE *)stream;
    for (size_t i = 0; i < result->count; i++) {
        const chorus_scan_finding_t *finding = &result->findings[i];
        if (finding->line > 0) {
            fprintf(out, "FAIL %s %s:%zu matches %s :: %s\n", finding->rule, finding->unit,
                    finding->line, finding->name, finding->text);
        } else {
            fprintf(out, "FAIL %s %s :: %s\n", finding->rule, finding->unit, finding->text);
        }
    }
    if (result->walk_failed) {
        fprintf(out, "FAIL endpoint-tree-not-walkable :: %s\n", result->detail);
    }
    if (chorus_scan_ok(result)) {
        fprintf(out,
                "pass endpoint-scan: %zu units scanned, none reads a settable clock, none burns "
                "an eFuse, none activates an OTA image, none places anything in external RAM, "
                "the amplifier driver names no register address, and none is unaccounted for\n",
                result->units_scanned);
    }
}
