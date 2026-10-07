#include "esp_knobs.h"

#include <stdint.h>

#include "esp_adc/adc_oneshot.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#include "chorus/controls.h"
#include "chorus/monotonic.h"

static const char *TAG = "chorus-knobs";

/* How often the wipers are read. ASSUMED: 50 ms, a turn followed within a
 * twentieth of a second; the hysteresis (CHORUS_CONTROLS_KNOB_HYSTERESIS)
 * keeps a reading at a step's edge from chattering. */
#define KNOB_PERIOD_MS 50

/* Small: two reads, one controls object (static) and a log line. ASSUMED
 * until the bench reads uxTaskGetStackHighWaterMark. */
#define KNOB_STACK_BYTES 3072
#define KNOB_PRIORITY 3

typedef struct {
    chorus_control_input_t input;
    int wired;
    adc_channel_t channel;
} knob_t;

static adc_oneshot_unit_handle_t adc;
static knob_t knobs[2];
static chorus_controls_t controls;
static chorus_playout_t *target;

/* A wiper's GPIO onto its ADC1 channel at the full 12 bits and the widest
 * attenuation. The configuration check has already held the pin to ADC1. */
static int knob_channel(knob_t *knob, chorus_control_input_t input, uint32_t pin)
{
    knob->input = input;
    knob->wired = 0;
    if (pin == CHORUS_PIN_NONE) {
        return 0;
    }
    adc_unit_t unit;
    if (adc_oneshot_io_to_channel((int)pin, &unit, &knob->channel) != ESP_OK ||
        unit != ADC_UNIT_1) {
        ESP_LOGE(TAG, "GPIO%u is not an ADC1 input", (unsigned)pin);
        return -1;
    }
    adc_oneshot_chan_cfg_t channel = {
        .atten = ADC_ATTEN_DB_12,
        .bitwidth = ADC_BITWIDTH_12,
    };
    if (adc_oneshot_config_channel(adc, knob->channel, &channel) != ESP_OK) {
        ESP_LOGE(TAG, "the ADC1 channel for GPIO%u could not be configured", (unsigned)pin);
        return -1;
    }
    knob->wired = 1;
    return 0;
}

static void knob_task(void *argument)
{
    (void)argument;
    static chorus_control_action_t actions[CHORUS_CONTROLS_MAX_ACTIONS];
    for (;;) {
        uint64_t now = chorus_monotonic_now_ns();
        for (size_t i = 0; i < sizeof(knobs) / sizeof(knobs[0]); i++) {
            int raw = 0;
            if (knobs[i].wired && adc_oneshot_read(adc, knobs[i].channel, &raw) == ESP_OK &&
                raw >= 0) {
                (void)chorus_controls_knob(&controls, knobs[i].input, (uint32_t)raw, now);
            }
        }
        /* The knobs are this controls object's only inputs, so every action
         * is a knob's; the chain takes both settings at once. */
        size_t n = chorus_controls_poll(&controls, now, actions, CHORUS_CONTROLS_MAX_ACTIONS);
        int moved = 0;
        for (size_t i = 0; i < n; i++) {
            moved |= actions[i].kind == CHORUS_ACTION_SUB_LEVEL ||
                     actions[i].kind == CHORUS_ACTION_SUB_PHASE;
        }
        if (moved) {
            chorus_playout_set_sub_knobs(target, controls.sub_level_tenths_db,
                                         controls.sub_phase_deg);
            ESP_LOGI(TAG, "level %d tenths of a dB, phase %d degrees",
                     (int)controls.sub_level_tenths_db, (int)controls.sub_phase_deg);
        }
        vTaskDelay(pdMS_TO_TICKS(KNOB_PERIOD_MS));
    }
}

int chorus_esp_knobs_start(const chorus_endpoint_config_t *config, chorus_playout_t *playout)
{
    const chorus_board_controls_t *c = &config->controls;
    if (c->knob_level == CHORUS_PIN_NONE && c->knob_phase == CHORUS_PIN_NONE) {
        return 0;
    }
    if (playout == NULL) {
        ESP_LOGE(TAG, "no playout path; the knobs are not read");
        return -1;
    }
    if (chorus_controls_init(&controls, CHORUS_CLASS_SUBWOOFER, "", "") != 0) {
        ESP_LOGE(TAG, "the subwoofer's controls could not be set up");
        return -1;
    }
    adc_oneshot_unit_init_cfg_t unit = {
        .unit_id = ADC_UNIT_1,
        .ulp_mode = ADC_ULP_MODE_DISABLE,
    };
    if (adc_oneshot_new_unit(&unit, &adc) != ESP_OK) {
        ESP_LOGE(TAG, "ADC1 could not be set up; the knobs stay at 0 dB and 0 degrees");
        return -1;
    }
    if (knob_channel(&knobs[0], CHORUS_INPUT_SUB_LEVEL_KNOB, c->knob_level) != 0 ||
        knob_channel(&knobs[1], CHORUS_INPUT_SUB_PHASE_KNOB, c->knob_phase) != 0) {
        return -1;
    }
    target = playout;
    if (xTaskCreate(knob_task, "chorus-knobs", KNOB_STACK_BYTES, NULL, KNOB_PRIORITY, NULL) !=
        pdPASS) {
        ESP_LOGE(TAG, "the knob task could not be started");
        return -1;
    }
    ESP_LOGI(TAG, "knobs on ADC1: level GPIO%d, phase GPIO%d",
             c->knob_level == CHORUS_PIN_NONE ? -1 : (int)c->knob_level,
             c->knob_phase == CHORUS_PIN_NONE ? -1 : (int)c->knob_phase);
    return 0;
}
