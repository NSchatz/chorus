/* chorus/store.h over NVS (esp_store.h says what this is and is not).
 *
 * One namespace, one blob per key, a commit after every change. NVS is the
 * medium the interface was shaped around:
 *
 *   - "Keys are ASCII strings; the maximum key length is currently 15
 *     characters" (ESP-IDF v6.1 docs/en/api-reference/storage/nvs_flash.rst:33,
 *     read 2026-10-02), which is CHORUS_STORE_MAX_KEY;
 *   - a set that is cut short by a power loss loses only the pair being
 *     written: "one should be able to power off the device at any point and
 *     time and then power it back on. This should not result in loss of data,
 *     except for the new key-value pair if it was being written at the moment
 *     of powering off" (the same file, :105). NVS writes the new value before
 *     it erases the old one, which is the interface's "replaces the whole
 *     value, or does nothing"; that it holds on this flash is NOT CLAIMED
 *     until a bench session cuts the power on one.
 *
 * NVS encryption is off and stays off: CONFIG_NVS_ENCRYPTION is on the refused
 * list (firmware/check/efuse-kconfig.list), because it needs Flash Encryption
 * or an eFuse key and guardrail 2 forbids both on development hardware. What
 * is stored here, the Wi-Fi passphrase included, is therefore readable by
 * anyone who can read the flash chip. The decision record says so.
 *
 * Nothing here logs a value, and nothing here erases the partition. */

#include "esp_store.h"

#include "esp_err.h"
#include "esp_log.h"
#include "nvs.h"
#include "nvs_flash.h"

static const char *TAG = "chorus-store";

/* The namespace every chorus key lives in (at most 15 characters, like a
 * key). The Wi-Fi driver and ESP-IDF's PHY keep their own namespaces in the
 * same partition and are not reachable through this interface. */
#define STORE_NAMESPACE "chorus"

static int store_ready;

int chorus_esp_store_init(void)
{
    if (store_ready) {
        return 0;
    }
    esp_err_t err = nvs_flash_init();
    if (err != ESP_OK) {
        /* ESP_ERR_NVS_NO_FREE_PAGES and ESP_ERR_NVS_NEW_VERSION_FOUND are the
         * two answers ESP-IDF's examples meet with an erase. Not here: see
         * esp_store.h. */
        ESP_LOGE(TAG,
                 "store: NVS could not be initialised (%s); nothing is erased, and the "
                 "store answers `failed` until the partition is put right",
                 esp_err_to_name(err));
        return -1;
    }
    store_ready = 1;
    ESP_LOGI(TAG, "store: NVS ready, namespace %s", STORE_NAMESPACE);
    return 0;
}

static chorus_store_status_t nvs_store_get(void *context, const char *key, void *out,
                                           size_t capacity, size_t *length)
{
    (void)context;
    if (!store_ready) {
        return CHORUS_STORE_FAILED;
    }
    nvs_handle_t handle;
    esp_err_t err = nvs_open(STORE_NAMESPACE, NVS_READONLY, &handle);
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        /* Nothing was ever written: the namespace itself does not exist yet. */
        return CHORUS_STORE_MISSING;
    }
    if (err != ESP_OK) {
        return CHORUS_STORE_FAILED;
    }
    /* The size first, so a value that does not fit is refused whole rather
     * than read in part. */
    size_t size = 0;
    err = nvs_get_blob(handle, key, NULL, &size);
    chorus_store_status_t status = CHORUS_STORE_OK;
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        status = CHORUS_STORE_MISSING;
    } else if (err != ESP_OK) {
        status = CHORUS_STORE_FAILED;
    } else if (size > capacity) {
        status = CHORUS_STORE_TOO_LARGE;
    } else if (size > 0) {
        err = nvs_get_blob(handle, key, out, &size);
        if (err != ESP_OK) {
            status = CHORUS_STORE_FAILED;
        }
    }
    nvs_close(handle);
    if (status == CHORUS_STORE_OK) {
        *length = size;
    }
    return status;
}

static chorus_store_status_t nvs_store_set(void *context, const char *key, const void *value,
                                           size_t length)
{
    (void)context;
    if (!store_ready) {
        return CHORUS_STORE_FAILED;
    }
    nvs_handle_t handle;
    if (nvs_open(STORE_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return CHORUS_STORE_FAILED;
    }
    esp_err_t err = nvs_set_blob(handle, key, value, length);
    if (err == ESP_OK) {
        err = nvs_commit(handle);
    }
    nvs_close(handle);
    if (err != ESP_OK) {
        /* The key and the platform's word, never the value. */
        ESP_LOGE(TAG, "store: set %s failed (%s)", key, esp_err_to_name(err));
        return CHORUS_STORE_FAILED;
    }
    return CHORUS_STORE_OK;
}

static chorus_store_status_t nvs_store_erase(void *context, const char *key)
{
    (void)context;
    if (!store_ready) {
        return CHORUS_STORE_FAILED;
    }
    nvs_handle_t handle;
    if (nvs_open(STORE_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return CHORUS_STORE_FAILED;
    }
    esp_err_t err = nvs_erase_key(handle, key);
    if (err == ESP_ERR_NVS_NOT_FOUND) {
        /* Removing a missing key is OK (chorus/store.h). */
        nvs_close(handle);
        return CHORUS_STORE_OK;
    }
    if (err == ESP_OK) {
        err = nvs_commit(handle);
    }
    nvs_close(handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "store: erase %s failed (%s)", key, esp_err_to_name(err));
        return CHORUS_STORE_FAILED;
    }
    return CHORUS_STORE_OK;
}

static const chorus_store_t nvs_store = {
    .context = NULL,
    .get = nvs_store_get,
    .set = nvs_store_set,
    .erase = nvs_store_erase,
};

const chorus_store_t *chorus_esp_store(void)
{
    /* No store before, or without, a working NVS: a caller that is handed
     * NULL refuses by name rather than running on a medium that is not there. */
    return store_ready ? &nvs_store : NULL;
}
