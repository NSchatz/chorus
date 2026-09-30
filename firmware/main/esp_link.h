/* The wired link's ESP-IDF binding: the chorus/link.h Ethernet interface,
 * backed by esp_eth, esp_netif and the pinned espressif/w5500 driver.
 *
 * Every DECISION (which transport, whether the wiring is allowed, the order
 * and the refusals) is in firmware/src/link.c and graded on a host by
 * firmware/tests/test_link.c. What is here is the wiring to the platform, and
 * it is NOT HOST-GRADABLE and NOT CLAIMED: nothing in this repository has
 * driven a W5500, and the bench packet's embedded session is what would.
 *
 * Compiled only by ESP-IDF. The host build never sees this file. */

#ifndef CHORUS_ESP_LINK_H
#define CHORUS_ESP_LINK_H

#include "chorus/link.h"

/* Bind the W5500 behind `ethernet`. Nothing is brought up by this call:
 * chorus_link_bring_up decides whether init, start and the address wait run
 * at all, and a wireless endpoint never reaches them. */
void chorus_esp_link_ethernet(chorus_ethernet_t *ethernet);

#endif /* CHORUS_ESP_LINK_H */
