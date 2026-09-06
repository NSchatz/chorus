/* The amplifier bring-up sequencer, graded on ORDER.
 *
 * Every assertion here is read off the simulated hardware's own event log,
 * which the driver cannot write to. What is being checked is not what the
 * driver says it did; it is what the part and the controller saw.
 *
 * The register numbers below belong to THIS TEST. They stand in for a
 * datasheet a bench will have and this phase does not, and the endpoint's own
 * configuration declares every one of them `unknown`. The scan in
 * firmware/check/endpoint_scan.c fails the suite if a hex literal ever appears
 * in firmware/src/amp.c, which is what keeps that true. */

#include "chorus/amp.h"
#include "fake_amp.h"
#include "harness.h"

#include <stddef.h>

/* The simulated part's map. Test-only, and named so that is unmistakable. */
#define TEST_ADDRESS 0x4C
#define TEST_REG_DEVICE_ID 0x00
#define TEST_REG_FAULT 0x71
#define TEST_REG_ANALOG_GAIN 0x54
#define TEST_REG_STATE_CONTROL 0x03
#define TEST_DEVICE_ID_VALUE 0x95
#define TEST_FAULT_CLEAR 0x00
#define TEST_GAIN_CODE 0x1F

static chorus_amp_config_t configured(void)
{
    chorus_amp_config_t config;
    memset(&config, 0, sizeof(config));
    config.address_known = 1;
    config.address = TEST_ADDRESS;
    config.reg_device_id_known = 1;
    config.reg_device_id = TEST_REG_DEVICE_ID;
    config.reg_fault_known = 1;
    config.reg_fault = TEST_REG_FAULT;
    config.reg_analog_gain_known = 1;
    config.reg_analog_gain = TEST_REG_ANALOG_GAIN;
    config.reg_state_control_known = 1;
    config.reg_state_control = TEST_REG_STATE_CONTROL;
    config.device_id_value_known = 1;
    config.device_id_value = TEST_DEVICE_ID_VALUE;
    config.fault_clear_value_known = 1;
    config.fault_clear_value = TEST_FAULT_CLEAR;
    config.analog_gain_ceiling_db = 0.0;
    snprintf(config.ceiling_source, sizeof(config.ceiling_source),
             "firmware/config/endpoint.conf");
    return config;
}

static chorus_amp_gain_t gain_of(double db)
{
    chorus_amp_gain_t gain;
    gain.db = db;
    gain.code_known = 1;
    gain.code = TEST_GAIN_CODE;
    return gain;
}

static chorus_i2s_clock_t good_clock(void)
{
    chorus_i2s_clock_t clock;
    clock.sample_rate_hz = 48000;
    clock.slot_bit_width = 24;
    clock.mclk_multiple = 384;
    clock.dma_frame_num = 240;
    clock.dma_desc_num = 6;
    return clock;
}

static void healthy_part(fake_amp_t *fake)
{
    fake_amp_init(fake, TEST_ADDRESS);
    fake->registers[TEST_REG_DEVICE_ID] = TEST_DEVICE_ID_VALUE;
    fake->registers[TEST_REG_FAULT] = TEST_FAULT_CLEAR;
}

