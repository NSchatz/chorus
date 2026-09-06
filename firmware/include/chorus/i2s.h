/* I2S clocking, the pin map, and where a DMA descriptor may live.
 *
 * Every rule in here is a quotation from a source carried in this repository's
 * spec folder and quoted in docs/decisions/0015-the-esp32-s3-endpoint.md, not
 * a convention. Each one names itself when it fires, because "the build
 * refused" is only useful if it says which value and which rule.
 *
 * These are checked BEFORE anything is built and again before a clock is
 * started, which is what makes AC-11's "refuse rather than build" true rather
 * than aspirational: firmware/Makefile runs chorus-endpoint-config-check
 * against firmware/config/endpoint.conf as a prerequisite of every object it
 * compiles. */

#ifndef CHORUS_I2S_H
#define CHORUS_I2S_H

#include <stddef.h>
#include <stdint.h>

/* Highest GPIO number an ESP32-S3 has. */
#define CHORUS_MAX_GPIO 48

/* Longest rule text a finding carries. */
#define CHORUS_FINDING_TEXT 320

/* Where a buffer lives. The carried ESP-IDF external-RAM guide is flat about
 * which of these a DMA transaction descriptor may be in. */
typedef enum {
    CHORUS_MEM_INTERNAL = 0,
    CHORUS_MEM_EXTERNAL
} chorus_mem_placement_t;

const char *chorus_mem_placement_name(chorus_mem_placement_t placement);
/* CHORUS_MEM_INTERNAL for a name the format does not define; `ok` says which. */
chorus_mem_placement_t chorus_mem_placement_from_name(const char *name, int *ok);

/* One reason a configuration is refused: the rule that fired, the value that
 * broke it, and the rule's own text. */
typedef struct {
    char rule[64];
    char detail[CHORUS_FINDING_TEXT];
} chorus_finding_t;

typedef struct {
    uint32_t sample_rate_hz;
    uint32_t slot_bit_width;
    uint32_t mclk_multiple;
    uint32_t dma_frame_num;
    uint32_t dma_desc_num;
} chorus_i2s_clock_t;

typedef struct {
    uint32_t mclk;
    uint32_t bclk;
    uint32_t ws;
    uint32_t dout;
    uint32_t sda;
    uint32_t scl;
    uint32_t amp_power_down;
    /* Whether the board carries octal flash or octal PSRAM, which extends the
     * reserved range. */
    int octal_psram;
} chorus_pin_map_t;

/* Append findings for every rule `clock` breaks. Returns how many were
 * appended; `count` is advanced. */
size_t chorus_i2s_validate_clock(const chorus_i2s_clock_t *clock, chorus_finding_t *findings,
                                 size_t capacity, size_t *count);

/* Append findings for every rule `pins` breaks. */
size_t chorus_pin_map_validate(const chorus_pin_map_t *pins, chorus_finding_t *findings,
                               size_t capacity, size_t *count);

/* Append a finding when a DMA transaction descriptor is placed in external
 * RAM. The platform forbids it and a descriptor that lives there fails at run
 * time on a wall, which is exactly the failure a host cannot observe and a
 * check has to. */
size_t chorus_dma_validate_placement(chorus_mem_placement_t placement, chorus_finding_t *findings,
                                     size_t capacity, size_t *count);

/* The bit clock this configuration produces, in Hz, for a two-slot (stereo)
 * frame. */
uint64_t chorus_i2s_bclk_hz(const chorus_i2s_clock_t *clock);

/* The master clock this configuration produces, in Hz. */
uint64_t chorus_i2s_mclk_hz(const chorus_i2s_clock_t *clock);

/* Whether MCLK divides into BCLK a whole number of times, which is the
 * property the 24-bit rule exists to preserve. */
int chorus_i2s_bclk_division_is_integral(const chorus_i2s_clock_t *clock);

#endif /* CHORUS_I2S_H */
