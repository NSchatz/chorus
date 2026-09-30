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
    case CHORUS_AMP_DID_NOT_REACH_PLAY:
        return "amplifier-did-not-reach-play";
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

size_t chorus_amp_keys(const chorus_amp_config_t *config, chorus_amp_key_t *out, size_t capacity)
{
    const chorus_amp_key_t keys[] = {
        {"amp_i2c_address", &config->address},
        {"amp_reg_page_select", &config->reg_page_select},
        {"amp_reg_book_select", &config->reg_book_select},
        {"amp_page_book_zero", &config->page_book_zero},
        {"amp_reg_device_id", &config->reg_device_id},
        {"amp_device_id_value", &config->device_id_value},
        {"amp_reg_fault_channel", &config->reg_fault[0]},
        {"amp_reg_fault_global1", &config->reg_fault[1]},
        {"amp_reg_fault_global2", &config->reg_fault[2]},
        {"amp_fault_clear_value", &config->fault_clear_value},
        {"amp_clock_fault_bit", &config->clock_fault_bit},
        {"amp_reg_fault_clear", &config->reg_fault_clear},
        {"amp_fault_clear_command", &config->fault_clear_command},
        {"amp_reg_analog_gain", &config->reg_analog_gain},
        {"amp_reg_audio_format", &config->reg_audio_format},
        {"amp_audio_format_value", &config->audio_format_value},
        {"amp_reg_state_control", &config->reg_state_control},
        {"amp_state_hiz", &config->state_hiz},
        {"amp_state_play", &config->state_play},
        {"amp_reg_power_state", &config->reg_power_state},
        {"amp_power_state_play", &config->power_state_play},
    };
    size_t n = sizeof(keys) / sizeof(keys[0]);
    for (size_t i = 0; i < n && i < capacity; i++) {
        out[i] = keys[i];
    }
    return n < capacity ? n : capacity;
}

/* Put the stage in high impedance (PDN low) and record that it is there. A
 * stage that refuses to go dead is the one condition that is worse than every
 * other condition here, so it is its own status. */
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

