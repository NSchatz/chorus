#include "esp_hal.h"

#include <string.h>

#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "driver/i2s_std.h"
#include "esp_err.h"
#include "esp_log.h"

static const char *TAG = "chorus-hal";

/* Everything the three interfaces need, in one place. Static because there is
 * one endpoint per board and a second would want a second amplifier. */
typedef struct {
    i2c_master_bus_handle_t i2c_bus;
    i2c_master_dev_handle_t amp;
    i2s_chan_handle_t tx;
    gpio_num_t power_down;
    int channel_created;
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
 * line low needs no address and therefore no datasheet, which is what lets the
 * endpoint reach its one safe state before it knows anything about the part -
 * including before it knows whether the part is there. */
static int hal_high_impedance(void *ctx)
{
    (void)ctx;
    return (gpio_set_level(hal.power_down, 0) == ESP_OK) ? 0 : -1;
}

static int hal_enable(void *ctx)
{
    (void)ctx;
    return (gpio_set_level(hal.power_down, 1) == ESP_OK) ? 0 : -1;
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
        .slot_cfg = I2S_STD_PHILIPS_SLOT_DEFAULT_CONFIG(
            (i2s_data_bit_width_t)clock->slot_bit_width, I2S_SLOT_MODE_STEREO),
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
        .device_address = config->amp.address,
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
                .mclk = (gpio_num_t)config->pins.mclk,
                .bclk = (gpio_num_t)config->pins.bclk,
                .ws = (gpio_num_t)config->pins.ws,
                .dout = (gpio_num_t)config->pins.dout,
                .din = I2S_GPIO_UNUSED,
                .invert_flags = {0},
            },
    };
    if (i2s_channel_init_std_mode(hal.tx, &std) != ESP_OK) {
        ESP_LOGE(TAG, "the I2S channel could not be configured");
        return -1;
    }

    bus->ctx = &hal;
    bus->read = hal_read;
    bus->write = hal_write;
    stage->ctx = &hal;
    stage->high_impedance = hal_high_impedance;
    stage->enable = hal_enable;
    controller->ctx = &hal;
    controller->apply_clock = hal_apply_clock;
    controller->stop_clock = hal_stop_clock;
    return 0;
}
