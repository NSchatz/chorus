/* The amplifier bring-up sequencer.
 *
 * # Why this file names no register address
 *
 * The part is a TAS5825M-class I2S amplifier. TI's datasheet is normative for
 * its register map and the research pass could not extract text from the PDF,
 * so the roadmap phase "asserts behaviour and names no address" and so does
 * this. Every register id, every device-id value and every fault-clear value
 * this sequencer uses arrives from `firmware/config/endpoint.conf`, where they
 * are DECLARED UNKNOWN until somebody reads them off the datasheet at
 * bring-up. There is not one register literal in the endpoint tree, and
 * firmware/check/endpoint-scan.c fails the suite if one appears.
 *
 * A register whose id reads `unknown` is not defaulted to anything. The
 * sequencer refuses, names the key, and leaves the output stage in high
 * impedance, which is the only safe thing to do with an amplifier whose
 * control surface you cannot address.
 *
 * # Why the order is the assertion
 *
 * An amplifier drives a real loudspeaker. A clock change made while the output
 * stage is live, or a gain written past what the driver takes, breaks a
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

/* The amplifier's output stage, driven through its power-down / mute line.
 *
 * This is deliberately NOT a register write: driving a pin needs no address,
 * so the endpoint can reach the one safe state before it knows anything about
 * the part. Returns 0 on success. */
typedef struct {
    void *ctx;
    int (*high_impedance)(void *ctx);
    int (*enable)(void *ctx);
} chorus_output_stage_t;

/* The I2S controller, injectable for the same reason. Returns 0 on success. */
typedef struct {
    void *ctx;
    int (*apply_clock)(void *ctx, const chorus_i2s_clock_t *clock);
    int (*stop_clock)(void *ctx);
} chorus_i2s_controller_t;

/* Everything the sequencer needs, all of it from committed configuration. An
 * `*_known` of 0 means the configuration declares that value `unknown`. */
typedef struct {
    int address_known;
    uint8_t address;
    int reg_device_id_known;
    uint8_t reg_device_id;
    int reg_fault_known;
    uint8_t reg_fault;
    int reg_analog_gain_known;
    uint8_t reg_analog_gain;
    int reg_state_control_known;
    uint8_t reg_state_control;
    int device_id_value_known;
    uint8_t device_id_value;
    int fault_clear_value_known;
    uint8_t fault_clear_value;

    /* The ceiling, in dB above the part's lowest analog gain setting, and the
     * file it is declared in. The file is carried so a refusal can name it,
     * which AC-14 asks for by name. */
    double analog_gain_ceiling_db;
    char ceiling_source[128];
} chorus_amp_config_t;

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
    CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED
} chorus_amp_status_t;

const char *chorus_amp_status_name(chorus_amp_status_t status);

#define CHORUS_AMP_DETAIL 512

typedef struct {
    chorus_amp_status_t status;
    char detail[CHORUS_AMP_DETAIL];
    /* The fault register's value, when one was read. */
    int fault_read;
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
 * The order, and the reason for each step:
 *
 *   1. high impedance, before anything else. It needs no register, so it is
 *      reachable before the part is known to be there at all.
 *   2. refuse a requested gain above the configured ceiling. Nothing has
 *      touched the bus yet, so a refusal here is provably before any write.
 *   3. refuse a register the configuration declares unknown, by name.
 *   4. read the device id over I2C. A NACK, a timeout or a bus error ends it.
 *   5. read the fault register. Anything but the configured clear value ends
 *      it.
 *   6. write the analog gain, which is now known to be inside the ceiling.
 *   7. apply the I2S clock. The stage is in high impedance and has been since
 *      step 1, so this and every later clock change is made into a dead output.
 *   8. enable the output.
 *
 * Steps 4 and 5 are both before step 8, which is the first half of AC-2; step
 * 1 is before step 7, which is the second half, including the first clock
 * change. */
chorus_amp_status_t chorus_amp_bring_up(const chorus_amp_config_t *config,
                                        const chorus_amp_gain_t *gain,
                                        const chorus_i2s_clock_t *clock, chorus_i2c_bus_t *bus,
                                        chorus_output_stage_t *stage,
                                        chorus_i2s_controller_t *controller,
                                        chorus_amp_report_t *report);

/* Read the fault register of a brought-up part.
 *
 * On a fault this stops the audio: the output stage goes to high impedance and
 * the I2S clock is stopped, and the caller is handed a status with a name to
 * publish. Continuing to play into a faulted amplifier is what damages a
 * driver, and docs/decisions/0015 records the choice. */
chorus_amp_status_t chorus_amp_poll_fault(const chorus_amp_config_t *config,
                                          chorus_i2c_bus_t *bus, chorus_output_stage_t *stage,
                                          chorus_i2s_controller_t *controller,
                                          chorus_amp_report_t *report);

#endif /* CHORUS_AMP_H */
