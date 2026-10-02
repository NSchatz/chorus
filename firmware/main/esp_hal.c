#include "esp_hal.h"

#include <stdio.h>
#include <string.h>

#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "driver/i2s_std.h"
#include "esp_err.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "nvs_flash.h"

#include "esp_marker.h"

static const char *TAG = "chorus-hal";

/* Everything the three interfaces need, in one place. Static because there is
 * one endpoint per board and a second would want a second amplifier. */
typedef struct {
    i2c_master_bus_handle_t i2c_bus;
    i2c_master_dev_handle_t amp;
    i2s_chan_handle_t tx;
    gpio_num_t power_down;
    int channel_created;
    /* The playout path the TX interrupt counts for; NULL until attached. */
    chorus_playout_t *volatile playout;
} esp_hal_t;

static esp_hal_t hal;

/* One byte out of one register. `esp_err_t` is folded onto the bus result the
 * sequencer understands, because "the part did not answer" and "the bus timed
 * out" are different conditions and the sequencer reports them by different
 * names. */
static chorus_i2c_result_t hal_read(void *ctx, uint8_t address, uint8_t reg, uint8_t *value)
{
    (void)ctx;
    (void)address;
    esp_err_t err = i2c_master_transmit_receive(hal.amp, &reg, 1, value, 1, 100);
    switch (err) {
    case ESP_OK:
        return CHORUS_I2C_ACK;
    case ESP_ERR_TIMEOUT:
        return CHORUS_I2C_TIMEOUT;
    case ESP_ERR_NOT_FOUND:
    case ESP_FAIL:
        return CHORUS_I2C_NACK;
    default:
        return CHORUS_I2C_BUS_ERROR;
    }
}

static chorus_i2c_result_t hal_write(void *ctx, uint8_t address, uint8_t reg, uint8_t value)
{
    (void)ctx;
    (void)address;
    const uint8_t payload[2] = {reg, value};
    esp_err_t err = i2c_master_transmit(hal.amp, payload, sizeof(payload), 100);
    switch (err) {
    case ESP_OK:
        return CHORUS_I2C_ACK;
    case ESP_ERR_TIMEOUT:
        return CHORUS_I2C_TIMEOUT;
    case ESP_ERR_NOT_FOUND:
    case ESP_FAIL:
        return CHORUS_I2C_NACK;
    default:
        return CHORUS_I2C_BUS_ERROR;
    }
}

/* High impedance is a PIN, not a register. Driving the amplifier's power-down
 * line low needs no address, which is what lets the endpoint reach its one safe
 * state before it knows anything about the part - including before it knows
 * whether the part is there. With PDN low the TAS5825M is in shutdown and does
 * not answer I2C (TAS5825M datasheet SLASEH7H, p. 4), so the sequencer powers
 * it up before its first transaction. */
static int hal_high_impedance(void *ctx)
{
    (void)ctx;
    return (gpio_set_level(hal.power_down, 0) == ESP_OK) ? 0 : -1;
}

/* PDN high: the part leaves shutdown in Deep Sleep, its output not switching
 * until it is commanded to Play over I2C (p. 48). */
static int hal_power_up(void *ctx)
{
    (void)ctx;
    return (gpio_set_level(hal.power_down, 1) == ESP_OK) ? 0 : -1;
}

/* At least `ms`: one tick more than the rounded-down count, so tick rounding
 * never shortens a datasheet minimum. */
static void hal_wait_ms(void *ctx, uint32_t ms)
{
    (void)ctx;
    vTaskDelay(pdMS_TO_TICKS(ms) + 1);
}

