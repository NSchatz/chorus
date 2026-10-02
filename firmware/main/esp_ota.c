/* chorus_ota_flash_t over ESP-IDF's app_update component, and the note over
 * NVS. Compiled only by ESP-IDF.
 *
 * What each operation maps to, at the pinned ESP-IDF v6.1 (read 2026-10-02):
 *
 *   begin    esp_ota_begin (components/app_update/esp_ota_ops.c:155): refuses
 *            the running partition and a running image still on trial, erases
 *            the slot, and drops the inactive otadata entry.
 *   write    esp_ota_write (:336), sequential.
 *   finish   esp_ota_end (:617): the image's own checksum and appended
 *            SHA-256 are verified from flash.
 *   set_boot esp_ota_set_boot_partition (:849): verifies the image again and
 *            writes the otadata entry as NEW (:127-135, :810).
 *   confirm  esp_ota_mark_app_valid_cancel_rollback (:1219).
 *   invalidate esp_ota_mark_app_invalid_rollback_and_reboot (:1229): returns
 *            only when no other image can be booted.
 *
 * NOT HOST-GRADABLE and NOT CLAIMED on hardware: the decisions are graded on
 * the host over firmware/tests/fake_flash.c, this binding by the emulator run
 * (make ota-qemu) and by the owner's bench session. No eFuse is read or
 * written here, and anti-rollback (which would burn one) is off and refused
 * by the image guard (tools/firmware-image-guard.sh). */

#include "esp_ota.h"

#include <string.h>

#include "esp_app_desc.h"
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_partition.h"
#include "esp_system.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs.h"
#include "nvs_flash.h"

#include "chorus/monotonic.h"

static const char *TAG = "chorus-ota";

/* The store's namespace and this unit's key in it (design envelope section
 * 1: `ota_note`). */
#define NOTE_NAMESPACE "chorus"
#define NOTE_KEY "ota_note"

typedef struct {
    esp_ota_handle_t handle;
    int writing;
    uint32_t written;
} glue_t;

static const esp_partition_t *slot_partition(int slot)
{
    if (slot != 0 && slot != 1) {
        return NULL;
    }
    return esp_partition_find_first(
        ESP_PARTITION_TYPE_APP, (esp_partition_subtype_t)(ESP_PARTITION_SUBTYPE_APP_OTA_0 + slot),
        NULL);
}

static int glue_running_slot(void *context)
{
    (void)context;
    const esp_partition_t *running = esp_ota_get_running_partition();
    if (running == NULL || running->type != ESP_PARTITION_TYPE_APP) {
        return -1;
    }
    if (running->subtype == ESP_PARTITION_SUBTYPE_APP_OTA_0) {
        return 0;
    }
    if (running->subtype == ESP_PARTITION_SUBTYPE_APP_OTA_1) {
        return 1;
    }
    return -1;
}

static chorus_ota_image_state_t glue_slot_state(void *context, int slot)
{
    (void)context;
    const esp_partition_t *partition = slot_partition(slot);
    esp_ota_img_states_t state = ESP_OTA_IMG_UNDEFINED;
    if (partition == NULL || esp_ota_get_state_partition(partition, &state) != ESP_OK) {
        /* ESP_ERR_NOT_FOUND: no otadata entry names this slot. */
        return CHORUS_OTA_IMAGE_UNDEFINED;
    }
    switch (state) {
    case ESP_OTA_IMG_NEW:
        return CHORUS_OTA_IMAGE_NEW;
    case ESP_OTA_IMG_PENDING_VERIFY:
        return CHORUS_OTA_IMAGE_PENDING_VERIFY;
    case ESP_OTA_IMG_VALID:
        return CHORUS_OTA_IMAGE_VALID;
    case ESP_OTA_IMG_INVALID:
        return CHORUS_OTA_IMAGE_INVALID;
    case ESP_OTA_IMG_ABORTED:
        return CHORUS_OTA_IMAGE_ABORTED;
    default:
        return CHORUS_OTA_IMAGE_UNDEFINED;
    }
}

static uint32_t glue_slot_capacity(void *context, int slot)
{
    (void)context;
    const esp_partition_t *partition = slot_partition(slot);
    return (partition == NULL) ? 0u : partition->size;
}

