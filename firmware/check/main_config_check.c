/* The build gate.
 *
 * `make firmware-check` runs this over firmware/config/endpoint.conf before it
 * compiles anything else, so a pin assignment that claims a reserved GPIO, or a
 * 24-bit slot width with an MCLK multiple not divisible by three, REFUSES
 * RATHER THAN BUILDS. Every finding names the offending value and the rule it
 * broke, with the rule's source quoted, because "the build refused" is only
 * useful if it says what to change.
 *
 *   chorus-endpoint-config-check [path/to/endpoint.conf [path/to/board-profile.conf]]
 *   chorus-endpoint-config-check --agrees path/to/endpoint.conf path/to/board-profile.conf
 *
 * With a board profile (firmware/boards/), the profile is laid over the base
 * first, by the reader the image uses. `--agrees` checks that every key the
 * profile sets already has that value in the base: endpoint.conf carries the
 * default profile's values, and the two may not drift. */

#include "chorus/conf.h"
#include "chorus/endpoint_config.h"

#include <stdio.h>
#include <string.h>

static chorus_conf_t base_table;
static chorus_conf_t profile_table;

static int agrees(const char *base_path, const char *profile_path)
{
    char detail[512];
    detail[0] = '\0';
    if (chorus_conf_load(&base_table, base_path, detail, sizeof(detail)) != CHORUS_CONF_OK ||
        chorus_conf_load(&profile_table, profile_path, detail, sizeof(detail)) != CHORUS_CONF_OK) {
        fprintf(stderr, "FAIL endpoint-configuration-unreadable :: %s\n", detail);
        return 2;
    }
    const char *named = chorus_conf_get(&base_table, "board_profile");
    int failures = 0;
    for (size_t i = 0; i < profile_table.count; i++) {
        const char *key = profile_table.pairs[i].key;
        const char *want = profile_table.pairs[i].value;
        const char *have = chorus_conf_get(&base_table, key);
        if (have == NULL || strcmp(have, want) != 0) {
            fprintf(stderr,
                    "FAIL default-profile-drift :: %s sets %s = %s and %s carries %s = %s; "
                    "endpoint.conf carries the default profile's values\n",
                    profile_path, key, want, base_path, key, have == NULL ? "(nothing)" : have);
            failures++;
        }
    }
    if (failures > 0) {
        return 1;
    }
    printf("pass default-profile-agrees: %s names board_profile = %s, and all %zu keys of %s "
           "have the same values there\n",
           base_path, named == NULL ? "(nothing)" : named, profile_table.count, profile_path);
    return 0;
}

#define MAX_FINDINGS 32

