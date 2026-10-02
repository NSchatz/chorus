/* The wired link's ESP-IDF binding: the chorus/link.h Ethernet interface,
 * backed by esp_eth, esp_netif and the pinned espressif/w5500 driver; and,
 * for the emulator's board alone, the same interface backed by esp_eth's
 * OpenCores controller.
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

/* Bind the emulator's OpenCores Ethernet controller behind `ethernet` (goal
 * 14): what app_main binds instead of the W5500 when the board profile's link
 * is `emulated`. Nothing is brought up by this call either. In an image built
 * without the controller (every speaker's) its init refuses and says why.
 *
 * NOT TIMING EVIDENCE and not a speaker's link: it exists so the store, the
 * identity, the session and the firmware update run as the code a speaker
 * runs, in an emulator, in `make gate`. */
void chorus_esp_link_emulated(chorus_ethernet_t *ethernet);

/* The gateway the interface's address lease named, as dotted text. 0 on
 * success; -1 before an address arrived or when `out` is too small. Read by
 * app_main for chorus_link_emulated_server (chorus/link.h). */
int chorus_esp_link_gateway(char *out, size_t out_len);

#endif /* CHORUS_ESP_LINK_H */