/* AC-2, both halves. */
static void the_bring_up_order_is_what_the_hardware_saw(void)
{
    fake_amp_t fake;
    healthy_part(&fake);
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(0.0);
    chorus_i2s_clock_t clock = good_clock();
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;

    chorus_amp_status_t status =
        chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
    chorus_check(status == CHORUS_AMP_OK, "a healthy part brings up (%s: %s)",
                 chorus_amp_status_name(status), report.detail);
    fake_amp_print(&fake);

    int identity = fake_amp_first_read(&fake, TEST_REG_DEVICE_ID);
    int fault = fake_amp_first_read(&fake, TEST_REG_FAULT);
    int enable = fake_amp_first(&fake, FAKE_EV_OUTPUT_ENABLE);
    int high_z = fake_amp_first(&fake, FAKE_EV_HIGH_IMPEDANCE);
    int clock_at = fake_amp_first(&fake, FAKE_EV_CLOCK_APPLIED);

    chorus_check(identity >= 0 && fault >= 0 && enable >= 0 && high_z >= 0 && clock_at >= 0,
                 "the log carries an identity read, a fault read, a high-impedance command, a "
                 "clock change and an output enable");
    chorus_check(identity < enable,
                 "the identity read (event %d) is before output is enabled (event %d)", identity,
                 enable);
    chorus_check(fault < enable, "the fault read (event %d) is before output is enabled (event %d)",
                 fault, enable);
    chorus_check(high_z < clock_at,
                 "high impedance (event %d) is before the FIRST clock change (event %d)", high_z,
                 clock_at);
    chorus_check(fake.clock_changes == 1 && !fake.clock_changed_while_not_high_impedance,
                 "the output stage was in high impedance at every one of the %zu clock changes",
                 fake.clock_changes);
    chorus_check(fake.stage == FAKE_STAGE_ENABLED && report.output_in_high_impedance == 0,
                 "the stage ends up enabled");
    chorus_check(report.clock_started == 1, "the report says a clock was started");
    chorus_check(fake_amp_first_write(&fake, TEST_REG_ANALOG_GAIN) > fault,
                 "the gain is written after the part is known to be healthy");
    chorus_check(fake_amp_first_write(&fake, TEST_REG_ANALOG_GAIN) < enable,
                 "the gain is written before output is enabled");
}

/* AC-15, in every shape the bus can fail. */
static void a_part_that_does_not_answer_leaves_the_stage_dead(void)
{
    const struct {
        chorus_i2c_result_t answer;
        chorus_amp_status_t expected;
        const char *what;
    } cases[] = {
        {CHORUS_I2C_NACK, CHORUS_AMP_DID_NOT_ANSWER, "a part that NACKs its address"},
        {CHORUS_I2C_TIMEOUT, CHORUS_AMP_I2C_TIMEOUT, "a bus that times out"},
        {CHORUS_I2C_BUS_ERROR, CHORUS_AMP_I2C_BUS_ERROR, "a bus-level failure"},
    };

    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        fake_amp_t fake;
        healthy_part(&fake);
        fake.answer = cases[i].answer;
        chorus_amp_config_t config = configured();
        chorus_amp_gain_t gain = gain_of(0.0);
        chorus_i2s_clock_t clock = good_clock();
        chorus_i2c_bus_t bus = fake_amp_bus(&fake);
        chorus_output_stage_t stage = fake_amp_stage(&fake);
        chorus_i2s_controller_t controller = fake_amp_controller(&fake);
        chorus_amp_report_t report;

        chorus_amp_status_t status =
            chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
        chorus_check(status == cases[i].expected, "%s is reported as %s (got %s)", cases[i].what,
                     chorus_amp_status_name(cases[i].expected), chorus_amp_status_name(status));
        chorus_check(fake.stage == FAKE_STAGE_HIGH_IMPEDANCE && report.output_in_high_impedance,
                     "%s: the output stage is left in high impedance", cases[i].what);
        chorus_check(fake_amp_count(&fake, FAKE_EV_CLOCK_APPLIED) == 0 && !report.clock_started,
                     "%s: no I2S clock is started", cases[i].what);
        chorus_check(fake_amp_count(&fake, FAKE_EV_OUTPUT_ENABLE) == 0,
                     "%s: output is never enabled", cases[i].what);
        chorus_check(strstr(report.detail, chorus_i2c_result_name(cases[i].answer)) != NULL,
                     "%s: the condition is reported by name (%s)", cases[i].what, report.detail);
    }
}