/* A refusal after the clock has started: PDN low first, the clock after it. */
static void unwind_after_clock(chorus_output_stage_t *stage, chorus_i2s_controller_t *controller,
                               chorus_amp_report_t *report)
{
    chorus_amp_status_t status = report->status;
    char detail[CHORUS_AMP_DETAIL];
    memcpy(detail, report->detail, sizeof(detail));
    if (go_high_impedance(stage, report) != 0) {
        return;
    }
    (void)controller->stop_clock(controller->ctx);
    report->clock_started = 0;
    report->status = status;
    memcpy(report->detail, detail, sizeof(detail));
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

/* Every register-map value this sequencer must have before it may touch the
 * bus, checked by the key it is written under, so a refusal names the line of
 * endpoint.conf a reader has to fill in. */
static chorus_amp_status_t require_configured(const chorus_amp_config_t *config,
                                              chorus_amp_report_t *report)
{
    chorus_amp_key_t keys[32];
    size_t n = chorus_amp_keys(config, keys, sizeof(keys) / sizeof(keys[0]));
    for (size_t i = 0; i < n; i++) {
        if (!keys[i].byte->known) {
            report_detail(report,
                          "%s is declared unknown in %s. No register-map value is defaulted: "
                          "read it off TI's TAS5825M datasheet and write it there, with its page. "
                          "The output stage stays in high impedance and no I2S clock is started.",
                          keys[i].key, config->ceiling_source);
            return CHORUS_AMP_REGISTER_NOT_CONFIGURED;
        }
    }
    return CHORUS_AMP_OK;
}

/* One I2C transaction, with the failure written into the report by name.
 * Returns 0 on an ACK. */
static int amp_read(const chorus_amp_config_t *config, chorus_i2c_bus_t *bus, uint8_t reg,
                    const char *what, uint8_t *value, chorus_amp_report_t *report)
{
    chorus_i2c_result_t result = bus->read(bus->ctx, config->address.value, reg, value);
    if (result != CHORUS_I2C_ACK) {
        report_detail(report,
                      "the amplifier at I2C address 0x%02x answered %s to the %s read. The "
                      "output stage is in high impedance and no I2S clock is running.",
                      config->address.value, chorus_i2c_result_name(result), what);
        report->status = status_for_i2c(result);
        return -1;
    }
    return 0;
}

static int amp_write(const chorus_amp_config_t *config, chorus_i2c_bus_t *bus, uint8_t reg,
                     uint8_t value, const char *what, chorus_amp_report_t *report)
{
    chorus_i2c_result_t result = bus->write(bus->ctx, config->address.value, reg, value);
    if (result != CHORUS_I2C_ACK) {
        report_detail(report,
                      "the amplifier at I2C address 0x%02x answered %s to the %s write. The "
                      "output stage is in high impedance and no I2S clock is running.",
                      config->address.value, chorus_i2c_result_name(result), what);
        report->status = status_for_i2c(result);
        return -1;
    }
    return 0;
}

/* Read every fault register. `excused` is a bit of GLOBAL_FAULT1 (the second
 * register) that is not a fault at this point; zero excuses nothing. Returns 0
 * when every register reads clear. */
static int read_faults(const chorus_amp_config_t *config, chorus_i2c_bus_t *bus, uint8_t excused,
                       const char *when, chorus_amp_report_t *report)
{
    static const char *const names[CHORUS_AMP_FAULT_REGISTERS] = {"channel-fault", "global-fault-1",
                                                                  "global-fault-2"};
    for (size_t i = 0; i < CHORUS_AMP_FAULT_REGISTERS; i++) {
        uint8_t reg = config->reg_fault[i].value;
        uint8_t value = 0;
        if (amp_read(config, bus, reg, names[i], &value, report) != 0) {
            return -1;
        }
        uint8_t graded = (i == 1) ? (uint8_t)(value & (uint8_t)~excused) : value;
        report->fault_read = 1;
        report->fault_register = reg;
        report->fault_bits = value;
        if (graded != config->fault_clear_value.value) {
            report_detail(report,
                          "the amplifier at I2C address 0x%02x reports a fault %s: register 0x%02x "
                          "(%s) reads 0x%02x and a part with no fault reads 0x%02x. The output "
                          "stage is in high impedance.",
                          config->address.value, when, reg, names[i], value,
                          config->fault_clear_value.value);
            report->status = CHORUS_AMP_REPORTS_FAULT;
            return -1;
        }
    }
    return 0;
}

/* A clock ratio the part accepts: the bit clock per frame the configuration
 * commits the part to, and the one the I2S configuration produces, agree. */
static int clock_ratio_matches(const chorus_amp_config_t *config, const chorus_i2s_clock_t *clock,
                               chorus_amp_report_t *report)
{
    uint64_t per_frame =
        (clock->sample_rate_hz == 0) ? 0 : chorus_i2s_bclk_hz(clock) / clock->sample_rate_hz;
    if (per_frame == config->sclk_per_frame) {
        return 0;
    }
    if (report->finding_count < sizeof(report->findings) / sizeof(report->findings[0])) {
        chorus_finding_t *f = &report->findings[report->finding_count++];
        snprintf(f->rule, sizeof(f->rule), "bclk-ratio-unsupported-by-amplifier");
        snprintf(f->detail, sizeof(f->detail),
                 "the I2S configuration gives %llu bit clocks per frame and amp_sclk_per_frame "
                 "commits the amplifier to %u",
                 (unsigned long long)per_frame, (unsigned)config->sclk_per_frame);
    }
    return -1;
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

    /* 1. PDN low, before anything else. */
    if (go_high_impedance(stage, report) != 0) {
        return report->status;
    }

    /* 2. Everything that can be refused without the bus. */
    if (gain->db > config->analog_gain_ceiling_db) {
        report_detail(report,
                      "a requested analog gain of %.3f dB is above the ceiling of %.3f dB "
                      "declared in %s. Output is NOT enabled, the output stage stays in high "
                      "impedance, and no I2S clock is started.",
                      gain->db, config->analog_gain_ceiling_db, config->ceiling_source);
        report->status = CHORUS_AMP_GAIN_ABOVE_CEILING;
        return report->status;
    }
    chorus_amp_status_t configured = require_configured(config, report);
    if (configured != CHORUS_AMP_OK) {
        report->status = configured;
        return report->status;
    }
    if (!gain->code_known) {
        report_detail(report,
                      "amp_analog_gain_code is declared unknown in %s, so the register value that "
                      "produces %.3f dB on this part is not known. It is never derived from the "
                      "dB figure: read it off TI's TAS5825M datasheet. The output stage stays in "
                      "high impedance and no I2S clock is started.",
                      config->ceiling_source, gain->db);
        report->status = CHORUS_AMP_REGISTER_NOT_CONFIGURED;
        return report->status;
    }
    if (chorus_i2s_validate_clock(clock, report->findings,
                                  sizeof(report->findings) / sizeof(report->findings[0]),
                                  &report->finding_count) > 0 ||
        clock_ratio_matches(config, clock, report) != 0) {
        report_detail(report,
                      "the I2S clock configuration is refused (%s). No clock is started and the "
                      "output stage stays in high impedance.",
                      report->findings[0].rule);
        report->status = CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED;
        return report->status;
    }

    /* 3. PDN high: Deep Sleep, output not switching; the power-up wait. */
    if (stage->power_up(stage->ctx) != 0) {
        (void)go_high_impedance(stage, report);
        report_detail(report, "the amplifier's power-down line could not be released");
        report->status = CHORUS_AMP_OUTPUT_STAGE_REFUSED;
        return report->status;
    }
    report->output_in_high_impedance = 0;
    stage->wait_ms(stage->ctx, config->power_up_wait_ms);

    /* 4. Book 0, page 0, and is it the part we think it is? */
    uint8_t device_id = 0;
    if (amp_write(config, bus, config->reg_page_select.value, config->page_book_zero.value,
                  "page-select", report) != 0 ||
        amp_write(config, bus, config->reg_book_select.value, config->page_book_zero.value,
                  "book-select", report) != 0 ||
        amp_read(config, bus, config->reg_device_id.value, "device-id", &device_id, report) != 0) {
        (void)go_high_impedance(stage, report);
        return report->status;
    }
    if (device_id != config->device_id_value.value) {
        report_detail(report,
                      "the part at I2C address 0x%02x reported device id 0x%02x and %s declares "
                      "0x%02x. The output stage stays in high impedance and no I2S clock is "
                      "started.",
                      config->address.value, device_id, config->ceiling_source,
                      config->device_id_value.value);
        report->status = CHORUS_AMP_IDENTITY_MISMATCH;
        (void)go_high_impedance(stage, report);
        return report->status;
    }

    /* 5. Faults, before any clock; only the clock-fault bit is excused. */
    if (read_faults(config, bus, config->clock_fault_bit.value, "before the clock starts",
                    report) != 0) {
        chorus_amp_status_t status = report->status;
        (void)go_high_impedance(stage, report);
        report->status = status;
        return report->status;
    }

    /* 6. The gain, now known to be inside the ceiling, and the audio format. */
    if (amp_write(config, bus, config->reg_analog_gain.value, gain->code, "analog-gain", report) !=
            0 ||
        amp_write(config, bus, config->reg_audio_format.value, config->audio_format_value.value,
                  "audio-format", report) != 0) {
        chorus_amp_status_t status = report->status;
        (void)go_high_impedance(stage, report);
        report->status = status;
        return report->status;
    }

    /* 7. The clock, into a part in Deep Sleep. */
    if (controller->apply_clock(controller->ctx, clock) != 0) {
        (void)go_high_impedance(stage, report);
        report_detail(report, "the I2S controller refused the clock configuration");
        report->status = CHORUS_AMP_CLOCK_REFUSED;
        return report->status;
    }
    report->clock_started = 1;

    /* 8. Clear what latched without a clock, Hi-Z with the DSP on, settle. */
    if (amp_write(config, bus, config->reg_fault_clear.value, config->fault_clear_command.value,
                  "fault-clear", report) != 0 ||
        amp_write(config, bus, config->reg_state_control.value, config->state_hiz.value,
                  "state-control (Hi-Z)", report) != 0) {
        unwind_after_clock(stage, controller, report);
        return report->status;
    }
    stage->wait_ms(stage->ctx, config->dsp_settle_wait_ms);

    /* 9. Faults again, nothing excused. */
    if (read_faults(config, bus, 0, "with the clock running", report) != 0) {
        unwind_after_clock(stage, controller, report);
        return report->status;
    }

    /* 10. Play, and did it get there? */
    uint8_t power_state = 0;
    if (amp_write(config, bus, config->reg_state_control.value, config->state_play.value,
                  "state-control (Play)", report) != 0 ||
        amp_read(config, bus, config->reg_power_state.value, "power-state", &power_state, report) !=
            0) {
        unwind_after_clock(stage, controller, report);
        return report->status;
    }
    if (power_state != config->power_state_play.value) {
        report_detail(report,
                      "the amplifier at I2C address 0x%02x was commanded to Play and its power "
                      "state reads 0x%02x, not 0x%02x. Audio is stopped and the output stage is "
                      "in high impedance.",
                      config->address.value, power_state, config->power_state_play.value);
        report->status = CHORUS_AMP_DID_NOT_REACH_PLAY;
        unwind_after_clock(stage, controller, report);
        return report->status;
    }

    report_detail(report,
                  "the amplifier at I2C address 0x%02x answered as device 0x%02x, reported no "
                  "fault, took %.3f dB of analog gain against a ceiling of %.3f dB, and reached "
                  "Play after its clock was applied in Deep Sleep",
                  config->address.value, device_id, gain->db, config->analog_gain_ceiling_db);
    report->status = CHORUS_AMP_OK;
    return report->status;
}

chorus_amp_status_t chorus_amp_poll_fault(const chorus_amp_config_t *config, chorus_i2c_bus_t *bus,
                                          chorus_output_stage_t *stage,
                                          chorus_i2s_controller_t *controller,
                                          chorus_amp_report_t *report)
{
    memset(report, 0, sizeof(*report));
    report->status = CHORUS_AMP_OK;
    report->clock_started = 1;

    chorus_amp_status_t configured = require_configured(config, report);
    if (configured != CHORUS_AMP_OK) {
        report->status = configured;
        unwind_after_clock(stage, controller, report);
        return report->status;
    }
    if (read_faults(config, bus, 0, "during playback", report) != 0) {
        /* A fault, or a part that stopped answering: either way the audio
         * stops at the pins, and the condition is surfaced by name. */
        unwind_after_clock(stage, controller, report);
        return report->status;
    }
    report_detail(report, "the amplifier reports no fault (0x%02x)", report->fault_bits);
    return CHORUS_AMP_OK;
}

int chorus_amp_shut_down(const chorus_amp_config_t *config, chorus_i2c_bus_t *bus,
                         chorus_output_stage_t *stage, chorus_i2s_controller_t *controller)
{
    int rc = 0;
    if (config->address.known && config->reg_state_control.known && config->state_hiz.known &&
        bus->write(bus->ctx, config->address.value, config->reg_state_control.value,
                   config->state_hiz.value) == CHORUS_I2C_ACK) {
        stage->wait_ms(stage->ctx, config->shutdown_wait_ms);
    } else {
        rc = -1;
    }
    if (stage->high_impedance(stage->ctx) != 0) {
        rc = -1;
    }
    if (controller->stop_clock(controller->ctx) != 0) {
        rc = -1;
    }
    return rc;
}
