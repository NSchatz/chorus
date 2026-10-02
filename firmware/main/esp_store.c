#include "esp_store.h"

#include "esp_err.h"
#include "esp_log.h"
#include "nvs.h"
#include "nvs_flash.h"

static const char *TAG = "chorus-store";

#define STORE_NAMESPACE "chorus"

static int ready;

static chorus_store_status_t nvs_store_get(void *context, const char *key, void *out,
                                           size_t capacity, size_t *length)
{
    (void)context;
    nvs_handle_t handle;
    esp_err_t opened = nvs_open(STORE_NAMESPACE, NVS_READONLY, &handle);
    if (opened == ESP_ERR_NVS_NOT_FOUND) {
        /* The namespace is made by the first write: nothing has been kept. */
        return CHORUS_STORE_MISSING;
    }
    if (opened != ESP_OK) {
        return CHORUS_STORE_FAILED;
    }
    size_t size = 0;
    esp_err_t found = nvs_get_blob(handle, key, NULL, &size);
    chorus_store_status_t status = CHORUS_STORE_OK;
    if (found == ESP_ERR_NVS_NOT_FOUND) {
        status = CHORUS_STORE_MISSING;
    } else if (found != ESP_OK) {
        status = CHORUS_STORE_FAILED;
    } else if (size > capacity) {
        status = CHORUS_STORE_TOO_LARGE;
    } else if (size > 0 && nvs_get_blob(handle, key, out, &size) != ESP_OK) {
        status = CHORUS_STORE_FAILED;
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
    nvs_handle_t handle;
    if (nvs_open(STORE_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return CHORUS_STORE_FAILED;
    }
    /* NVS replaces a blob by writing the new one and only then dropping the
     * old (docs.espressif.com/projects/esp-idf/en/v6.1/esp32s3/api-reference/
     * storage/nvs_flash.html, read 2026-10-02), so a failed set leaves the old
     * value readable: the seam's contract. */
    esp_err_t wrote = nvs_set_blob(handle, key, value, length);
    if (wrote == ESP_OK) {
        wrote = nvs_commit(handle);
    }
    nvs_close(handle);
    return (wrote == ESP_OK) ? CHORUS_STORE_OK : CHORUS_STORE_FAILED;
}

static chorus_store_status_t nvs_store_erase(void *context, const char *key)
{
    (void)context;
    nvs_handle_t handle;
    if (nvs_open(STORE_NAMESPACE, NVS_READWRITE, &handle) != ESP_OK) {
        return CHORUS_STORE_FAILED;
    }
    esp_err_t erased = nvs_erase_key(handle, key);
    if (erased == ESP_OK) {
        erased = nvs_commit(handle);
    }
    nvs_close(handle);
    return (erased == ESP_OK || erased == ESP_ERR_NVS_NOT_FOUND) ? CHORUS_STORE_OK
                                                                 : CHORUS_STORE_FAILED;
}

static const chorus_store_t store = {
    .context = NULL,
    .get = nvs_store_get,
    .set = nvs_store_set,
    .erase = nvs_store_erase,
};

int chorus_esp_store_init(void)
{
    if (ready) {
        return 0;
    }
    esp_err_t nvs = nvs_flash_init();
    if (nvs != ESP_OK) {
        ESP_LOGE(TAG,
                 "the non-volatile store could not be prepared (%s); it is NOT erased here, "
                 "because that would erase this board's identity",
                 esp_err_to_name(nvs));
        return -1;
    }
    ready = 1;
    return 0;
}

const chorus_store_t *chorus_esp_store(void)
{
    return ready ? &store : NULL;
}