/* AC-15, the other half: it answers, and it answers with a fault. */
static void a_part_that_reports_a_fault_at_bring_up_starts_no_clock(void)
{
    fake_amp_t fake;
    healthy_part(&fake);
    fake.registers[TEST_REG_FAULT] = 0x0C;
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(0.0);
    chorus_i2s_clock_t clock = good_clock();
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;

    chorus_amp_status_t status =
        chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
    chorus_check(status == CHORUS_AMP_REPORTS_FAULT,
                 "a part reporting a fault at bring-up is %s (got %s)",
                 chorus_amp_status_name(CHORUS_AMP_REPORTS_FAULT),
                 chorus_amp_status_name(status));
    chorus_check(fake.stage == FAKE_STAGE_HIGH_IMPEDANCE && report.output_in_high_impedance,
                 "the output stage is left in high impedance");
    chorus_check(fake_amp_count(&fake, FAKE_EV_CLOCK_APPLIED) == 0 && !report.clock_started,
                 "no I2S clock is started");
    chorus_check(report.fault_read && report.fault_bits == 0x0C,
                 "the fault bits are carried out for telemetry (0x%02x)", report.fault_bits);

    /* A part that is not the part we think it is. */
    fake_amp_t other;
    healthy_part(&other);
    other.registers[TEST_REG_DEVICE_ID] = 0x11;
    bus = fake_amp_bus(&other);
    stage = fake_amp_stage(&other);
    controller = fake_amp_controller(&other);
    status = chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
    chorus_check(status == CHORUS_AMP_IDENTITY_MISMATCH,
                 "a part answering with the wrong device id is %s",
                 chorus_amp_status_name(status));
    chorus_check(other.stage == FAKE_STAGE_HIGH_IMPEDANCE &&
                     fake_amp_count(&other, FAKE_EV_CLOCK_APPLIED) == 0,
                 "the stage is dead and no clock started");
    chorus_check(fake_amp_first_read(&other, TEST_REG_FAULT) < 0,
                 "the fault register is not even read once the identity is wrong");
}

/* AC-14. */
static void a_gain_above_the_ceiling_is_refused_before_anything_happens(void)
{
    fake_amp_t fake;
    healthy_part(&fake);
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(6.0);
    chorus_i2s_clock_t clock = good_clock();
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;

    chorus_amp_status_t status =
        chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
    chorus_check(status == CHORUS_AMP_GAIN_ABOVE_CEILING, "a gain above the ceiling is %s",
                 chorus_amp_status_name(status));
    chorus_check(fake_amp_count(&fake, FAKE_EV_OUTPUT_ENABLE) == 0,
                 "output is NOT enabled");
    chorus_check(fake.stage == FAKE_STAGE_HIGH_IMPEDANCE && report.output_in_high_impedance,
                 "the output stage is left in high impedance");
    chorus_check(fake_amp_i2c_transactions(&fake) == 0,
                 "the refusal is before the bus is touched at all (%zu transactions)",
                 fake_amp_i2c_transactions(&fake));
    chorus_check(fake_amp_count(&fake, FAKE_EV_CLOCK_APPLIED) == 0, "no clock is started");
    chorus_check(strstr(report.detail, "6.000") != NULL,
                 "the refusal names the requested value: %s", report.detail);
    chorus_check(strstr(report.detail, "0.000") != NULL, "the refusal names the ceiling");
    chorus_check(strstr(report.detail, "firmware/config/endpoint.conf") != NULL,
                 "the refusal names the file the ceiling is declared in");

    /* Exactly at the ceiling is inside it. A ceiling that refused its own value
     * would make the endpoint's committed configuration unbringable. */
    fake_amp_t at_ceiling;
    healthy_part(&at_ceiling);
    chorus_amp_gain_t exact = gain_of(0.0);
    bus = fake_amp_bus(&at_ceiling);
    stage = fake_amp_stage(&at_ceiling);
    controller = fake_amp_controller(&at_ceiling);
    chorus_check(chorus_amp_bring_up(&config, &exact, &clock, &bus, &stage, &controller,
                                     &report) == CHORUS_AMP_OK,
                 "a gain exactly at the ceiling is inside it");
}

