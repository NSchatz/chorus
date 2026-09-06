/* The endpoint tree's safety scans, run against this repository and against
 * scratch copies with a violation smuggled in.
 *
 * The demonstrations are the point. A scan that has only ever been green is a
 * scan nobody has seen work, so every rule here is shown firing on a tree that
 * breaks it - and shown NAMING the file, which is what AC-12, AC-13 and AC-16
 * each ask for by name. This is the shape crates/audio-path/tests/audio_path.rs
 * already uses on the Rust side, for the same reason.
 *
 * The copies are made under the system temporary directory and removed
 * afterwards, so a demonstration never touches the working tree. */

#include "endpoint_scan.h"
#include "harness.h"

#include <dirent.h>
#include <errno.h>
#include <sys/stat.h>
#include <unistd.h>

static int copy_file(const char *from, const char *to)
{
    FILE *in = fopen(from, "rb");
    if (in == NULL) {
        return -1;
    }
    FILE *out = fopen(to, "wb");
    if (out == NULL) {
        fclose(in);
        return -1;
    }
    char buffer[8192];
    size_t got;
    while ((got = fread(buffer, 1, sizeof(buffer), in)) > 0) {
        if (fwrite(buffer, 1, got, out) != got) {
            fclose(in);
            fclose(out);
            return -1;
        }
    }
    fclose(in);
    fclose(out);
    return 0;
}

static int copy_tree(const char *from, const char *to)
{
    if (mkdir(to, 0755) != 0 && errno != EEXIST) {
        return -1;
    }
    DIR *dir = opendir(from);
    if (dir == NULL) {
        return -1;
    }
    struct dirent *entry;
    int failed = 0;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        char source[2048];
        char target[2048];
        snprintf(source, sizeof(source), "%.1700s/%.255s", from, entry->d_name);
        snprintf(target, sizeof(target), "%.1700s/%.255s", to, entry->d_name);
        struct stat info;
        if (stat(source, &info) != 0) {
            continue;
        }
        if (S_ISDIR(info.st_mode)) {
            if (copy_tree(source, target) != 0) {
                failed = 1;
            }
        } else if (copy_file(source, target) != 0) {
            failed = 1;
        }
    }
    closedir(dir);
    return failed ? -1 : 0;
}

static int remove_tree(const char *path)
{
    DIR *dir = opendir(path);
    if (dir == NULL) {
        return remove(path);
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        char child[2048];
        snprintf(child, sizeof(child), "%.1700s/%.255s", path, entry->d_name);
        struct stat info;
        if (stat(child, &info) == 0 && S_ISDIR(info.st_mode)) {
            remove_tree(child);
        } else {
            remove(child);
        }
    }
    closedir(dir);
    return rmdir(path);
}

/* A scratch copy of the parts of the tree the scan reads: the unit list, the
 * source and the headers. Nothing else is needed, and copying nothing else
 * keeps a demonstration cheap. */
static int scratch_copy(const char *name, char *out, size_t out_len)
{
    snprintf(out, out_len, "/tmp/chorus-endpoint-scan-%s-%d", name, (int)getpid());
    remove_tree(out);
    if (mkdir(out, 0755) != 0 && errno != EEXIST) {
        return -1;
    }
    char firmware[2048];
    snprintf(firmware, sizeof(firmware), "%.1900s/firmware", out);
    if (mkdir(firmware, 0755) != 0 && errno != EEXIST) {
        return -1;
    }
    char source[2048];
    char target[2048];
    chorus_repo_path(source, sizeof(source), "firmware/src");
    snprintf(target, sizeof(target), "%.1900s/src", firmware);
    if (copy_tree(source, target) != 0) {
        return -1;
    }
    chorus_repo_path(source, sizeof(source), "firmware/include");
    snprintf(target, sizeof(target), "%.1900s/include", firmware);
    if (copy_tree(source, target) != 0) {
        return -1;
    }
    /* firmware/main is copied too, or the scan of a scratch tree would fail
     * because it could not walk that directory - and a demonstration that goes
     * red for the wrong reason demonstrates nothing. */
    chorus_repo_path(source, sizeof(source), "firmware/main");
    snprintf(target, sizeof(target), "%.1900s/main", firmware);
    if (copy_tree(source, target) != 0) {
        return -1;
    }
    chorus_repo_path(source, sizeof(source), "firmware/endpoint-units.conf");
    snprintf(target, sizeof(target), "%.1900s/endpoint-units.conf", firmware);
    return copy_file(source, target);
}