static int hal_apply_clock(void *ctx, const chorus_i2s_clock_t *clock)
{
    (void)ctx;

    i2s_std_config_t std = {
        .clk_cfg =
            {
                .sample_rate_hz = clock->sample_rate_hz,
                .clk_src = I2S_CLK_SRC_DEFAULT,
                /* The whole of AC-3, in one field. The committed configuration
                 * carries the multiple and firmware/src/i2s.c has already
                 * refused anything not divisible by three at a 24-bit slot
                 * width, both here and at build time. */
                .mclk_multiple = (i2s_mclk_multiple_t)clock->mclk_multiple,
            },
        .slot_cfg = I2S_STD_PHILIPS_SLOT_DEFAULT_CONFIG((i2s_data_bit_width_t)clock->slot_bit_width,
                                                        I2S_SLOT_MODE_STEREO),
        .gpio_cfg =
            {
                .mclk = (gpio_num_t)-1,
                .bclk = (gpio_num_t)-1,
                .ws = (gpio_num_t)-1,
                .dout = (gpio_num_t)-1,
                .din = I2S_GPIO_UNUSED,
                .invert_flags = {0},
            },
    };
    /* The pins were fixed when the channel was created; re-applying a clock
     * only reconfigures the clock. */
    std.gpio_cfg.mclk = I2S_GPIO_UNUSED;
    std.gpio_cfg.bclk = I2S_GPIO_UNUSED;
    std.gpio_cfg.ws = I2S_GPIO_UNUSED;
    std.gpio_cfg.dout = I2S_GPIO_UNUSED;

    if (hal.channel_created) {
        if (i2s_channel_disable(hal.tx) != ESP_OK) {
            return -1;
        }
    }
    if (i2s_channel_reconfig_std_clock(hal.tx, &std.clk_cfg) != ESP_OK) {
        return -1;
    }
    if (i2s_channel_enable(hal.tx) != ESP_OK) {
        return -1;
    }
    hal.channel_created = 1;
    return 0;
}

static int hal_stop_clock(void *ctx)
{
    (void)ctx;
    if (!hal.channel_created) {
        return 0;
    }
    return (i2s_channel_disable(hal.tx) == ESP_OK) ? 0 : -1;
}

/* The I2S TX `on_sent` callback: the DMA just finished sending one buffer of
 * `event->size` bytes. It runs in the interrupt (ESP-IDF v6.1 I2S guide: "it is
 * an interrupt callback, so do not add complex logic, run floating operation,
 * or call non-reentrant functions in the callback"), so it only counts: the
 * frames and their monotonic stamp go to the playout path's hook, which takes
 * no lock and does no float. Returns false: no task was woken. */
static bool hal_on_sent(i2s_chan_handle_t handle, i2s_event_data_t *event, void *ctx)
{
    (void)handle;
    (void)ctx;
    chorus_playout_t *playout = hal.playout;
    if (playout != NULL) {
        chorus_playout_on_dma_sent(playout, event->size);
        /* The GPIO marker: arms its alarm when the buffer now starting holds
         * a marked boundary (goal 9; esp_marker.c). */
        chorus_esp_marker_on_dma_sent(playout);
    }
    return false;
}