int main(int argc, char **argv)
{
    if (argc == 4 && strcmp(argv[1], "--agrees") == 0) {
        return agrees(argv[2], argv[3]);
    }
    const char *path = (argc > 1) ? argv[1] : chorus_endpoint_config_default_path();
    const char *profile = (argc > 2) ? argv[2] : NULL;

    static chorus_endpoint_config_t config;
    char detail[512];
    detail[0] = '\0';
    int loaded =
        (profile == NULL)
            ? chorus_endpoint_config_load(&config, path, detail, sizeof(detail))
            : chorus_endpoint_config_load_profile(&config, path, profile, detail, sizeof(detail));
    if (loaded != 0) {
        fprintf(stderr, "FAIL endpoint-configuration-unreadable :: %s\n", detail);
        return 2;
    }

    chorus_finding_t findings[MAX_FINDINGS];
    size_t count = 0;
    chorus_endpoint_config_validate(&config, findings, MAX_FINDINGS, &count);

    for (size_t i = 0; i < count; i++) {
        fprintf(stderr, "FAIL %s :: %s\n", findings[i].rule, findings[i].detail);
    }
    if (count > 0) {
        fprintf(stderr,
                "chorus-endpoint-config-check: %s is refused, so nothing that depends on it is "
                "built. Fix the value the rule names, not the rule.\n",
                path);
        return 1;
    }

    if (profile == NULL) {
        printf("pass endpoint-configuration: %s\n", path);
    } else {
        printf("pass endpoint-configuration: %s with board profile %s\n", path, profile);
    }
    printf("  board          profile=%s model=\"%s\" status=%s%s%s%s flash=%u MB\n",
           config.board.profile, config.board.model,
           chorus_board_status_name(config.board.model_status),
           config.board.model_status == CHORUS_BOARD_ASSUMED ? " needs_item=\"" : "",
           config.board.model_status == CHORUS_BOARD_ASSUMED ? config.board.needs_item : "",
           config.board.model_status == CHORUS_BOARD_ASSUMED ? "\"" : "",
           config.board.flash_size_mb);
    if (config.board.audio_output == CHORUS_AUDIO_OUTPUT_NONE) {
        printf("  audio output   none: no amplifier, no I2S and no I2C are brought up (the "
               "emulated board)\n");
    }
    if (config.link.transport == CHORUS_TRANSPORT_WIRED) {
        printf("  link           transport=wired phy=w5500 spi=%s clock=%u MHz sclk=%u mosi=%u "
               "miso=%u cs=%u int=%u\n",
               chorus_spi_host_name(config.eth.spi_host), config.eth.spi_clock_mhz, config.eth.sclk,
               config.eth.mosi, config.eth.miso, config.eth.cs, config.eth.int_pin);
    } else if (config.link.transport == CHORUS_TRANSPORT_EMULATED) {
        printf("  link           transport=emulated phy=openeth (the emulator's Ethernet; the "
               "W5500 pins and the radio are not driven)\n");
    } else {
        printf("  link           transport=wireless power_save=%s (the W5500 pins are not "
               "driven)\n",
               chorus_wifi_ps_name(config.link.power_save));
    }
    printf("  i2s            %u Hz, %u-bit samples in %u-bit slots, MCLK x%u (%llu Hz), BCLK %llu "
           "Hz, integral "
           "division %s\n",
           config.clock.sample_rate_hz, config.clock.slot_bit_width,
           config.clock.wire_slot_bit_width, config.clock.mclk_multiple,
           (unsigned long long)chorus_i2s_mclk_hz(&config.clock),
           (unsigned long long)chorus_i2s_bclk_hz(&config.clock),
           chorus_i2s_bclk_division_is_integral(&config.clock) ? "yes" : "no");
    printf("  dma            %u frames x %u descriptors, placed in %s memory\n",
           config.clock.dma_frame_num, config.clock.dma_desc_num,
           chorus_mem_placement_name(config.dma_placement));
    char mclk[16];
    if (config.pins.mclk == CHORUS_PIN_NONE) {
        snprintf(mclk, sizeof(mclk), "none");
    } else {
        snprintf(mclk, sizeof(mclk), "%u", config.pins.mclk);
    }
    printf("  pins           mclk=%s bclk=%u ws=%u dout=%u sda=%u scl=%u amp_pdn=%u "
           "octal_psram=%s\n",
           mclk, config.pins.bclk, config.pins.ws, config.pins.dout, config.pins.sda,
           config.pins.scl, config.pins.amp_power_down, config.pins.octal_psram ? "yes" : "no");
    const chorus_board_controls_t *c = &config.controls;
    const uint32_t control_pins[] = {c->play_pause, c->volume_up, c->volume_down,     c->next,
                                     c->previous,   c->pairing,   c->status_led_data, c->mic_bclk,
                                     c->mic_ws,     c->mic_din,   c->mic_mute};
    char text[11][16];
    for (size_t i = 0; i < sizeof(control_pins) / sizeof(control_pins[0]); i++) {
        if (control_pins[i] == CHORUS_PIN_NONE) {
            snprintf(text[i], sizeof(text[i]), "none");
        } else {
            snprintf(text[i], sizeof(text[i]), "%u", (unsigned)control_pins[i]);
        }
    }
    printf("  controls       buttons play_pause=%s volume_up=%s volume_down=%s next=%s "
           "previous=%s pairing=%s, status_led=%s on %s, mic i2s1 bclk=%s ws=%s din=%s "
           "mute=%s\n",
           text[0], text[1], text[2], text[3], text[4], text[5],
           chorus_status_led_name(c->status_led), text[6], text[7], text[8], text[9], text[10]);
    printf("  analog gain    %.3f dB requested against a ceiling of %.3f dB\n", config.gain.db,
           config.amp.analog_gain_ceiling_db);
    chorus_amp_key_t keys[32];
    size_t key_count = chorus_amp_keys(&config.amp, keys, sizeof(keys) / sizeof(keys[0]));
    size_t unknown = 0;
    for (size_t i = 0; i < key_count; i++) {
        unknown += keys[i].byte->known ? 0u : 1u;
    }
    unknown += config.gain.code_known ? 0u : 1u;
    printf("  amplifier      i2c address 0x%02x, %zu register-map values, %zu DECLARED UNKNOWN, "
           "%u bit clocks per frame\n",
           config.amp.address.value, key_count + 1u, unknown, (unsigned)config.amp.sclk_per_frame);
    printf("  toolchain      esp-idf %s\n", config.espidf_version);
    return 0;
}
