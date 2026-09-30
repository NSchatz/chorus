/* config/sync.conf, read by the endpoint (audit A-12).
 *
 * The Linux client compiles these values as constants and a test holds them to
 * the file; the endpoint reads the file itself, embedded in the image as
 * firmware/config/endpoint.conf is, through the same reader the host build
 * uses. So when SYNC-4 tuning changes config/sync.conf, the endpoint changes
 * with it rather than keeping copied literals. */

#ifndef CHORUS_SYNC_CONF_H
#define CHORUS_SYNC_CONF_H

#include <stddef.h>
#include <stdint.h>

typedef struct {
    uint32_t filter_window;
    double smoothing_alpha;
    uint32_t hard_resync_threshold_us;
    double max_correction_ppm;
    uint32_t staleness_limit_ms;
    uint32_t max_rtt_us;
    uint32_t sync_interval_ms;
    uint32_t playout_latency_us;
    uint32_t mute_us;
} chorus_sync_conf_t;

/* Parse the text of config/sync.conf. Returns 0, or -1 with `detail` naming
 * the key and `label`. Every key above is required. */
int chorus_sync_conf_parse(chorus_sync_conf_t *out, const char *label, const char *text,
                           char *detail, size_t detail_len);

/* The same, from a file (the host build). */
int chorus_sync_conf_load(chorus_sync_conf_t *out, const char *path, char *detail,
                          size_t detail_len);

#endif /* CHORUS_SYNC_CONF_H */