int chorus_esp_hal_init(const chorus_endpoint_config_t *config, chorus_i2c_bus_t *bus,
                        chorus_output_stage_t *stage, chorus_i2s_controller_t *controller)
{
    memset(&hal, 0, sizeof(hal));
    hal.power_down = (gpio_num_t)config->pins.amp_power_down;

    /* The power-down line first, driven low, so the output stage is dead
     * before an I2S pin is so much as configured. */
    gpio_config_t pd = {
        .pin_bit_mask = 1ULL << config->pins.amp_power_down,
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_ENABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    if (gpio_config(&pd) != ESP_OK || gpio_set_level(hal.power_down, 0) != ESP_OK) {
        ESP_LOGE(TAG, "the amplifier power-down line could not be driven low");
        return -1;
    }

    i2c_master_bus_config_t bus_config = {
        .i2c_port = -1,
        .sda_io_num = (gpio_num_t)config->pins.sda,
        .scl_io_num = (gpio_num_t)config->pins.scl,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags = {.enable_internal_pullup = true},
    };
    if (i2c_new_master_bus(&bus_config, &hal.i2c_bus) != ESP_OK) {
        ESP_LOGE(TAG, "the I2C bus could not be created");
        return -1;
    }

    /* The address comes from the committed configuration and is DECLARED
     * UNKNOWN until somebody reads it off the datasheet. The sequencer refuses
     * by name in that case, so this only ever runs with a real value. */
    i2c_device_config_t device = {
        .dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .device_address = config->amp.address.value,
        .scl_speed_hz = 100000,
    };
    if (i2c_master_bus_add_device(hal.i2c_bus, &device, &hal.amp) != ESP_OK) {
        ESP_LOGE(TAG, "the amplifier could not be added to the I2C bus");
        return -1;
    }

    /* DMA descriptors live in internal RAM. The platform forbids anything else
     * ("DMA transaction descriptors cannot be placed in PSRAM"), the committed
     * configuration says internal, and firmware/check/endpoint_scan.c fails
     * the suite if a PSRAM placement ever appears in this tree. */
    i2s_chan_config_t channel = I2S_CHANNEL_DEFAULT_CONFIG(I2S_NUM_AUTO, I2S_ROLE_MASTER);
    channel.dma_desc_num = config->clock.dma_desc_num;
    channel.dma_frame_num = config->clock.dma_frame_num;
    channel.auto_clear = true;
    if (i2s_new_channel(&channel, &hal.tx, NULL) != ESP_OK) {
        ESP_LOGE(TAG, "the I2S channel could not be created");
        return -1;
    }

    i2s_std_config_t std = {
        .clk_cfg =
            {
                .sample_rate_hz = config->clock.sample_rate_hz,
                .clk_src = I2S_CLK_SRC_DEFAULT,
                .mclk_multiple = (i2s_mclk_multiple_t)config->clock.mclk_multiple,
            },
        .slot_cfg = I2S_STD_PHILIPS_SLOT_DEFAULT_CONFIG(
            (i2s_data_bit_width_t)config->clock.slot_bit_width, I2S_SLOT_MODE_STEREO),
        .gpio_cfg =
            {
                /* `none` on a board that does not route MCLK out. */
                .mclk = (config->pins.mclk == CHORUS_PIN_NONE) ? I2S_GPIO_UNUSED
                                                               : (gpio_num_t)config->pins.mclk,
                .bclk = (gpio_num_t)config->pins.bclk,
                .ws = (gpio_num_t)config->pins.ws,
                .dout = (gpio_num_t)config->pins.dout,
                .din = I2S_GPIO_UNUSED,
                .invert_flags = {0},
            },
    };
    /* The sample sits in a wider slot on the wire: two 32-bit slots are the 64
     * bit clocks per frame the TAS5825M accepts (datasheet SLASEH7H, pp. 7, 29),
     * where two 24-bit ones would be 48. */
    std.slot_cfg.slot_bit_width = (i2s_slot_bit_width_t)config->clock.wire_slot_bit_width;
    if (i2s_channel_init_std_mode(hal.tx, &std) != ESP_OK) {
        ESP_LOGE(TAG, "the I2S channel could not be configured");
        return -1;
    }

    /* The DMA-consumed-frames measure. DMA event callbacks "can only be
     * registered or deregistered before the channel is enabled"
     * (components/esp_driver_i2s/include/driver/i2s_common.h, v6.1), so it is
     * registered here, before the amplifier's bring-up first starts the clock;
     * it counts nothing until a playout path is attached. */
    i2s_event_callbacks_t callbacks = {
        .on_sent = hal_on_sent,
    };
    if (i2s_channel_register_event_callback(hal.tx, &callbacks, NULL) != ESP_OK) {
        ESP_LOGE(TAG, "the I2S sent-buffer callback could not be registered");
        return -1;
    }

    bus->ctx = &hal;
    bus->read = hal_read;
    bus->write = hal_write;
    stage->ctx = &hal;
    stage->high_impedance = hal_high_impedance;
    stage->power_up = hal_power_up;
    stage->wait_ms = hal_wait_ms;
    controller->ctx = &hal;
    controller->apply_clock = hal_apply_clock;
    controller->stop_clock = hal_stop_clock;
    return 0;
}

/* --- the playout path ----------------------------------------------------------- */

int chorus_esp_hal_attach_playout(chorus_playout_t *playout, const chorus_endpoint_config_t *config)
{
    if (hal.channel_created) {
        /* Preloading needs the channel READY (never started): once the
         * amplifier has started the clock, the DMA is already cycling buffers
         * nobody counted, and written and consumed would not count the same
         * frames. */
        ESP_LOGE(TAG, "the playout path is attached before the clock starts, not after");
        return -1;
    }
    /* One DMA buffer of silence at a time until the DMA ring is full
     * (i2s_channel_preload_data: "when the bytes_loaded is smaller than the
     * size, it means the DMA buffers are full"). Every frame loaded counts as
     * written, so the first interrupt consumes frames the count knows about. */
    static uint8_t silence[CHORUS_ESP_HAL_MAX_DMA_BYTES];
    size_t frame_bytes = 2u * ((size_t)config->clock.slot_bit_width / 8u);
    size_t block = (size_t)config->clock.dma_frame_num * frame_bytes;
    if (block == 0 || block > sizeof(silence)) {
        ESP_LOGE(TAG, "a DMA buffer of %u frames does not fit the preload buffer",
                 (unsigned)config->clock.dma_frame_num);
        return -1;
    }
    memset(silence, 0, block);
    chorus_playout_dma_reset(playout);
    for (uint32_t i = 0; i < config->clock.dma_desc_num + 1u; i++) {
        size_t loaded = 0;
        if (i2s_channel_preload_data(hal.tx, silence, block, &loaded) != ESP_OK) {
            return -1;
        }
        chorus_playout_note_preloaded(playout, (uint32_t)(loaded / frame_bytes));
        if (loaded < block) {
            break;
        }
    }
    hal.playout = playout;
    return 0;
}

i2s_chan_handle_t chorus_esp_hal_i2s_tx(void)
{
    return hal.tx;
}

/* --- the radio ---------------------------------------------------------------
 *
 * The wireless half of `chorus#WIFI-7`, bound to the platform. Every DECISION
 * is in firmware/src/wifi.c and graded on a host; what is here is the wiring,
 * and the wiring is not claimed by this repository.
 *
 * The ORDER is the platform's own, from the carried ESP-IDF guide: the power
 * save mode is set "after calling esp_wifi_init()", and modem sleep starts
 * "when station connects to AP". So `hal_radio_init` goes as far as starting
 * the station, `chorus_wifi_bring_up` sets and reads back the mode, and
 * `hal_radio_join` is what connects. A mode set after the connect would be a
 * mode set after modem sleep had already begun. */

static wifi_ps_type_t to_platform_mode(chorus_wifi_ps_t mode)
{
    switch (mode) {
    case CHORUS_WIFI_PS_NONE:
        return WIFI_PS_NONE;
    case CHORUS_WIFI_PS_MIN_MODEM:
        return WIFI_PS_MIN_MODEM;
    case CHORUS_WIFI_PS_MAX_MODEM:
        return WIFI_PS_MAX_MODEM;
    case CHORUS_WIFI_PS_UNKNOWN:
        break;
    }
    /* Unreachable through the committed configuration: the reader refuses a
     * mode it cannot name, so nothing ever asks this for an unknown one. The
     * platform default is returned rather than an invented value, and the
     * readback is what would then disagree and withhold the bound. */
    return WIFI_PS_MIN_MODEM;
}

static chorus_wifi_ps_t from_platform_mode(wifi_ps_type_t mode)
{
    switch (mode) {
    case WIFI_PS_NONE:
        return CHORUS_WIFI_PS_NONE;
    case WIFI_PS_MIN_MODEM:
        return CHORUS_WIFI_PS_MIN_MODEM;
    case WIFI_PS_MAX_MODEM:
        return CHORUS_WIFI_PS_MAX_MODEM;
    default:
        /* A mode this build has no word for is reported as unknown, and a mode
         * nobody can name is not a mode anything may be claimed about. */
        return CHORUS_WIFI_PS_UNKNOWN;
    }
}

static int hal_radio_init(void *ctx)
{
    (void)ctx;
    /* The radio keeps its calibration in NVS. NVS is the store's to bring up
     * (firmware/main/esp_store.c, goal 14) and holds the board's identity, its
     * server pins and its Wi-Fi network, so a partition NVS cannot read is
     * REFUSED here and never erased: the erase this function used to do on
     * ESP_ERR_NVS_NO_FREE_PAGES and ESP_ERR_NVS_NEW_VERSION_FOUND would have
     * thrown all three away to start a radio. A second nvs_flash_init is
     * free. */
    esp_err_t nvs = nvs_flash_init();
    if (nvs != ESP_OK) {
        ESP_LOGE(TAG,
                 "nvs-not-usable (%s): the radio is not started and nothing is erased; the "
                 "partition holds this board's identity and network",
                 esp_err_to_name(nvs));
        return -1;
    }
    if (esp_netif_init() != ESP_OK) {
        return -1;
    }
    esp_err_t loop = esp_event_loop_create_default();
    if (loop != ESP_OK && loop != ESP_ERR_INVALID_STATE) {
        return -1;
    }
    if (esp_netif_create_default_wifi_sta() == NULL) {
        return -1;
    }
    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    if (esp_wifi_init(&init) != ESP_OK) {
        return -1;
    }
    if (esp_wifi_set_mode(WIFI_MODE_STA) != ESP_OK) {
        return -1;
    }
    /* Nothing here starts a network time service. The endpoint's only clock is
     * firmware/src/monotonic.c and every instant on the audio path comes from
     * it; the safety scans fail this file if a settable one ever appears. */
    return (esp_wifi_start() == ESP_OK) ? 0 : -1;
}

static int hal_radio_set_power_save(void *ctx, chorus_wifi_ps_t mode)
{
    (void)ctx;
    return (esp_wifi_set_ps(to_platform_mode(mode)) == ESP_OK) ? 0 : -1;
}

static int hal_radio_get_power_save(void *ctx, chorus_wifi_ps_t *mode)
{
    (void)ctx;
    wifi_ps_type_t reported = WIFI_PS_MIN_MODEM;
    if (esp_wifi_get_ps(&reported) != ESP_OK) {
        return -1;
    }
    *mode = from_platform_mode(reported);
    return 0;
}

/* Whether this IMAGE was built with software coexistence enabled.
 *
 * A build-time answer, and the honest one: the caveat is about what the
 * platform does with its time slice, the image's own configuration is what
 * decides that, and there is no portable runtime question that answers it. A
 * board whose coexistence is a hardware fact rather than a build one declares
 * it in firmware/config/endpoint.conf, and either source withholding the bound
 * is enough. */
static int hal_radio_coexistence_active(void *ctx, int *active)
{
    (void)ctx;
#if defined(CONFIG_SW_COEXIST_ENABLE)
    *active = 1;
#else
    *active = 0;
#endif
    return 0;
}

static int hal_radio_join(void *ctx, const char *ssid, const char *secret)
{
    (void)ctx;
    wifi_config_t station;
    memset(&station, 0, sizeof(station));
    snprintf((char *)station.sta.ssid, sizeof(station.sta.ssid), "%s", ssid);
    snprintf((char *)station.sta.password, sizeof(station.sta.password), "%s", secret);
    if (esp_wifi_set_config(WIFI_IF_STA, &station) != ESP_OK) {
        return -1;
    }
    return (esp_wifi_connect() == ESP_OK) ? 0 : -1;
}

void chorus_esp_hal_radio(chorus_radio_t *radio)
{
    radio->ctx = &hal;
    radio->init = hal_radio_init;
    radio->set_power_save = hal_radio_set_power_save;
    radio->get_power_save = hal_radio_get_power_save;
    radio->coexistence_active = hal_radio_coexistence_active;
    radio->join = hal_radio_join;
}
