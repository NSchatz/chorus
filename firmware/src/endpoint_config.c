#include "chorus/endpoint_config.h"

#include "chorus/conf.h"
#include "chorus/line_dac.h"
#include "chorus/volume.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef CHORUS_ENDPOINT_CONFIG_PATH
#define CHORUS_ENDPOINT_CONFIG_PATH "firmware/config/endpoint.conf"
#endif

const char *chorus_endpoint_config_default_path(void)
{
    return CHORUS_ENDPOINT_CONFIG_PATH;
}

/* A byte that the configuration is allowed to declare `unknown`. `known` is
 * set to 0 in that case and the value is left at zero, which nothing reads. */
static int optional_u8(const chorus_conf_t *conf, const char *key, uint8_t *value, int *known,
                       char *detail, size_t detail_len)
{
    if (chorus_conf_get(conf, key) == NULL) {
        snprintf(detail, detail_len, "%s has no %s", conf->path, key);
        return -1;
    }
    if (chorus_conf_is_unknown(conf, key)) {
        *known = 0;
        *value = 0;
        return 0;
    }
    /* Written as decimal or as 0x-prefixed hex, because a register id read off
     * a datasheet is normally written in hex and transcribing it to decimal is
     * a place to make a mistake. */
    const char *text = chorus_conf_get(conf, key);
    unsigned parsed = 0;
    int consumed = 0;
    if (text[0] == '0' && (text[1] == 'x' || text[1] == 'X')) {
        if (sscanf(text, "%x%n", &parsed, &consumed) != 1 || text[consumed] != '\0') {
            snprintf(detail, detail_len, "%s: %s = %s is not a hex byte", conf->path, key, text);
            return -1;
        }
    } else {
        if (sscanf(text, "%u%n", &parsed, &consumed) != 1 || text[consumed] != '\0') {
            snprintf(detail, detail_len, "%s: %s = %s is not a byte", conf->path, key, text);
            return -1;
        }
    }
    if (parsed > 0xFF) {
        snprintf(detail, detail_len, "%s: %s = %s does not fit in a byte", conf->path, key, text);
        return -1;
    }
    *known = 1;
    *value = (uint8_t)parsed;
    return 0;
}

static int from_conf(chorus_endpoint_config_t *out, const chorus_conf_t *conf_in, const char *path,
                     char *detail, size_t detail_len);

const char *chorus_board_status_name(chorus_board_status_t status)
{
    return (status == CHORUS_BOARD_CONFIRMED) ? "confirmed" : "ASSUMED";
}

const char *chorus_audio_output_name(chorus_audio_output_t output)
{
    switch (output) {
    case CHORUS_AUDIO_OUTPUT_NONE:
        return "none";
    case CHORUS_AUDIO_OUTPUT_LINE_DAC:
        return "line-dac";
    case CHORUS_AUDIO_OUTPUT_AMPLIFIER:
        break;
    }
    return "amplifier";
}

const char *chorus_status_led_name(chorus_status_led_t led)
{
    return (led == CHORUS_STATUS_LED_WS2812) ? "ws2812" : "none";
}

/* A GPIO number, or `none` for a pin the board does not route out. */
static int read_pin(const chorus_conf_t *conf, const char *key, uint32_t *out, char *detail,
                    size_t detail_len)
{
    const char *text = chorus_conf_get(conf, key);
    if (text != NULL && strcmp(text, "none") == 0) {
        *out = CHORUS_PIN_NONE;
        return 0;
    }
    return (chorus_conf_u32(conf, key, out, detail, detail_len) == CHORUS_CONF_OK) ? 0 : -1;
}

/* The keys a board profile may set: what a board decides, and nothing that is
 * a platform rule, a credential or a pin of the toolchain. */
static int board_owned(const char *key)
{
    static const char *const prefixes[] = {"board_", "pin_", "eth_"};
    for (size_t i = 0; i < sizeof(prefixes) / sizeof(prefixes[0]); i++) {
        if (strncmp(key, prefixes[i], strlen(prefixes[i])) == 0) {
            return 1;
        }
    }
    return strcmp(key, "link_transport") == 0;
}

static int merge_profile(chorus_endpoint_config_t *out, chorus_conf_t *base_table,
                         chorus_conf_t *profile_table, const char *base_label,
                         const char *base_text, const char *profile_label, const char *profile_text,
                         char *detail, size_t detail_len);

int chorus_endpoint_config_parse_profile(chorus_endpoint_config_t *out, const char *base_label,
                                         const char *base_text, const char *profile_label,
                                         const char *profile_text, char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    /* On the heap and only for the length of this call: a table of 128 pairs
     * is 64 KiB, too large for a task's stack on the board and too large to
     * keep in static RAM for a reader that runs once at boot. */
    chorus_conf_t *tables = malloc(2 * sizeof(chorus_conf_t));
    if (tables == NULL) {
        snprintf(detail, detail_len, "no memory to read %s and %s", base_label, profile_label);
        return -1;
    }
    int rc = merge_profile(out, &tables[0], &tables[1], base_label, base_text, profile_label,
                           profile_text, detail, detail_len);
    free(tables);
    return rc;
}