static int load_committed_list(chorus_unit_list_t *list)
{
    char path[2048];
    chorus_repo_path(path, sizeof(path), "firmware/endpoint-units.conf");
    char detail[CHORUS_SCAN_TEXT];
    detail[0] = '\0';
    int ok = chorus_unit_list_load(list, path, detail, sizeof(detail)) == 0;
    chorus_check(ok, "firmware/endpoint-units.conf parses (%s)", detail);
    return ok;
}

static int has_rule(const chorus_scan_result_t *result, const char *rule, const char *unit)
{
    for (size_t i = 0; i < result->count; i++) {
        if (strcmp(result->findings[i].rule, rule) == 0 &&
            strstr(result->findings[i].unit, unit) != NULL) {
            return 1;
        }
    }
    return 0;
}

static void print_findings(const chorus_scan_result_t *result)
{
    for (size_t i = 0; i < result->count; i++) {
        printf("     %s %s:%zu %s\n", result->findings[i].rule, result->findings[i].unit,
               result->findings[i].line, result->findings[i].name);
    }
}

/* Append `body` to a unit in a scratch tree. */
static int smuggle_into(const char *scratch, const char *unit, const char *body)
{
    char path[2048];
    snprintf(path, sizeof(path), "%.1000s/%.1000s", scratch, unit);
    FILE *file = fopen(path, "ab");
    if (file == NULL) {
        return -1;
    }
    fputs(body, file);
    fclose(file);
    return 0;
}

static void one_demonstration(const char *name, const char *unit, const char *body,
                              const char *rule, const char *what)
{
    chorus_unit_list_t list;
    if (!load_committed_list(&list)) {
        return;
    }
    char scratch[2048];
    if (scratch_copy(name, scratch, sizeof(scratch)) != 0) {
        chorus_check(0, "a scratch copy for the %s demonstration is makeable", what);
        return;
    }
    if (smuggle_into(scratch, unit, body) != 0) {
        chorus_check(0, "%s is writable in the scratch copy", unit);
        remove_tree(scratch);
        return;
    }

    chorus_scan_result_t result;
    chorus_endpoint_scan(scratch, &list, &result);
    chorus_check(!chorus_scan_ok(&result), "%s turns the scan red", what);
    chorus_check(has_rule(&result, rule, unit), "%s is reported as %s, naming %s", what, rule,
                 unit);
    if (!has_rule(&result, rule, unit)) {
        print_findings(&result);
    }
    remove_tree(scratch);
}

/* The completeness half: a first-party unit added under firmware/src that the
 * list does not account for. Moving a forbidden call one file down is exactly
 * the shrink this closes. */
static void an_unlisted_unit_turns_it_red(void)
{
    chorus_unit_list_t list;
    if (!load_committed_list(&list)) {
        return;
    }
    char scratch[2048];
    if (scratch_copy("unlisted-unit", scratch, sizeof(scratch)) != 0) {
        chorus_check(0, "a scratch copy for the unlisted-unit demonstration is makeable");
        return;
    }
    char path[2048];
    snprintf(path, sizeof(path), "%.1900s/firmware/src/smuggled.c", scratch);
    FILE *file = fopen(path, "wb");
    if (file == NULL) {
        chorus_check(0, "a smuggled unit is writable in the scratch copy");
        remove_tree(scratch);
        return;
    }
    fputs("/* Introduced by the endpoint-scan demonstration: a unit under\n"
          " * firmware/src that the list does not account for. */\n"
          "int chorus_smuggled(void) { return 0; }\n",
          file);
    fclose(file);

    chorus_scan_result_t result;
    chorus_endpoint_scan(scratch, &list, &result);
    chorus_check(!chorus_scan_ok(&result),
                 "a first-party unit the list does not account for turns the scan red");
    chorus_check(has_rule(&result, "unit-missing-from-the-list", "smuggled.c"),
                 "the report names firmware/src/smuggled.c");
    if (!has_rule(&result, "unit-missing-from-the-list", "smuggled.c")) {
        print_findings(&result);
    }
    remove_tree(scratch);
}

