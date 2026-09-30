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
    "CLOCK_REALTIME", "gettimeofday",   "settimeofday", "time(NULL)",    "time(0)",
    "localtime",      "gmtime",         "mktime",       "ctime(",        "sntp_",
    "esp_sntp",       "esp_netif_sntp", "adjtime",      "clock_settime",
};

/* Burning an eFuse. "Each eFuse is a one-bit field which can be programmed to
 * 1 after which it cannot be reverted back to 0", so there is no recovery and
 * no place for one of these in this phase.
 *
 * The first row is the original list. The rest were read out of ESP-IDF
 * v5.3.6's own headers (local copy, 2026-09-30, audit A-16) and re-read out of
 * v6.1's (commit fff9895c, local copy, 2026-09-30, chorus goal 6), which adds
 * esp_efuse_set_recovery_bootloader_offset (caught by "esp_efuse_set_") and
 * esp_flash_encryption_use_efuse_key (named below):
 * components/efuse/include/esp_efuse.h (the enable_/disable_ burns, the
 * secure-version update, destroy_block), components/efuse/private_include/
 * esp_efuse_utility.h (the burn and write helpers every public writer ends
 * in), components/bootloader_support/include/esp_flash_encrypt.h and
 * esp_secure_boot.h (the enabling calls), components/hal/esp32s3/include/hal/
 * efuse_hal.h and efuse_ll.h (the programming command, which an inlined LL
 * call reaches without any esp_efuse_ name at all), components/soc/esp32s3/
 * include/soc/efuse_reg.h and efuse_defs.h (the same, by register), and
 * components/esp_rom/esp32s3/ld/esp32s3.rom.ld (the ROM's own programmer).
 * tools/firmware-image-guard.sh checks the linked image for the same acts, so
 * a spelling this textual list cannot see still has to get past the linker's
 * record of what was actually pulled in. */
static const char *const EFUSE_WRITE_NAMES[] = {
    "esp_efuse_write",
    "esp_efuse_burn",
    "esp_efuse_batch_write",
    "esp_efuse_set_",
    "espefuse",
    "efuse_hal_write",
    "esp_secure_boot_enable",
    "esp_flash_encryption_enable",
    "esp_efuse_disable_",
    "esp_efuse_enable_",
    "esp_efuse_destroy_block",
    "esp_efuse_update_secure_version",
    "esp_efuse_utility_burn",
    "esp_efuse_utility_write",
    "esp_flash_encrypt_",
    "esp_flash_encryption_set_release_mode",
    "esp_flash_encryption_use_efuse_key",
    "esp_flash_write_protect_crypt_cnt",
    "esp_secure_boot_permanently_enable",
    "esp_secure_boot_v2_permanently_enable",
    "efuse_hal_program",
    "efuse_ll_set_pgm_cmd",
    "efuse_ll_set_conf_write_op_code",
    "EFUSE_WRITE_OP_CODE",
    "EFUSE_PGM_",
    "EFUSE.cmd",
    "EFUSE.conf",
    "EFUSE.pgm",
    "ets_efuse_program",
    "ets_efuse_write_key",
};

/* Activating an OTA image. chorus#FLEET-10 owns this, with a deliberately bad
 * image shipped and reverted as its evidence. */
static const char *const OTA_NAMES[] = {
    "esp_ota_set_boot_partition",
    "esp_ota_mark_app_valid",
    "esp_ota_mark_app_invalid",
    "esp_ota_begin",
    "esp_ota_write",
    "esp_ota_end",
    "esp_https_ota",
    "esp_ota_get_next_update_partition",
};

/* Placing anything in external RAM. */
static const char *const EXTERNAL_RAM_NAMES[] = {
    "EXT_RAM_BSS_ATTR", "EXT_RAM_NOINIT_ATTR", "EXT_RAM_ATTR", "MALLOC_CAP_SPIRAM", "SPIRAM_MALLOC",
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
    enum {
        NONE,
        ON_PATH,
        EXCLUDED
    } section = NONE;

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
                        size_t line_no, const char *code, const char *raw, const char *const *names,
                        size_t name_count)
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
        check_names(out, "ota-activation-in-the-endpoint-tree", unit, line_no, code, raw, OTA_NAMES,
                    COUNT_OF(OTA_NAMES));
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

/* --- the build configuration (audit A-15) ------------------------------------
 *
 * The source rules above cannot see the one route an ordinary build takes to
 * burn an eFuse: a Kconfig option. CONFIG_SECURE_FLASH_ENC_ENABLED=y in
 * sdkconfig.defaults builds a bootloader that encrypts the flash and burns the
 * key on first boot, and not one line of C in this tree changes. So every file
 * that feeds the image's configuration is read too: each firmware/sdkconfig*
 * (the committed defaults, a target-specific defaults file, and the sdkconfig
 * idf.py writes into firmware/ when it is run without -B, which it then reads
 * back in preference to the defaults) and each CMakeLists.txt or *.cmake under
 * firmware/. The options come from CHORUS_EFUSE_KCONFIG_LIST, which the
 * post-build guard reads as well. firmware/build is skipped: it is output,
 * gitignored, and full of ESP-IDF's own CMake, and the generated sdkconfig
 * that matters is graded after the build by tools/firmware-image-guard.sh. */

