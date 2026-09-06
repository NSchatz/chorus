#include "chorus/amp.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>

const char *chorus_i2c_result_name(chorus_i2c_result_t result)
{
    switch (result) {
    case CHORUS_I2C_ACK:
        return "ack";
    case CHORUS_I2C_NACK:
        return "nack";
    case CHORUS_I2C_TIMEOUT:
        return "timeout";
    case CHORUS_I2C_BUS_ERROR:
        return "bus-error";
    }
    return "unknown-result";
}

const char *chorus_amp_status_name(chorus_amp_status_t status)
{
    switch (status) {
    case CHORUS_AMP_OK:
        return "ok";
    case CHORUS_AMP_REGISTER_NOT_CONFIGURED:
        return "amplifier-register-not-configured";
    case CHORUS_AMP_GAIN_ABOVE_CEILING:
        return "analog-gain-above-ceiling";
    case CHORUS_AMP_DID_NOT_ANSWER:
        return "amplifier-did-not-answer";
    case CHORUS_AMP_I2C_TIMEOUT:
        return "amplifier-i2c-timeout";
    case CHORUS_AMP_I2C_BUS_ERROR:
        return "amplifier-i2c-bus-error";
    case CHORUS_AMP_IDENTITY_MISMATCH:
        return "amplifier-identity-mismatch";
    case CHORUS_AMP_REPORTS_FAULT:
        return "amplifier-reports-fault";
    case CHORUS_AMP_OUTPUT_STAGE_REFUSED:
        return "output-stage-refused";
    case CHORUS_AMP_CLOCK_REFUSED:
        return "i2s-clock-refused";
    case CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED:
        return "i2s-clock-configuration-refused";
    }
    return "unknown-status";
}

static void report_detail(chorus_amp_report_t *report, const char *fmt, ...)
{
    va_list args;
    va_start(args, fmt);
    vsnprintf(report->detail, sizeof(report->detail), fmt, args);
    va_end(args);
}

/* Put the stage in high impedance and record that it is there. A stage that
 * refuses to go dead is the one condition that is worse than every other
 * condition here, so it is its own status. */
static int go_high_impedance(chorus_output_stage_t *stage, chorus_amp_report_t *report)
{
    if (stage->high_impedance(stage->ctx) != 0) {
        report->output_in_high_impedance = 0;
        report->status = CHORUS_AMP_OUTPUT_STAGE_REFUSED;
        report_detail(report,
                      "the output stage did not go to high impedance; nothing further is safe");
        return -1;
    }
    report->output_in_high_impedance = 1;
    return 0;
}

static chorus_amp_status_t status_for_i2c(chorus_i2c_result_t result)
{
    switch (result) {
    case CHORUS_I2C_NACK:
        return CHORUS_AMP_DID_NOT_ANSWER;
    case CHORUS_I2C_TIMEOUT:
        return CHORUS_AMP_I2C_TIMEOUT;
    case CHORUS_I2C_BUS_ERROR:
        return CHORUS_AMP_I2C_BUS_ERROR;
    case CHORUS_I2C_ACK:
        return CHORUS_AMP_OK;
    }
    return CHORUS_AMP_I2C_BUS_ERROR;
}

/* Every configuration value this sequencer must have before it may touch the
 * bus, with the key each one is written under, so a refusal names the line of
 * endpoint.conf a reader has to fill in. */
static chorus_amp_status_t require_configured(const chorus_amp_config_t *config,
                                              chorus_amp_report_t *report)
{
    const struct {
        int known;
        const char *key;
    } required[] = {
        {config->address_known, "amp_i2c_address"},
        {config->reg_device_id_known, "amp_reg_device_id"},
        {config->reg_fault_known, "amp_reg_fault"},
        {config->reg_analog_gain_known, "amp_reg_analog_gain"},
        {config->reg_state_control_known, "amp_reg_state_control"},
        {config->device_id_value_known, "amp_device_id_value"},
        {config->fault_clear_value_known, "amp_fault_clear_value"},
    };
    for (size_t i = 0; i < sizeof(required) / sizeof(required[0]); i++) {
        if (!required[i].known) {
            report_detail(report,
                          "%s is declared unknown in %s. This phase names no register address: "
                          "read it off the TAS5825M datasheet at bring-up and write it there. "
                          "The output stage stays in high impedance and no I2S clock is started.",
                          required[i].key, config->ceiling_source);
            return CHORUS_AMP_REGISTER_NOT_CONFIGURED;
        }
    }
    return CHORUS_AMP_OK;
}

