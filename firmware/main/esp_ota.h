/* The firmware update's binding to ESP-IDF (chorus/ota.h has the decisions).
 *
 * This unit, firmware/main/esp_ota.c, is THE ONLY one in the endpoint tree
 * allowed to name ESP-IDF's OTA calls: the source scan
 * (firmware/check/endpoint_scan.c, OTA_GLUE_UNIT) refuses them everywhere
 * else. */

#ifndef CHORUS_ESP_OTA_H
#define CHORUS_ESP_OTA_H

#include <stdint.h>

#include "chorus/ota.h"

/* Log which slot runs and what the bootloader recorded about it. Called once
 * from app_main, after the configuration is validated and before any
 * hardware is brought up.
 *
 * When the running image is on its trial boot it also arms the trial's
 * backstop: a task that, `confirm_seconds` plus a margin after boot, marks the
 * image invalid and reboots if it is STILL unconfirmed. The update unit
 * normally decides that itself, through the session; the backstop is for the
 * image that never gets that far (a bring-up that fails and returns from
 * app_main, a session task that never starts), which would otherwise sit
 * unconfirmed until somebody cycled its power. */
void chorus_esp_ota_boot_report(uint32_t confirm_seconds);

/* The update unit for this boot, started over the real flash: what the
 * session is handed as chorus_session_config_t.ota. `board` is the board
 * profile the image was built for and `confirm_seconds` endpoint.conf's
 * ota_confirm_seconds. NULL when the partition table has no two OTA slots,
 * in which case the endpoint does not offer the `ota` feature. */
chorus_ota_t *chorus_esp_ota_unit(const char *board, uint32_t confirm_seconds);

#endif /* CHORUS_ESP_OTA_H */
