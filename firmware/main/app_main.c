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

#include <stdint.h>
#include <string.h>

#include "console_esp.h"
#include "esp_discovery.h"
#include "esp_hal.h"
#include "esp_heap_caps.h"
#include "esp_identity.h"
#include "esp_link.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#include "esp_playout.h"
#include "esp_provision.h"
#include "esp_store.h"

#include "chorus/amp.h"
#include "chorus/endpoint_config.h"
#include "chorus/endpoint_dsp.h"
#include "chorus/link.h"
#include "chorus/playout.h"
#include "chorus/session.h"
#include "chorus/sync_conf.h"
#include "chorus/telemetry.h"
#include "chorus/wifi.h"

static const char *TAG = "chorus-endpoint";

/* firmware/config/endpoint.conf, embedded by the component's CMakeLists. */
extern const char endpoint_conf_start[] __asm__("_binary_endpoint_conf_start");
extern const char endpoint_conf_end[] __asm__("_binary_endpoint_conf_end");
/* The board profile the image was built for (firmware/boards/, chosen by
 * tools/firmware-image.sh), embedded under one fixed name. */
extern const char board_profile_conf_start[] __asm__("_binary_board_profile_conf_start");
extern const char board_profile_conf_end[] __asm__("_binary_board_profile_conf_end");

/* config/sync.conf, embedded the same way (audit A-12): the loop's constants
 * come from the one file the Linux client is held to, not from literals. */
extern const char sync_conf_start[] __asm__("_binary_sync_conf_start");
extern const char sync_conf_end[] __asm__("_binary_sync_conf_end");

/* The session task's stack. The session decodes FLAC and Opus inline, and
 * libopus (built with VAR_ARRAYS, third_party/opus/chorus-build.txt) keeps its
 * scratch on the caller's stack: decoding the RFC 8251 CELT stereo vector took
 * 22,056 bytes of stack on the host (x86-64, -O2; the method is in
 * docs/decisions/0058-the-endpoint-playout-path.md), FLAC 10,048. 32 KiB is that plus the session's
 * own frames with headroom for the Xtensa windowed ABI; ASSUMED until the bench reads
 * uxTaskGetStackHighWaterMark (ESP-IDF's xTaskCreate takes bytes). */
#define SESSION_STACK_BYTES 32768
#define SESSION_PRIORITY 5

/* How often the amplifier's fault register is read while audio is playing. A
 * fault that is surfaced a second late is still surfaced; one that is never
 * read is not. */
#define FAULT_POLL_MS 1000

/* Everything the fault watch needs. It shares the I2C bus and the output stage
 * with nothing else: the session supervisor touches a socket, a filter and the
 * playout buffer, and the playout writer only writes to the I2S channel, which
 * the driver serialises itself (ESP-IDF v6.1 I2S guide, "Thread Safety"). */
typedef struct {
    const chorus_endpoint_config_t *config;
    chorus_i2c_bus_t *bus;
    chorus_output_stage_t *stage;
    chorus_i2s_controller_t *controller;
    chorus_telemetry_t *telemetry;
} fault_watch_t;

/* Everything the session task needs; static, like the rest of what app_main
 * hands to a task that outlives it. */
typedef struct {
    chorus_session_config_t session;
    chorus_i2s_controller_t *controller;
    chorus_output_stage_t *stage;
    const chorus_amp_config_t *amp;
    chorus_i2c_bus_t *bus;
} session_task_t;

/* AC-5, on the board: read the fault register, and on a fault stop the audio
 * and say which fault by name. Never swallow one, and never keep playing
 * through one. */
