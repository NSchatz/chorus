/* I2S clocking, the pin map, and where a DMA descriptor may live.
 *
 * Every rule here is a quotation from a source carried in this spec's folder
 * and quoted in docs/decisions/0015. The tests come in pairs: the committed
 * configuration passes, AND a smuggled value makes the same check go red and
 * name what it found. A rule that has only ever been green is a rule nobody
 * has seen work. */

#include "chorus/endpoint_config.h"
#include "chorus/i2s.h"
#include "harness.h"

#include <inttypes.h>
#include <unistd.h>

#ifndef CHORUS_CONFIG_CHECK_BIN
#define CHORUS_CONFIG_CHECK_BIN "chorus-endpoint-config-check"
#endif

#define MAX_FINDINGS 32

static chorus_i2s_clock_t committed_clock(void)
{
    chorus_i2s_clock_t clock;
    clock.sample_rate_hz = 48000;
    clock.slot_bit_width = 24;
    clock.mclk_multiple = 384;
    clock.dma_frame_num = 240;
    clock.dma_desc_num = 6;
    return clock;
}

static int has_rule(const chorus_finding_t *findings, size_t count, const char *rule)
{
    for (size_t i = 0; i < count; i++) {
        if (strcmp(findings[i].rule, rule) == 0) {
            return 1;
        }
    }
    return 0;
}

static const char *detail_of(const chorus_finding_t *findings, size_t count, const char *rule)
{
    for (size_t i = 0; i < count; i++) {
        if (strcmp(findings[i].rule, rule) == 0) {
            return findings[i].detail;
        }
    }
    return "";
}

/* AC-3's host-checkable half, and AC-11's clock half. */
static void the_twenty_four_bit_mclk_rule(void)
{
    chorus_finding_t findings[MAX_FINDINGS];
    size_t count = 0;
    chorus_i2s_clock_t clock = committed_clock();
    chorus_i2s_validate_clock(&clock, findings, MAX_FINDINGS, &count);
    chorus_check(count == 0, "the committed 48 kHz / 24-bit / x384 configuration is accepted");
    chorus_check(chorus_i2s_mclk_hz(&clock) == 18432000ull &&
                     chorus_i2s_bclk_hz(&clock) == 2304000ull,
                 "MCLK is %llu Hz and BCLK is %llu Hz",
                 (unsigned long long)chorus_i2s_mclk_hz(&clock),
                 (unsigned long long)chorus_i2s_bclk_hz(&clock));
    chorus_check(chorus_i2s_bclk_division_is_integral(&clock),
                 "MCLK divides into BCLK a whole 8 times, so the bit-clock division is integral");

    /* Every multiple ESP-IDF names, decided one way or the other, so the rule
     * is exercised across the whole enum and not at one point. */
    const struct {
        uint32_t multiple;
        int acceptable;
    } multiples[] = {
        {128, 0}, {192, 1},  {256, 0},  {384, 1},  {512, 0},
        {576, 1}, {768, 1},  {1024, 0}, {1152, 1},
    };
    for (size_t i = 0; i < sizeof(multiples) / sizeof(multiples[0]); i++) {
        chorus_i2s_clock_t candidate = committed_clock();
        candidate.mclk_multiple = multiples[i].multiple;
        count = 0;
        chorus_i2s_validate_clock(&candidate, findings, MAX_FINDINGS, &count);
        int divisible = has_rule(findings, count, "mclk-multiple-not-divisible-by-three") ? 0 : 1;
        chorus_check(divisible == multiples[i].acceptable,
                     "I2S_MCLK_MULTIPLE_%u at a 24-bit slot width is %s", multiples[i].multiple,
                     multiples[i].acceptable ? "accepted" : "refused");
    }

    /* The refusal has to name the value and the rule, or a build that stopped
     * says nothing useful. */
    chorus_i2s_clock_t bad = committed_clock();
    bad.mclk_multiple = 256;
    count = 0;
    chorus_i2s_validate_clock(&bad, findings, MAX_FINDINGS, &count);
    const char *detail = detail_of(findings, count, "mclk-multiple-not-divisible-by-three");
    chorus_check(strstr(detail, "256") != NULL,
                 "the refusal names the offending value: %s", detail);
    chorus_check(strstr(detail, "the BCLK division is not a integer") != NULL,
                 "the refusal quotes the rule it broke");

    /* A 16-bit slot width has no such rule, which is what makes the 24-bit one
     * a rule about 24 bits rather than about the number 3. */
    chorus_i2s_clock_t sixteen = committed_clock();
    sixteen.slot_bit_width = 16;
    sixteen.mclk_multiple = 256;
    sixteen.dma_frame_num = 256;
    count = 0;
    chorus_i2s_validate_clock(&sixteen, findings, MAX_FINDINGS, &count);
    chorus_check(count == 0, "16-bit slots at x256 are accepted, which 24-bit slots are not");

    /* dma_frame_num carries the same divisibility rule at 24 bits. */
    chorus_i2s_clock_t frames = committed_clock();
    frames.dma_frame_num = 256;
    count = 0;
    chorus_i2s_validate_clock(&frames, findings, MAX_FINDINGS, &count);
    chorus_check(has_rule(findings, count, "dma-frame-num-not-multiple-of-three"),
                 "a dma_frame_num of 256 at a 24-bit slot width is refused");

    /* And the general form, which is what the reference states first:
     * "Normally, MCLK should be the multiple of sample rate and BCLK at the
     * same time." Every multiple in ESP-IDF's own enum happens to satisfy it
     * at every slot width, so the case that exercises this rule is a multiple
     * from outside that enum - which is exactly the configuration a hand-typed
     * number produces. */
    chorus_i2s_clock_t fractional = committed_clock();
    fractional.slot_bit_width = 16;
    fractional.mclk_multiple = 100;
    fractional.dma_frame_num = 100;
    count = 0;
    chorus_i2s_validate_clock(&fractional, findings, MAX_FINDINGS, &count);
    chorus_check(has_rule(findings, count, "bclk-division-is-not-integral"),
                 "x100 at 16-bit slots gives a fractional bit-clock division and is refused: %s",
                 detail_of(findings, count, "bclk-division-is-not-integral"));
}

