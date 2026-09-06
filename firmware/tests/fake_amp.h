/* A simulated TAS5825M-class part, a simulated output stage and a simulated
 * I2S controller, all writing into ONE event log.
 *
 * The log is appended by these fakes and NOT by the driver. That is the whole
 * design: AC-2 is about the order things happen at the pins, and a log the
 * driver wrote would be the driver's account of itself. Here the driver has no
 * way to reach the log at all, so what the test reads is what the hardware
 * saw.
 *
 * The simulated part's register numbering comes from the TEST and not from the
 * endpoint's configuration or its source. This phase names no register address
 * and the fake's numbers are a stand-in for the datasheet a bench will have,
 * not a claim about the real part. */

#ifndef CHORUS_FAKE_AMP_H
#define CHORUS_FAKE_AMP_H

#include "chorus/amp.h"

#include <stddef.h>
#include <stdint.h>

typedef enum {
    FAKE_EV_I2C_READ,
    FAKE_EV_I2C_WRITE,
    FAKE_EV_HIGH_IMPEDANCE,
    FAKE_EV_OUTPUT_ENABLE,
    FAKE_EV_CLOCK_APPLIED,
    FAKE_EV_CLOCK_STOPPED
} fake_event_kind_t;

typedef struct {
    fake_event_kind_t kind;
    uint8_t address;
    uint8_t reg;
    uint8_t value;
    chorus_i2c_result_t result;
} fake_event_t;

/* What the output stage is, as the hardware sees it. UNKNOWN is where a part
 * powers up: it is NOT high impedance, which is why the sequencer has to
 * command that state explicitly before the first clock change. */
typedef enum {
    FAKE_STAGE_UNKNOWN = 0,
    FAKE_STAGE_HIGH_IMPEDANCE,
    FAKE_STAGE_ENABLED
} fake_stage_state_t;

#define FAKE_MAX_EVENTS 64
#define FAKE_MAX_REGISTERS 256

typedef struct {
    fake_event_t events[FAKE_MAX_EVENTS];
    size_t event_count;
    fake_stage_state_t stage;

    /* The simulated part. */
    uint8_t address;
    uint8_t registers[FAKE_MAX_REGISTERS];
    /* What every transaction answers with. ACK is a healthy bus. */
    chorus_i2c_result_t answer;
    /* Set to make the stage or the controller refuse. */
    int stage_refuses_high_impedance;
    int stage_refuses_enable;
    int controller_refuses_clock;

    /* Recorded so a test can assert a clock change never happened into a live
     * stage, rather than inferring it from the order. */
    int clock_changed_while_not_high_impedance;
    size_t clock_changes;
} fake_amp_t;

void fake_amp_init(fake_amp_t *fake, uint8_t address);

chorus_i2c_bus_t fake_amp_bus(fake_amp_t *fake);
chorus_output_stage_t fake_amp_stage(fake_amp_t *fake);
chorus_i2s_controller_t fake_amp_controller(fake_amp_t *fake);

/* Index of the first event of `kind`, or -1. */
int fake_amp_first(const fake_amp_t *fake, fake_event_kind_t kind);
/* Index of the first read of `reg`, or -1. */
int fake_amp_first_read(const fake_amp_t *fake, uint8_t reg);
/* Index of the first write to `reg`, or -1. */
int fake_amp_first_write(const fake_amp_t *fake, uint8_t reg);
/* How many events of `kind` happened. */
size_t fake_amp_count(const fake_amp_t *fake, fake_event_kind_t kind);
/* How many I2C transactions of any sort happened. */
size_t fake_amp_i2c_transactions(const fake_amp_t *fake);

const char *fake_event_kind_name(fake_event_kind_t kind);
void fake_amp_print(const fake_amp_t *fake);

#endif /* CHORUS_FAKE_AMP_H */