#define KCONFIG_MAX_RULES 64

typedef struct {
    char refuse[KCONFIG_MAX_RULES][96];
    size_t refuse_count;
    char derived[KCONFIG_MAX_RULES][96];
    size_t derived_count;
} kconfig_rules_t;

static int kconfig_rules_load(const char *root, kconfig_rules_t *rules, char *detail,
                              size_t detail_len)
{
    memset(rules, 0, sizeof(*rules));
    char path[CHORUS_SCAN_PATH * 2];
    snprintf(path, sizeof(path), "%s/%s", root, CHORUS_EFUSE_KCONFIG_LIST);
    char *text = read_file(path);
    if (text == NULL) {
        snprintf(detail, detail_len, "%s could not be read", CHORUS_EFUSE_KCONFIG_LIST);
        return -1;
    }
    size_t line_no = 0;
    char *cursor = text;
    int failed = 0;
    while (*cursor != '\0' && !failed) {
        line_no++;
        char *eol = strchr(cursor, '\n');
        size_t raw_len = (eol == NULL) ? strlen(cursor) : (size_t)(eol - cursor);
        char line[CHORUS_SCAN_TEXT];
        size_t copy = (raw_len < sizeof(line) - 1) ? raw_len : sizeof(line) - 1;
        memcpy(line, cursor, copy);
        line[copy] = '\0';
        cursor = (eol == NULL) ? cursor + raw_len : eol + 1;

        char *content = trim(line);
        if (*content == '\0' || *content == '#') {
            continue;
        }
        char kind[16];
        char name[96];
        int consumed = 0;
        if (sscanf(content, "%15s %95s%n", kind, name, &consumed) != 2) {
            snprintf(detail, detail_len, "%s:%zu is not `refuse|derived NAME = reason`",
                     CHORUS_EFUSE_KCONFIG_LIST, line_no);
            failed = 1;
            break;
        }
        char *rest = trim(content + consumed);
        if (rest[0] != '=' || *trim(rest + 1) == '\0') {
            snprintf(detail, detail_len,
                     "%s:%zu names %s with no reason, and an entry without one is just a "
                     "shorter list",
                     CHORUS_EFUSE_KCONFIG_LIST, line_no, name);
            failed = 1;
            break;
        }
        if (strncmp(name, "CONFIG_", 7) != 0) {
            snprintf(detail, detail_len, "%s:%zu: %s is not a CONFIG_ name",
                     CHORUS_EFUSE_KCONFIG_LIST, line_no, name);
            failed = 1;
            break;
        }
        if (strcmp(kind, "refuse") == 0 && rules->refuse_count < KCONFIG_MAX_RULES) {
            snprintf(rules->refuse[rules->refuse_count++], sizeof(rules->refuse[0]), "%s", name);
        } else if (strcmp(kind, "derived") == 0 && rules->derived_count < KCONFIG_MAX_RULES) {
            snprintf(rules->derived[rules->derived_count++], sizeof(rules->derived[0]), "%s", name);
        } else {
            snprintf(detail, detail_len,
                     "%s:%zu: `%s` is neither refuse nor derived, or the "
                     "list is over %d entries",
                     CHORUS_EFUSE_KCONFIG_LIST, line_no, kind, KCONFIG_MAX_RULES);
            failed = 1;
        }
    }
    free(text);
    if (!failed && rules->refuse_count == 0) {
        /* A list that refuses nothing would make every configuration clean. */
        snprintf(detail, detail_len, "%s refuses nothing", CHORUS_EFUSE_KCONFIG_LIST);
        failed = 1;
    }
    return failed ? -1 : 0;
}

/* The refuse prefix `name` falls under, or NULL when it is clean or is one of
 * the derived capability flags. */
static const char *kconfig_refused(const kconfig_rules_t *rules, const char *name)
{
    for (size_t i = 0; i < rules->derived_count; i++) {
        if (strcmp(rules->derived[i], name) == 0) {
            return NULL;
        }
    }
    for (size_t i = 0; i < rules->refuse_count; i++) {
        if (strncmp(name, rules->refuse[i], strlen(rules->refuse[i])) == 0) {
            return rules->refuse[i];
        }
    }
    return NULL;
}

/* One sdkconfig-format file: `CONFIG_X=value` lines, `#` comments, and the
 * `# CONFIG_X is not set` form, which is clean. A refused option set to `y` is
 * a finding. Numbers and strings are not: every option that burns is a bool,
 * and the stock esp32s3 configuration carries promptless numbers under
 * SECURE_ (the image digest length, for one) with Secure Boot off. */