/* AC-11's pin-map half. */
static void the_reserved_pins(void)
{
    chorus_finding_t findings[MAX_FINDINGS];
    size_t count = 0;

    chorus_pin_map_t pins;
    memset(&pins, 0, sizeof(pins));
    pins.mclk = 16;
    pins.bclk = 17;
    pins.ws = 18;
    pins.dout = 15;
    pins.sda = 8;
    pins.scl = 9;
    pins.amp_power_down = 21;
    pins.octal_psram = 1;
    chorus_pin_map_validate(&pins, findings, MAX_FINDINGS, &count);
    chorus_check(count == 0, "the committed pin map is accepted");

    const struct {
        uint32_t pin;
        const char *rule;
        const char *why;
    } reserved[] = {
        {26, "gpio-reserved-for-flash-and-psram", "the bottom of the flash and PSRAM range"},
        {29, "gpio-reserved-for-flash-and-psram", "the middle of the flash and PSRAM range"},
        {32, "gpio-reserved-for-flash-and-psram", "the top of the flash and PSRAM range"},
        {33, "gpio-reserved-for-octal-flash-or-psram", "the bottom of the octal range"},
        {37, "gpio-reserved-for-octal-flash-or-psram", "the top of the octal range"},
        {19, "gpio-reserved-for-usb-jtag", "USB-JTAG"},
        {20, "gpio-reserved-for-usb-jtag", "USB-JTAG"},
        {0, "gpio-is-a-strapping-pin", "a strapping pin"},
        {3, "gpio-is-a-strapping-pin", "a strapping pin"},
        {45, "gpio-is-a-strapping-pin", "a strapping pin"},
        {46, "gpio-is-a-strapping-pin", "a strapping pin"},
        {49, "gpio-out-of-range", "beyond the highest GPIO an ESP32-S3 has"},
    };

    for (size_t i = 0; i < sizeof(reserved) / sizeof(reserved[0]); i++) {
        chorus_pin_map_t candidate;
        memset(&candidate, 0, sizeof(candidate));
        candidate.mclk = 16;
        candidate.bclk = 17;
        candidate.ws = 18;
        candidate.dout = reserved[i].pin;
        candidate.sda = 8;
        candidate.scl = 9;
        candidate.amp_power_down = 21;
        candidate.octal_psram = 1;
        count = 0;
        chorus_pin_map_validate(&candidate, findings, MAX_FINDINGS, &count);
        chorus_check(has_rule(findings, count, reserved[i].rule),
                     "pin_i2s_dout = GPIO%u is refused as %s (%s)", reserved[i].pin,
                     reserved[i].why, reserved[i].rule);
        const char *why = detail_of(findings, count, reserved[i].rule);
        char needle[32];
        snprintf(needle, sizeof(needle), "GPIO%u", reserved[i].pin);
        chorus_check(strstr(why, needle) != NULL && strstr(why, "pin_i2s_dout") != NULL,
                     "the refusal names the pin and the assignment: %s", why);
    }

    /* GPIO33 to GPIO37 are reserved only on an octal part, which is why the
     * board declares whether it is one. */
    chorus_pin_map_t not_octal;
    memset(&not_octal, 0, sizeof(not_octal));
    not_octal.mclk = 16;
    not_octal.bclk = 17;
    not_octal.ws = 18;
    not_octal.dout = 35;
    not_octal.sda = 8;
    not_octal.scl = 9;
    not_octal.amp_power_down = 21;
    not_octal.octal_psram = 0;
    count = 0;
    chorus_pin_map_validate(&not_octal, findings, MAX_FINDINGS, &count);
    chorus_check(count == 0,
                 "GPIO35 is accepted on a board that declares board_octal_psram = no");

    /* Two signals on one pin is not a pin map. */
    chorus_pin_map_t doubled;
    memset(&doubled, 0, sizeof(doubled));
    doubled.mclk = 16;
    doubled.bclk = 17;
    doubled.ws = 17;
    doubled.dout = 15;
    doubled.sda = 8;
    doubled.scl = 9;
    doubled.amp_power_down = 21;
    doubled.octal_psram = 1;
    count = 0;
    chorus_pin_map_validate(&doubled, findings, MAX_FINDINGS, &count);
    chorus_check(has_rule(findings, count, "gpio-assigned-twice"),
                 "two signals on GPIO17 is refused: %s",
                 detail_of(findings, count, "gpio-assigned-twice"));
}