static int glue_begin(void *context, int slot, uint32_t size)
{
    glue_t *glue = context;
    const esp_partition_t *partition = slot_partition(slot);
    if (partition == NULL || glue->writing) {
        return -1;
    }
    esp_err_t err = esp_ota_begin(partition, size, &glue->handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "slot %d was not prepared: %s", slot, esp_err_to_name(err));
        return -1;
    }
    glue->writing = 1;
    glue->written = 0;
    return 0;
}

static int glue_write(void *context, uint32_t offset, const uint8_t *data, size_t length)
{
    glue_t *glue = context;
    /* The unit writes in order; the handle's own position is the offset. */
    if (!glue->writing || offset != glue->written) {
        return -1;
    }
    esp_err_t err = esp_ota_write(glue->handle, data, length);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "write at %u failed: %s", (unsigned)offset, esp_err_to_name(err));
        return -1;
    }
    glue->written += (uint32_t)length;
    return 0;
}

static int glue_finish(void *context)
{
    glue_t *glue = context;
    if (!glue->writing) {
        return -1;
    }
    /* The handle is released whatever the verdict (esp_ota_ops.c:655-662). */
    glue->writing = 0;
    esp_err_t err = esp_ota_end(glue->handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "the written image did not verify: %s", esp_err_to_name(err));
        return -1;
    }
    return 0;
}

static void glue_abandon(void *context)
{
    glue_t *glue = context;
    if (glue->writing) {
        glue->writing = 0;
        (void)esp_ota_abort(glue->handle);
    }
}

static int glue_set_boot(void *context, int slot)
{
    (void)context;
    const esp_partition_t *partition = slot_partition(slot);
    if (partition == NULL) {
        return -1;
    }
    esp_err_t err = esp_ota_set_boot_partition(partition);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "slot %d was not selected: %s", slot, esp_err_to_name(err));
        return -1;
    }
    return 0;
}

static int glue_confirm_running(void *context)
{
    (void)context;
    return (esp_ota_mark_app_valid_cancel_rollback() == ESP_OK) ? 0 : -1;
}

static int glue_invalidate_running_and_reboot(void *context)
{
    (void)context;
    ESP_LOGE(TAG, "this image did not confirm in time; marking it invalid and rebooting into "
                  "the previous one");
    /* Returns only when it could not (no other bootable image). */
    esp_err_t err = esp_ota_mark_app_invalid_rollback_and_reboot();
    ESP_LOGE(TAG, "the rollback was not possible: %s", esp_err_to_name(err));
    return -1;
}

static void glue_reboot(void *context)
{
    (void)context;
    ESP_LOGI(TAG, "rebooting into the new image");
    esp_restart();
}

/* --- the note, in NVS ---------------------------------------------------------- */

static int note_load(void *context, uint8_t *out, size_t capacity, size_t *length)
{
    (void)context;
    nvs_handle_t handle;
    esp_err_t err = nvs_open(NOTE_NAMESPACE, NVS_READONLY, &handle);
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        return 1;
    }
    if (err != ESP_OK) {
        return -1;
    }
    size_t size = capacity;
    err = nvs_get_blob(handle, NOTE_KEY, out, &size);
    nvs_close(handle);
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        return 1;
    }
    if (err != ESP_OK) {
        return -1;
    }
    *length = size;
    return 0;
}

