/* The amplifier bring-up sequencer.
 *
 * # Where the register map lives
 *
 * The part is TI's TAS5825M. Every register id, every value written to it and
 * every value read back is a datasheet fact, and every one of them arrives from
 * `firmware/config/endpoint.conf`, where each line cites the page of TI's
 * datasheet it was read from (SLASEH7H, revision H, January 2023; the reading is
 * docs/research/tas5825m-register-map.md). There is not one register literal in
 * the endpoint tree, and firmware/check/endpoint_scan.c fails the suite if one
 * appears.
 *
 * A value the configuration declares `unknown` is not defaulted to anything.
 * The sequencer refuses, names the key, and leaves the output stage dead, which
 * is the only safe thing to do with an amplifier whose control surface you
 * cannot address.
 *
 * # Why the order is the assertion
 *
 * An amplifier drives a real loudspeaker. A clock change made while the output
 * stage is switching, or a gain written past what the driver takes, breaks a
 * speaker rather than failing a test. So the order below is checked by a test
 * that watches the SIMULATED PART and the SIMULATED CONTROLLER rather than the
 * driver's own narration: the event log is appended by the fakes, at the pins,
 * and the driver has no way to write to it. */

#ifndef CHORUS_AMP_H
#define CHORUS_AMP_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/i2s.h"

/* What a transaction on the I2C bus did. */
typedef enum {
    CHORUS_I2C_ACK = 0,
    /* The part did not acknowledge its address. Nothing is there, or it is not
     * powered. */
    CHORUS_I2C_NACK,
    /* The bus did not complete in time. */
    CHORUS_I2C_TIMEOUT,
    /* Arbitration lost, stuck line, or any other bus-level failure. */
    CHORUS_I2C_BUS_ERROR
} chorus_i2c_result_t;

const char *chorus_i2c_result_name(chorus_i2c_result_t result);

/* The I2C bus, injectable so that the order of transactions is observable on a
 * host with no amplifier attached. */
typedef struct {
    void *ctx;
    chorus_i2c_result_t (*read)(void *ctx, uint8_t address, uint8_t reg, uint8_t *value);
    chorus_i2c_result_t (*write)(void *ctx, uint8_t address, uint8_t reg, uint8_t value);
} chorus_i2c_bus_t;

/* The amplifier's power-down line, and the waits the datasheet puts around it.
 *
 * `high_impedance` drives PDN low. That needs no address, so the endpoint can
 * reach the one safe state before it knows anything about the part: with PDN
 * low the TAS5825M is in shutdown with its internal regulators off (datasheet
 * p. 4, the PDN pin) and its outputs are not driven. It also does not answer
 * I2C in that state, which is why `power_up` exists.
 *
 * `power_up` drives PDN high. The part comes out of shutdown in Deep Sleep
 * (DEVICE_CTRL2's reset state, p. 48), with its output stage still not
 * switching; it switches only once it is commanded to Play over I2C.
 *
 * `wait_ms` waits AT LEAST the given number of milliseconds. The datasheet's
 * startup and shutdown procedures (pp. 42-43) are written in minimum waits, and
 * a wait that rounded down would break them.
 *
 * Each returns 0 on success. */
typedef struct {
    void *ctx;
    int (*high_impedance)(void *ctx);
    int (*power_up)(void *ctx);
    void (*wait_ms)(void *ctx, uint32_t ms);
} chorus_output_stage_t;

/* The I2S controller, injectable for the same reason. Returns 0 on success. */
typedef struct {
    void *ctx;
    int (*apply_clock)(void *ctx, const chorus_i2s_clock_t *clock);
    int (*stop_clock)(void *ctx);
} chorus_i2s_controller_t;

/* One byte of the register map: a register id or a value. `known` is 0 when
 * the configuration declares it `unknown`. */
typedef struct {
    int known;
    uint8_t value;
} chorus_amp_byte_t;

/* The fault registers the sequencer reads, in the order it reads them. */
enum {
    CHORUS_AMP_FAULT_REGISTERS = 3
};

/* Everything the sequencer needs, all of it from committed configuration; each
 * field is the endpoint.conf key named beside it. */