/* AC-12's runtime half. The source half is firmware/check/endpoint_scan.c. */
static void a_dma_descriptor_may_not_live_in_external_ram(void)
{
    chorus_finding_t findings[MAX_FINDINGS];
    size_t count = 0;
    chorus_dma_validate_placement(CHORUS_MEM_INTERNAL, findings, MAX_FINDINGS, &count);
    chorus_check(count == 0, "internal placement is accepted");

    count = 0;
    chorus_dma_validate_placement(CHORUS_MEM_EXTERNAL, findings, MAX_FINDINGS, &count);
    chorus_check(has_rule(findings, count, "dma-descriptor-in-external-ram"),
                 "external placement is refused");
    const char *why = detail_of(findings, count, "dma-descriptor-in-external-ram");
    chorus_check(strstr(why, "DMA transaction descriptors cannot be placed in PSRAM") != NULL,
                 "the refusal quotes the platform: %s", why);
}

/* The committed configuration itself, and the entry point that gates the
 * build on it. */
static void the_committed_configuration_and_the_gate(void)
{
    chorus_endpoint_config_t config;
    char detail[512];
    detail[0] = '\0';
    chorus_check(chorus_endpoint_config_load(&config, chorus_endpoint_config_default_path(),
                                             detail, sizeof(detail)) == 0,
                 "the committed endpoint.conf loads (%s)", detail);

    chorus_finding_t findings[MAX_FINDINGS];
    size_t count = 0;
    chorus_endpoint_config_validate(&config, findings, MAX_FINDINGS, &count);
    for (size_t i = 0; i < count; i++) {
        printf("     unexpected finding: %s :: %s\n", findings[i].rule, findings[i].detail);
    }
    chorus_check(count == 0, "the committed endpoint.conf breaks no rule");
    chorus_check(config.clock.slot_bit_width == 24 && config.clock.mclk_multiple % 3 == 0,
                 "the committed slot width is 24 bits and the MCLK multiple (%u) is divisible by "
                 "three",
                 config.clock.mclk_multiple);
    chorus_check(config.dma_placement == CHORUS_MEM_INTERNAL,
                 "the committed DMA descriptor placement is internal");
    chorus_check(!config.amp.address_known && !config.amp.reg_device_id_known &&
                     !config.amp.reg_fault_known && !config.amp.reg_analog_gain_known &&
                     !config.amp.reg_state_control_known && !config.gain.code_known,
                 "every amplifier register in the committed configuration is DECLARED UNKNOWN");
    chorus_check(config.gain.db <= config.amp.analog_gain_ceiling_db,
                 "the requested analog gain (%.3f dB) is at or below the ceiling (%.3f dB)",
                 config.gain.db, config.amp.analog_gain_ceiling_db);

    /* And the demonstration that the gate goes red: a scratch copy with one
     * value changed, handed to the entry point the Makefile runs. */
    const struct {
        const char *from;
        const char *to;
        const char *what;
    } smuggled[] = {
        {"i2s_mclk_multiple = 384", "i2s_mclk_multiple = 256",
         "an MCLK multiple not divisible by three at a 24-bit slot width"},
        {"pin_i2s_dout = 15", "pin_i2s_dout = 30", "a pin the flash and PSRAM range reserves"},
        {"pin_i2s_dout = 15", "pin_i2s_dout = 19", "a pin USB-JTAG reserves"},
        {"pin_i2s_dout = 15", "pin_i2s_dout = 35", "a pin an octal part reserves"},
        {"dma_descriptor_placement = internal", "dma_descriptor_placement = external",
         "a DMA descriptor placed in external RAM"},
        {"amp_analog_gain_db = 0.0", "amp_analog_gain_db = 9.0",
         "a requested analog gain above the committed ceiling"},
    };

    char source_path[1024];
    snprintf(source_path, sizeof(source_path), "%s", chorus_endpoint_config_default_path());
    FILE *source = fopen(source_path, "rb");
    if (source == NULL) {
        chorus_check(0, "the committed endpoint.conf is readable");
        return;
    }
    static char text[65536];
    size_t read = fread(text, 1, sizeof(text) - 1, source);
    fclose(source);
    text[read] = '\0';

    for (size_t i = 0; i < sizeof(smuggled) / sizeof(smuggled[0]); i++) {
        static char mutated[65536];
        char *at = strstr(text, smuggled[i].from);
        if (at == NULL) {
            chorus_check(0, "the committed file still carries `%s`", smuggled[i].from);
            continue;
        }
        size_t prefix = (size_t)(at - text);
        memcpy(mutated, text, prefix);
        size_t written = prefix;
        written += (size_t)snprintf(mutated + written, sizeof(mutated) - written, "%s",
                                    smuggled[i].to);
        snprintf(mutated + written, sizeof(mutated) - written, "%s",
                 at + strlen(smuggled[i].from));

        char scratch[1024];
        snprintf(scratch, sizeof(scratch), "/tmp/chorus-endpoint-smuggled-%d-%zu.conf",
                 (int)getpid(), i);
        FILE *out = fopen(scratch, "wb");
        if (out == NULL) {
            chorus_check(0, "a scratch configuration is writable");
            continue;
        }
        fputs(mutated, out);
        fclose(out);

        chorus_endpoint_config_t bad;
        size_t bad_count = 0;
        chorus_check(chorus_endpoint_config_load(&bad, scratch, detail, sizeof(detail)) == 0,
                     "the smuggled configuration still loads (%s)", smuggled[i].what);
        chorus_endpoint_config_validate(&bad, findings, MAX_FINDINGS, &bad_count);
        chorus_check(bad_count > 0, "%s makes the check go red (%zu findings)", smuggled[i].what,
                     bad_count);

        /* The entry point, not only the library: this is the thing the
         * Makefile runs before it compiles anything, so it is the thing that
         * makes "refuse rather than build" true. */
        char command[2048];
        snprintf(command, sizeof(command), "%s %s > /dev/null 2>&1", CHORUS_CONFIG_CHECK_BIN,
                 scratch);
        int status = system(command);
        chorus_check(status != 0, "chorus-endpoint-config-check exits non-zero for %s",
                     smuggled[i].what);
        remove(scratch);
    }

    /* And the entry point accepts the committed file, so the refusals above
     * are about the values and not about the checker being broken. */
    char command[2048];
    snprintf(command, sizeof(command), "%s %s > /dev/null 2>&1", CHORUS_CONFIG_CHECK_BIN,
             source_path);
    chorus_check(system(command) == 0,
                 "chorus-endpoint-config-check accepts the committed configuration");
}

int main(void)
{
    chorus_section("the 24-bit MCLK rule");
    the_twenty_four_bit_mclk_rule();

    chorus_section("the reserved pins");
    the_reserved_pins();

    chorus_section("where a DMA descriptor may live");
    a_dma_descriptor_may_not_live_in_external_ram();

    chorus_section("the committed configuration, and the gate that refuses rather than builds");
    the_committed_configuration_and_the_gate();

    return chorus_test_report("endpoint clocking and pin map");
}