static int merge_profile(chorus_endpoint_config_t *out, chorus_conf_t *base_table,
                         chorus_conf_t *profile_table, const char *base_label,
                         const char *base_text, const char *profile_label, const char *profile_text,
                         char *detail, size_t detail_len)
{
    chorus_conf_t *base = base_table;
    chorus_conf_t *profile_conf = profile_table;
    if (chorus_conf_parse(base, base_label, base_text, detail, detail_len) != CHORUS_CONF_OK ||
        chorus_conf_parse(profile_conf, profile_label, profile_text, detail, detail_len) !=
            CHORUS_CONF_OK) {
        return -1;
    }
    for (size_t i = 0; i < profile_conf->count; i++) {
        const char *key = profile_conf->pairs[i].key;
        if (!board_owned(key)) {
            snprintf(detail, detail_len,
                     "%s sets %s, which is not a board key (board_*, pin_*, eth_*, "
                     "link_transport); a board profile never carries a platform value",
                     profile_label, key);
            return -1;
        }
        size_t j = 0;
        while (j < base->count && strcmp(base->pairs[j].key, key) != 0) {
            j++;
        }
        if (j == base->count) {
            snprintf(detail, detail_len, "%s sets %s, which %s does not carry", profile_label, key,
                     base_label);
            return -1;
        }
        snprintf(base->pairs[j].value, sizeof(base->pairs[j].value), "%s",
                 profile_conf->pairs[i].value);
    }
    char label[CHORUS_CONF_MAX_TEXT];
    snprintf(label, sizeof(label), "%s + %s", base_label, profile_label);
    snprintf(base->path, sizeof(base->path), "%s", label);
    return from_conf(out, base, label, detail, detail_len);
}

static int read_text_file(const char *path, char *text, size_t text_len, char *detail,
                          size_t detail_len)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        snprintf(detail, detail_len, "%s could not be opened", path);
        return -1;
    }
    size_t read = fread(text, 1, text_len - 1, file);
    int whole = feof(file) || fgetc(file) == EOF;
    fclose(file);
    if (!whole) {
        snprintf(detail, detail_len, "%s is larger than %zu bytes", path, text_len - 1);
        return -1;
    }
    text[read] = '\0';
    return 0;
}

int chorus_endpoint_config_load_profile(chorus_endpoint_config_t *out, const char *base_path,
                                        const char *profile_path, char *detail, size_t detail_len)
{
    static char base[32768];
    static char profile[8192];
    if (read_text_file(base_path, base, sizeof(base), detail, detail_len) != 0 ||
        read_text_file(profile_path, profile, sizeof(profile), detail, detail_len) != 0) {
        memset(out, 0, sizeof(*out));
        return -1;
    }
    return chorus_endpoint_config_parse_profile(out, base_path, base, profile_path, profile, detail,
                                                detail_len);
}

int chorus_endpoint_config_load(chorus_endpoint_config_t *out, const char *path, char *detail,
                                size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    /* The pairs are 64 KiB (chorus/conf.h): too large for a task's stack and
     * not worth keeping in static RAM after start-up, so they are borrowed
     * from the heap for the parse and handed back. */
    chorus_conf_t *conf = malloc(sizeof(*conf));
    if (conf == NULL) {
        snprintf(detail, detail_len, "%s: no memory to parse the configuration", path);
        return -1;
    }
    int status = (chorus_conf_load(conf, path, detail, detail_len) == CHORUS_CONF_OK)
                     ? from_conf(out, conf, path, detail, detail_len)
                     : -1;
    free(conf);
    return status;
}

int chorus_endpoint_config_parse(chorus_endpoint_config_t *out, const char *label, const char *text,
                                 char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    chorus_conf_t *conf = malloc(sizeof(*conf));
    if (conf == NULL) {
        snprintf(detail, detail_len, "%s: no memory to parse the configuration", label);
        return -1;
    }
    int status = (chorus_conf_parse(conf, label, text, detail, detail_len) == CHORUS_CONF_OK)
                     ? from_conf(out, conf, label, detail, detail_len)
                     : -1;
    free(conf);
    return status;
}

