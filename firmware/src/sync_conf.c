#include "chorus/sync_conf.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "chorus/conf.h"

static int from_conf(chorus_sync_conf_t *out, const chorus_conf_t *conf, char *detail,
                     size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    if (chorus_conf_u32(conf, "filter_window", &out->filter_window, detail, detail_len) !=
            CHORUS_CONF_OK ||
        chorus_conf_f64(conf, "smoothing_alpha", &out->smoothing_alpha, detail, detail_len) !=
            CHORUS_CONF_OK ||
        chorus_conf_u32(conf, "hard_resync_threshold_us", &out->hard_resync_threshold_us, detail,
                        detail_len) != CHORUS_CONF_OK ||
        chorus_conf_f64(conf, "max_correction_ppm", &out->max_correction_ppm, detail, detail_len) !=
            CHORUS_CONF_OK ||
        chorus_conf_u32(conf, "staleness_limit_ms", &out->staleness_limit_ms, detail, detail_len) !=
            CHORUS_CONF_OK ||
        chorus_conf_u32(conf, "max_rtt_us", &out->max_rtt_us, detail, detail_len) !=
            CHORUS_CONF_OK ||
        chorus_conf_u32(conf, "sync_interval_ms", &out->sync_interval_ms, detail, detail_len) !=
            CHORUS_CONF_OK ||
        chorus_conf_u32(conf, "playout_latency_us", &out->playout_latency_us, detail, detail_len) !=
            CHORUS_CONF_OK ||
        chorus_conf_u32(conf, "mute_us", &out->mute_us, detail, detail_len) != CHORUS_CONF_OK) {
        return -1;
    }
    if (out->filter_window == 0 || out->sync_interval_ms == 0) {
        snprintf(detail, detail_len, "%s: filter_window and sync_interval_ms must be above zero",
                 conf->path);
        return -1;
    }
    return 0;
}

/* The parsed pairs are 64 KiB (firmware/include/chorus/conf.h: 128 pairs of
 * two 256-byte strings): too large for a task's stack and not worth keeping in
 * static RAM after start-up, so they are borrowed from the heap for the parse. */
int chorus_sync_conf_parse(chorus_sync_conf_t *out, const char *label, const char *text,
                           char *detail, size_t detail_len)
{
    chorus_conf_t *conf = malloc(sizeof(*conf));
    if (conf == NULL) {
        snprintf(detail, detail_len, "%s: no memory to parse the configuration", label);
        return -1;
    }
    int status = (chorus_conf_parse(conf, label, text, detail, detail_len) == CHORUS_CONF_OK)
                     ? from_conf(out, conf, detail, detail_len)
                     : -1;
    free(conf);
    return status;
}

int chorus_sync_conf_load(chorus_sync_conf_t *out, const char *path, char *detail,
                          size_t detail_len)
{
    chorus_conf_t *conf = malloc(sizeof(*conf));
    if (conf == NULL) {
        snprintf(detail, detail_len, "%s: no memory to parse the configuration", path);
        return -1;
    }
    int status = (chorus_conf_load(conf, path, detail, detail_len) == CHORUS_CONF_OK)
                     ? from_conf(out, conf, detail, detail_len)
                     : -1;
    free(conf);
    return status;
}
