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
 * from app_main, after the configuration is validated; changes nothing. */
void chorus_esp_ota_boot_report(void);

/* The update unit for this boot, started over the real flash: what the
 * session is handed as chorus_session_config_t.ota. `board` is the board
 * profile the image was built for and `confirm_seconds` endpoint.conf's
 * ota_confirm_seconds. NULL when the partition table has no two OTA slots,
 * in which case the endpoint does not offer the `ota` feature. */
chorus_ota_t *chorus_esp_ota_unit(const char *board, uint32_t confirm_seconds);

#endif /* CHORUS_ESP_OTA_H */