static int from_conf(chorus_endpoint_config_t *out, const chorus_conf_t *conf_in, const char *path,
                     char *detail, size_t detail_len)
{
    /* Read through the caller's table, never a copy of it: a table is 64 KiB,
     * which a task's stack on the board does not have. */
    const chorus_conf_t *const conf = conf_in;

#define NEED(call)                                                                                 \
    do {                                                                                           \
        if ((call) != CHORUS_CONF_OK) {                                                            \
            return -1;                                                                             \
        }                                                                                          \
    } while (0)

    /* The board. Its model is ASSUMED until the owner's own board is read
     * (brief section 0.8), and an ASSUMED model names the Needs item it waits
     * on, so the assumption is visible wherever the model is. */
    NEED(chorus_conf_string(conf, "board_profile", out->board.profile, sizeof(out->board.profile),
                            detail, detail_len));
    NEED(chorus_conf_string(conf, "board_model", out->board.model, sizeof(out->board.model), detail,
                            detail_len));
    char status_text[CHORUS_ENDPOINT_TEXT];
    NEED(chorus_conf_string(conf, "board_model_status", status_text, sizeof(status_text), detail,
                            detail_len));
    if (strcmp(status_text, "ASSUMED") == 0) {
        out->board.model_status = CHORUS_BOARD_ASSUMED;
    } else if (strcmp(status_text, "confirmed") == 0) {
        out->board.model_status = CHORUS_BOARD_CONFIRMED;
    } else {
        snprintf(detail, detail_len, "%s: board_model_status = %s is not `ASSUMED` or `confirmed`",
                 conf->path, status_text);
        return -1;
    }
    NEED(chorus_conf_string(conf, "board_needs_item", out->board.needs_item,
                            sizeof(out->board.needs_item), detail, detail_len));
    NEED(chorus_conf_u32(conf, "board_flash_size_mb", &out->board.flash_size_mb, detail,
                         detail_len));
    char output_text[CHORUS_ENDPOINT_TEXT];
    NEED(chorus_conf_string(conf, "board_audio_output", output_text, sizeof(output_text), detail,
                            detail_len));
    if (strcmp(output_text, "amplifier") == 0) {
        out->board.audio_output = CHORUS_AUDIO_OUTPUT_AMPLIFIER;
    } else if (strcmp(output_text, "line-dac") == 0) {
        out->board.audio_output = CHORUS_AUDIO_OUTPUT_LINE_DAC;
    } else if (strcmp(output_text, "none") == 0) {
        out->board.audio_output = CHORUS_AUDIO_OUTPUT_NONE;
    } else {
        snprintf(detail, detail_len,
                 "%s: board_audio_output = %s is not `amplifier`, `line-dac` or `none`", conf->path,
                 output_text);
        return -1;
    }
    NEED(chorus_conf_u32(conf, "board_output_delay_frames", &out->board.output_delay_frames, detail,
                         detail_len));

    /* The W5500 (chorus/link.h). */
    char host_text[CHORUS_ENDPOINT_TEXT];
    NEED(
        chorus_conf_string(conf, "eth_spi_host", host_text, sizeof(host_text), detail, detail_len));
    out->eth.spi_host = chorus_spi_host_from_name(host_text);
    if (out->eth.spi_host == CHORUS_SPI_HOST_UNKNOWN) {
        snprintf(detail, detail_len, "%s: eth_spi_host = %s is not `spi2` or `spi3`", conf->path,
                 host_text);
        return -1;
    }
    NEED(chorus_conf_u32(conf, "eth_spi_clock_mhz", &out->eth.spi_clock_mhz, detail, detail_len));
    if (read_pin(conf, "pin_eth_sclk", &out->eth.sclk, detail, detail_len) != 0 ||
        read_pin(conf, "pin_eth_mosi", &out->eth.mosi, detail, detail_len) != 0 ||
        read_pin(conf, "pin_eth_miso", &out->eth.miso, detail, detail_len) != 0 ||
        read_pin(conf, "pin_eth_cs", &out->eth.cs, detail, detail_len) != 0 ||
        read_pin(conf, "pin_eth_int", &out->eth.int_pin, detail, detail_len) != 0 ||
        read_pin(conf, "pin_eth_rst", &out->eth.rst, detail, detail_len) != 0) {
        return -1;
    }
    NEED(chorus_conf_u32(conf, "link_address_timeout_ms", &out->eth.address_timeout_ms, detail,
                         detail_len));

    /* The link. A value declared `unknown` here is a fact about somebody's
     * house that this repository does not have, which is a third class beside
     * the values the endpoint phase FIXED and the register map it declared
     * unknown; `known` is 0 and the buffer is left empty, so there is no
     * default network anywhere for a join to fall back to. */
    char link_text[CHORUS_ENDPOINT_TEXT];
    NEED(chorus_conf_string(conf, "link_transport", link_text, sizeof(link_text), detail,
                            detail_len));
    int link_ok = 0;
    out->link.transport = chorus_transport_from_name(link_text, &link_ok);
    if (!link_ok) {
        snprintf(detail, detail_len,
                 "%s: link_transport = %s is not `wired`, `wireless` or `emulated`", conf->path,
                 link_text);
        return -1;
    }
    NEED(chorus_conf_string(conf, "link_wifi_power_save", link_text, sizeof(link_text), detail,
                            detail_len));
    out->link.power_save = chorus_wifi_ps_from_name(link_text, &link_ok);
    if (!link_ok) {
        snprintf(detail, detail_len,
                 "%s: link_wifi_power_save = %s is not `none`, `min-modem` or `max-modem`. It is "
                 "not a value this repository declares unknown either: the whole point of the "
                 "phase is that the mode is SET rather than inherited",
                 conf->path, link_text);
        return -1;
    }
    NEED(chorus_conf_bool(conf, "link_wifi_coexistence", &out->link.coexistence, detail,
                          detail_len));
    if (chorus_conf_get(conf, "link_wifi_ssid") == NULL) {
        snprintf(detail, detail_len, "%s has no link_wifi_ssid", conf->path);
        return -1;
    }
    if (chorus_conf_is_unknown(conf, "link_wifi_ssid")) {
        out->link.ssid_known = 0;
        out->link.ssid[0] = '\0';
    } else {
        NEED(chorus_conf_string(conf, "link_wifi_ssid", out->link.ssid, sizeof(out->link.ssid),
                                detail, detail_len));
        out->link.ssid_known = 1;
    }
    if (chorus_conf_get(conf, "link_wifi_secret") == NULL) {
        snprintf(detail, detail_len, "%s has no link_wifi_secret", conf->path);
        return -1;
    }
    if (chorus_conf_is_unknown(conf, "link_wifi_secret")) {
        out->link.secret_known = 0;
        out->link.secret[0] = '\0';
    } else {
        NEED(chorus_conf_string(conf, "link_wifi_secret", out->link.secret,
                                sizeof(out->link.secret), detail, detail_len));
        out->link.secret_known = 1;
    }
    snprintf(out->link.source, sizeof(out->link.source), "%s", path);

    NEED(chorus_conf_string(conf, "server_address", out->server_address,
                            sizeof(out->server_address), detail, detail_len));
    NEED(chorus_conf_u32(conf, "reconnect_first_backoff_ms", &out->reconnect_first_backoff_ms,
                         detail, detail_len));
    NEED(chorus_conf_u32(conf, "reconnect_max_backoff_ms", &out->reconnect_max_backoff_ms, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "outage_minutes_seconds", &out->outage_minutes_seconds, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "ota_confirm_seconds", &out->ota_confirm_seconds, detail,
                         detail_len));

    NEED(chorus_conf_u32(conf, "i2s_sample_rate_hz", &out->clock.sample_rate_hz, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "i2s_slot_bit_width", &out->clock.slot_bit_width, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "i2s_wire_slot_bit_width", &out->clock.wire_slot_bit_width, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "i2s_mclk_multiple", &out->clock.mclk_multiple, detail, detail_len));
    NEED(chorus_conf_u32(conf, "i2s_dma_frame_num", &out->clock.dma_frame_num, detail, detail_len));
    NEED(chorus_conf_u32(conf, "i2s_dma_desc_num", &out->clock.dma_desc_num, detail, detail_len));

    if (read_pin(conf, "pin_i2s_mclk", &out->pins.mclk, detail, detail_len) != 0 ||
        read_pin(conf, "pin_i2s_bclk", &out->pins.bclk, detail, detail_len) != 0 ||
        read_pin(conf, "pin_i2s_ws", &out->pins.ws, detail, detail_len) != 0 ||
        read_pin(conf, "pin_i2s_dout", &out->pins.dout, detail, detail_len) != 0 ||
        read_pin(conf, "pin_i2c_sda", &out->pins.sda, detail, detail_len) != 0 ||
        read_pin(conf, "pin_i2c_scl", &out->pins.scl, detail, detail_len) != 0 ||
        read_pin(conf, "pin_amp_power_down", &out->pins.amp_power_down, detail, detail_len) != 0 ||
        read_pin(conf, "pin_marker", &out->pins.marker, detail, detail_len) != 0) {
        return -1;
    }
    NEED(chorus_conf_u32(conf, "marker_period_ms", &out->marker_period_ms, detail, detail_len));
    if (out->marker_period_ms == 0) {
        snprintf(detail, detail_len,
                 "marker_period_ms = 0: the marker's period must be at least 1 ms (set "
                 "pin_marker = none to turn the marker off)");
        return -1;
    }
    NEED(chorus_conf_bool(conf, "board_octal_psram", &out->pins.octal_psram, detail, detail_len));

    /* The controls, the status light and the microphone (docs/hardware/controls.md). */
    chorus_board_controls_t *controls = &out->controls;
    if (read_pin(conf, "pin_button_play_pause", &controls->play_pause, detail, detail_len) != 0 ||
        read_pin(conf, "pin_button_volume_up", &controls->volume_up, detail, detail_len) != 0 ||
        read_pin(conf, "pin_button_volume_down", &controls->volume_down, detail, detail_len) != 0 ||
        read_pin(conf, "pin_button_next", &controls->next, detail, detail_len) != 0 ||
        read_pin(conf, "pin_button_previous", &controls->previous, detail, detail_len) != 0 ||
        read_pin(conf, "pin_button_pairing", &controls->pairing, detail, detail_len) != 0 ||
        read_pin(conf, "pin_knob_level", &controls->knob_level, detail, detail_len) != 0 ||
        read_pin(conf, "pin_knob_phase", &controls->knob_phase, detail, detail_len) != 0 ||
        read_pin(conf, "pin_status_led", &controls->status_led_data, detail, detail_len) != 0 ||
        read_pin(conf, "pin_mic_bclk", &controls->mic_bclk, detail, detail_len) != 0 ||
        read_pin(conf, "pin_mic_ws", &controls->mic_ws, detail, detail_len) != 0 ||
        read_pin(conf, "pin_mic_din", &controls->mic_din, detail, detail_len) != 0 ||
        read_pin(conf, "pin_mic_mute", &controls->mic_mute, detail, detail_len) != 0) {
        return -1;
    }
    char led_text[CHORUS_ENDPOINT_TEXT];
    NEED(chorus_conf_string(conf, "board_status_led", led_text, sizeof(led_text), detail,
                            detail_len));
    if (strcmp(led_text, "none") == 0) {
        controls->status_led = CHORUS_STATUS_LED_NONE;
    } else if (strcmp(led_text, "ws2812") == 0) {
        controls->status_led = CHORUS_STATUS_LED_WS2812;
    } else {
        snprintf(detail, detail_len, "%s: board_status_led = %s is not `none` or `ws2812`",
                 conf->path, led_text);
        return -1;
    }

    char placement[CHORUS_ENDPOINT_TEXT];
    NEED(chorus_conf_string(conf, "dma_descriptor_placement", placement, sizeof(placement), detail,
                            detail_len));
    int placement_ok = 0;
    out->dma_placement = chorus_mem_placement_from_name(placement, &placement_ok);
    if (!placement_ok) {
        snprintf(detail, detail_len,
                 "%s: dma_descriptor_placement = %s is not `internal` or `external`", conf->path,
                 placement);
        return -1;
    }

    NEED(chorus_conf_f64(conf, "amp_analog_gain_ceiling_db", &out->amp.analog_gain_ceiling_db,
                         detail, detail_len));
    NEED(chorus_conf_f64(conf, "amp_analog_gain_db", &out->gain.db, detail, detail_len));
    snprintf(out->amp.ceiling_source, sizeof(out->amp.ceiling_source), "%s", path);

    /* Every register-map byte, by the same table the sequencer refuses from, so
     * a key cannot be loaded under one name and refused under another. */
    chorus_amp_key_t keys[32];
    size_t key_count = chorus_amp_keys(&out->amp, keys, sizeof(keys) / sizeof(keys[0]));
    for (size_t i = 0; i < key_count; i++) {
        chorus_amp_byte_t *byte = (chorus_amp_byte_t *)keys[i].byte;
        if (optional_u8(conf, keys[i].key, &byte->value, &byte->known, detail, detail_len) != 0) {
            return -1;
        }
    }
    if (optional_u8(conf, "amp_analog_gain_code", &out->gain.code, &out->gain.code_known, detail,
                    detail_len) != 0) {
        return -1;
    }
    NEED(chorus_conf_u32(conf, "amp_power_up_wait_ms", &out->amp.power_up_wait_ms, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "amp_dsp_settle_wait_ms", &out->amp.dsp_settle_wait_ms, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "amp_shutdown_wait_ms", &out->amp.shutdown_wait_ms, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "amp_sclk_per_frame", &out->amp.sclk_per_frame, detail, detail_len));

    /* The endpoint's own volume ceiling (goal 11, chorus/volume.h): nothing on
     * the wire takes the endpoint above it. Written as a decimal, 0 to 1. */
    const char *max_volume = chorus_conf_get(conf, "max_volume");
    if (max_volume == NULL) {
        snprintf(detail, detail_len, "%s has no max_volume", conf->path);
        return -1;
    }
    if (chorus_volume_parse(max_volume, &out->max_volume_thousandths) != 0) {
        snprintf(detail, detail_len,
                 "%s: max_volume = %s is not a decimal from 0 to 1 with at most three places "
                 "(for example 1.000 or 0.5)",
                 conf->path, max_volume);
        return -1;
    }

    /* The two-way split (goal 12, chorus/endpoint_dsp.h): the speaker's own
     * drivers. Read whether or not it is on, so a value that is wrong is
     * refused before anyone turns it on. */
    const char *two_way = chorus_conf_get(conf, "two_way");
    if (two_way == NULL) {
        snprintf(detail, detail_len, "%s has no two_way", conf->path);
        return -1;
    }
    if (strcmp(two_way, "on") == 0) {
        out->two_way.enabled = true;
    } else if (strcmp(two_way, "off") == 0) {
        out->two_way.enabled = false;
    } else {
        snprintf(detail, detail_len, "%s: two_way = %s is not `on` or `off`", conf->path, two_way);
        return -1;
    }
    NEED(chorus_conf_u32(conf, "two_way_crossover_hz", &out->two_way.crossover_hz, detail,
                         detail_len));
    /* The chain's own corner rule (chorus/dsp.h): 20 Hz up to 0.45 x the rate,
     * here the I2S rate every stream this endpoint plays is at. */
    if (out->two_way.crossover_hz < 20u ||
        (double)out->two_way.crossover_hz > 0.45 * (double)out->clock.sample_rate_hz) {
        snprintf(detail, detail_len,
                 "%s: two_way_crossover_hz = %u is outside 20 Hz to 0.45 x i2s_sample_rate_hz "
                 "(%u Hz)",
                 conf->path, (unsigned)out->two_way.crossover_hz,
                 (unsigned)(0.45 * (double)out->clock.sample_rate_hz));
        return -1;
    }
    uint32_t woofer = 0, tweeter = 0;
    NEED(chorus_conf_u32(conf, "two_way_woofer_slot", &woofer, detail, detail_len));
    NEED(chorus_conf_u32(conf, "two_way_tweeter_slot", &tweeter, detail, detail_len));
    if (woofer > 1u || tweeter > 1u || woofer == tweeter) {
        snprintf(detail, detail_len,
                 "%s: two_way_woofer_slot = %u and two_way_tweeter_slot = %u are not the two I2S "
                 "slots, 0 and 1, one each",
                 conf->path, (unsigned)woofer, (unsigned)tweeter);
        return -1;
    }
    out->two_way.woofer_slot = (uint8_t)woofer;
    out->two_way.tweeter_slot = (uint8_t)tweeter;

    NEED(chorus_conf_string(conf, "espidf_version", out->espidf_version,
                            sizeof(out->espidf_version), detail, detail_len));

#undef NEED
    return 0;
}

