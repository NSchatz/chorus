#include "console_esp.h"

#include <stdio.h>
#include <string.h>

#include "esp_console.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

#include "chorus/console.h"
#include "chorus/monotonic.h"
#include "esp_hal.h"

static const char *TAG = "chorus-console";

/* The REPL task's stack. decode-cost runs FLAC and Opus on this task, and
 * libopus keeps its scratch on the caller's stack (goal 6's note), so this is
 * well above ESP-IDF's 4096-byte default. ASSUMED, not measured: the least
 * free stack on a board is a bench question (the decode-cost reply carries it
 * as stack_free_bytes, and tools/decode-cost-run.sh records it), and
 * FreeRTOS's stack-overflow check names the task if it is too small. */
#define CONSOLE_STACK_BYTES 16384

/* The decode-cost fixtures, embedded from fixtures/codec by the component's
 * CMakeLists (EMBED_FILES): the same files the host test and the Linux client
 * read, so a figure on the chip is about the decode the host checked. */
extern const uint8_t flac_fields_start[] asm("_binary_flac_s16_stereo_44k1_fields_start");
extern const uint8_t flac_fields_end[] asm("_binary_flac_s16_stereo_44k1_fields_end");
extern const uint8_t flac_chunks_start[] asm("_binary_flac_s16_stereo_44k1_chunks_start");
extern const uint8_t flac_chunks_end[] asm("_binary_flac_s16_stereo_44k1_chunks_end");
extern const uint8_t opus_fields_start[] asm("_binary_opus_tv10_celt_stereo_fields_start");
extern const uint8_t opus_fields_end[] asm("_binary_opus_tv10_celt_stereo_fields_end");
extern const uint8_t opus_chunks_start[] asm("_binary_opus_tv10_celt_stereo_chunks_start");
extern const uint8_t opus_chunks_end[] asm("_binary_opus_tv10_celt_stereo_chunks_end");

static size_t span(const uint8_t *start, const uint8_t *end)
{
    return (size_t)((uintptr_t)end - (uintptr_t)start);
}

/* What the session hands on and what the console hands back, under one lock:
 * the session and the REPL are different tasks. */
typedef struct {
    SemaphoreHandle_t lock;
    int have_telemetry;
    chorus_telemetry_t telemetry;
    int server_pending;
    char server[CHORUS_SESSION_ADDRESS_MAX];
} shared_t;

static shared_t shared;
static chorus_radio_t radio;
static chorus_decode_fixture_t fixtures[2];
static chorus_console_t console;

static void on_telemetry(void *ctx, const chorus_telemetry_t *telemetry)
{
    shared_t *s = (shared_t *)ctx;
    if (xSemaphoreTake(s->lock, portMAX_DELAY) == pdTRUE) {
        s->telemetry = *telemetry;
        s->have_telemetry = 1;
        xSemaphoreGive(s->lock);
    }
}

static int read_telemetry(void *ctx, chorus_telemetry_t *out)
{
    shared_t *s = (shared_t *)ctx;
    int rc = -1;
    if (xSemaphoreTake(s->lock, portMAX_DELAY) == pdTRUE) {
        if (s->have_telemetry) {
            *out = s->telemetry;
            rc = 0;
        }
        xSemaphoreGive(s->lock);
    }
    return rc;
}

/* uxTaskGetStackHighWaterMark counts bytes on ESP-IDF's FreeRTOS port
 * (StackType_t is uint8_t there). */
static uint32_t stack_free_bytes(void)
{
    return (uint32_t)uxTaskGetStackHighWaterMark(NULL);
}

static int set_server(void *ctx, const char *address)
{
    shared_t *s = (shared_t *)ctx;
    if (strlen(address) >= sizeof(s->server)) {
        return -1;
    }
    if (xSemaphoreTake(s->lock, portMAX_DELAY) != pdTRUE) {
        return -1;
    }
    snprintf(s->server, sizeof(s->server), "%s", address);
    s->server_pending = 1;
    xSemaphoreGive(s->lock);
    return 0;
}

static int server_update(void *ctx, char *address, size_t len)
{
    shared_t *s = (shared_t *)ctx;
    int changed = 0;
    if (xSemaphoreTake(s->lock, portMAX_DELAY) == pdTRUE) {
        if (s->server_pending) {
            snprintf(address, len, "%s", s->server);
            s->server_pending = 0;
            changed = 1;
        }
        xSemaphoreGive(s->lock);
    }
    return changed;
}

/* One ESP-IDF command per console command, all landing here: the words are
 * joined back into the line console.c parses, and its one reply line goes to
 * the serial port. */
