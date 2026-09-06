/* Everything firmware/config/endpoint.conf declares, read once.
 *
 * One reader, so that a check and the thing it checks cannot drift apart. The
 * shell entry points under tools/ read the same file with the same key names. */

#ifndef CHORUS_ENDPOINT_CONFIG_H
#define CHORUS_ENDPOINT_CONFIG_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/amp.h"
#include "chorus/i2s.h"

#define CHORUS_ENDPOINT_TEXT 128

typedef struct {
    char server_address[CHORUS_ENDPOINT_TEXT];
    uint32_t reconnect_first_backoff_ms;
    uint32_t reconnect_max_backoff_ms;
    uint32_t outage_minutes_seconds;

    chorus_i2s_clock_t clock;
    chorus_pin_map_t pins;
    chorus_mem_placement_t dma_placement;

    chorus_amp_config_t amp;
    chorus_amp_gain_t gain;

    char espidf_version[CHORUS_ENDPOINT_TEXT];
} chorus_endpoint_config_t;

/* Load and type-check every value. Returns 0 on success; on failure `detail`
 * names the key, the value as written and the file. */
int chorus_endpoint_config_load(chorus_endpoint_config_t *out, const char *path, char *detail,
                                size_t detail_len);

/* The same, from text already in hand. The board has no filesystem, so the
 * committed configuration is embedded in the image and parsed through here;
 * `label` is what a diagnostic names as the source. One parser either way, so
 * the values a bench runs on are the values a host graded. */
int chorus_endpoint_config_parse(chorus_endpoint_config_t *out, const char *label,
                                 const char *text, char *detail, size_t detail_len);

/* Where firmware/config/endpoint.conf is, relative to the repository root
 * this build was configured with. */
const char *chorus_endpoint_config_default_path(void);

/* Append a finding for every rule the loaded configuration breaks: the clock
 * rules, the pin-map rules and the DMA-placement rule. Returns the number
 * appended. */
size_t chorus_endpoint_config_validate(const chorus_endpoint_config_t *config,
                                       chorus_finding_t *findings, size_t capacity,
                                       size_t *count);

#endif /* CHORUS_ENDPOINT_CONFIG_H */
