#include "esp_playout.h"

#include <stdint.h>
#include <string.h>

#include "esp_hal.h"
#include "esp_marker.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"

#include "chorus/monotonic.h"

static const char *TAG = "chorus-playout";

/* The writer's stack. It fills one DMA buffer (the buffer itself is static)
 * and calls i2s_channel_write and the loop's tick, neither of which recurses;
 * 4 KiB is ESP-IDF's usual task size for a driver-calling task and is ASSUMED
 * until the bench reads uxTaskGetStackHighWaterMark. */
#define WRITER_STACK_BYTES 4096

/* Above the session (which only fills the buffer), below nothing that must not
 * be preempted: the writer is the one task whose lateness is audible. */
#define WRITER_PRIORITY (configMAX_PRIORITIES - 2)

static chorus_playout_t playout;
static int created;
static SemaphoreHandle_t lock;
static uint8_t block[CHORUS_ESP_HAL_MAX_DMA_BYTES];
static uint32_t block_frames;
static size_t block_bytes;

static void take(void *ctx)
{
    (void)ctx;
    (void)xSemaphoreTake(lock, portMAX_DELAY);
}

static void give(void *ctx)
{
    (void)ctx;
    (void)xSemaphoreGive(lock);
}

chorus_playout_t *chorus_esp_playout_create(const chorus_endpoint_config_t *config,
                                            const chorus_sync_conf_t *sync)
{
    chorus_playout_config_t c = chorus_playout_config_from(
        sync, config->clock.sample_rate_hz, (uint8_t)config->clock.slot_bit_width,
        config->clock.dma_frame_num, CHORUS_PLAYOUT_BUFFER_MS);
    /* The endpoint's own volume ceiling (max_volume, ADR 0070). */
    c.max_volume_thousandths = config->max_volume_thousandths;
    /* The GPIO marker's period, only when the board names a marker pin. */
    if (config->pins.marker != CHORUS_PIN_NONE) {
        c.marker_period_ns = (uint64_t)config->marker_period_ms * 1000000ull;
    }
    size_t ring_bytes = chorus_playout_ring_bytes(&c);
    size_t chunk_bytes = chorus_playout_chunk_bytes(&c);
    /* Internal RAM, never external: firmware/endpoint-units.conf rule 3. */
    uint8_t *ring = heap_caps_malloc(ring_bytes, MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    chorus_playout_chunk_t *chunks =
        heap_caps_malloc(chunk_bytes, MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    lock = xSemaphoreCreateMutex();
    if (ring == NULL || chunks == NULL || lock == NULL) {
        ESP_LOGE(TAG, "no internal RAM for a %u-byte jitter buffer (%u bytes free)",
                 (unsigned)ring_bytes,
                 (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT));
        return NULL;
    }
    if (chorus_playout_init(&playout, &c, ring, chunks, chorus_monotonic_now_ns) != 0) {
        ESP_LOGE(TAG, "the playout configuration is not playable");
        return NULL;
    }
    chorus_playout_set_lock(&playout, take, give, NULL);
    block_frames = config->clock.dma_frame_num;
    block_bytes = (size_t)block_frames * 2u * ((size_t)config->clock.slot_bit_width / 8u);
    if (block_bytes > sizeof(block) || chorus_esp_hal_attach_playout(&playout, config) != 0) {
        ESP_LOGE(TAG, "the playout path could not be attached to the I2S interrupt");
        return NULL;
    }
    (void)chorus_esp_marker_start(config);
    created = 1;
    ESP_LOGI(TAG, "jitter buffer %u bytes (%u frames), internal RAM free after it: %u bytes",
             (unsigned)ring_bytes, (unsigned)c.capacity_frames,
             (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT));
    return &playout;
}

static void writer(void *argument)
{
    chorus_playout_t *p = (chorus_playout_t *)argument;
    i2s_chan_handle_t tx = chorus_esp_hal_i2s_tx();
    uint64_t interval_ns = (uint64_t)p->config.interval_ms * 1000000ull;
    uint64_t next_tick_ns = 0;
    for (;;) {
        (void)chorus_playout_fill(p, block, block_frames);
        size_t written = 0;
        /* Blocks until the DMA frees a buffer, which is the writer's pacing:
         * it runs exactly as fast as the I2S clock consumes. */
        esp_err_t err = i2s_channel_write(tx, block, block_bytes, &written, 1000);
        if (err != ESP_OK) {
            /* The clock was stopped: an amplifier fault (app_main's fault
             * watch) or a channel never started. The output stage is already
             * in high impedance; the counts no longer describe a DMA. */
            ESP_LOGE(TAG, "i2s_channel_write: %s; the writer waits", esp_err_to_name(err));
            vTaskDelay(pdMS_TO_TICKS(1000));
            continue;
        }
        uint64_t now = chorus_monotonic_now_ns();
        if (!chorus_playout_acquired(p) || now >= next_tick_ns) {
            (void)chorus_playout_observe(p, NULL);
            next_tick_ns = now + interval_ns;
        }
    }
}

chorus_playout_t *chorus_esp_playout_get(void)
{
    return created ? &playout : NULL;
}

int chorus_esp_playout_start(chorus_playout_t *p)
{
    if (xTaskCreate(writer, "chorus-playout", WRITER_STACK_BYTES, p, WRITER_PRIORITY, NULL) !=
        pdPASS) {
        ESP_LOGE(TAG, "the playout writer could not be started");
        return -1;
    }
    return 0;
}