typedef struct {
    chorus_amp_byte_t address;         /* amp_i2c_address */
    chorus_amp_byte_t reg_page_select; /* amp_reg_page_select */
    chorus_amp_byte_t reg_book_select; /* amp_reg_book_select */
    chorus_amp_byte_t page_book_zero;  /* amp_page_book_zero */
    chorus_amp_byte_t reg_device_id;   /* amp_reg_device_id */
    chorus_amp_byte_t device_id_value; /* amp_device_id_value */
    /* amp_reg_fault_channel, amp_reg_fault_global1, amp_reg_fault_global2 */
    chorus_amp_byte_t reg_fault[CHORUS_AMP_FAULT_REGISTERS];
    chorus_amp_byte_t fault_clear_value;   /* amp_fault_clear_value */
    chorus_amp_byte_t clock_fault_bit;     /* amp_clock_fault_bit, in amp_reg_fault_global1 */
    chorus_amp_byte_t reg_fault_clear;     /* amp_reg_fault_clear */
    chorus_amp_byte_t fault_clear_command; /* amp_fault_clear_command */
    chorus_amp_byte_t reg_analog_gain;     /* amp_reg_analog_gain */
    chorus_amp_byte_t reg_audio_format;    /* amp_reg_audio_format */
    chorus_amp_byte_t audio_format_value;  /* amp_audio_format_value */
    chorus_amp_byte_t reg_state_control;   /* amp_reg_state_control */
    chorus_amp_byte_t state_hiz;           /* amp_state_hiz */
    chorus_amp_byte_t state_play;          /* amp_state_play */
    chorus_amp_byte_t reg_power_state;     /* amp_reg_power_state */
    chorus_amp_byte_t power_state_play;    /* amp_power_state_play */

    /* The datasheet's minimum waits, in milliseconds (pp. 42-43). */
    uint32_t power_up_wait_ms;   /* amp_power_up_wait_ms */
    uint32_t dsp_settle_wait_ms; /* amp_dsp_settle_wait_ms */
    uint32_t shutdown_wait_ms;   /* amp_shutdown_wait_ms */

    /* The serial port's bit clock per frame the part is committed to, one of
     * the datasheet's supported ratios (pp. 7, 29): amp_sclk_per_frame. */
    uint32_t sclk_per_frame;

    /* The ceiling, in dB above the part's lowest analog gain setting, and the
     * file it is declared in. The file is carried so a refusal can name it,
     * which AC-14 asks for by name. */
    double analog_gain_ceiling_db;
    char ceiling_source[128];
} chorus_amp_config_t;

/* Every register-map byte of `config` with its endpoint.conf key, in the order
 * a refusal checks them. Returns how many were written to `out` (at most
 * `capacity`); the endpoint's loader and the refusal use the same table. */
typedef struct {
    const char *key;
    const chorus_amp_byte_t *byte;
} chorus_amp_key_t;

size_t chorus_amp_keys(const chorus_amp_config_t *config, chorus_amp_key_t *out, size_t capacity);

/* What the endpoint asks for. `code` is the register value that produces `db`
 * on this part, which is a datasheet fact and therefore configuration: the
 * sequencer never derives one from the other. */
typedef struct {
    double db;
    int code_known;
    uint8_t code;
} chorus_amp_gain_t;

/* How bring-up ended. Every one of these has a stable name, because
 * "surface it in telemetry rather than continue playing silently" is only
 * satisfied by a name a reader can act on. */
typedef enum {
    CHORUS_AMP_OK = 0,
    CHORUS_AMP_REGISTER_NOT_CONFIGURED,
    CHORUS_AMP_GAIN_ABOVE_CEILING,
    CHORUS_AMP_DID_NOT_ANSWER,
    CHORUS_AMP_I2C_TIMEOUT,
    CHORUS_AMP_I2C_BUS_ERROR,
    CHORUS_AMP_IDENTITY_MISMATCH,
    CHORUS_AMP_REPORTS_FAULT,
    CHORUS_AMP_OUTPUT_STAGE_REFUSED,
    CHORUS_AMP_CLOCK_REFUSED,
    CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED,
    CHORUS_AMP_DID_NOT_REACH_PLAY
} chorus_amp_status_t;

const char *chorus_amp_status_name(chorus_amp_status_t status);

#define CHORUS_AMP_DETAIL 512

