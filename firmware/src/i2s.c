#include "chorus/i2s.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>

const char *chorus_mem_placement_name(chorus_mem_placement_t placement)
{
    return (placement == CHORUS_MEM_EXTERNAL) ? "external" : "internal";
}

chorus_mem_placement_t chorus_mem_placement_from_name(const char *name, int *ok)
{
    *ok = 1;
    if (strcmp(name, "internal") == 0) {
        return CHORUS_MEM_INTERNAL;
    }
    if (strcmp(name, "external") == 0) {
        return CHORUS_MEM_EXTERNAL;
    }
    *ok = 0;
    return CHORUS_MEM_INTERNAL;
}

static size_t add_finding(chorus_finding_t *findings, size_t capacity, size_t *count,
                          const char *rule, const char *fmt, ...)
{
    if (*count >= capacity) {
        return 0;
    }
    chorus_finding_t *finding = &findings[*count];
    snprintf(finding->rule, sizeof(finding->rule), "%s", rule);
    va_list args;
    va_start(args, fmt);
    vsnprintf(finding->detail, sizeof(finding->detail), fmt, args);
    va_end(args);
    (*count)++;
    return 1;
}

uint64_t chorus_i2s_bclk_hz(const chorus_i2s_clock_t *clock)
{
    /* Two slots per frame: the endpoint carries stereo, and the reserved TV
     * channel counts are chorus#TV-9's, not this phase's. */
    return (uint64_t)clock->sample_rate_hz * 2ull * (uint64_t)clock->slot_bit_width;
}

uint64_t chorus_i2s_mclk_hz(const chorus_i2s_clock_t *clock)
{
    return (uint64_t)clock->sample_rate_hz * (uint64_t)clock->mclk_multiple;
}

int chorus_i2s_bclk_division_is_integral(const chorus_i2s_clock_t *clock)
{
    uint64_t bclk = chorus_i2s_bclk_hz(clock);
    if (bclk == 0) {
        return 0;
    }
    uint64_t mclk = chorus_i2s_mclk_hz(clock);
    return (mclk % bclk == 0) ? 1 : 0;
}

size_t chorus_i2s_validate_clock(const chorus_i2s_clock_t *clock, chorus_finding_t *findings,
                                 size_t capacity, size_t *count)
{
    size_t before = *count;

    if (clock->sample_rate_hz == 0) {
        add_finding(findings, capacity, count, "sample-rate-is-zero",
                    "i2s_sample_rate_hz = 0 is not a rate");
    }
    if (clock->slot_bit_width != 8 && clock->slot_bit_width != 16 && clock->slot_bit_width != 24 &&
        clock->slot_bit_width != 32) {
        add_finding(findings, capacity, count, "slot-bit-width-not-supported",
                    "i2s_slot_bit_width = %u is not one of 8, 16, 24 or 32",
                    clock->slot_bit_width);
    }
    if (clock->mclk_multiple == 0) {
        add_finding(findings, capacity, count, "mclk-multiple-is-zero",
                    "i2s_mclk_multiple = 0 produces no master clock");
    }

    if (clock->slot_bit_width == 24) {
        /* The rule this phase is named for. The carried ESP-IDF I2S reference
         * for ESP32-S3 states it twice, and this is the shorter of the two:
         * "Please set the mclk_multiple to I2S_MCLK_MULTIPLE_384 while using
         * 24 bits data width Otherwise the sample rate might be imprecise
         * since the BCLK division is not a integer". */
        if (clock->mclk_multiple % 3 != 0) {
            add_finding(findings, capacity, count, "mclk-multiple-not-divisible-by-three",
                        "i2s_mclk_multiple = %u is not divisible by 3 at a 24-bit slot width. "
                        "ESP-IDF (esp32s3 i2s reference, I2S_STD_CLK_DEFAULT_CONFIG): \"Please "
                        "set the mclk_multiple to I2S_MCLK_MULTIPLE_384 while using 24 bits data "
                        "width Otherwise the sample rate might be imprecise since the BCLK "
                        "division is not a integer\"",
                        clock->mclk_multiple);
        }
        /* The same reference, on the buffers: "when the data width is 24-bit,
         * the data buffer should be aligned with 3-byte ... Additionally,
         * i2s_chan_config_t::dma_frame_num, i2s_std_clk_config_t::
         * mclk_multiple, and the writing buffer size should be the multiple of
         * 3, otherwise the data on the line or the sample rate will be
         * incorrect." */
        if (clock->dma_frame_num == 0 || clock->dma_frame_num % 3 != 0) {
            add_finding(findings, capacity, count, "dma-frame-num-not-multiple-of-three",
                        "i2s_dma_frame_num = %u is not a nonzero multiple of 3 at a 24-bit slot "
                        "width. ESP-IDF (esp32s3 i2s reference): \"i2s_chan_config_t::"
                        "dma_frame_num, i2s_std_clk_config_t::mclk_multiple, and the writing "
                        "buffer size should be the multiple of 3, otherwise the data on the line "
                        "or the sample rate will be incorrect\"",
                        clock->dma_frame_num);
        }
    }

    /* MCLK has to be a whole multiple of BCLK as well as of the sample rate,
     * which is the general form of the rule above: "Normally, MCLK should be
     * the multiple of sample rate and BCLK at the same time." A configuration
     * that fails this produces a fractional bit-clock division whatever the
     * slot width is. */
    if (clock->sample_rate_hz != 0 && clock->slot_bit_width != 0 && clock->mclk_multiple != 0 &&
        !chorus_i2s_bclk_division_is_integral(clock)) {
        add_finding(findings, capacity, count, "bclk-division-is-not-integral",
                    "i2s_mclk_multiple = %u at a %u-bit slot width gives MCLK %llu Hz and BCLK "
                    "%llu Hz, and %llu does not divide %llu a whole number of times",
                    clock->mclk_multiple, clock->slot_bit_width,
                    (unsigned long long)chorus_i2s_mclk_hz(clock),
                    (unsigned long long)chorus_i2s_bclk_hz(clock),
                    (unsigned long long)chorus_i2s_bclk_hz(clock),
                    (unsigned long long)chorus_i2s_mclk_hz(clock));
    }

    if (clock->dma_desc_num < 2) {
        add_finding(findings, capacity, count, "dma-desc-num-too-small",
                    "i2s_dma_desc_num = %u leaves no descriptor to fill while another drains",
                    clock->dma_desc_num);
    }

    return *count - before;
}

