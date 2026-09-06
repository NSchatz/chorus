/* What the endpoint publishes about itself.
 *
 * AC-5 is the assertion under all of this: an amplifier fault is SURFACED IN
 * TELEMETRY rather than played through. The two halves are graded separately
 * because they fail separately - a system can report a fault and keep playing,
 * and a system can stop playing and say nothing - and both of those are the
 * failure the criterion names.
 *
 * The amplifier half of it, that a fault actually stops the audio at the pins,
 * is graded in test_amp.c against the simulated part. This file grades what a
 * reader of the telemetry sees. */

#include "chorus/telemetry.h"
#include "fake_amp.h"
#include "harness.h"

#define TEST_ADDRESS 0x4C
#define TEST_REG_DEVICE_ID 0x00
#define TEST_REG_FAULT 0x71
#define TEST_REG_ANALOG_GAIN 0x54
#define TEST_REG_STATE_CONTROL 0x03
#define TEST_DEVICE_ID_VALUE 0x95
#define TEST_FAULT_CLEAR 0x00

static void a_line_carries_all_three_surfaces(void)
{
    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    char line[512];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    printf("     %s\n", line);

    chorus_check(strstr(line, "link=") != NULL, "the line carries the link state");
    chorus_check(strstr(line, "offset_ns=") != NULL && strstr(line, "bound_ns=") != NULL,
                 "the line carries the sync health");
    chorus_check(strstr(line, "amp=") != NULL, "the line carries the amplifier's state");
    chorus_check(strstr(line, "audio=") != NULL, "the line says whether audio is running");

    /* A client that has never completed an exchange is making no claim, and
     * `bound_ns=0` is a claim that its offset is exact. This is the same rule
     * the Linux client's telemetry carries. */
    chorus_check(strstr(line, "offset_ns=none") != NULL &&
                     strstr(line, "round_trip_ns=none") != NULL &&
                     strstr(line, "bound_ns=none") != NULL,
                 "before any exchange the three sync fields read `none` and not `0`");
    chorus_check(strstr(line, "bound_ns=0") == NULL, "the string bound_ns=0 does not appear");

    telemetry.offset_known = 1;
    telemetry.offset_ns = -1234;
    telemetry.round_trip_ns = 400000;
    telemetry.bound_ns = 200000;
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    chorus_check(strstr(line, "offset_ns=-1234") != NULL &&
                     strstr(line, "round_trip_ns=400000") != NULL &&
                     strstr(line, "bound_ns=200000") != NULL,
                 "once an exchange has completed the three fields carry it: %s", line);
}

/* AC-5. */
static void a_fault_is_surfaced_by_name_and_the_audio_stops(void)
{
    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    telemetry.link = CHORUS_LINK_UP;
    telemetry.audio = CHORUS_AUDIO_RUNNING;
    telemetry.chunks_played = 500;

    char line[512];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    chorus_check(strstr(line, "audio=running") != NULL && strstr(line, "amp=ok") != NULL,
                 "a healthy endpoint says so");

    chorus_amp_report_t report;
    memset(&report, 0, sizeof(report));
    report.status = CHORUS_AMP_REPORTS_FAULT;
    report.fault_read = 1;
    report.fault_bits = 0x21;
    chorus_telemetry_record_amp(&telemetry, &report);
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    printf("     %s\n", line);

    chorus_check(strstr(line, "amp=amplifier-reports-fault") != NULL,
                 "the fault is surfaced BY NAME: %s", line);
    chorus_check(strstr(line, "amp_fault_bits=0x21") != NULL,
                 "the fault register's value is surfaced with it");
    chorus_check(strstr(line, "audio=stopped-on-amp-fault") != NULL,
                 "and the audio is stopped rather than played through");
    chorus_check(strstr(line, "audio=running") == NULL,
                 "there is no state of this struct that reports a fault and `audio=running`");

    /* Every non-OK amplifier status stops the audio, not only the one called
     * `fault`. An amplifier that has stopped answering is not a healthy one. */
    const chorus_amp_status_t stopping[] = {
        CHORUS_AMP_DID_NOT_ANSWER,   CHORUS_AMP_I2C_TIMEOUT,
        CHORUS_AMP_I2C_BUS_ERROR,    CHORUS_AMP_IDENTITY_MISMATCH,
        CHORUS_AMP_GAIN_ABOVE_CEILING, CHORUS_AMP_REGISTER_NOT_CONFIGURED,
        CHORUS_AMP_OUTPUT_STAGE_REFUSED, CHORUS_AMP_CLOCK_REFUSED,
        CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED,
    };
    for (size_t i = 0; i < sizeof(stopping) / sizeof(stopping[0]); i++) {
        chorus_telemetry_t one;
        chorus_telemetry_init(&one);
        one.audio = CHORUS_AUDIO_RUNNING;
        chorus_amp_report_t one_report;
        memset(&one_report, 0, sizeof(one_report));
        one_report.status = stopping[i];
        chorus_telemetry_record_amp(&one, &one_report);
        chorus_telemetry_line(&one, line, sizeof(line));
        chorus_check(one.audio == CHORUS_AUDIO_STOPPED_ON_AMP_FAULT &&
                         strstr(line, chorus_amp_status_name(stopping[i])) != NULL,
                     "%s stops the audio and is named in the line",
                     chorus_amp_status_name(stopping[i]));
    }
}