#define CONTROL_PIN_COUNT 13
#define BOARD_PIN_MAX 27 /* 13 controls, eight audio pins and six Ethernet pins */

/* Every pin the profile names but the controls', for the one-signal-per-pin
 * rule below; the Ethernet pins only when the link drives them. */
static size_t board_pins(const chorus_endpoint_config_t *config, const char **names, uint32_t *pins)
{
    const chorus_board_controls_t *c = &config->controls;
    const chorus_pin_map_t *p = &config->pins;
    const chorus_eth_config_t *e = &config->eth;
    const char *const control_names[CONTROL_PIN_COUNT] = {
        "pin_button_play_pause", "pin_button_volume_up", "pin_button_volume_down",
        "pin_button_next",       "pin_button_previous",  "pin_button_pairing",
        "pin_knob_level",        "pin_knob_phase",       "pin_status_led",
        "pin_mic_bclk",          "pin_mic_ws",           "pin_mic_din",
        "pin_mic_mute"};
    const uint32_t control_pins[CONTROL_PIN_COUNT] = {
        c->play_pause, c->volume_up,  c->volume_down,     c->next,     c->previous, c->pairing,
        c->knob_level, c->knob_phase, c->status_led_data, c->mic_bclk, c->mic_ws,   c->mic_din,
        c->mic_mute};
    size_t n = 0;
    for (size_t i = 0; i < CONTROL_PIN_COUNT; i++) {
        names[n] = control_names[i];
        pins[n++] = control_pins[i];
    }
    const char *const audio_names[] = {"pin_i2s_mclk",       "pin_i2s_bclk", "pin_i2s_ws",
                                       "pin_i2s_dout",       "pin_i2c_sda",  "pin_i2c_scl",
                                       "pin_amp_power_down", "pin_marker"};
    const uint32_t audio_pins[] = {p->mclk, p->bclk,           p->ws,    p->dout, p->sda,
                                   p->scl,  p->amp_power_down, p->marker};
    for (size_t i = 0; i < sizeof(audio_pins) / sizeof(audio_pins[0]) && n < BOARD_PIN_MAX; i++) {
        names[n] = audio_names[i];
        pins[n++] = audio_pins[i];
    }
    if (config->link.transport == CHORUS_TRANSPORT_WIRED) {
        const char *const eth_names[] = {"pin_eth_sclk", "pin_eth_mosi", "pin_eth_miso",
                                         "pin_eth_cs",   "pin_eth_int",  "pin_eth_rst"};
        const uint32_t eth_pins[] = {e->sclk, e->mosi, e->miso, e->cs, e->int_pin, e->rst};
        for (size_t i = 0; i < sizeof(eth_pins) / sizeof(eth_pins[0]) && n < BOARD_PIN_MAX; i++) {
            names[n] = eth_names[i];
            pins[n++] = eth_pins[i];
        }
    }
    return n;
}