static void scan_sdkconfig_file(chorus_scan_result_t *out, const kconfig_rules_t *rules,
                                const char *unit, char *text)
{
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

        char line[CHORUS_SCAN_TEXT];
        snprintf(line, sizeof(line), "%s", raw);
        char *content = trim(line);
        if (*content == '#' || *content == '\0') {
            continue;
        }
        char *equals = strchr(content, '=');
        if (equals == NULL) {
            continue;
        }
        *equals = '\0';
        char *name = trim(content);
        char *value = trim(equals + 1);
        if (strcmp(value, "y") != 0) {
            continue;
        }
        const char *prefix = kconfig_refused(rules, name);
        if (prefix != NULL) {
            char shown[CHORUS_SCAN_TEXT];
            snprintf(shown, sizeof(shown), "%s", raw);
            add(out, "efuse-burning-option-in-the-build-configuration", unit, line_no, name,
                trim(shown));
        }
    }
}

/* One CMake file. CMake cannot set a Kconfig option by itself, but it can
 * point SDKCONFIG or SDKCONFIG_DEFAULTS at a file this scan never reads, and
 * it can hand CONFIG_ names to the compiler. So, with `#` comments cut: any
 * refused name, spelled with or without its CONFIG_ prefix, is a finding, and
 * so is any mention of SDKCONFIG at all. */
static void scan_cmake_file(chorus_scan_result_t *out, const kconfig_rules_t *rules,
                            const char *unit, char *text)
{
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
        snprintf(code, sizeof(code), "%s", raw);
        char *hash = strchr(code, '#');
        if (hash != NULL) {
            *hash = '\0';
        }
        char shown[CHORUS_SCAN_TEXT];
        snprintf(shown, sizeof(shown), "%s", raw);

        if (strstr(code, "SDKCONFIG") != NULL) {
            add(out, "build-configuration-redirected-in-cmake", unit, line_no, "SDKCONFIG",
                trim(shown));
            continue;
        }
        for (size_t r = 0; r < rules->refuse_count; r++) {
            const char *stem = rules->refuse[r] + 7; /* past "CONFIG_" */
            size_t stem_len = strlen(stem);
            for (const char *hit = strstr(code, stem); hit != NULL; hit = strstr(hit + 1, stem)) {
                size_t at = (size_t)(hit - code);
                int bare = (at == 0) || !is_ident_byte(code[at - 1]);
                int prefixed = (at >= 7) && strncmp(hit - 7, "CONFIG_", 7) == 0 &&
                               (at == 7 || !is_ident_byte(code[at - 8]));
                if (!bare && !prefixed) {
                    continue;
                }
                /* The whole identifier, so a derived flag is not a finding. */
                char name[96];
                size_t end = at + stem_len;
                while (is_ident_byte(code[end])) {
                    end++;
                }
                size_t len = end - at;
                if (len > sizeof(name) - 8) {
                    len = sizeof(name) - 8;
                }
                snprintf(name, sizeof(name), "CONFIG_%.*s", (int)len, hit);
                if (kconfig_refused(rules, name) != NULL) {
                    add(out, "efuse-burning-option-in-the-build-configuration", unit, line_no, name,
                        trim(shown));
                    break;
                }
            }
        }
    }
}

static int is_sdkconfig_name(const char *base)
{
    return strncmp(base, "sdkconfig", 9) == 0;
}

static int is_cmake_name(const char *base)
{
    size_t len = strlen(base);
    return strcmp(base, "CMakeLists.txt") == 0 ||
           (len > 6 && strcmp(base + len - 6, ".cmake") == 0);
}

static void walk_build_configuration(chorus_scan_result_t *out, const char *root,
                                     const char *relative, const kconfig_rules_t *rules)
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
        if (strcmp(child_relative, "firmware/build") == 0) {
            continue;
        }
        char child_path[CHORUS_SCAN_PATH * 4];
        snprintf(child_path, sizeof(child_path), "%s/%s", root, child_relative);
        struct stat info;
        if (stat(child_path, &info) != 0) {
            continue;
        }
        if (S_ISDIR(info.st_mode)) {
            walk_build_configuration(out, root, child_relative, rules);
            continue;
        }
        int sdkconfig = is_sdkconfig_name(entry->d_name);
        if (!sdkconfig && !is_cmake_name(entry->d_name)) {
            continue;
        }
        char *text = read_file(child_path);
        if (text == NULL) {
            add(out, "build-configuration-unreadable", child_relative, 0, child_relative,
                "feeds the image's configuration and could not be read, so it was not checked");
            continue;
        }
        out->config_files_scanned++;
        if (sdkconfig) {
            scan_sdkconfig_file(out, rules, child_relative, text);
        } else {
            scan_cmake_file(out, rules, child_relative, text);
        }
        free(text);
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

    kconfig_rules_t rules;
    char detail[CHORUS_SCAN_TEXT];
    detail[0] = '\0';
    if (kconfig_rules_load(root, &rules, detail, sizeof(detail)) != 0) {
        /* A guard that cannot read its own list has checked nothing. */
        add(out, "efuse-kconfig-list-unreadable", CHORUS_EFUSE_KCONFIG_LIST, 0,
            CHORUS_EFUSE_KCONFIG_LIST, detail);
        return;
    }
    walk_build_configuration(out, root, "firmware", &rules);
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
                "the amplifier driver names no register address, and none is unaccounted for; "
                "%zu build-configuration files (firmware/sdkconfig*, CMake) enable no option "
                "that burns an eFuse\n",
                result->units_scanned, result->config_files_scanned);
    }
}
