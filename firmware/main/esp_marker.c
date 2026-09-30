#include "esp_marker.h"

#include <stdbool.h>
#include <stdint.h>

#include "driver/gpio.h"
#include "driver/gptimer.h"
#include "esp_log.h"

#include "chorus/i2s.h"

static const char *TAG = "chorus-marker";

/* A 1 MHz timer: the alarm lands on a 1 us grid, a twentieth of a frame at
 * 48 kHz. */
#define MARKER_TIMER_HZ 1000000u

static gptimer_handle_t timer;
static gpio_num_t pin = GPIO_NUM_NC;
static volatile uint32_t pending_level;

/* The alarm: set the level and stop. Runs in the timer's interrupt; the
 * pinned v6.1 tree's components/esp_driver_gptimer/include/driver/gptimer.h
 * (read 2026-09-30) says the callbacks run in ISR context and that
 * gptimer_stop, gptimer_start, gptimer_set_raw_count and
 * gptimer_set_alarm_action are "allowed to run within ISR context". */
static bool on_alarm(gptimer_handle_t t, const gptimer_alarm_event_data_t *event, void *ctx)
{
    (void)event;
    (void)ctx;
    (void)gpio_set_level(pin, pending_level);
    (void)gptimer_stop(t);
    return false;
}

int chorus_esp_marker_start(const chorus_endpoint_config_t *config)
{
    if (config->pins.marker == CHORUS_PIN_NONE) {
        ESP_LOGI(TAG, "marker off (pin_marker = none)");
        return 0;
    }
    pin = (gpio_num_t)config->pins.marker;
    gpio_config_t io = {
        .pin_bit_mask = 1ULL << config->pins.marker,
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    if (gpio_config(&io) != ESP_OK || gpio_set_level(pin, 0) != ESP_OK) {
        ESP_LOGE(TAG, "the marker pin GPIO%u could not be configured; marker off",
                 (unsigned)config->pins.marker);
        pin = GPIO_NUM_NC;
        return -1;
    }
    gptimer_config_t tc = {
        .clk_src = GPTIMER_CLK_SRC_DEFAULT,
        .direction = GPTIMER_COUNT_UP,
        .resolution_hz = MARKER_TIMER_HZ,
    };
    gptimer_event_callbacks_t cbs = {.on_alarm = on_alarm};
    if (gptimer_new_timer(&tc, &timer) != ESP_OK ||
        gptimer_register_event_callbacks(timer, &cbs, NULL) != ESP_OK ||
        gptimer_enable(timer) != ESP_OK) {
        ESP_LOGE(TAG, "the marker's timer could not be set up; marker off");
        timer = NULL;
        return -1;
    }
    ESP_LOGI(TAG, "marker on GPIO%u every %u ms of the server timeline",
             (unsigned)config->pins.marker, (unsigned)config->marker_period_ms);
    return 0;
}

void chorus_esp_marker_on_dma_sent(chorus_playout_t *playout)
{
    if (timer == NULL) {
        return;
    }
    uint32_t delay_ns = 0;
    uint32_t level = 0;
    if (!chorus_playout_marker_due(playout, &delay_ns, &level)) {
        return;
    }
    pending_level = level;
    uint32_t delay_us = delay_ns / 1000u;
    if (delay_us == 0u) {
        (void)gpio_set_level(pin, level);
        return;
    }
    gptimer_alarm_config_t alarm = {.alarm_count = delay_us};
    (void)gptimer_set_raw_count(timer, 0);
    (void)gptimer_set_alarm_action(timer, &alarm);
    (void)gptimer_start(timer);
}