/* The register map is DECLARED UNKNOWN in the committed configuration, so the
 * sequencer as the endpoint actually ships it refuses by name. */
static void an_unconfigured_register_refuses_by_name(void)
{
    const struct {
        const char *key;
        size_t offset;
    } keys[] = {
        {"amp_i2c_address", offsetof(chorus_amp_config_t, address_known)},
        {"amp_reg_device_id", offsetof(chorus_amp_config_t, reg_device_id_known)},
        {"amp_reg_fault", offsetof(chorus_amp_config_t, reg_fault_known)},
        {"amp_reg_analog_gain", offsetof(chorus_amp_config_t, reg_analog_gain_known)},
        {"amp_reg_state_control", offsetof(chorus_amp_config_t, reg_state_control_known)},
        {"amp_device_id_value", offsetof(chorus_amp_config_t, device_id_value_known)},
        {"amp_fault_clear_value", offsetof(chorus_amp_config_t, fault_clear_value_known)},
    };

    for (size_t i = 0; i < sizeof(keys) / sizeof(keys[0]); i++) {
        fake_amp_t fake;
        healthy_part(&fake);
        chorus_amp_config_t config = configured();
        *(int *)((char *)&config + keys[i].offset) = 0;
        chorus_amp_gain_t gain = gain_of(0.0);
        chorus_i2s_clock_t clock = good_clock();
        chorus_i2c_bus_t bus = fake_amp_bus(&fake);
        chorus_output_stage_t stage = fake_amp_stage(&fake);
        chorus_i2s_controller_t controller = fake_amp_controller(&fake);
        chorus_amp_report_t report;

        chorus_amp_status_t status =
            chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
        chorus_check(status == CHORUS_AMP_REGISTER_NOT_CONFIGURED &&
                         strstr(report.detail, keys[i].key) != NULL,
                     "%s declared unknown refuses by name (%s)", keys[i].key,
                     chorus_amp_status_name(status));
        chorus_check(fake.stage == FAKE_STAGE_HIGH_IMPEDANCE &&
                         fake_amp_i2c_transactions(&fake) == 0 &&
                         fake_amp_count(&fake, FAKE_EV_CLOCK_APPLIED) == 0,
                     "%s: the stage is dead, the bus untouched and no clock started",
                     keys[i].key);
    }

    /* And the gain code, which is a datasheet value like every other. */
    fake_amp_t fake;
    healthy_part(&fake);
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(0.0);
    gain.code_known = 0;
    chorus_i2s_clock_t clock = good_clock();
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;
    chorus_check(chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller,
                                     &report) == CHORUS_AMP_REGISTER_NOT_CONFIGURED &&
                     strstr(report.detail, "amp_analog_gain_code") != NULL,
                 "an unknown gain code refuses by name (%s)", report.detail);
}

/* A clock configuration that breaks a platform rule never reaches a pin. */
static void a_refused_clock_configuration_never_reaches_the_controller(void)
{
    fake_amp_t fake;
    healthy_part(&fake);
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(0.0);
    chorus_i2s_clock_t clock = good_clock();
    clock.mclk_multiple = 256; /* not divisible by three at a 24-bit slot width */
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;

    chorus_amp_status_t status =
        chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
    chorus_check(status == CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED,
                 "an MCLK multiple of 256 at 24 bits is refused before the controller sees it "
                 "(%s)",
                 chorus_amp_status_name(status));
    chorus_check(fake_amp_count(&fake, FAKE_EV_CLOCK_APPLIED) == 0,
                 "the controller was never asked to apply it");
    chorus_check(fake.stage == FAKE_STAGE_HIGH_IMPEDANCE, "the stage is left in high impedance");
    chorus_check(report.finding_count > 0 &&
                     strcmp(report.findings[0].rule,
                            "mclk-multiple-not-divisible-by-three") == 0,
                 "the finding names the rule: %s",
                 report.finding_count > 0 ? report.findings[0].rule : "<none>");
}

