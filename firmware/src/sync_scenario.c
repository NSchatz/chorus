#include "chorus/conf.h"
#include "chorus/sync.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* A committed simulator scenario, read from a file under `fixtures/sync/`.
 *
 * Mirrored from crates/sync/src/scenario.rs, including its refusals: an
 * unknown key is an error rather than a shrug, because a typo in a committed
 * fixture that silently fell back to a default would quietly weaken the
 * regression it exists to enforce. */

static const char *const KEYS[] = {
    "name",
    "seed",
    "duration_ms",
    "step_ms",
    "sync_interval_ms",
    "server_ppm",
    "client_ppm",
    "initial_offset_ns",
    "base_one_way_delay_us",
    "jitter_model",
    "jitter_scale_us",
    "settle_deadline_ms",
    "error_bound_ns",
};

#define KEY_COUNT (sizeof(KEYS) / sizeof(KEYS[0]))

const char *chorus_scenario_status_name(chorus_scenario_status_t status)
{
    switch (status) {
    case CHORUS_SCENARIO_OK:
        return "ok";
    case CHORUS_SCENARIO_ERR_MALFORMED:
        return "malformed";
    case CHORUS_SCENARIO_ERR_DUPLICATE_KEY:
        return "duplicate-key";
    case CHORUS_SCENARIO_ERR_UNKNOWN_KEY:
        return "unknown-key";
    case CHORUS_SCENARIO_ERR_MISSING_KEY:
        return "missing-key";
    case CHORUS_SCENARIO_ERR_BAD_VALUE:
        return "bad-value";
    case CHORUS_SCENARIO_ERR_INVALID_CONFIG:
        return "invalid-config";
    case CHORUS_SCENARIO_ERR_UNREADABLE:
        return "unreadable";
    }
    return "unknown-status";
}

static void say_detail(char *detail, size_t detail_len, const char *fmt, const char *a,
                       const char *b)
{
    if (detail == NULL || detail_len == 0) {
        return;
    }
    snprintf(detail, detail_len, fmt, a, b);
}

static chorus_scenario_status_t from_conf(chorus_conf_status_t status)
{
    switch (status) {
    case CHORUS_CONF_OK:
        return CHORUS_SCENARIO_OK;
    case CHORUS_CONF_ERR_UNREADABLE:
        return CHORUS_SCENARIO_ERR_UNREADABLE;
    case CHORUS_CONF_ERR_DUPLICATE_KEY:
        return CHORUS_SCENARIO_ERR_DUPLICATE_KEY;
    case CHORUS_CONF_ERR_MISSING_KEY:
        return CHORUS_SCENARIO_ERR_MISSING_KEY;
    case CHORUS_CONF_ERR_BAD_VALUE:
        return CHORUS_SCENARIO_ERR_BAD_VALUE;
    default:
        return CHORUS_SCENARIO_ERR_MALFORMED;
    }
}

static chorus_scenario_status_t need_u64(const chorus_conf_t *conf, const char *key, uint64_t *out,
                                         char *detail, size_t detail_len)
{
    const char *value = chorus_conf_get(conf, key);
    if (value == NULL) {
        say_detail(detail, detail_len, "%s has no %s", conf->path, key);
        return CHORUS_SCENARIO_ERR_MISSING_KEY;
    }
    /* A leading minus is not a whole number here: strtoull would wrap it into
     * a very large positive one, which is how a scenario with a negative seed
     * would quietly become a different run. */
    if (value[0] == '-') {
        say_detail(detail, detail_len, "%s = %s is not an unsigned whole number", key, value);
        return CHORUS_SCENARIO_ERR_BAD_VALUE;
    }
    char *end = NULL;
    unsigned long long parsed = strtoull(value, &end, 10);
    if (end == value || *end != '\0') {
        say_detail(detail, detail_len, "%s = %s does not parse as a whole number", key, value);
        return CHORUS_SCENARIO_ERR_BAD_VALUE;
    }
    *out = (uint64_t)parsed;
    return CHORUS_SCENARIO_OK;
}

static chorus_scenario_status_t need_i64(const chorus_conf_t *conf, const char *key, int64_t *out,
                                         char *detail, size_t detail_len)
{
    const char *value = chorus_conf_get(conf, key);
    if (value == NULL) {
        say_detail(detail, detail_len, "%s has no %s", conf->path, key);
        return CHORUS_SCENARIO_ERR_MISSING_KEY;
    }
    char *end = NULL;
    long long parsed = strtoll(value, &end, 10);
    if (end == value || *end != '\0') {
        say_detail(detail, detail_len, "%s = %s does not parse as a whole number", key, value);
        return CHORUS_SCENARIO_ERR_BAD_VALUE;
    }
    *out = (int64_t)parsed;
    return CHORUS_SCENARIO_OK;
}