static void add_finding(chorus_finding_t *findings, size_t capacity, size_t *count,
                        const char *rule, const char *detail)
{
    if (*count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule), "%s", rule);
        snprintf(findings[*count].detail, sizeof(findings[*count].detail), "%s", detail);
        (*count)++;
    }
}

/* The controls' rules: each pin is held to the GPIO rules every other pin is,
 * no control shares a pin with any signal, and the parts that come as a set
 * are wired as a set. */
static void validate_controls(const chorus_endpoint_config_t *config, chorus_finding_t *findings,
                              size_t capacity, size_t *count)
{
    const char *names[BOARD_PIN_MAX];
    uint32_t pins[BOARD_PIN_MAX];
    size_t n = board_pins(config, names, pins);
    char detail[CHORUS_FINDING_TEXT];

    for (size_t i = 0; i < CONTROL_PIN_COUNT; i++) {
        chorus_gpio_validate(names[i], pins[i], config->pins.octal_psram, findings, capacity,
                             count);
    }
    /* The controls come first in the list, so every pair with a control in it
     * is (i, j) with i a control; the pairs of the other pins are the pin map's
     * and the link's own rules. */
    for (size_t i = 0; i < CONTROL_PIN_COUNT; i++) {
        if (pins[i] == CHORUS_PIN_NONE) {
            continue;
        }
        for (size_t j = i + 1; j < n; j++) {
            if (pins[i] == pins[j]) {
                snprintf(detail, sizeof(detail), "%s and %s are both GPIO%u", names[i], names[j],
                         (unsigned)pins[i]);
                add_finding(findings, capacity, count, "gpio-assigned-twice", detail);
            }
        }
    }

    const chorus_board_controls_t *c = &config->controls;
    /* A knob is read by ADC1, which is GPIO1 to GPIO10 on the ESP32-S3 (ESP-IDF
     * v6.1 components/soc/esp32s3/include/soc/adc_channel.h: ADC1_GPIO1_CHANNEL
     * 0 to ADC1_GPIO10_CHANNEL 9). ADC2 is not offered: its reads contend with
     * the radio on a Wi-Fi board. */
    const struct {
        const char *name;
        uint32_t pin;
    } knobs[] = {{"pin_knob_level", c->knob_level}, {"pin_knob_phase", c->knob_phase}};
    for (size_t i = 0; i < sizeof(knobs) / sizeof(knobs[0]); i++) {
        if (knobs[i].pin != CHORUS_PIN_NONE && (knobs[i].pin < 1 || knobs[i].pin > 10)) {
            snprintf(detail, sizeof(detail),
                     "%s = %u; a knob is read by ADC1, GPIO1 to GPIO10 on the ESP32-S3",
                     knobs[i].name, (unsigned)knobs[i].pin);
            add_finding(findings, capacity, count, "knob-not-on-adc1", detail);
        }
    }
    if ((c->status_led == CHORUS_STATUS_LED_NONE) != (c->status_led_data == CHORUS_PIN_NONE)) {
        snprintf(detail, sizeof(detail),
                 "board_status_led = %s with pin_status_led %s; a status light has a data pin "
                 "and a data pin has a light",
                 chorus_status_led_name(c->status_led),
                 c->status_led_data == CHORUS_PIN_NONE ? "= none" : "set");
        add_finding(findings, capacity, count, "status-led-half-wired", detail);
    }
    size_t mic_routed = (c->mic_bclk != CHORUS_PIN_NONE) + (c->mic_ws != CHORUS_PIN_NONE) +
                        (c->mic_din != CHORUS_PIN_NONE);
    if (mic_routed != 0 && mic_routed != 3) {
        add_finding(findings, capacity, count, "microphone-half-wired",
                    "pin_mic_bclk, pin_mic_ws and pin_mic_din are all set or all `none`: an I2S "
                    "microphone needs its bit clock, word select and data");
    }
    /* docs/hardware/controls.md: a microphone is behind a hardware mute switch
     * whose second pole the firmware reads, and a switch reads nothing alone. */
    if (mic_routed == 3 && c->mic_mute == CHORUS_PIN_NONE) {
        add_finding(findings, capacity, count, "microphone-without-mute-switch",
                    "the microphone is wired and pin_mic_mute = none; a speaker's microphone is "
                    "behind a hardware mute switch whose second pole goes to a GPIO "
                    "(docs/hardware/controls.md)");
    }
    if (mic_routed == 0 && c->mic_mute != CHORUS_PIN_NONE) {
        add_finding(findings, capacity, count, "mute-switch-without-microphone",
                    "pin_mic_mute is set and no microphone is wired (pin_mic_bclk, pin_mic_ws, "
                    "pin_mic_din = none)");
    }
}