/* AC-5: a fault during playback stops the audio rather than being logged
 * beside it. */
static void a_fault_during_playback_stops_the_audio(void)
{
    fake_amp_t fake;
    healthy_part(&fake);
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(0.0);
    chorus_i2s_clock_t clock = good_clock();
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;
    chorus_check(chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller,
                                     &report) == CHORUS_AMP_OK,
                 "the part is playing");

    /* A quiet poll changes nothing. */
    chorus_amp_status_t status = chorus_amp_poll_fault(&config, &bus, &stage, &controller,
                                                       &report);
    chorus_check(status == CHORUS_AMP_OK && fake.stage == FAKE_STAGE_ENABLED,
                 "a healthy poll leaves the output enabled");

    fake.registers[TEST_REG_FAULT] = 0x21;
    status = chorus_amp_poll_fault(&config, &bus, &stage, &controller, &report);
    chorus_check(status == CHORUS_AMP_REPORTS_FAULT, "a fault during playback is %s",
                 chorus_amp_status_name(status));
    chorus_check(fake.stage == FAKE_STAGE_HIGH_IMPEDANCE,
                 "the output stage goes to high impedance");
    chorus_check(fake_amp_count(&fake, FAKE_EV_CLOCK_STOPPED) >= 1, "the I2S clock is stopped");
    chorus_check(!report.clock_started, "the report says the clock is not running");
    chorus_check(report.fault_read && report.fault_bits == 0x21,
                 "the fault bits reach the report for telemetry (0x%02x)", report.fault_bits);

    /* An amplifier that stops answering mid-run is not a healthy amplifier. */
    fake_amp_t silent;
    healthy_part(&silent);
    chorus_i2c_bus_t silent_bus = fake_amp_bus(&silent);
    chorus_output_stage_t silent_stage = fake_amp_stage(&silent);
    chorus_i2s_controller_t silent_controller = fake_amp_controller(&silent);
    chorus_check(chorus_amp_bring_up(&config, &gain, &clock, &silent_bus, &silent_stage,
                                     &silent_controller, &report) == CHORUS_AMP_OK,
                 "the second part is playing");
    silent.answer = CHORUS_I2C_NACK;
    status = chorus_amp_poll_fault(&config, &silent_bus, &silent_stage, &silent_controller,
                                   &report);
    chorus_check(status == CHORUS_AMP_DID_NOT_ANSWER &&
                     silent.stage == FAKE_STAGE_HIGH_IMPEDANCE,
                 "an amplifier that stops answering mid-run stops the audio too (%s)",
                 chorus_amp_status_name(status));
}

/* The other half of AC-2's clock rule, on the paths the bring-up trigger does
 * not reach: an unwind stops the I2S clock, and stopping a clock is a clock
 * change like any other. chorus/amp.h's step 7 says "this and EVERY LATER clock
 * change is made into a dead output", so the order is asserted here, again off
 * the simulated hardware's own log and its independent record of what the stage
 * was doing at each change - and not off the driver's account of itself. */
