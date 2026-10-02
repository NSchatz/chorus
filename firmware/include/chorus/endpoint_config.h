/* Everything firmware/config/endpoint.conf declares, read once.
 *
 * One reader, so that a check and the thing it checks cannot drift apart. The
 * shell entry points under tools/ read the same file with the same key names. */

#ifndef CHORUS_ENDPOINT_CONFIG_H
#define CHORUS_ENDPOINT_CONFIG_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/amp.h"
#include "chorus/endpoint_dsp.h"
#include "chorus/i2s.h"
#include "chorus/link.h"
#include "chorus/wifi.h"

#define CHORUS_ENDPOINT_TEXT 128

/* Whether a board profile's model is the owner's own board, read off it, or a
 * stand-in until the Needs item it names is answered (brief section 0.8). */
typedef enum {
    CHORUS_BOARD_ASSUMED = 0,
    CHORUS_BOARD_CONFIRMED
} chorus_board_status_t;

const char *chorus_board_status_name(chorus_board_status_t status);

/* The board a profile describes (firmware/boards/<profile>.conf). */
typedef struct {
    char profile[CHORUS_ENDPOINT_TEXT];
    char model[CHORUS_ENDPOINT_TEXT];
    chorus_board_status_t model_status;
    /* The Needs item an ASSUMED model waits on, verbatim. */
    char needs_item[CHORUS_ENDPOINT_TEXT];
    uint32_t flash_size_mb;
} chorus_board_t;

/* The smallest flash the image's layout fits: two 3 MiB app slots, otadata,
 * NVS and the PHY data end at 0x620000 (firmware/partitions.csv), and the
 * build declares 8 MB (firmware/sdkconfig.defaults,
 * CONFIG_ESPTOOLPY_FLASHSIZE_8MB; goal 14). The 8 MB itself is ASSUMED for the
 * owner's board until its module markings and a read-only chip report say. */
#define CHORUS_MIN_FLASH_MB 8u

typedef struct {
    /* The board this configuration is for, and how sure the repository is. */
    chorus_board_t board;

    /* The link: which transport it is, the power save mode to set on a radio,
     * and the network, whose name and secret this repository declares unknown.
     * `chorus#WIFI-7` owns every value in it. */
    chorus_wifi_config_t link;
    /* The W5500's wiring, read for every profile and driven only when the
     * link is wired (chorus/link.h). */
    chorus_eth_config_t eth;

    char server_address[CHORUS_ENDPOINT_TEXT];
    uint32_t reconnect_first_backoff_ms;
    uint32_t reconnect_max_backoff_ms;
    uint32_t outage_minutes_seconds;
    /* How long after boot a new image on trial has to reach its server before
     * it rolls itself back (chorus/ota.h rule 3). */
    uint32_t ota_confirm_seconds;

    chorus_i2s_clock_t clock;
    chorus_pin_map_t pins;
    chorus_mem_placement_t dma_placement;
    /* The GPIO marker's period on the server timeline (pins.marker drives it). */
    uint32_t marker_period_ms;

    chorus_amp_config_t amp;
    chorus_amp_gain_t gain;

    /* The endpoint's own volume ceiling, in thousandths of full amplitude
     * (`max_volume`; chorus/volume.h, ADR 0074). */
    uint32_t max_volume_thousandths;

    /* The endpoint's own two-way split (`two_way*`; goal 12,
     * chorus/endpoint_dsp.h): the speaker's drivers, not the room's sound. */
    chorus_endpoint_two_way_t two_way;

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
int chorus_endpoint_config_parse(chorus_endpoint_config_t *out, const char *label, const char *text,
                                 char *detail, size_t detail_len);

/* The same, with a board profile laid over the committed base first.
 *
 * A profile may set only keys the base already carries, and only the keys a
 * board decides (board_*, link_transport, pin_*, eth_*); any other key is
 * refused by name, so a profile can never carry a platform value (a clock
 * rule, a credential, a toolchain pin) past the base's review. The profile's
 * own `board_profile` names it. */
int chorus_endpoint_config_parse_profile(chorus_endpoint_config_t *out, const char *base_label,
                                         const char *base_text, const char *profile_label,
                                         const char *profile_text, char *detail, size_t detail_len);

/* The same, from two files. */
int chorus_endpoint_config_load_profile(chorus_endpoint_config_t *out, const char *base_path,
                                        const char *profile_path, char *detail, size_t detail_len);

/* Where firmware/config/endpoint.conf is, relative to the repository root
 * this build was configured with. */
const char *chorus_endpoint_config_default_path(void);

/* Append a finding for every rule the loaded configuration breaks: the clock
 * rules, the pin-map rules and the DMA-placement rule. Returns the number
 * appended. */
size_t chorus_endpoint_config_validate(const chorus_endpoint_config_t *config,
                                       chorus_finding_t *findings, size_t capacity, size_t *count);

#endif /* CHORUS_ENDPOINT_CONFIG_H */