static void fault_watch(void *argument)
{
    fault_watch_t *watch = (fault_watch_t *)argument;
    chorus_amp_report_t report;
    char line[1024];
    for (;;) {
        vTaskDelay(pdMS_TO_TICKS(FAULT_POLL_MS));
        chorus_amp_status_t status = chorus_amp_poll_fault(
            &watch->config->amp, watch->bus, watch->stage, watch->controller, &report);
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

/* The session runs in its own task, sized for the decoders, rather than on
 * app_main's (CONFIG_ESP_MAIN_TASK_STACK_SIZE). */
static void session_task(void *argument)
{
    session_task_t *task = (session_task_t *)argument;
    static chorus_session_result_t result;
    (void)chorus_session_run(&task->session, &result);

    /* run_seconds is zero, so the line above does not return while the board
     * has power. If it ever does, the output stage goes dead rather than being
     * left live with nothing feeding it. */
    if (task->amp != NULL) {
        (void)chorus_amp_shut_down(task->amp, task->bus, task->stage, task->controller);
        ESP_LOGE(TAG, "the session ended; the output stage is in high impedance");
    } else {
        ESP_LOGE(TAG, "the session ended (%s): %s", chorus_session_end_name(result.end),
                 result.detail);
    }
    vTaskDelete(NULL);
}

/* The output stage back to high impedance, on every path that stops after the
 * amplifier came up. A board that plays through nothing (the emulated one) has
 * no output stage, and nothing is asked of a bus that was never created. */
static void output_stage_off(int plays, const chorus_amp_config_t *amp, chorus_i2c_bus_t *bus,
                             chorus_output_stage_t *stage, chorus_i2s_controller_t *controller)
{
    if (plays) {
        (void)chorus_amp_shut_down(amp, bus, stage, controller);
    }
}

void app_main(void)
{
    static char text[32768];
    static char board_text[4096];
    /* The two linker symbols bound one object, but C only defines subtracting
     * pointers into the same array, and the compiler sees two. The addresses
     * are subtracted as integers instead, which says what is meant. */
    size_t length = (size_t)((uintptr_t)endpoint_conf_end - (uintptr_t)endpoint_conf_start);
    size_t board_length =
        (size_t)((uintptr_t)board_profile_conf_end - (uintptr_t)board_profile_conf_start);
    if (length >= sizeof(text) || board_length >= sizeof(board_text)) {
        ESP_LOGE(TAG, "the embedded configuration does not fit");
        return;
    }
    memcpy(text, endpoint_conf_start, length);
    text[length] = '\0';
    memcpy(board_text, board_profile_conf_start, board_length);
    board_text[board_length] = '\0';

    /* Static, like everything below that the fault watch is handed: the watch
     * is its own task and outlives this function if the session ever returns,
     * so nothing it points at may live on this task's stack. The board
     * profile is laid over endpoint.conf by the reader the host build grades
     * (firmware/tests/test_link.c). */
    static chorus_endpoint_config_t config;
    char detail[512];
    detail[0] = '\0';
    if (chorus_endpoint_config_parse_profile(&config, "firmware/config/endpoint.conf", text,
                                             "the embedded board profile", board_text, detail,
                                             sizeof(detail)) != 0) {
        ESP_LOGE(TAG, "%s", detail);
        return;
    }
    /* Which board this image believes it is on, and how sure it is. */
    ESP_LOGI(TAG, "board profile=%s model=\"%s\" status=%s needs_item=\"%s\" link=%s",
             config.board.profile, config.board.model,
             chorus_board_status_name(config.board.model_status), config.board.needs_item,
             chorus_transport_name(config.link.transport));

    /* The serial console (audit A-13), as soon as the configuration has parsed,
     * so it answers on a board whose bring-up stops below. Runtime-only
     * settings and the bench's commands; a console that cannot start is logged
     * and the endpoint runs on without it. */
    (void)chorus_esp_console_start(&config);

    /* The same validation the host build gates its compile on. A board that
     * somehow booted an image built from a configuration that breaks a
     * platform rule stops here rather than driving a loudspeaker with it. */
    static chorus_finding_t findings[16];
    size_t finding_count = 0;
    chorus_endpoint_config_validate(&config, findings, 16, &finding_count);
    for (size_t i = 0; i < finding_count; i++) {
        ESP_LOGE(TAG, "FAIL %s :: %s", findings[i].rule, findings[i].detail);
    }
    if (finding_count > 0) {
        ESP_LOGE(TAG, "the committed configuration is refused; no clock is started");
        return;
    }

    /* What the board keeps across boots (goal 14, chorus/store.h): NVS, once,
     * before anything reads it. A store that cannot be initialised is said
     * and nothing is erased; whoever needs it then refuses by name. */
    (void)chorus_esp_store_init();

    /* Whether this board plays through anything (goal 14, chorus/
     * endpoint_config.h). Every speaker does. The emulator's board declares
     * `none` (the validation above refuses that on any other link), and then
     * no I2C bus, I2S channel, amplifier, playout path or fault watch is
     * brought up: the session runs with nowhere to play, and everything else
     * below is the code a speaker runs. */
    const int plays = config.board.audio_output == CHORUS_AUDIO_OUTPUT_AMPLIFIER;
    if (!plays) {
        ESP_LOGW(TAG, "board_audio_output = none: no amplifier, no I2S and no I2C are brought up "
                      "(the emulated board); the session runs with nowhere to play");
    }

    static chorus_i2c_bus_t bus;
    static chorus_output_stage_t stage;
    static chorus_i2s_controller_t controller;
    if (plays && chorus_esp_hal_init(&config, &bus, &stage, &controller) != 0) {
        ESP_LOGE(TAG, "the hardware could not be brought up; the output stage stays dead");
        return;
    }

    /* config/sync.conf, then the playout path, BEFORE the amplifier's
     * bring-up starts the I2S clock: the path preloads the stopped channel's
     * DMA ring so that written and consumed count the same frames. */
    static char sync_text[4096];
    size_t sync_length = (size_t)((uintptr_t)sync_conf_end - (uintptr_t)sync_conf_start);
    if (sync_length >= sizeof(sync_text)) {
        ESP_LOGE(TAG, "the embedded config/sync.conf does not fit");
        return;
    }
    memcpy(sync_text, sync_conf_start, sync_length);
    sync_text[sync_length] = '\0';
    static chorus_sync_conf_t sync;
    if (chorus_sync_conf_parse(&sync, "config/sync.conf", sync_text, detail, sizeof(detail)) != 0) {
        ESP_LOGE(TAG, "%s", detail);
        return;
    }
    chorus_playout_t *playout = plays ? chorus_esp_playout_create(&config, &sync) : NULL;
    if (plays && playout == NULL) {
        ESP_LOGE(TAG, "no playout path; the output stage stays dead");
        return;
    }

    /* The sound chain (goal 12, chorus/endpoint_dsp.h), configured from
     * endpoint.conf's two-way and from each `sound` and stream_format the
     * session hands the playout path; out of the path until a `sound` arrives
     * or the two-way is on. It is one object of about 73 KB (the chain alone
     * is 72680 bytes) and nothing on the audio path allocates, but as a static
     * it does not fit the linker's static DRAM region beside the rest of the
     * image (it overflowed dram0_0_seg by 38256 bytes), so it is taken from
     * the internal heap once, here at boot, as the jitter buffer is
     * (esp_playout.c), never from external RAM (firmware/endpoint-units.conf
     * rule 3). Without the RAM the endpoint plays as before goal 12, with the
     * room's gain, and says so. The subwoofer's level and phase knobs reach
     * the chain through chorus_playout_set_sub_knobs; their ADC binding is not
     * written yet (ADR 0063), so until it is they sit at 0 dB and 0 degrees. */
    chorus_endpoint_dsp_t *dsp = plays ? heap_caps_malloc(sizeof(chorus_endpoint_dsp_t),
                                                          MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT)
                                       : NULL;
    if (!plays) {
        /* Nothing to run a sound chain for. */
    } else if (dsp == NULL) {
        ESP_LOGE(TAG,
                 "no internal RAM for the %u-byte sound chain (%u bytes free); playing "
                 "without it",
                 (unsigned)sizeof(chorus_endpoint_dsp_t),
                 (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT));
    } else {
        chorus_endpoint_dsp_init(dsp, &config.two_way);
        chorus_playout_set_dsp(playout, dsp);
        ESP_LOGI(TAG,
                 "sound chain: %u bytes, two_way=%s crossover_hz=%u woofer_slot=%u "
                 "tweeter_slot=%u, internal RAM free after it: %u bytes",
                 (unsigned)sizeof(chorus_endpoint_dsp_t), config.two_way.enabled ? "on" : "off",
                 (unsigned)config.two_way.crossover_hz, (unsigned)config.two_way.woofer_slot,
                 (unsigned)config.two_way.tweeter_slot,
                 (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT));
    }

    static chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);

    static chorus_amp_report_t report;
    if (plays) {
        chorus_amp_status_t status = chorus_amp_bring_up(&config.amp, &config.gain, &config.clock,
                                                         &bus, &stage, &controller, &report);
        chorus_telemetry_record_amp(&telemetry, &report);
        if (status != CHORUS_AMP_OK) {
            static char line[1024];
            chorus_telemetry_line(&telemetry, line, sizeof(line));
            /* Surfaced, not swallowed, and the endpoint stops here rather than
             * starting a session it cannot play. */
            ESP_LOGE(TAG, "%s", line);
            ESP_LOGE(TAG, "%s", report.detail);
            return;
        }
        ESP_LOGI(TAG, "%s", report.detail);
    }

    /* The link, before the session is told it is usable.
     *
     * P1: a wired endpoint brings up the W5500 and never the radio; a
     * wireless one (the compact speakers' Wi-Fi tier, K91) runs
     * `chorus#WIFI-7`'s bring-up, which SETS the power save mode from the
     * committed configuration rather than inheriting it, reads it back and
     * publishes it on the line below. firmware/src/link.c decides which, and
     * firmware/tests/test_link.c grades that decision. A bring-up that left the
     * link down stops here rather than opening a session against no network,
     * and the output stage goes back to high impedance rather than being left
     * live with nothing feeding it. */
    chorus_radio_t radio;
    chorus_esp_hal_radio(&radio);
    chorus_ethernet_t ethernet;
    if (config.link.transport == CHORUS_TRANSPORT_EMULATED) {
        /* The emulator's controller instead of the W5500 (esp_link.h). */
        chorus_esp_link_emulated(&ethernet);
    } else {
        chorus_esp_link_ethernet(&ethernet);
    }
    /* Goal 14: a wireless speaker gets its network from the store, or raises
     * its own access point and waits here until a phone gives it one
     * (esp_provision.h). In a wired image this is nothing. */
    chorus_esp_provision_or_join(&config, &radio);
    static chorus_link_report_t link;
    chorus_bring_up_status_t link_status =
        chorus_link_bring_up(&config.link, &config.eth, &config.pins, &ethernet, &radio, &link);
    chorus_telemetry_record_wifi(&telemetry, &link.wifi);
    {
        static char line[1024];
        chorus_telemetry_line(&telemetry, line, sizeof(line));
        if (link.link_up) {
            ESP_LOGI(TAG, "%s", line);
            ESP_LOGI(TAG, "%s", link.detail);
        } else {
            ESP_LOGE(TAG, "%s", line);
            ESP_LOGE(TAG, "%s", link.detail);
            ESP_LOGE(TAG, "%s", link.wifi.detail);
        }
    }
    if (!link.link_up) {
        ESP_LOGE(TAG, "the link is down (%s); no session is opened",
                 chorus_bring_up_status_name(link_status));
        output_stage_off(plays, &config.amp, &bus, &stage, &controller);
        return;
    }

    /* The writer, now that the clock runs: it paces itself on the DMA. */
    if (plays && chorus_esp_playout_start(playout) != 0) {
        output_stage_off(plays, &config.amp, &bus, &stage, &controller);
        return;
    }

    static session_task_t task;
    chorus_session_config_t *session = &task.session;
    memset(session, 0, sizeof(*session));
    snprintf(session->server, sizeof(session->server), "%s", config.server_address);
    if (config.link.transport == CHORUS_TRANSPORT_EMULATED) {
        /* The emulator's network carries no multicast and the committed
         * address is the guest's own loopback; its server is the gateway the
         * address lease named, at the committed port (chorus/link.h). The
         * emulator is also the one board with nobody at its console, so the
         * session's events go to the console, where the run that grades it
         * reads them (tools/qemu-boot-run.sh). */
        static char gateway[32];
        static char emulated_server[sizeof(session->server)];
        if (chorus_esp_link_gateway(gateway, sizeof(gateway)) == 0 &&
            chorus_link_emulated_server(gateway, config.server_address, emulated_server,
                                        sizeof(emulated_server)) == 0) {
            snprintf(session->server, sizeof(session->server), "%s", emulated_server);
            ESP_LOGI(TAG, "emulated link: the server is the gateway, %s", session->server);
        } else {
            ESP_LOGE(TAG,
                     "emulated link: the address lease named no gateway; the session keeps "
                     "the configured %s",
                     session->server);
        }
        session->event_log_path = "/dev/console";
    }
    session->first_backoff_ms = config.reconnect_first_backoff_ms;
    session->max_backoff_ms = config.reconnect_max_backoff_ms;
    session->run_seconds = 0; /* until the power goes away */
    session->sync_interval_ms = sync.sync_interval_ms;
    session->filter_window = sync.filter_window;
    session->smoothing_alpha = sync.smoothing_alpha;
    session->playout = playout;
    task.controller = &controller;
    task.stage = &stage;
    /* NULL on a board that plays through nothing: the session task then has no
     * output stage to put in high impedance if the session ever ends. */
    task.amp = plays ? &config.amp : NULL;
    task.bus = &bus;

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
    if (plays && xTaskCreate(fault_watch, "chorus-amp-fault", 4096, &watch, 5, NULL) != pdPASS) {
        ESP_LOGE(TAG, "the amplifier fault watch could not be started; nothing would notice a "
                      "fault, so the output stage goes back to high impedance");
        output_stage_off(plays, &config.amp, &bus, &stage, &controller);
        return;
    }

    /* The console's hooks into the session (audit A-13): `server` and `status`. */
    chorus_esp_console_attach(session);

    /* Who this board is and where its server is (goal 14), with the link up
     * and before the session runs. The identity is the store's: an id and a
     * Noise key made at the first boot and the same at every boot after, so
     * the server that adopted this board knows it again. A board that cannot
     * keep one opens no session: under an id and key that change at the next
     * boot it would be adopted once and refused ever after. Then the server:
     * a DNS-SD browse, else the last server this board shook hands with, else
     * a configured address that is not loopback (firmware/src/discovery.c). */
    if (chorus_esp_identity_load(session) != 0) {
        ESP_LOGE(TAG, "this board has no identity it can keep; no session is opened and the "
                      "output stage goes back to high impedance");
        output_stage_off(plays, &config.amp, &bus, &stage, &controller);
        return;
    }
    chorus_esp_discovery_locate(session);

    if (xTaskCreate(session_task, "chorus-session", SESSION_STACK_BYTES, &task, SESSION_PRIORITY,
                    NULL) != pdPASS) {
        ESP_LOGE(TAG, "the session task could not be started; the output stage goes back to "
                      "high impedance");
        output_stage_off(plays, &config.amp, &bus, &stage, &controller);
    }
    /* app_main returns and its task is deleted (CONFIG_ESP_MAIN_TASK_STACK_SIZE
     * help: "If app_main() returns then this task is deleted"); everything the
     * tasks above use is static. */
}