chorus_amp_status_t chorus_amp_bring_up(const chorus_amp_config_t *config,
                                        const chorus_amp_gain_t *gain,
                                        const chorus_i2s_clock_t *clock, chorus_i2c_bus_t *bus,
                                        chorus_output_stage_t *stage,
                                        chorus_i2s_controller_t *controller,
                                        chorus_amp_report_t *report)
{
    memset(report, 0, sizeof(*report));
    report->status = CHORUS_AMP_OK;

    /* 1. High impedance, before anything else. */
    if (go_high_impedance(stage, report) != 0) {
        return report->status;
    }

    /* 2. The gain ceiling. Checked before the bus is touched, so a refusal
     *    here is provably before any register write and before any clock. */
    if (gain->db > config->analog_gain_ceiling_db) {
        report_detail(report,
                      "a requested analog gain of %.3f dB is above the ceiling of %.3f dB "
                      "declared in %s. Output is NOT enabled, the output stage stays in high "
                      "impedance, and no I2S clock is started.",
                      gain->db, config->analog_gain_ceiling_db, config->ceiling_source);
        report->status = CHORUS_AMP_GAIN_ABOVE_CEILING;
        return report->status;
    }

    /* 3. Registers this phase declares unknown. */
    chorus_amp_status_t configured = require_configured(config, report);
    if (configured != CHORUS_AMP_OK) {
        report->status = configured;
        return report->status;
    }
    if (!gain->code_known) {
        report_detail(report,
                      "amp_analog_gain_code is declared unknown in %s, so the register value that "
                      "produces %.3f dB on this part is not known. This phase names no register "
                      "value: read it off the TAS5825M datasheet at bring-up. The output stage "
                      "stays in high impedance and no I2S clock is started.",
                      config->ceiling_source, gain->db);
        report->status = CHORUS_AMP_REGISTER_NOT_CONFIGURED;
        return report->status;
    }

    /* Refuse a clock configuration that breaks a platform rule before it can
     * reach a pin, rather than after. */
    if (chorus_i2s_validate_clock(clock, report->findings,
                                  sizeof(report->findings) / sizeof(report->findings[0]),
                                  &report->finding_count) > 0) {
        report_detail(report,
                      "the I2S clock configuration is refused (%s). No clock is started and the "
                      "output stage stays in high impedance.",
                      report->findings[0].rule);
        report->status = CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED;
        return report->status;
    }

    /* 4. Does the part answer, and is it the part we think it is? */
    uint8_t device_id = 0;
    chorus_i2c_result_t result = bus->read(bus->ctx, config->address, config->reg_device_id,
                                           &device_id);
    if (result != CHORUS_I2C_ACK) {
        report_detail(report,
                      "the amplifier at I2C address 0x%02x answered %s to the device-id read. "
                      "The output stage stays in high impedance and no I2S clock is started.",
                      config->address, chorus_i2c_result_name(result));
        report->status = status_for_i2c(result);
        return report->status;
    }
    if (device_id != config->device_id_value) {
        report_detail(report,
                      "the part at I2C address 0x%02x reported device id 0x%02x and %s declares "
                      "0x%02x. The output stage stays in high impedance and no I2S clock is "
                      "started.",
                      config->address, device_id, config->ceiling_source, config->device_id_value);
        report->status = CHORUS_AMP_IDENTITY_MISMATCH;
        return report->status;
    }

    /* 5. Does it report a fault? */
    uint8_t fault = 0;
    result = bus->read(bus->ctx, config->address, config->reg_fault, &fault);
    if (result != CHORUS_I2C_ACK) {
        report_detail(report,
                      "the amplifier at I2C address 0x%02x answered %s to the fault read. The "
                      "output stage stays in high impedance and no I2S clock is started.",
                      config->address, chorus_i2c_result_name(result));
        report->status = status_for_i2c(result);
        return report->status;
    }
    report->fault_read = 1;
    report->fault_bits = fault;
    if (fault != config->fault_clear_value) {
        report_detail(report,
                      "the amplifier at I2C address 0x%02x reports a fault: its fault register "
                      "reads 0x%02x and a part with no fault reads 0x%02x. The output stage "
                      "stays in high impedance and no I2S clock is started.",
                      config->address, fault, config->fault_clear_value);
        report->status = CHORUS_AMP_REPORTS_FAULT;
        return report->status;
    }

    /* 6. The gain, now known to be inside the ceiling. */
    result = bus->write(bus->ctx, config->address, config->reg_analog_gain, gain->code);
    if (result != CHORUS_I2C_ACK) {
        report_detail(report,
                      "the amplifier at I2C address 0x%02x answered %s to the analog-gain write. "
                      "The output stage stays in high impedance and no I2S clock is started.",
                      config->address, chorus_i2c_result_name(result));
        report->status = status_for_i2c(result);
        return report->status;
    }

    /* 7. The clock, into a stage that has been dead since step 1. */
    if (controller->apply_clock(controller->ctx, clock) != 0) {
        report_detail(report, "the I2S controller refused the clock configuration");
        report->status = CHORUS_AMP_CLOCK_REFUSED;
        return report->status;
    }
    report->clock_started = 1;

    /* 8. Output. */
    if (stage->enable(stage->ctx) != 0) {
        (void)go_high_impedance(stage, report);
        (void)controller->stop_clock(controller->ctx);
        report->clock_started = 0;
        report_detail(report, "the output stage refused to enable");
        report->status = CHORUS_AMP_OUTPUT_STAGE_REFUSED;
        return report->status;
    }
    report->output_in_high_impedance = 0;

    report_detail(report,
                  "the amplifier at I2C address 0x%02x answered, reported no fault, took %.3f dB "
                  "of analog gain against a ceiling of %.3f dB, and its output was enabled after "
                  "the clock was applied into a high-impedance stage",
                  config->address, gain->db, config->analog_gain_ceiling_db);
    report->status = CHORUS_AMP_OK;
    return report->status;
}