static int note_save(void *context, const uint8_t *note, size_t length)
{
    (void)context;
    nvs_handle_t handle;
    if (nvs_open(NOTE_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return -1;
    }
    esp_err_t err = nvs_set_blob(handle, NOTE_KEY, note, length);
    if (err == ESP_OK) {
        err = nvs_commit(handle);
    }
    nvs_close(handle);
    return (err == ESP_OK) ? 0 : -1;
}

static int note_clear(void *context)
{
    (void)context;
    nvs_handle_t handle;
    if (nvs_open(NOTE_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return -1;
    }
    esp_err_t err = nvs_erase_key(handle, NOTE_KEY);
    if (err == ESP_OK) {
        err = nvs_commit(handle);
    }
    nvs_close(handle);
    return (err == ESP_OK || err == ESP_ERR_NVS_NOT_FOUND) ? 0 : -1;
}

/* --- what app_main calls ------------------------------------------------------- */

/* How long after the unit's own deadline the backstop acts. ASSUMED 30 s:
 * long enough that the unit, when it runs, always decides first (and tells
 * the server), short enough that a stuck image is gone within two minutes. */
#define TRIAL_BACKSTOP_MARGIN_SECONDS 30u
#define TRIAL_BACKSTOP_STACK_BYTES 4096u

static void trial_backstop(void *arg)
{
    uint32_t seconds = (uint32_t)(uintptr_t)arg;
    vTaskDelay((TickType_t)seconds * (TickType_t)configTICK_RATE_HZ);
    if (glue_slot_state(NULL, glue_running_slot(NULL)) == CHORUS_OTA_IMAGE_PENDING_VERIFY) {
        ESP_LOGE(TAG, "still unconfirmed %u s after boot, with or without a session",
                 (unsigned)seconds);
        (void)glue_invalidate_running_and_reboot(NULL);
    }
    vTaskDelete(NULL);
}

void chorus_esp_ota_boot_report(uint32_t confirm_seconds)
{
    int slot = glue_running_slot(NULL);
    chorus_ota_image_state_t state = glue_slot_state(NULL, slot);
    const esp_app_desc_t *description = esp_app_get_description();
    ESP_LOGI(TAG, "running slot=%d state=%s version=%s", slot, chorus_ota_image_state_name(state),
             description->version);
    if (state != CHORUS_OTA_IMAGE_PENDING_VERIFY) {
        return;
    }
    uint32_t seconds = confirm_seconds + TRIAL_BACKSTOP_MARGIN_SECONDS;
    if (xTaskCreate(trial_backstop, "chorus-ota-trial", TRIAL_BACKSTOP_STACK_BYTES,
                    (void *)(uintptr_t)seconds, 5, NULL) != pdPASS) {
        /* Without the backstop the unit's own deadline still stands, and a
         * power cycle still rolls an unconfirmed image back. */
        ESP_LOGE(TAG, "the trial's backstop task could not be started");
    }
}

chorus_ota_t *chorus_esp_ota_unit(const char *board, uint32_t confirm_seconds)
{
    static glue_t glue;
    static chorus_ota_t unit;
    static const chorus_ota_flash_t flash = {
        .context = &glue,
        .running_slot = glue_running_slot,
        .slot_state = glue_slot_state,
        .slot_capacity = glue_slot_capacity,
        .begin = glue_begin,
        .write = glue_write,
        .finish = glue_finish,
        .abandon = glue_abandon,
        .set_boot = glue_set_boot,
        .confirm_running = glue_confirm_running,
        .invalidate_running_and_reboot = glue_invalidate_running_and_reboot,
        .reboot = glue_reboot,
    };
    static const chorus_ota_notes_t notes = {
        .context = NULL,
        .load = note_load,
        .save = note_save,
        .clear = note_clear,
    };
    if (slot_partition(0) == NULL || slot_partition(1) == NULL || glue_running_slot(NULL) < 0) {
        ESP_LOGE(TAG, "the partition table has no two OTA slots this image runs from; the "
                      "endpoint takes no update");
        return NULL;
    }
    /* The note's store. NVS is the board's key-value store and its bring-up
     * is idempotent, so this is safe beside the store unit's own. NVS
     * encryption stays off (it is on the refused Kconfig list). A store that
     * does not come up costs the rollback REPORT, never the rollback. */
    const chorus_ota_notes_t *kept = &notes;
    if (nvs_flash_init() != ESP_OK) {
        ESP_LOGE(TAG, "NVS did not come up; a rollback will not be reported");
        kept = NULL;
    }
    chorus_ota_config_t config;
    memset(&config, 0, sizeof(config));
    config.flash = &flash;
    config.notes = kept;
    const esp_app_desc_t *description = esp_app_get_description();
    strlcpy(config.version, description->version, sizeof(config.version));
    strlcpy(config.board, (board == NULL) ? "" : board, sizeof(config.board));
    config.confirm_ns = (uint64_t)confirm_seconds * 1000000000ull;
#ifdef CHORUS_OTA_NEVER_CONFIRM
    /* The rollback test's bad image (ota.h, never_confirm): built only by
     * the emulator run, with this definition on its command line. */
    config.never_confirm = 1;
#endif
    /* esp_timer's clock is the monotonic one the session passes in too
     * (firmware/src/monotonic.c). */
    if (chorus_ota_boot(&unit, &config, chorus_monotonic_now_ns()) != 0) {
        return NULL;
    }
    ESP_LOGI(TAG, "update unit state=%s", chorus_ota_state_name(unit.state));
    return &unit;
}
