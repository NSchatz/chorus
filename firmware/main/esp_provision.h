/* Wi-Fi provisioning's ESP-IDF binding: chorus/provision.h over the radio,
 * the store and ESP-IDF's provisioning manager.
 *
 * Every DECISION (the states, what a form body is refused for, where a
 * credential may go, what a reset erases) is firmware/src/provision.c's and is
 * graded on a host by firmware/tests/test_provision.c. What is here is the
 * wiring: the speaker's own access point and HTTP server, the two pages, the
 * pinned espressif/network_provisioning manager for Espressif's own clients,
 * and the join that waits for an address. It is NOT HOST-GRADABLE and NOT
 * CLAIMED: nothing in this repository has run it, and docs/bench-packet.md S9
 * is the session that would.
 *
 * Compiled only by ESP-IDF, and only into an image whose board profile is
 * wireless (firmware/main/CMakeLists.txt defines CHORUS_PROVISIONING for
 * it). A wired image carries none of it, and the call below is then nothing. */

#ifndef CHORUS_ESP_PROVISION_H
#define CHORUS_ESP_PROVISION_H

#include "chorus/endpoint_config.h"
#include "chorus/wifi.h"

#if defined(CHORUS_PROVISIONING)

/* Give the wireless link a network, or wait until somebody does.
 *
 * Called by app_main after chorus_esp_hal_radio() and before
 * chorus_link_bring_up(). With a network in the store it joins it; with none
 * (or one that will not join) it raises the access point, prints the setup
 * secret on the console, and stays here serving the join form until a network
 * joins. It returns when the speaker is on a network, having written that
 * network into `config->link` (in RAM; the committed `unknown` now means
 * "provisioned at run time") and having wrapped `radio` so the bring-up that
 * follows finds the link already joined instead of joining twice.
 *
 * When provisioning cannot run at all (the store or the access point refused)
 * it says why and returns with `config->link` untouched, and the bring-up that
 * follows refuses on the unknown credential exactly as it did before goal 14:
 * the link stays down and no session opens. */
void chorus_esp_provision_or_join(chorus_endpoint_config_t *config, chorus_radio_t *radio);

/* The reset, for whatever binds the pairing button's long hold (the controls
 * of goal 9 have no GPIO binding yet): the same act as the console's
 * `wifi-reset`. Erases the stored network; the setup secret stays. */
void chorus_esp_provision_request_reset(void);

#else

static inline void chorus_esp_provision_or_join(chorus_endpoint_config_t *config,
                                                chorus_radio_t *radio)
{
    /* A wired image: nothing to provision, and nothing of the manager, the
     * HTTP server or the access point is linked. */
    (void)config;
    (void)radio;
}

#endif /* CHORUS_PROVISIONING */

#endif /* CHORUS_ESP_PROVISION_H */
