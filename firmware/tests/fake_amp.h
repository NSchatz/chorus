/* A simulated TAS5825M, a simulated power-down line and a simulated I2S
 * controller, all writing into ONE event log.
 *
 * The log is appended by these fakes and NOT by the driver. That is the whole
 * design: AC-2 is about the order things happen at the pins, and a log the
 * driver wrote would be the driver's account of itself. Here the driver has no
 * way to reach the log at all, so what the test reads is what the hardware
 * saw.
 *
 * The simulated part is modelled on TI's datasheet (SLASEH7H, revision H) with
 * its OWN register numbers, below, and not the endpoint's configuration: the
 * test that drives it with the committed endpoint.conf is then a check that the
 * configuration and the datasheet agree, rather than the configuration agreeing
 * with itself. What it models, each with its page:
 *
 *   - PDN low is shutdown: the part does not answer I2C (p. 4, "PDN place the
 *     amplifier in Shutdown, turn off all internal regulators") and its
 *     registers return to their reset values when PDN rises;
 *   - an I2C transaction sooner than 5 ms after PDN rises is a violation of the
 *     startup procedure (p. 42) and is recorded as one;
 *   - DEVICE_CTRL2 (p. 48) resets to Deep Sleep with the DSP held (0x10); the
 *     output switches only in Play, with the DSP enabled, a clock running and
 *     no fault; POWER_STATE (p. 73) reports Hi-Z for Play without a clock
 *     (p. 29, a halted clock puts the part in Hi-Z);
 *   - Play sooner than 5 ms after Hi-Z with the DSP enabled is a violation
 *     (p. 42);
 *   - with PDN high and no clock, GLOBAL_FAULT1's clock-fault bit sets and
 *     latches until FAULT_CLEAR bit 7 is written (pp. 29, 78, 82); a test can
 *     turn that off, since the datasheet does not say whether it sets in Deep
 *     Sleep;
 *   - an injected fault either persists (a real short) or is latched and
 *     cleared by FAULT_CLEAR. */

#ifndef CHORUS_FAKE_AMP_H
#define CHORUS_FAKE_AMP_H

#include "chorus/amp.h"

#include <stddef.h>
#include <stdint.h>

/* The simulated part's map, from the datasheet (pages above). */
#define FAKE_TAS_ADDRESS 0x4C
#define FAKE_TAS_REG_PAGE 0x00
#define FAKE_TAS_REG_DEVICE_CTRL2 0x03
#define FAKE_TAS_REG_SAP_CTRL1 0x33
#define FAKE_TAS_REG_AGAIN 0x54
#define FAKE_TAS_REG_DIE_ID 0x67
#define FAKE_TAS_REG_POWER_STATE 0x68
#define FAKE_TAS_REG_CHAN_FAULT 0x70
#define FAKE_TAS_REG_GLOBAL_FAULT1 0x71
#define FAKE_TAS_REG_GLOBAL_FAULT2 0x72
#define FAKE_TAS_REG_FAULT_CLEAR 0x78
#define FAKE_TAS_REG_BOOK 0x7F
#define FAKE_TAS_DIE_ID 0x95
#define FAKE_TAS_CLK_FAULT 0x04

typedef enum {
    FAKE_EV_I2C_READ,
    FAKE_EV_I2C_WRITE,
    FAKE_EV_HIGH_IMPEDANCE,
    FAKE_EV_POWER_UP,
    FAKE_EV_WAIT,
    FAKE_EV_CLOCK_APPLIED,
    FAKE_EV_CLOCK_STOPPED
} fake_event_kind_t;

typedef struct {
    fake_event_kind_t kind;
    uint8_t address;
    uint8_t reg;
    uint8_t value;
    uint32_t ms;
    chorus_i2c_result_t result;
} fake_event_t;

/* Where the power-down line is. UNKNOWN is where a board powers up: it is NOT
 * driven low, which is why the sequencer has to drive it explicitly before the
 * first clock change. */
typedef enum {
    FAKE_PDN_UNKNOWN = 0,
    FAKE_PDN_LOW,
    FAKE_PDN_HIGH
} fake_pdn_t;

#define FAKE_MAX_EVENTS 96
#define FAKE_MAX_REGISTERS 256

typedef struct {
    fake_event_t events[FAKE_MAX_EVENTS];
    size_t event_count;
    fake_pdn_t pdn;

    /* The simulated part. */
    uint8_t address;
    uint8_t book;
    uint8_t page;
    uint8_t registers[FAKE_MAX_REGISTERS];
    int clock_running;
    /* Milliseconds of simulated time, advanced only by waits. */
    uint32_t now_ms;
    uint32_t powered_at_ms;
    uint32_t dsp_enabled_at_ms;
    int dsp_enabled;

    /* Injected conditions. */
    chorus_i2c_result_t answer; /* what every transaction answers with while powered */
    uint8_t die_id;             /* what DIE_ID reads; FAKE_TAS_DIE_ID for a TAS5825M */
    uint8_t
        persistent_fault[3]; /* CHAN_FAULT, GLOBAL_FAULT1, GLOBAL_FAULT2 bits a clear cannot drop */
    uint8_t latched_fault[3];      /* bits FAULT_CLEAR drops */
    int clock_fault_without_clock; /* default 1 */
    int refuses_play;              /* stays in Hi-Z when commanded to Play */
    int stage_refuses_high_impedance;
    int stage_refuses_power_up;
    int controller_refuses_clock;

    /* Recorded so a test can assert on the pins rather than the order. */
    int clock_changed_while_switching;
    size_t clock_changes;
    int i2c_before_power_up_wait;
    int play_before_dsp_settled;
    int i2c_while_pdn_low;
} fake_amp_t;

void fake_amp_init(fake_amp_t *fake);

chorus_i2c_bus_t fake_amp_bus(fake_amp_t *fake);
chorus_output_stage_t fake_amp_stage(fake_amp_t *fake);
chorus_i2s_controller_t fake_amp_controller(fake_amp_t *fake);

/* Whether the output stage is switching: PDN high, Play, the DSP enabled, a
 * clock running and no fault. */
int fake_amp_switching(const fake_amp_t *fake);

/* Index of the first event of `kind`, or -1. */
int fake_amp_first(const fake_amp_t *fake, fake_event_kind_t kind);
/* Index of the first read of `reg`, or -1. */
int fake_amp_first_read(const fake_amp_t *fake, uint8_t reg);
/* Index of the first write to `reg`, or -1; and of the first write of `value`. */
int fake_amp_first_write(const fake_amp_t *fake, uint8_t reg);
int fake_amp_first_write_of(const fake_amp_t *fake, uint8_t reg, uint8_t value);
/* How many events of `kind` happened. */
size_t fake_amp_count(const fake_amp_t *fake, fake_event_kind_t kind);
/* How many I2C transactions of any sort happened. */
size_t fake_amp_i2c_transactions(const fake_amp_t *fake);

const char *fake_event_kind_name(fake_event_kind_t kind);
void fake_amp_print(const fake_amp_t *fake);

#endif /* CHORUS_FAKE_AMP_H */
