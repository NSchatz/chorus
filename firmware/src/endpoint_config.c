#include "chorus/endpoint_config.h"

#include "chorus/conf.h"

#include <stdio.h>
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

static int from_conf(chorus_endpoint_config_t *out, const chorus_conf_t *conf_in,
                     const char *path, char *detail, size_t detail_len);

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

int chorus_endpoint_config_parse(chorus_endpoint_config_t *out, const char *label,
                                 const char *text, char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    chorus_conf_t conf;
    if (chorus_conf_parse(&conf, label, text, detail, detail_len) != CHORUS_CONF_OK) {
        return -1;
    }
    return from_conf(out, &conf, label, detail, detail_len);
}

static int from_conf(chorus_endpoint_config_t *out, const chorus_conf_t *conf_in,
                     const char *path, char *detail, size_t detail_len)
{
    const chorus_conf_t conf = *conf_in;

#define NEED(call)                                                                                 \
    do {                                                                                           \
        if ((call) != CHORUS_CONF_OK) {                                                            \
            return -1;                                                                             \
        }                                                                                          \
    } while (0)

    NEED(chorus_conf_string(&conf, "server_address", out->server_address,
                            sizeof(out->server_address), detail, detail_len));
    NEED(chorus_conf_u32(&conf, "reconnect_first_backoff_ms", &out->reconnect_first_backoff_ms,
                         detail, detail_len));
    NEED(chorus_conf_u32(&conf, "reconnect_max_backoff_ms", &out->reconnect_max_backoff_ms, detail,
                         detail_len));
    NEED(chorus_conf_u32(&conf, "outage_minutes_seconds", &out->outage_minutes_seconds, detail,
                         detail_len));

    NEED(chorus_conf_u32(&conf, "i2s_sample_rate_hz", &out->clock.sample_rate_hz, detail,
                         detail_len));
    NEED(chorus_conf_u32(&conf, "i2s_slot_bit_width", &out->clock.slot_bit_width, detail,
                         detail_len));
    NEED(chorus_conf_u32(&conf, "i2s_mclk_multiple", &out->clock.mclk_multiple, detail,
                         detail_len));
    NEED(chorus_conf_u32(&conf, "i2s_dma_frame_num", &out->clock.dma_frame_num, detail,
                         detail_len));
    NEED(chorus_conf_u32(&conf, "i2s_dma_desc_num", &out->clock.dma_desc_num, detail, detail_len));

    NEED(chorus_conf_u32(&conf, "pin_i2s_mclk", &out->pins.mclk, detail, detail_len));
    NEED(chorus_conf_u32(&conf, "pin_i2s_bclk", &out->pins.bclk, detail, detail_len));
    NEED(chorus_conf_u32(&conf, "pin_i2s_ws", &out->pins.ws, detail, detail_len));
    NEED(chorus_conf_u32(&conf, "pin_i2s_dout", &out->pins.dout, detail, detail_len));
    NEED(chorus_conf_u32(&conf, "pin_i2c_sda", &out->pins.sda, detail, detail_len));
    NEED(chorus_conf_u32(&conf, "pin_i2c_scl", &out->pins.scl, detail, detail_len));
    NEED(chorus_conf_u32(&conf, "pin_amp_power_down", &out->pins.amp_power_down, detail,
                         detail_len));
    NEED(chorus_conf_bool(&conf, "board_octal_psram", &out->pins.octal_psram, detail, detail_len));

    char placement[CHORUS_ENDPOINT_TEXT];
    NEED(chorus_conf_string(&conf, "dma_descriptor_placement", placement, sizeof(placement),
                            detail, detail_len));
    int placement_ok = 0;
    out->dma_placement = chorus_mem_placement_from_name(placement, &placement_ok);
    if (!placement_ok) {
        snprintf(detail, detail_len,
                 "%s: dma_descriptor_placement = %s is not `internal` or `external`", conf.path,
                 placement);
        return -1;
    }

    NEED(chorus_conf_f64(&conf, "amp_analog_gain_ceiling_db", &out->amp.analog_gain_ceiling_db,
                         detail, detail_len));
    NEED(chorus_conf_f64(&conf, "amp_analog_gain_db", &out->gain.db, detail, detail_len));
    snprintf(out->amp.ceiling_source, sizeof(out->amp.ceiling_source), "%s", path);

    if (optional_u8(&conf, "amp_i2c_address", &out->amp.address, &out->amp.address_known, detail,
                    detail_len) != 0 ||
        optional_u8(&conf, "amp_reg_device_id", &out->amp.reg_device_id,
                    &out->amp.reg_device_id_known, detail, detail_len) != 0 ||
        optional_u8(&conf, "amp_reg_fault", &out->amp.reg_fault, &out->amp.reg_fault_known, detail,
                    detail_len) != 0 ||
        optional_u8(&conf, "amp_reg_analog_gain", &out->amp.reg_analog_gain,
                    &out->amp.reg_analog_gain_known, detail, detail_len) != 0 ||
        optional_u8(&conf, "amp_reg_state_control", &out->amp.reg_state_control,
                    &out->amp.reg_state_control_known, detail, detail_len) != 0 ||
        optional_u8(&conf, "amp_device_id_value", &out->amp.device_id_value,
                    &out->amp.device_id_value_known, detail, detail_len) != 0 ||
        optional_u8(&conf, "amp_fault_clear_value", &out->amp.fault_clear_value,
                    &out->amp.fault_clear_value_known, detail, detail_len) != 0 ||
        optional_u8(&conf, "amp_analog_gain_code", &out->gain.code, &out->gain.code_known, detail,
                    detail_len) != 0) {
        return -1;
    }

    NEED(chorus_conf_string(&conf, "espidf_version", out->espidf_version,
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

    /* The requested gain is configuration too, so a file that asks for more
     * than its own ceiling is refused where every other configuration rule is,
     * rather than only at bring-up. */
    if (config->gain.db > config->amp.analog_gain_ceiling_db && *count < capacity) {
        snprintf(findings[*count].rule, sizeof(findings[*count].rule),
                 "analog-gain-above-ceiling");
        snprintf(findings[*count].detail, sizeof(findings[*count].detail),
                 "amp_analog_gain_db = %.3f is above amp_analog_gain_ceiling_db = %.3f, both "
                 "declared in %s",
                 config->gain.db, config->amp.analog_gain_ceiling_db, config->amp.ceiling_source);
        (*count)++;
    }

    return *count - before;
}