/* A scratch copy with NOTHING smuggled into it has to be green, or every
 * demonstration below is red for the wrong reason and proves nothing. */
static void a_clean_scratch_copy_is_green(void)
{
    chorus_unit_list_t list;
    if (!load_committed_list(&list)) {
        return;
    }
    char scratch[2048];
    if (scratch_copy("clean", scratch, sizeof(scratch)) != 0) {
        chorus_check(0, "a clean scratch copy is makeable");
        return;
    }
    chorus_scan_result_t result;
    chorus_endpoint_scan(scratch, &list, &result);
    if (!chorus_scan_ok(&result)) {
        print_findings(&result);
    }
    chorus_check(chorus_scan_ok(&result),
                 "a scratch copy with nothing smuggled into it passes every rule, so the "
                 "demonstrations below are about what was smuggled");
    remove_tree(scratch);
}

static void the_line_reader_does_what_it_says(void)
{
    char out[CHORUS_SCAN_TEXT];

    chorus_scan_code_only("int a = 1; /* x */ // gettimeofday", out, sizeof(out));
    chorus_check(strstr(out, "gettimeofday") == NULL, "a line comment is cut: %s", out);

    /* A forbidden name hidden after a URL in a string is a real one, so the
     * clock rule keeps literals. This is the same finding crates/audio-path
     * records about its own scanner. */
    chorus_scan_code_only("const char *u = \"https://example.invalid\"; gettimeofday(&t, 0);", out,
                          sizeof(out));
    chorus_check(strstr(out, "gettimeofday") != NULL,
                 "a name after a URL inside a string is still found: %s", out);

    /* The register-literal rule wants the opposite treatment, because a format
     * string that prints a byte in hex is prose and not an address. */
    chorus_scan_code_outside_literals("printf(\"reg=0x%02x\", value);", out, sizeof(out));
    chorus_check(strstr(out, "0x") == NULL, "a hex escape inside a string is blanked: %s", out);
    chorus_scan_code_outside_literals("uint8_t reg = 0x71; /* the fault register */", out,
                                      sizeof(out));
    chorus_check(strstr(out, "0x71") != NULL, "a hex literal in code survives: %s", out);
}

static void the_committed_tree_passes(void)
{
    chorus_unit_list_t list;
    if (!load_committed_list(&list)) {
        return;
    }
    chorus_check(list.on_path_count >= 10, "the list names %zu units on the endpoint path",
                 list.on_path_count);

    chorus_scan_result_t result;
    chorus_endpoint_scan(CHORUS_REPO_ROOT, &list, &result);
    if (!chorus_scan_ok(&result)) {
        print_findings(&result);
    }
    chorus_check(chorus_scan_ok(&result),
                 "the committed endpoint tree passes every rule (%zu units scanned)",
                 result.units_scanned);
    chorus_check(result.units_scanned == list.on_path_count,
                 "every listed unit exists and was scanned (%zu of %zu)", result.units_scanned,
                 list.on_path_count);
}

static void an_exclusion_without_a_reason_is_refused(void)
{
    chorus_unit_list_t list;
    char detail[CHORUS_SCAN_TEXT];
    detail[0] = '\0';
    const char *text = "[on-path]\nfirmware/src/conf.c\n[excluded]\nfirmware/src/protocol.c\n";
    chorus_check(chorus_unit_list_parse(&list, text, detail, sizeof(detail)) != 0,
                 "an exclusion with no reason is refused: %s", detail);

    const char *empty = "[on-path]\nfirmware/src/conf.c\n[excluded]\nfirmware/src/protocol.c =\n";
    chorus_check(chorus_unit_list_parse(&list, empty, detail, sizeof(detail)) != 0,
                 "an exclusion with an empty reason is refused: %s", detail);

    const char *good =
        "[on-path]\nfirmware/src/conf.c\n[excluded]\nfirmware/src/protocol.c = a reason\n";
    chorus_check(chorus_unit_list_parse(&list, good, detail, sizeof(detail)) == 0 &&
                     list.excluded_count == 1,
                 "an exclusion with a reason parses");
}