typedef struct {
    const char *name;
    uint32_t pin;
} named_pin_t;

static void check_one_pin(const char *name, uint32_t pin, int octal_psram,
                          chorus_finding_t *findings, size_t capacity, size_t *count)
{
    if (pin > CHORUS_MAX_GPIO) {
        add_finding(findings, capacity, count, "gpio-out-of-range",
                    "%s = GPIO%u is beyond GPIO%d, which is the highest an ESP32-S3 has", name,
                    pin, CHORUS_MAX_GPIO);
        return;
    }
    if (pin >= 26 && pin <= 32) {
        add_finding(findings, capacity, count, "gpio-reserved-for-flash-and-psram",
                    "%s = GPIO%u is reserved. ESP-IDF (esp32s3 gpio reference): \"GPIO26 ~ "
                    "GPIO32 are usually used for SPI flash and PSRAM and not recommended for "
                    "other uses\"",
                    name, pin);
        return;
    }
    if (octal_psram && pin >= 33 && pin <= 37) {
        add_finding(findings, capacity, count, "gpio-reserved-for-octal-flash-or-psram",
                    "%s = GPIO%u is reserved on this board because board_octal_psram = yes. "
                    "ESP-IDF (esp32s3 gpio reference): \"When using Octal flash or Octal PSRAM "
                    "or both, GPIO33 ~ GPIO37 are connected to SPIIO4 ~ SPIIO7 and SPIDQS. "
                    "Therefore, on boards embedded with ESP32-S3R8 / ESP32-S3R8V chip, GPIO33 ~ "
                    "GPIO37 are also not recommended for other uses\"",
                    name, pin);
        return;
    }
    if (pin == 19 || pin == 20) {
        add_finding(findings, capacity, count, "gpio-reserved-for-usb-jtag",
                    "%s = GPIO%u is reserved. ESP-IDF (esp32s3 gpio reference): \"GPIO19 and "
                    "GPIO20 are used by USB-JTAG by default. If they are reconfigured to operate "
                    "as normal GPIOs, USB-JTAG functionality will be disabled\"",
                    name, pin);
        return;
    }
    if (pin == 0 || pin == 3 || pin == 45 || pin == 46) {
        add_finding(findings, capacity, count, "gpio-is-a-strapping-pin",
                    "%s = GPIO%u is a strapping pin. ESP-IDF (esp32s3 gpio reference): \"GPIO0, "
                    "GPIO3, GPIO45 and GPIO46 are strapping pins\"",
                    name, pin);
        return;
    }
}

size_t chorus_pin_map_validate(const chorus_pin_map_t *pins, chorus_finding_t *findings,
                               size_t capacity, size_t *count)
{
    size_t before = *count;

    const named_pin_t all[] = {
        {"pin_i2s_mclk", pins->mclk},
        {"pin_i2s_bclk", pins->bclk},
        {"pin_i2s_ws", pins->ws},
        {"pin_i2s_dout", pins->dout},
        {"pin_i2c_sda", pins->sda},
        {"pin_i2c_scl", pins->scl},
        {"pin_amp_power_down", pins->amp_power_down},
    };
    const size_t pin_count = sizeof(all) / sizeof(all[0]);

    for (size_t i = 0; i < pin_count; i++) {
        check_one_pin(all[i].name, all[i].pin, pins->octal_psram, findings, capacity, count);
    }

    /* Two signals on one pin is not a pin map, and it is the mistake a table
     * of numbers makes easiest to write. */
    for (size_t i = 0; i < pin_count; i++) {
        for (size_t j = i + 1; j < pin_count; j++) {
            if (all[i].pin == all[j].pin) {
                add_finding(findings, capacity, count, "gpio-assigned-twice",
                            "%s and %s are both GPIO%u", all[i].name, all[j].name, all[i].pin);
            }
        }
    }

    return *count - before;
}

size_t chorus_dma_validate_placement(chorus_mem_placement_t placement, chorus_finding_t *findings,
                                     size_t capacity, size_t *count)
{
    size_t before = *count;
    if (placement == CHORUS_MEM_EXTERNAL) {
        add_finding(findings, capacity, count, "dma-descriptor-in-external-ram",
                    "dma_descriptor_placement = external places a DMA transaction descriptor in "
                    "PSRAM. ESP-IDF (esp32s3 external RAM guide): \"DMA transaction descriptors "
                    "cannot be placed in PSRAM.\"");
    }
    return *count - before;
}