static chorus_scenario_status_t need_f64(const chorus_conf_t *conf, const char *key, double *out,
                                         char *detail, size_t detail_len)
{
    chorus_conf_status_t status = chorus_conf_f64(conf, key, out, detail, detail_len);
    if (status != CHORUS_CONF_OK) {
        return from_conf(status);
    }
    return CHORUS_SCENARIO_OK;
}

chorus_scenario_status_t chorus_scenario_parse(chorus_scenario_t *out, const char *text,
                                               char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));

    chorus_conf_t conf;
    chorus_conf_status_t conf_status = chorus_conf_parse(&conf, "<scenario>", text, detail,
                                                         detail_len);
    if (conf_status != CHORUS_CONF_OK) {
        return from_conf(conf_status);
    }

    for (size_t i = 0; i < conf.count; i++) {
        int known = 0;
        for (size_t k = 0; k < KEY_COUNT; k++) {
            if (strcmp(conf.pairs[i].key, KEYS[k]) == 0) {
                known = 1;
                break;
            }
        }
        if (!known) {
            say_detail(detail, detail_len, "unknown key %s%s", conf.pairs[i].key, "");
            return CHORUS_SCENARIO_ERR_UNKNOWN_KEY;
        }
    }

    const char *jitter_name = chorus_conf_get(&conf, "jitter_model");
    if (jitter_name == NULL) {
        say_detail(detail, detail_len, "missing key %s%s", "jitter_model", "");
        return CHORUS_SCENARIO_ERR_MISSING_KEY;
    }
    double jitter_scale_us = 0.0;
    chorus_scenario_status_t status =
        need_f64(&conf, "jitter_scale_us", &jitter_scale_us, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    int jitter_ok = 0;
    chorus_jitter_kind_t jitter_kind = chorus_jitter_from_name(jitter_name, &jitter_ok);
    if (!jitter_ok) {
        say_detail(detail, detail_len, "jitter_model = %s does not parse%s", jitter_name, "");
        return CHORUS_SCENARIO_ERR_BAD_VALUE;
    }
    out->config.jitter.kind = jitter_kind;
    out->config.jitter.scale_us = jitter_scale_us;

    status = need_u64(&conf, "seed", &out->config.seed, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    status = need_u64(&conf, "duration_ms", &out->config.duration_ms, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    status = need_u64(&conf, "step_ms", &out->config.step_ms, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    status = need_u64(&conf, "sync_interval_ms", &out->config.sync_interval_ms, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    status = need_f64(&conf, "server_ppm", &out->config.server_ppm, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    status = need_f64(&conf, "client_ppm", &out->config.client_ppm, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    status = need_i64(&conf, "initial_offset_ns", &out->config.initial_offset_ns, detail,
                      detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    status = need_f64(&conf, "base_one_way_delay_us", &out->config.base_one_way_delay_us, detail,
                      detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    out->config.servo = chorus_servo_config_default();

    if (chorus_sim_validate(&out->config) != CHORUS_SIM_OK) {
        say_detail(detail, detail_len, "the configuration is refused: %s%s",
                   chorus_sim_status_name(chorus_sim_validate(&out->config)), "");
        return CHORUS_SCENARIO_ERR_INVALID_CONFIG;
    }

    /* error_bound_ns is the one optional key, and it defaults to the 1 ms
     * FOUNDATION-1 committed. */
    const char *bound = chorus_conf_get(&conf, "error_bound_ns");
    if (bound == NULL) {
        out->error_bound_ns = 1000000;
    } else {
        status = need_i64(&conf, "error_bound_ns", &out->error_bound_ns, detail, detail_len);
        if (status != CHORUS_SCENARIO_OK) {
            return status;
        }
    }

    const char *name = chorus_conf_get(&conf, "name");
    if (name == NULL) {
        say_detail(detail, detail_len, "missing key %s%s", "name", "");
        return CHORUS_SCENARIO_ERR_MISSING_KEY;
    }
    snprintf(out->name, sizeof(out->name), "%s", name);

    status = need_u64(&conf, "settle_deadline_ms", &out->settle_deadline_ms, detail, detail_len);
    if (status != CHORUS_SCENARIO_OK) {
        return status;
    }
    return CHORUS_SCENARIO_OK;
}

chorus_scenario_status_t chorus_scenario_load(chorus_scenario_t *out, const char *path,
                                              char *detail, size_t detail_len)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        say_detail(detail, detail_len, "%s could not be opened%s", path, "");
        return CHORUS_SCENARIO_ERR_UNREADABLE;
    }
    static char text[65536];
    size_t read = fread(text, 1, sizeof(text) - 1, file);
    fclose(file);
    text[read] = '\0';
    return chorus_scenario_parse(out, text, detail, detail_len);
}

uint64_t chorus_scenario_settle_deadline_ns(const chorus_scenario_t *scenario)
{
    if (scenario->settle_deadline_ms > UINT64_MAX / 1000000ull) {
        return UINT64_MAX;
    }
    return scenario->settle_deadline_ms * 1000000ull;
}