chorus_amp_status_t chorus_amp_poll_fault(const chorus_amp_config_t *config,
                                          chorus_i2c_bus_t *bus, chorus_output_stage_t *stage,
                                          chorus_i2s_controller_t *controller,
                                          chorus_amp_report_t *report)
{
    memset(report, 0, sizeof(*report));
    report->status = CHORUS_AMP_OK;
    report->clock_started = 1;

    if (!config->reg_fault_known || !config->fault_clear_value_known || !config->address_known) {
        chorus_amp_status_t configured = require_configured(config, report);
        report->status = configured;
        (void)go_high_impedance(stage, report);
        (void)controller->stop_clock(controller->ctx);
        report->clock_started = 0;
        return report->status;
    }

    uint8_t fault = 0;
    chorus_i2c_result_t result = bus->read(bus->ctx, config->address, config->reg_fault, &fault);
    if (result != CHORUS_I2C_ACK) {
        /* An amplifier that has stopped answering during playback is not a
         * healthy amplifier. It is surfaced by name and the audio stops, for
         * the same reason a reported fault does. */
        (void)go_high_impedance(stage, report);
        (void)controller->stop_clock(controller->ctx);
        report->clock_started = 0;
        report_detail(report,
                      "the amplifier at I2C address 0x%02x answered %s to a fault read during "
                      "playback. Audio is stopped and the output stage is in high impedance.",
                      config->address, chorus_i2c_result_name(result));
        report->status = status_for_i2c(result);
        return report->status;
    }

    report->fault_read = 1;
    report->fault_bits = fault;
    if (fault == config->fault_clear_value) {
        report->output_in_high_impedance = 0;
        report_detail(report, "the amplifier reports no fault (0x%02x)", fault);
        return CHORUS_AMP_OK;
    }

    (void)go_high_impedance(stage, report);
    (void)controller->stop_clock(controller->ctx);
    report->clock_started = 0;
    report_detail(report,
                  "the amplifier at I2C address 0x%02x reports a fault: its fault register reads "
                  "0x%02x and a part with no fault reads 0x%02x. Audio is stopped and the output "
                  "stage is in high impedance.",
                  config->address, fault, config->fault_clear_value);
    report->status = CHORUS_AMP_REPORTS_FAULT;
    return report->status;
}