/* The whole path, end to end on the host: bring a simulated part up, publish,
 * fault it, publish again. */
static void the_amplifier_and_the_telemetry_agree(void)
{
    fake_amp_t fake;
    fake_amp_init(&fake, TEST_ADDRESS);
    fake.registers[TEST_REG_DEVICE_ID] = TEST_DEVICE_ID_VALUE;
    fake.registers[TEST_REG_FAULT] = TEST_FAULT_CLEAR;

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

    chorus_amp_gain_t gain;
    gain.db = 0.0;
    gain.code_known = 1;
    gain.code = 0x1F;

    chorus_i2s_clock_t clock;
    clock.sample_rate_hz = 48000;
    clock.slot_bit_width = 24;
    clock.mclk_multiple = 384;
    clock.dma_frame_num = 240;
    clock.dma_desc_num = 6;

    chorus_i2c_bus_t bus = fake_amp_bus(&fake);
    chorus_output_stage_t stage = fake_amp_stage(&fake);
    chorus_i2s_controller_t controller = fake_amp_controller(&fake);
    chorus_amp_report_t report;

    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    telemetry.link = CHORUS_LINK_UP;
    telemetry.audio = CHORUS_AUDIO_RUNNING;

    chorus_amp_bring_up(&config, &gain, &clock, &bus, &stage, &controller, &report);
    chorus_telemetry_record_amp(&telemetry, &report);
    char line[512];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    chorus_check(telemetry.audio == CHORUS_AUDIO_RUNNING && strstr(line, "amp=ok") != NULL,
                 "a healthy bring-up leaves the audio running");

    fake.registers[TEST_REG_FAULT] = 0x40;
    chorus_amp_poll_fault(&config, &bus, &stage, &controller, &report);
    chorus_telemetry_record_amp(&telemetry, &report);
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    printf("     %s\n", line);
    chorus_check(fake.stage == FAKE_STAGE_HIGH_IMPEDANCE &&
                     telemetry.audio == CHORUS_AUDIO_STOPPED_ON_AMP_FAULT &&
                     strstr(line, "amp=amplifier-reports-fault") != NULL &&
                     strstr(line, "amp_fault_bits=0x40") != NULL,
                 "a fault stops the output stage AND is named in the published line");
}

int main(void)
{
    chorus_section("a published line");
    a_line_carries_all_three_surfaces();

    chorus_section("an amplifier fault");
    a_fault_is_surfaced_by_name_and_the_audio_stops();

    chorus_section("the amplifier and the telemetry, end to end");
    the_amplifier_and_the_telemetry_agree();

    return chorus_test_report("endpoint telemetry");
}