size_t chorus_endpoint_config_validate(const chorus_endpoint_config_t *config,
                                       chorus_finding_t *findings, size_t capacity, size_t *count)
{
    size_t before = *count;
    chorus_i2s_validate_clock(&config->clock, findings, capacity, count);
    chorus_pin_map_validate(&config->pins, findings, capacity, count);
    chorus_dma_validate_placement(config->dma_placement, findings, capacity, count);
    chorus_link_validate(&config->link, &config->eth, &config->pins, findings, capacity, count);
    validate_controls(config, findings, capacity, count);

    /* Only the I2S master clock may be left unrouted: every other audio pin
     * carries a signal the amplifier cannot run without. A line DAC has no
     * control bus (chorus/line_dac.h), so its board routes no I2C at all, and
     * its mute is on the power-down pin. */
    const int line_dac = config->board.audio_output == CHORUS_AUDIO_OUTPUT_LINE_DAC;
    const uint32_t required[] = {config->pins.bclk, config->pins.ws,  config->pins.dout,
                                 config->pins.sda,  config->pins.scl, config->pins.amp_power_down};
    for (size_t i = 0; i < sizeof(required) / sizeof(required[0]); i++) {
        int bus = i == 3 || i == 4; /* pin_i2c_sda, pin_i2c_scl */
        if (required[i] == CHORUS_PIN_NONE && !(line_dac && bus) && *count < capacity) {
            snprintf(findings[*count].rule, sizeof(findings[*count].rule), "audio-pin-not-routed");
            snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                     "an I2S, I2C or amplifier power-down pin reads `none`; only pin_i2s_mclk "
                     "may be left unrouted");
            (*count)++;
        }
    }
    if (line_dac && (config->pins.sda != CHORUS_PIN_NONE || config->pins.scl != CHORUS_PIN_NONE) &&
        *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "line-dac-with-a-control-bus");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "board_audio_output = line-dac with pin_i2c_sda or pin_i2c_scl set; the line "
                 "DAC (PCM5102A) has no control bus, so both read `none`");
        (*count)++;
    }
    /* The line DAC's filter delays every sample by its group delay, and the
     * playout path counts the profile's figure in the device delay: the two
     * agree, or the subwoofer plays that far from its sync target. */
    if (line_dac && config->board.output_delay_frames != CHORUS_LINE_DAC_GROUP_DELAY_FRAMES &&
        *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "line-dac-output-delay-not-the-datasheets");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "board_output_delay_frames = %u with board_audio_output = line-dac; the "
                 "PCM5102A's normal filter delays its output %u frames (SLAS859C Table 4, p. 17)",
                 (unsigned)config->board.output_delay_frames,
                 (unsigned)CHORUS_LINE_DAC_GROUP_DELAY_FRAMES);
        (*count)++;
    }

    if (config->board.flash_size_mb < CHORUS_MIN_FLASH_MB && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule), "board-flash-too-small");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "board_flash_size_mb = %u; the image's two 3 MiB update slots need the 8 MB of "
                 "flash the build declares (firmware/partitions.csv, firmware/sdkconfig.defaults)",
                 (unsigned)config->board.flash_size_mb);
        (*count)++;
    }
    /* The emulator and the missing amplifier go together, both ways (goal 14):
     * a speaker's profile cannot switch its amplifier's bring-up off, and the
     * emulator, which has neither I2C nor I2S, cannot be asked to drive one. */
    int emulated = config->link.transport == CHORUS_TRANSPORT_EMULATED;
    int silent = config->board.audio_output == CHORUS_AUDIO_OUTPUT_NONE;
    if (silent && !emulated && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "no-audio-output-on-a-speaker");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "board_audio_output = none with link_transport = %s; only the emulated board "
                 "(link_transport = emulated) has no amplifier, and a speaker's amplifier "
                 "bring-up is never skipped",
                 chorus_transport_name(config->link.transport));
        (*count)++;
    }
    if (emulated && !silent && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "emulated-link-with-an-amplifier");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "link_transport = emulated with board_audio_output = %s; the emulator has no "
                 "I2C and no I2S, so the emulated board declares board_audio_output = none",
                 chorus_audio_output_name(config->board.audio_output));
        (*count)++;
    }
    /* A trial too short rolls back a good image before its link is up; one
     * with no end is no trial. */
    if ((config->ota_confirm_seconds < 10 || config->ota_confirm_seconds > 3600) &&
        *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "ota-confirm-window-out-of-range");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "ota_confirm_seconds = %u; a new image's trial is 10 to 3600 seconds",
                 (unsigned)config->ota_confirm_seconds);
        (*count)++;
    }
    if (config->board.model_status == CHORUS_BOARD_ASSUMED &&
        (config->board.needs_item[0] == '\0' || strcmp(config->board.needs_item, "none") == 0) &&
        *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "assumed-board-names-no-needs-item");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "board_model = %s is ASSUMED and board_needs_item names no Needs item; an "
                 "assumption names the owner's answer it waits on (brief section 0.8)",
                 config->board.model);
        (*count)++;
    }

    /* The requested gain is configuration too, so a file that asks for more
     * than its own ceiling is refused where every other configuration rule is,
     * rather than only at bring-up. */
    if (config->gain.db > config->amp.analog_gain_ceiling_db && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule), "analog-gain-above-ceiling");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "amp_analog_gain_db = %.3f is above amp_analog_gain_ceiling_db = %.3f, both "
                 "declared in %s",
                 config->gain.db, config->amp.analog_gain_ceiling_db, config->amp.ceiling_source);
        (*count)++;
    }

    /* The bit clock per frame the I2S configuration produces has to be the one
     * the amplifier is committed to (amp_sclk_per_frame, a ratio the TAS5825M
     * datasheet lists as supported, pp. 7 and 29), or the part reports a clock
     * error and stays in Hi-Z (p. 29); on a line DAC, one its PLL runs from. */
    uint64_t per_frame = (config->clock.sample_rate_hz == 0)
                             ? 0
                             : chorus_i2s_bclk_hz(&config->clock) / config->clock.sample_rate_hz;
    if (line_dac && !chorus_line_dac_bck_ratio_ok(per_frame) && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "bclk-ratio-unsupported-by-line-dac");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "i2s_sample_rate_hz, i2s_slot_bit_width and i2s_wire_slot_bit_width give %llu bit "
                 "clocks per frame; the PCM5102A's PLL runs from 32 or 64 (SLAS859C Table 11, "
                 "p. 25)",
                 (unsigned long long)per_frame);
        (*count)++;
    }
    if (!line_dac && per_frame != config->amp.sclk_per_frame && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "bclk-ratio-unsupported-by-amplifier");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "i2s_sample_rate_hz, i2s_slot_bit_width and i2s_wire_slot_bit_width give %llu bit "
                 "clocks per frame and amp_sclk_per_frame = %u",
                 (unsigned long long)per_frame, (unsigned)config->amp.sclk_per_frame);
        (*count)++;
    }

    return *count - before;
}
