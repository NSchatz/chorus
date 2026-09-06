/* The endpoint, on the board.
 *
 * Deliberately thin. Everything of consequence - the protocol core, the sync
 * core, the bring-up ORDER, the clock and pin rules, the session supervisor's
 * rejoin - is in firmware/src and is graded by `make firmware-check` on a
 * machine with no ESP32-S3. What is here is the sequence those pieces are
 * asked to run in, and the sequence is the same one
 * firmware/tests/test_amp.c grades against a simulated part.
 *
 * NOT HOST-GRADABLE and NOT CLAIMED. This file and esp_hal.c are compiled only
 * by ESP-IDF; the two criteria that grade them, AC-1 and AC-3, are the
 * operator ones, and tools/endpoint-rig-run.sh refuses by name until somebody
 * has the hardware. docs/verification-record.md says so in as many words.
 *
 * The committed configuration is EMBEDDED in the image rather than read from a
 * filesystem the board does not have, and it is parsed by the same reader the
 * host build uses, so the values a bench runs on are the values a host
 * graded. */

#include <string.h>

#include "esp_hal.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#include "chorus/amp.h"
#include "chorus/endpoint_config.h"
#include "chorus/session.h"
#include "chorus/telemetry.h"

static const char *TAG = "chorus-endpoint";

/* firmware/config/endpoint.conf, embedded by the component's CMakeLists. */
extern const char endpoint_conf_start[] asm("_binary_endpoint_conf_start");
extern const char endpoint_conf_end[] asm("_binary_endpoint_conf_end");

/* How often the amplifier's fault register is read while audio is playing. A
 * fault that is surfaced a second late is still surfaced; one that is never
 * read is not. */
#define FAULT_POLL_MS 1000

/* Everything the fault watch needs. It shares the I2C bus, the output stage
 * and the I2S controller with nothing else: the session supervisor touches a
 * socket and a filter and no hardware at all, so there is no contention to
 * guard against here. */
typedef struct {
    const chorus_endpoint_config_t *config;
    chorus_i2c_bus_t *bus;
    chorus_output_stage_t *stage;
    chorus_i2s_controller_t *controller;
    chorus_telemetry_t *telemetry;
} fault_watch_t;

/* AC-5, on the board: read the fault register, and on a fault stop the audio
 * and say which fault by name. Never swallow one, and never keep playing
 * through one. */
static void fault_watch(void *argument)
{
    fault_watch_t *watch = (fault_watch_t *)argument;
    chorus_amp_report_t report;
    char line[512];
    for (;;) {
        vTaskDelay(pdMS_TO_TICKS(FAULT_POLL_MS));
        chorus_amp_status_t status = chorus_amp_poll_fault(&watch->config->amp, watch->bus,
                                                           watch->stage, watch->controller,
                                                           &report);
        chorus_telemetry_record_amp(watch->telemetry, &report);
        if (status != CHORUS_AMP_OK) {
            chorus_telemetry_line(watch->telemetry, line, sizeof(line));
            ESP_LOGE(TAG, "%s", line);
            ESP_LOGE(TAG, "%s", report.detail);
            /* chorus_amp_poll_fault has already stopped the clock and put the
             * output stage in high impedance. The watch stops rather than
             * re-reading a part it has just shut down. */
            vTaskDelete(NULL);
            return;
        }
    }
}

void app_main(void)
{
    static char text[8192];
    size_t length = (size_t)(endpoint_conf_end - endpoint_conf_start);
    if (length >= sizeof(text)) {
        ESP_LOGE(TAG, "the embedded configuration does not fit");
        return;
    }
    memcpy(text, endpoint_conf_start, length);
    text[length] = '\0';

    chorus_endpoint_config_t config;
    char detail[512];
    detail[0] = '\0';
    if (chorus_endpoint_config_parse(&config, "firmware/config/endpoint.conf", text, detail,
                                     sizeof(detail)) != 0) {
        ESP_LOGE(TAG, "%s", detail);
        return;
    }

    /* The same validation the host build gates its compile on. A board that
     * somehow booted an image built from a configuration that breaks a
     * platform rule stops here rather than driving a loudspeaker with it. */
    chorus_finding_t findings[16];
    size_t finding_count = 0;
    chorus_endpoint_config_validate(&config, findings, 16, &finding_count);
    for (size_t i = 0; i < finding_count; i++) {
        ESP_LOGE(TAG, "FAIL %s :: %s", findings[i].rule, findings[i].detail);
    }
    if (finding_count > 0) {
        ESP_LOGE(TAG, "the committed configuration is refused; no clock is started");
        return;
    }

    chorus_i2c_bus_t bus;
    chorus_output_stage_t stage;
    chorus_i2s_controller_t controller;
    if (chorus_esp_hal_init(&config, &bus, &stage, &controller) != 0) {
        ESP_LOGE(TAG, "the hardware could not be brought up; the output stage stays dead");
        return;
    }

    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);

    chorus_amp_report_t report;
    chorus_amp_status_t status =
        chorus_amp_bring_up(&config.amp, &config.gain, &config.clock, &bus, &stage, &controller,
                            &report);
    chorus_telemetry_record_amp(&telemetry, &report);
    if (status != CHORUS_AMP_OK) {
        char line[512];
        chorus_telemetry_line(&telemetry, line, sizeof(line));
        /* Surfaced, not swallowed, and the endpoint stops here rather than
         * starting a session it cannot play. */
        ESP_LOGE(TAG, "%s", line);
        ESP_LOGE(TAG, "%s", report.detail);
        return;
    }
    ESP_LOGI(TAG, "%s", report.detail);

    chorus_session_config_t session;
    memset(&session, 0, sizeof(session));
    snprintf(session.server, sizeof(session.server), "%s", config.server_address);
    session.first_backoff_ms = config.reconnect_first_backoff_ms;
    session.max_backoff_ms = config.reconnect_max_backoff_ms;
    session.run_seconds = 0; /* until the power goes away */
    session.sync_interval_ms = 500;
    session.filter_window = 64;
    session.smoothing_alpha = 0.0625;
    session.event_log_path = NULL;

    /* The fault poll runs BESIDE the session, in its own task, rather than
     * between slices of it: the supervisor's state - the backoff, the filter,
     * the counters - lives inside one call to chorus_session_run, and cutting
     * that call into one-second pieces would reset the very thing AC-4 is
     * about. An amplifier that faults while audio is playing has its output
     * stage placed in high impedance and its clock stopped by
     * chorus_amp_poll_fault, so the audio stops at the pins whatever the
     * session is doing, and the condition is published by name. */
    static fault_watch_t watch;
    watch.config = &config;
    watch.bus = &bus;
    watch.stage = &stage;
    watch.controller = &controller;
    watch.telemetry = &telemetry;
    if (xTaskCreate(fault_watch, "chorus-amp-fault", 4096, &watch, 5, NULL) != pdPASS) {
        ESP_LOGE(TAG, "the amplifier fault watch could not be started; nothing would notice a "
                      "fault, so the output stage goes back to high impedance");
        (void)controller.stop_clock(controller.ctx);
        (void)stage.high_impedance(stage.ctx);
        return;
    }

    chorus_session_result_t result;
    (void)chorus_session_run(&session, &result);

    /* run_seconds is zero, so the line above does not return while the board
     * has power. If it ever does, the output stage goes dead rather than being
     * left live with nothing feeding it. */
    (void)controller.stop_clock(controller.ctx);
    (void)stage.high_impedance(stage.ctx);
    ESP_LOGE(TAG, "the session ended; the output stage is in high impedance");
}
