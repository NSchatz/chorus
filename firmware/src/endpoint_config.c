#include "chorus/endpoint_config.h"

#include "chorus/conf.h"

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
    static char base[16384];
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
    chorus_conf_t conf;
    if (chorus_conf_load(&conf, path, detail, detail_len) != CHORUS_CONF_OK) {
        return -1;
    }
    return from_conf(out, &conf, path, detail, detail_len);
}

int chorus_endpoint_config_parse(chorus_endpoint_config_t *out, const char *label, const char *text,
                                 char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    chorus_conf_t conf;
    if (chorus_conf_parse(&conf, label, text, detail, detail_len) != CHORUS_CONF_OK) {
        return -1;
    }
    return from_conf(out, &conf, label, detail, detail_len);
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
        snprintf(detail, detail_len, "%s: link_transport = %s is not `wired` or `wireless`",
                 conf->path, link_text);
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

    NEED(chorus_conf_u32(conf, "i2s_sample_rate_hz", &out->clock.sample_rate_hz, detail,
                         detail_len));
    NEED(chorus_conf_u32(conf, "i2s_slot_bit_width", &out->clock.slot_bit_width, detail,
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
        read_pin(conf, "pin_amp_power_down", &out->pins.amp_power_down, detail, detail_len) != 0) {
        return -1;
    }
    NEED(chorus_conf_bool(conf, "board_octal_psram", &out->pins.octal_psram, detail, detail_len));

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

    if (optional_u8(conf, "amp_i2c_address", &out->amp.address, &out->amp.address_known, detail,
                    detail_len) != 0 ||
        optional_u8(conf, "amp_reg_device_id", &out->amp.reg_device_id,
                    &out->amp.reg_device_id_known, detail, detail_len) != 0 ||
        optional_u8(conf, "amp_reg_fault", &out->amp.reg_fault, &out->amp.reg_fault_known, detail,
                    detail_len) != 0 ||
        optional_u8(conf, "amp_reg_analog_gain", &out->amp.reg_analog_gain,
                    &out->amp.reg_analog_gain_known, detail, detail_len) != 0 ||
        optional_u8(conf, "amp_reg_state_control", &out->amp.reg_state_control,
                    &out->amp.reg_state_control_known, detail, detail_len) != 0 ||
        optional_u8(conf, "amp_device_id_value", &out->amp.device_id_value,
                    &out->amp.device_id_value_known, detail, detail_len) != 0 ||
        optional_u8(conf, "amp_fault_clear_value", &out->amp.fault_clear_value,
                    &out->amp.fault_clear_value_known, detail, detail_len) != 0 ||
        optional_u8(conf, "amp_analog_gain_code", &out->gain.code, &out->gain.code_known, detail,
                    detail_len) != 0) {
        return -1;
    }

    NEED(chorus_conf_string(conf, "espidf_version", out->espidf_version,
                            sizeof(out->espidf_version), detail, detail_len));

#undef NEED
    return 0;
}

size_t chorus_endpoint_config_validate(const chorus_endpoint_config_t *config,
                                       chorus_finding_t *findings, size_t capacity, size_t *count)
{
    size_t before = *count;
    chorus_i2s_validate_clock(&config->clock, findings, capacity, count);
    chorus_pin_map_validate(&config->pins, findings, capacity, count);
    chorus_dma_validate_placement(config->dma_placement, findings, capacity, count);
    chorus_link_validate(&config->link, &config->eth, &config->pins, findings, capacity, count);

    /* Only the I2S master clock may be left unrouted: every other audio pin
     * carries a signal the amplifier cannot run without. */
    const uint32_t required[] = {config->pins.bclk, config->pins.ws,  config->pins.dout,
                                 config->pins.sda,  config->pins.scl, config->pins.amp_power_down};
    for (size_t i = 0; i < sizeof(required) / sizeof(required[0]); i++) {
        if (required[i] == CHORUS_PIN_NONE && *count < capacity) {
            snprintf(findings[*count].rule, sizeof(findings[*count].rule), "audio-pin-not-routed");
            snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                     "an I2S, I2C or amplifier power-down pin reads `none`; only pin_i2s_mclk "
                     "may be left unrouted");
            (*count)++;
        }
    }

    if (config->board.flash_size_mb < CHORUS_MIN_FLASH_MB && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule), "board-flash-too-small");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "board_flash_size_mb = %u; the image's 1.5 MB application partition sits in the "
                 "default 2 MB of flash (firmware/sdkconfig.defaults)",
                 (unsigned)config->board.flash_size_mb);
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

    return *count - before;
}