static void every_unwind_kills_the_output_before_it_stops_the_clock(void)
{
    /* A fault read during playback: the stage is LIVE when the unwind starts,
     * which is what makes this the dangerous one. */
    fake_amp_t fake;
    healthy_part(&fake);
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(0.0);
    chorus_i2s_clock_t clock = good_clock();
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;
    chorus_check(chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller,
                                     &report) == CHORUS_AMP_OK &&
                     fake.stage == FAKE_STAGE_ENABLED,
                 "the part is playing with the output stage live");

    /* The log is cleared so the unwind is read on its own; the fake's record of
     * what the stage was doing at each clock change is deliberately NOT. */
    fake.event_count = 0;
    fake.registers[TEST_REG_FAULT] = 0x21;
    chorus_check(chorus_amp_poll_fault(&config, &bus, &stage, &controller, &report) ==
                     CHORUS_AMP_REPORTS_FAULT,
                 "a fault during playback unwinds");
    fake_amp_print(&fake);
    int hiz = fake_amp_first(&fake, FAKE_EV_HIGH_IMPEDANCE);
    int stopped = fake_amp_first(&fake, FAKE_EV_CLOCK_STOPPED);
    chorus_check(hiz >= 0 && stopped >= 0 && hiz < stopped,
                 "high impedance (event %d) is commanded BEFORE the clock stops (event %d)", hiz,
                 stopped);
    chorus_check(fake.clock_changes == 2 && !fake.clock_changed_while_not_high_impedance,
                 "the output stage was in high impedance at every one of the %zu clock changes, "
                 "the stop included",
                 fake.clock_changes);

    /* A part that goes silent mid-run unwinds the same way. */
    fake_amp_t silent;
    healthy_part(&silent);
    chorus_i2c_bus_t silent_bus = fake_amp_bus(&silent);
    chorus_output_stage_t silent_stage = fake_amp_stage(&silent);
    chorus_i2s_controller_t silent_controller = fake_amp_controller(&silent);
    chorus_check(chorus_amp_bring_up(&config, &gain, &clock, &silent_bus, &silent_stage,
                                     &silent_controller, &report) == CHORUS_AMP_OK,
                 "a second part is playing");
    silent.event_count = 0;
    silent.answer = CHORUS_I2C_NACK;
    chorus_check(chorus_amp_poll_fault(&config, &silent_bus, &silent_stage, &silent_controller,
                                       &report) == CHORUS_AMP_DID_NOT_ANSWER,
                 "a part that stops answering unwinds");
    hiz = fake_amp_first(&silent, FAKE_EV_HIGH_IMPEDANCE);
    stopped = fake_amp_first(&silent, FAKE_EV_CLOCK_STOPPED);
    chorus_check(hiz >= 0 && stopped >= 0 && hiz < stopped,
                 "a silent part: high impedance (event %d) before the clock stops (event %d)", hiz,
                 stopped);
    chorus_check(!silent.clock_changed_while_not_high_impedance,
                 "a silent part: no clock change was made into a live output stage");

    /* A fault poll on a configuration that declares the fault register unknown
     * unwinds without ever touching the bus, and in the same order. */
    fake_amp_t unconfigured;
    healthy_part(&unconfigured);
    chorus_i2c_bus_t unconfigured_bus = fake_amp_bus(&unconfigured);
    chorus_output_stage_t unconfigured_stage = fake_amp_stage(&unconfigured);
    chorus_i2s_controller_t unconfigured_controller = fake_amp_controller(&unconfigured);
    chorus_check(chorus_amp_bring_up(&config, &gain, &clock, &unconfigured_bus,
                                     &unconfigured_stage, &unconfigured_controller, &report) ==
                     CHORUS_AMP_OK,
                 "a third part is playing");
    unconfigured.event_count = 0;
    chorus_amp_config_t unknown_fault = config;
    unknown_fault.reg_fault_known = 0;
    chorus_check(chorus_amp_poll_fault(&unknown_fault, &unconfigured_bus, &unconfigured_stage,
                                       &unconfigured_controller, &report) ==
                     CHORUS_AMP_REGISTER_NOT_CONFIGURED,
                 "a poll against an unknown fault register refuses by name");
    hiz = fake_amp_first(&unconfigured, FAKE_EV_HIGH_IMPEDANCE);
    stopped = fake_amp_first(&unconfigured, FAKE_EV_CLOCK_STOPPED);
    chorus_check(hiz >= 0 && stopped >= 0 && hiz < stopped,
                 "an unknown register: high impedance (event %d) before the clock stops "
                 "(event %d)",
                 hiz, stopped);
    chorus_check(!unconfigured.clock_changed_while_not_high_impedance,
                 "an unknown register: no clock change was made into a live output stage");

    /* And the bring-up path that applies a clock and then has to take it back:
     * an output stage that refuses to enable. */
    fake_amp_t refuses;
    healthy_part(&refuses);
    refuses.stage_refuses_enable = 1;
    chorus_i2c_bus_t refuses_bus = fake_amp_bus(&refuses);
    chorus_output_stage_t refuses_stage = fake_amp_stage(&refuses);
    chorus_i2s_controller_t refuses_controller = fake_amp_controller(&refuses);
    chorus_check(chorus_amp_bring_up(&config, &gain, &clock, &refuses_bus, &refuses_stage,
                                     &refuses_controller, &report) ==
                     CHORUS_AMP_OUTPUT_STAGE_REFUSED,
                 "an output stage that refuses to enable is reported by name");
    int applied = fake_amp_first(&refuses, FAKE_EV_CLOCK_APPLIED);
    stopped = fake_amp_first(&refuses, FAKE_EV_CLOCK_STOPPED);
    hiz = -1;
    for (size_t i = 0; i < refuses.event_count; i++) {
        if ((int)i > applied && refuses.events[i].kind == FAKE_EV_HIGH_IMPEDANCE) {
            hiz = (int)i;
            break;
        }
    }
    chorus_check(applied >= 0 && hiz > applied && stopped > hiz,
                 "the unwind after the clock was applied (event %d) re-commands high impedance "
                 "(event %d) before it stops the clock (event %d)",
                 applied, hiz, stopped);
    chorus_check(refuses.clock_changes == 2 && !refuses.clock_changed_while_not_high_impedance,
                 "a refused enable: the stage was dead at both clock changes");
}