typedef struct {
    chorus_amp_status_t status;
    char detail[CHORUS_AMP_DETAIL];
    /* The fault registers were read, and the first one that did not read clear
     * (or the last one read, when all were clear): its id and its value. */
    int fault_read;
    uint8_t fault_register;
    uint8_t fault_bits;
    /* Whether the output stage was left in high impedance. Every non-OK path
     * leaves this 1. */
    int output_in_high_impedance;
    /* Whether an I2S clock was started at all. */
    int clock_started;
    /* Findings from the clock configuration, when it was the clock that was
     * refused. */
    chorus_finding_t findings[8];
    size_t finding_count;
} chorus_amp_report_t;

/* Bring the amplifier up, or refuse and say why.
 *
 * The datasheet's startup procedure (TAS5825M datasheet SLASEH7H, section
 * 9.5.3.1, p. 42) is: power, then PDN high and wait at least 5 ms, then start
 * SCLK and LRCLK; once they are stable, set the device to Hi-Z with the DSP
 * enabled over I2C; wait at least 5 ms; then Play. The sequence here is that
 * procedure with the endpoint's checks placed inside it:
 *
 *   1. PDN low: the output stage is dead before anything else. It needs no
 *      register, so it is reachable before the part is known to be there.
 *   2. refuse a requested gain above the configured ceiling, a register-map
 *      value the configuration declares unknown (by name), and a clock
 *      configuration that breaks a platform rule or a ratio the part does not
 *      accept. Nothing has touched the bus yet.
 *   3. PDN high, and the datasheet's power-up wait. The part is in Deep Sleep:
 *      it answers I2C and its output does not switch.
 *   4. book 0, page 0 (p. 41), then the device id (p. 72). A NACK, a timeout,
 *      a bus error or another part ends it.
 *   5. the fault registers (pp. 77-79). Anything but clear ends it, except the
 *      clock-fault bit, which a part with no clock yet may report (p. 29).
 *   6. the analog gain, now known to be inside the ceiling (p. 63), and the
 *      serial audio format (p. 53).
 *   7. the I2S clock, into a part still in Deep Sleep.
 *   8. clear the latched faults (p. 82), then Hi-Z with the DSP enabled (pp.
 *      42, 48), then the datasheet's settle wait.
 *   9. the fault registers again, with no bit excused. A fault ends it.
 *  10. Play, and the power-state register read back (p. 73). A part that did
 *      not reach Play ends it.
 *
 * Every refusal before step 7 starts no clock and leaves PDN low. A refusal at
 * step 9 or 10, which the datasheet's order puts after the clock, drives PDN
 * low FIRST and stops the clock after it, so no clock change is ever made into
 * a switching output. */
chorus_amp_status_t chorus_amp_bring_up(const chorus_amp_config_t *config,
                                        const chorus_amp_gain_t *gain,
                                        const chorus_i2s_clock_t *clock, chorus_i2c_bus_t *bus,
                                        chorus_output_stage_t *stage,
                                        chorus_i2s_controller_t *controller,
                                        chorus_amp_report_t *report);

/* Read the fault registers of a playing part.
 *
 * On a fault this stops the audio, in that order and not the other one: PDN
 * goes low FIRST and the I2S clock is stopped after it, because stopping a
 * clock is a clock change and no clock change is made into a switching output.
 * The caller is handed a status with a name to publish. Continuing to play into
 * a faulted amplifier is what damages a driver, and docs/decisions/0015
 * records the choice. */
chorus_amp_status_t chorus_amp_poll_fault(const chorus_amp_config_t *config, chorus_i2c_bus_t *bus,
                                          chorus_output_stage_t *stage,
                                          chorus_i2s_controller_t *controller,
                                          chorus_amp_report_t *report);

/* Stop a playing part the datasheet's way (section 9.5.3.2, p. 43): Hi-Z over
 * I2C so the digital volume ramps down, the shutdown wait, then PDN low, then
 * the clock. PDN goes low even when the I2C write fails. Returns 0 when every
 * step succeeded. */
int chorus_amp_shut_down(const chorus_amp_config_t *config, chorus_i2c_bus_t *bus,
                         chorus_output_stage_t *stage, chorus_i2s_controller_t *controller);

#endif /* CHORUS_AMP_H */