static void a_listed_unit_that_is_not_in_the_tree_is_a_failure(void)
{
    chorus_unit_list_t list;
    char detail[CHORUS_SCAN_TEXT];
    const char *text = "[on-path]\nfirmware/src/there-is-no-such-file.c\n";
    chorus_check(chorus_unit_list_parse(&list, text, detail, sizeof(detail)) == 0,
                 "a list naming an absent unit parses");
    chorus_scan_result_t result;
    chorus_endpoint_scan(CHORUS_REPO_ROOT, &list, &result);
    chorus_check(has_rule(&result, "unit-does-not-exist", "there-is-no-such-file.c"),
                 "a listed unit that is not in the tree is reported");
}

int main(void)
{
    chorus_section("the committed endpoint tree");
    the_committed_tree_passes();
    a_clean_scratch_copy_is_green();

    chorus_section("the line reader");
    the_line_reader_does_what_it_says();

    chorus_section("a settable wall clock, smuggled onto the endpoint path");
    one_demonstration("clock-read", "firmware/src/session.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "unsigned long chorus_demonstration_wall_clock(void)\n"
                      "{\n"
                      "    struct timespec ts;\n"
                      "    clock_gettime(CLOCK_REALTIME, &ts);\n"
                      "    return (unsigned long)ts.tv_sec;\n"
                      "}\n",
                      "settable-clock-on-the-endpoint-path",
                      "a settable clock read on the endpoint path");

    one_demonstration("gettimeofday", "firmware/src/telemetry.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "void chorus_demonstration_other_clock(void)\n"
                      "{\n"
                      "    struct timeval now;\n"
                      "    gettimeofday(&now, 0);\n"
                      "}\n",
                      "settable-clock-on-the-endpoint-path",
                      "the other ordinary spelling of a settable clock read");

    chorus_section("an eFuse burn, smuggled into the endpoint tree");
    one_demonstration("efuse", "firmware/src/monotonic.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "void chorus_demonstration_efuse(void)\n"
                      "{\n"
                      "    esp_efuse_write_field_bit(handle);\n"
                      "}\n",
                      "efuse-write-in-the-endpoint-tree", "an eFuse write");

    chorus_section("an OTA activation, smuggled into the endpoint tree");
    one_demonstration("ota", "firmware/src/session.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "void chorus_demonstration_ota(void)\n"
                      "{\n"
                      "    esp_ota_set_boot_partition(next);\n"
                      "}\n",
                      "ota-activation-in-the-endpoint-tree", "an OTA image activation");

    one_demonstration("ota-confirm", "firmware/src/session.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "void chorus_demonstration_ota_confirm(void)\n"
                      "{\n"
                      "    esp_ota_mark_app_valid_cancel_rollback();\n"
                      "}\n",
                      "ota-activation-in-the-endpoint-tree",
                      "an image confirming itself, which disarms the only recovery there is");

    chorus_section("a DMA descriptor placed in external RAM");
    one_demonstration("psram", "firmware/src/i2s.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "EXT_RAM_BSS_ATTR static char chorus_demonstration_descriptors[64];\n",
                      "dma-descriptor-in-external-ram",
                      "a DMA descriptor declared in external RAM");

    one_demonstration("psram-heap", "firmware/src/i2s.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "void *chorus_demonstration_alloc(void)\n"
                      "{\n"
                      "    return heap_caps_malloc(64, MALLOC_CAP_SPIRAM);\n"
                      "}\n",
                      "dma-descriptor-in-external-ram",
                      "a descriptor allocated out of PSRAM at run time");

    chorus_section("a register address, smuggled into the amplifier driver");
    one_demonstration("register-literal", "firmware/src/amp.c",
                      "\n/* Introduced by the endpoint-scan demonstration. */\n"
                      "static const unsigned char chorus_demonstration_fault_register = 0x71;\n",
                      "register-literal-in-the-amplifier-driver",
                      "a register address written into the amplifier driver");

    chorus_section("a unit the list does not account for");
    an_unlisted_unit_turns_it_red();

    chorus_section("the list's own rules");
    an_exclusion_without_a_reason_is_refused();
    a_listed_unit_that_is_not_in_the_tree_is_a_failure();

    return chorus_test_report("endpoint safety scans");
}