static int run(void *context, int argc, char **argv)
{
    (void)context;
    char line[256];
    size_t used = 0;
    line[0] = '\0';
    for (int i = 0; i < argc && used < sizeof(line); i++) {
        used += (size_t)snprintf(line + used, sizeof(line) - used, "%s%s", i ? " " : "", argv[i]);
    }
    static char reply[CHORUS_CONSOLE_REPLY];
    int rc = chorus_console_execute(&console, line, reply, sizeof(reply));
    printf("%s\n", reply);
    return rc;
}

int chorus_esp_console_start(const chorus_endpoint_config_t *config,
                             chorus_session_config_t *session)
{
    memset(&shared, 0, sizeof(shared));
    shared.lock = xSemaphoreCreateMutex();
    if (shared.lock == NULL) {
        ESP_LOGE(TAG, "no memory for the console's lock; the endpoint runs without a console");
        return -1;
    }

    fixtures[0] = (chorus_decode_fixture_t){
        "flac-s16-stereo-44k1", (const char *)flac_fields_start,
        span(flac_fields_start, flac_fields_end), flac_chunks_start,
        span(flac_chunks_start, flac_chunks_end)};
    fixtures[1] = (chorus_decode_fixture_t){
        "opus-tv10-celt-stereo", (const char *)opus_fields_start,
        span(opus_fields_start, opus_fields_end), opus_chunks_start,
        span(opus_chunks_start, opus_chunks_end)};

    memset(&console, 0, sizeof(console));
    console.transport = config->link.transport;
    if (console.transport == CHORUS_TRANSPORT_WIRELESS) {
        /* The same binding the bring-up used, over the one static radio. */
        chorus_esp_hal_radio(&radio);
        console.radio = &radio;
    }
    console.telemetry = read_telemetry;
    console.telemetry_ctx = &shared;
    console.set_server = set_server;
    console.server_ctx = &shared;
    console.fixtures = fixtures;
    console.fixture_count = 2;
    console.now_ns = chorus_monotonic_now_ns;
    console.stack_free_bytes = stack_free_bytes;

    session->server_update = server_update;
    session->server_update_ctx = &shared;
    session->on_telemetry = on_telemetry;
    session->telemetry_ctx = &shared;

    /* The REPL on whichever serial line this image's console is configured
     * for, as ESP-IDF's own examples choose it (examples/system/console/basic
     * in the pinned v6.1 tree; components/console/esp_console.h, read
     * 2026-09-30). */
    esp_console_repl_t *repl = NULL;
    esp_console_repl_config_t repl_config = ESP_CONSOLE_REPL_CONFIG_DEFAULT();
    repl_config.prompt = "chorus>";
    repl_config.task_stack_size = CONSOLE_STACK_BYTES;
    esp_err_t err = ESP_FAIL;
#if defined(CONFIG_ESP_CONSOLE_UART_DEFAULT) || defined(CONFIG_ESP_CONSOLE_UART_CUSTOM)
    esp_console_dev_uart_config_t dev = ESP_CONSOLE_DEV_UART_CONFIG_DEFAULT();
    err = esp_console_new_repl_uart(&dev, &repl_config, &repl);
#elif defined(CONFIG_ESP_CONSOLE_USB_SERIAL_JTAG)
    esp_console_dev_usb_serial_jtag_config_t dev = ESP_CONSOLE_DEV_USB_SERIAL_JTAG_CONFIG_DEFAULT();
    err = esp_console_new_repl_usb_serial_jtag(&dev, &repl_config, &repl);
#endif
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "the console REPL could not be created (%s); the endpoint runs without one",
                 esp_err_to_name(err));
        return -1;
    }
    static const char *const help[] = {
        "power-save none|min-modem|max-modem: set the Wi-Fi power save mode and read it back",
        "server <host>:<port>: the server the next connection goes to (runtime only)",
        "status: the telemetry line",
        "decode-cost [fixture]: time FLAC and Opus decode of the carried fixtures",
        "help: the commands"};
    /* chorus's own `help` rather than ESP-IDF's: the bench scripts check the
     * console is chorus's by its reply (tools/lib.sh require_endpoint_console). */
    for (size_t i = 0; i < chorus_console_command_count; i++) {
        esp_console_cmd_t cmd = {
            .command = chorus_console_commands[i],
            .help = help[i],
            .func_w_context = run,
            .context = NULL,
        };
        if (esp_console_cmd_register(&cmd) != ESP_OK) {
            ESP_LOGE(TAG, "the console command %s could not be registered",
                     chorus_console_commands[i]);
        }
    }
    err = esp_console_start_repl(repl);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "the console REPL could not start (%s)", esp_err_to_name(err));
        return -1;
    }
    ESP_LOGI(TAG, "console up: power-save, server, status, decode-cost (values are runtime only)");
    return 0;
}