/* The one condition worse than every other: a stage that will not go dead. */
static void a_stage_that_will_not_go_dead_stops_everything(void)
{
    fake_amp_t fake;
    healthy_part(&fake);
    fake.stage_refuses_high_impedance = 1;
    chorus_amp_config_t config = configured();
    chorus_amp_gain_t gain = gain_of(0.0);
    chorus_i2s_clock_t clock = good_clock();
    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;

    chorus_amp_status_t status =
        chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
    chorus_check(status == CHORUS_AMP_OUTPUT_STAGE_REFUSED, "a stage that refuses is %s",
                 chorus_amp_status_name(status));
    chorus_check(fake_amp_i2c_transactions(&fake) == 0 &&
                     fake_amp_count(&fake, FAKE_EV_CLOCK_APPLIED) == 0,
                 "nothing else is attempted");
}

int main(void)
{
    chorus_section("the bring-up order, as the simulated hardware saw it");
    the_bring_up_order_is_what_the_hardware_saw();

    chorus_section("a part that does not answer");
    a_part_that_does_not_answer_leaves_the_stage_dead();

    chorus_section("a part that answers reporting a fault");
    a_part_that_reports_a_fault_at_bring_up_starts_no_clock();

    chorus_section("a gain above the ceiling");
    a_gain_above_the_ceiling_is_refused_before_anything_happens();

    chorus_section("a register the configuration declares unknown");
    an_unconfigured_register_refuses_by_name();

    chorus_section("a clock configuration that breaks a platform rule");
    a_refused_clock_configuration_never_reaches_the_controller();

    chorus_section("a fault during playback");
    a_fault_during_playback_stops_the_audio();

    chorus_section("every unwind kills the output before it stops the clock");
    every_unwind_kills_the_output_before_it_stops_the_clock();

    chorus_section("an output stage that will not go to high impedance");
    a_stage_that_will_not_go_dead_stops_everything();

    return chorus_test_report("endpoint amplifier bring-up");
}
