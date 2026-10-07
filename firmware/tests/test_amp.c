/* The amplifier bring-up sequencer, graded on ORDER, against the part.
 *
 * Every assertion here is read off the simulated hardware's own event log and
 * state, which the driver cannot write to. What is being checked is not what
 * the driver says it did; it is what the part and the controller saw.
 *
 * The register map is the COMMITTED one: every test loads
 * firmware/config/endpoint.conf, and the simulated part in fake_amp.c is
 * modelled on TI's TAS5825M datasheet (SLASEH7H, revision H) with its own
 * register numbers. A healthy bring-up here is therefore the committed map and
 * the datasheet agreeing, not the configuration agreeing with itself. */

#include "chorus/amp.h"
#include "chorus/endpoint_config.h"
#include "chorus/line_dac.h"
#include "fake_amp.h"
#include "harness.h"

#include <stddef.h>
#include <string.h>

static chorus_endpoint_config_t committed;

static void load_committed(void)
{
    char detail[512];
    detail[0] = '\0';
    int rc = chorus_endpoint_config_load(&committed, chorus_endpoint_config_default_path(), detail,
                                         sizeof(detail));
    chorus_check(rc == 0, "the committed endpoint.conf loads (%s)", detail);
}

typedef struct {
    fake_amp_t fake;
    chorus_amp_config_t config;
    chorus_amp_gain_t gain;
    chorus_i2s_clock_t clock;
    chorus_i2c_bus_t bus;
    chorus_output_stage_t stage;
    chorus_i2s_controller_t controller;
    chorus_amp_report_t report;
} rig_t;

static void rig_init(rig_t *rig)
{
    memset(rig, 0, sizeof(*rig));
    fake_amp_init(&rig->fake);
    rig->config = committed.amp;
    rig->gain = committed.gain;
    rig->clock = committed.clock;
    rig->bus = fake_amp_bus(&rig->fake);
    rig->stage = fake_amp_stage(&rig->fake);
    rig->controller = fake_amp_controller(&rig->fake);
}

static chorus_amp_status_t rig_bring_up(rig_t *rig)
{
    return chorus_amp_bring_up(&rig->config, &rig->gain, &rig->clock, &rig->bus, &rig->stage,
                               &rig->controller, &rig->report);
}

/* Index of the first wait at or after `from`, and its length. */
static int wait_after(const fake_amp_t *fake, int from, uint32_t *ms)
{
    for (size_t i = (size_t)(from < 0 ? 0 : from); i < fake->event_count; i++) {
        if (fake->events[i].kind == FAKE_EV_WAIT) {
            *ms = fake->events[i].ms;
            return (int)i;
        }
    }
    return -1;
}

/* The committed register map is the datasheet's. */
static void the_committed_map_is_complete(void)
{
    chorus_amp_key_t keys[32];
    size_t n = chorus_amp_keys(&committed.amp, keys, sizeof(keys) / sizeof(keys[0]));
    size_t unknown = 0;
    for (size_t i = 0; i < n; i++) {
        if (!keys[i].byte->known) {
            printf("     %s is unknown\n", keys[i].key);
            unknown++;
        }
    }
    chorus_check(n == 21 && unknown == 0 && committed.gain.code_known,
                 "every one of the %zu register-map values and the gain code is known (%zu "
                 "unknown)",
                 n, unknown);
    chorus_check(committed.amp.address.value == FAKE_TAS_ADDRESS &&
                     committed.amp.device_id_value.value == FAKE_TAS_DIE_ID &&
                     committed.amp.reg_device_id.value == FAKE_TAS_REG_DIE_ID,
                 "the committed address and identity are the datasheet's (0x%02x, DIE_ID 0x%02x "
                 "at 0x%02x)",
                 committed.amp.address.value, committed.amp.device_id_value.value,
                 committed.amp.reg_device_id.value);
    chorus_check(committed.gain.code == 0x1F && committed.gain.db == 0.0 &&
                     committed.amp.analog_gain_ceiling_db == 0.0,
                 "the committed gain is the part's lowest setting (code 0x%02x, -15.5 dB), 0 dB "
                 "above it, at the ceiling",
                 committed.gain.code);
    chorus_check(committed.amp.power_up_wait_ms >= 5 && committed.amp.dsp_settle_wait_ms >= 5 &&
                     committed.amp.shutdown_wait_ms >= 6,
                 "the committed waits are at least the datasheet's 5, 5 and 6 ms");
    chorus_check(chorus_i2s_bclk_hz(&committed.clock) / committed.clock.sample_rate_hz ==
                         committed.amp.sclk_per_frame &&
                     committed.amp.sclk_per_frame == 64,
                 "the committed I2S runs at 64 bit clocks per frame, a ratio the part accepts");
}

/* AC-2, against the datasheet's startup procedure. */
static void the_committed_map_brings_the_datasheet_part_to_play(void)
{
    for (int clock_fault = 1; clock_fault >= 0; clock_fault--) {
        rig_t rig;
        rig_init(&rig);
        rig.fake.clock_fault_without_clock = clock_fault;
        chorus_amp_status_t status = rig_bring_up(&rig);
        chorus_check(status == CHORUS_AMP_OK,
                     "the part brings up to Play (clock fault without a clock: %s) (%s: %s)",
                     clock_fault ? "latched" : "not raised", chorus_amp_status_name(status),
                     rig.report.detail);
        if (clock_fault) {
            fake_amp_print(&rig.fake);
        }
        fake_amp_t *f = &rig.fake;

        int pdn_low = fake_amp_first(f, FAKE_EV_HIGH_IMPEDANCE);
        int pdn_high = fake_amp_first(f, FAKE_EV_POWER_UP);
        uint32_t power_wait = 0;
        int first_wait = wait_after(f, pdn_high, &power_wait);
        int identity = fake_amp_first_read(f, FAKE_TAS_REG_DIE_ID);
        int fault = fake_amp_first_read(f, FAKE_TAS_REG_CHAN_FAULT);
        int gain = fake_amp_first_write(f, FAKE_TAS_REG_AGAIN);
        int format = fake_amp_first_write(f, FAKE_TAS_REG_SAP_CTRL1);
        int clock_at = fake_amp_first(f, FAKE_EV_CLOCK_APPLIED);
        int cleared = fake_amp_first_write(f, FAKE_TAS_REG_FAULT_CLEAR);
        int hiz = fake_amp_first_write_of(f, FAKE_TAS_REG_DEVICE_CTRL2, 0x02);
        uint32_t settle = 0;
        int settle_at = wait_after(f, hiz, &settle);
        int play = fake_amp_first_write_of(f, FAKE_TAS_REG_DEVICE_CTRL2, 0x03);
        int reported = fake_amp_first_read(f, FAKE_TAS_REG_POWER_STATE);

        chorus_check(pdn_low == 0, "PDN is driven low first (event %d)", pdn_low);
        chorus_check(pdn_high > pdn_low && first_wait > pdn_high && power_wait >= 5,
                     "PDN goes high (event %d), then a wait of %u ms (event %d), at least the "
                     "datasheet's 5",
                     pdn_high, (unsigned)power_wait, first_wait);
        chorus_check(!f->i2c_before_power_up_wait && !f->i2c_while_pdn_low,
                     "no I2C transaction before the part was powered and settled");
        chorus_check(identity > first_wait && fault > identity && gain > fault && format > fault &&
                         clock_at > gain && clock_at > format,
                     "identity (%d), faults (%d), gain (%d) and format (%d) are before the clock "
                     "(%d)",
                     identity, fault, gain, format, clock_at);
        chorus_check(cleared > clock_at && hiz > cleared && settle_at > hiz && settle >= 5 &&
                         play > settle_at && reported > play,
                     "after the clock: fault clear (%d), Hi-Z with the DSP (%d), a %u ms settle "
                     "(%d), Play (%d), the power state read back (%d)",
                     cleared, hiz, (unsigned)settle, settle_at, play, reported);
        chorus_check(!f->play_before_dsp_settled, "Play was not commanded before the DSP settled");
        chorus_check(f->clock_changes == 1 && !f->clock_changed_while_switching,
                     "the output was not switching at any of the %zu clock changes",
                     f->clock_changes);
        chorus_check(fake_amp_switching(f) && f->registers[FAKE_TAS_REG_POWER_STATE] == 3,
                     "the part ends up switching, in Play");
        chorus_check(f->registers[FAKE_TAS_REG_AGAIN] == 0x1F,
                     "the analog gain register holds the lowest setting (0x%02x)",
                     f->registers[FAKE_TAS_REG_AGAIN]);
        chorus_check(rig.report.clock_started == 1 && rig.report.output_in_high_impedance == 0,
                     "the report says the clock runs and the output is live");
    }
}

/* AC-15, in every shape the bus can fail, and at a wrong address. */
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

    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]) + 1; i++) {
        rig_t rig;
        rig_init(&rig);
        chorus_i2c_result_t answer = CHORUS_I2C_NACK;
        chorus_amp_status_t expected = CHORUS_AMP_DID_NOT_ANSWER;
        const char *what = "a part strapped to another address (0x4D)";
        if (i < sizeof(cases) / sizeof(cases[0])) {
            rig.fake.answer = cases[i].answer;
            answer = cases[i].answer;
            expected = cases[i].expected;
            what = cases[i].what;
        } else {
            rig.fake.address = 0x4D;
        }
        chorus_amp_status_t status = rig_bring_up(&rig);
        chorus_check(status == expected, "%s is reported as %s (got %s)", what,
                     chorus_amp_status_name(expected), chorus_amp_status_name(status));
        chorus_check(rig.fake.pdn == FAKE_PDN_LOW && rig.report.output_in_high_impedance,
                     "%s: PDN is left low", what);
        chorus_check(fake_amp_count(&rig.fake, FAKE_EV_CLOCK_APPLIED) == 0 &&
                         !rig.report.clock_started,
                     "%s: no I2S clock is started", what);
        chorus_check(strstr(rig.report.detail, chorus_i2c_result_name(answer)) != NULL,
                     "%s: the condition is reported by name (%s)", what, rig.report.detail);
    }
}

/* AC-15, the other half: it answers, with a fault or as another part. */
static void a_part_that_reports_a_fault_at_bring_up_starts_no_clock(void)
{
    const struct {
        size_t reg;
        uint8_t bits;
        const char *what;
    } cases[] = {
        {0, 0x0C, "a DC fault on both channels (CHAN_FAULT)"},
        {1, 0x02, "PVDD over-voltage (GLOBAL_FAULT1)"},
        {1, 0x06, "PVDD over-voltage beside the excused clock bit"},
        {2, 0x01, "an over-temperature shutdown (GLOBAL_FAULT2)"},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        rig_t rig;
        rig_init(&rig);
        rig.fake.persistent_fault[cases[i].reg] = cases[i].bits;
        chorus_amp_status_t status = rig_bring_up(&rig);
        chorus_check(status == CHORUS_AMP_REPORTS_FAULT, "%s is %s (got %s: %s)", cases[i].what,
                     chorus_amp_status_name(CHORUS_AMP_REPORTS_FAULT),
                     chorus_amp_status_name(status), rig.report.detail);
        chorus_check(rig.fake.pdn == FAKE_PDN_LOW &&
                         fake_amp_count(&rig.fake, FAKE_EV_CLOCK_APPLIED) == 0 &&
                         !rig.report.clock_started,
                     "%s: PDN low and no I2S clock started", cases[i].what);
        chorus_check(rig.report.fault_read &&
                         rig.report.fault_register == FAKE_TAS_REG_CHAN_FAULT + cases[i].reg &&
                         rig.report.fault_bits ==
                             (uint8_t)(cases[i].bits | (cases[i].reg == 1 ? 0x04 : 0x00)),
                     "%s: register 0x%02x and bits 0x%02x reach the report for telemetry",
                     cases[i].what, rig.report.fault_register, rig.report.fault_bits);
    }

    rig_t other;
    rig_init(&other);
    other.fake.die_id = 0x11;
    chorus_amp_status_t status = rig_bring_up(&other);
    chorus_check(status == CHORUS_AMP_IDENTITY_MISMATCH,
                 "a part answering with the wrong device id is %s", chorus_amp_status_name(status));
    chorus_check(other.fake.pdn == FAKE_PDN_LOW &&
                     fake_amp_count(&other.fake, FAKE_EV_CLOCK_APPLIED) == 0,
                 "PDN low and no clock started");
    chorus_check(fake_amp_first_read(&other.fake, FAKE_TAS_REG_CHAN_FAULT) < 0,
                 "the fault registers are not even read once the identity is wrong");
}

/* The datasheet's order puts some checks after the clock; those unwind with
 * PDN low BEFORE the clock stops. */
static void a_fault_or_a_refusal_after_the_clock_unwinds_in_order(void)
{
    const struct {
        int clock_fault_persists;
        int refuses_play;
        int controller_refuses;
        chorus_amp_status_t expected;
        const char *what;
    } cases[] = {
        {1, 0, 0, CHORUS_AMP_REPORTS_FAULT, "a clock fault that stays once the clock runs"},
        {0, 1, 0, CHORUS_AMP_DID_NOT_REACH_PLAY, "a part that stays in Hi-Z when told to Play"},
        {0, 0, 1, CHORUS_AMP_CLOCK_REFUSED, "an I2S controller that refuses the clock"},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        rig_t rig;
        rig_init(&rig);
        if (cases[i].clock_fault_persists) {
            rig.fake.persistent_fault[1] = FAKE_TAS_CLK_FAULT;
        }
        rig.fake.refuses_play = cases[i].refuses_play;
        rig.fake.controller_refuses_clock = cases[i].controller_refuses;
        chorus_amp_status_t status = rig_bring_up(&rig);
        chorus_check(status == cases[i].expected, "%s is %s (got %s: %s)", cases[i].what,
                     chorus_amp_status_name(cases[i].expected), chorus_amp_status_name(status),
                     rig.report.detail);
        int applied = fake_amp_first(&rig.fake, FAKE_EV_CLOCK_APPLIED);
        int stopped = fake_amp_first(&rig.fake, FAKE_EV_CLOCK_STOPPED);
        int last_low = -1;
        for (size_t e = 0; e < rig.fake.event_count; e++) {
            if (rig.fake.events[e].kind == FAKE_EV_HIGH_IMPEDANCE) {
                last_low = (int)e;
            }
        }
        if (cases[i].controller_refuses) {
            chorus_check(applied < 0 && rig.fake.pdn == FAKE_PDN_LOW && !rig.report.clock_started,
                         "%s: no clock, PDN low", cases[i].what);
        } else {
            chorus_check(applied >= 0 && last_low > applied && stopped > last_low,
                         "%s: PDN low (event %d) before the clock stops (event %d)", cases[i].what,
                         last_low, stopped);
        }
        chorus_check(!rig.fake.clock_changed_while_switching && !fake_amp_switching(&rig.fake),
                     "%s: no clock change into a switching output, and it is dead", cases[i].what);
    }
}

/* AC-14. */
static void a_gain_above_the_ceiling_is_refused_before_anything_happens(void)
{
    rig_t rig;
    rig_init(&rig);
    rig.gain.db = 6.0;
    chorus_amp_status_t status = rig_bring_up(&rig);
    chorus_check(status == CHORUS_AMP_GAIN_ABOVE_CEILING, "a gain above the ceiling is %s",
                 chorus_amp_status_name(status));
    chorus_check(rig.fake.pdn == FAKE_PDN_LOW && rig.report.output_in_high_impedance,
                 "PDN is left low");
    chorus_check(fake_amp_i2c_transactions(&rig.fake) == 0 &&
                     fake_amp_count(&rig.fake, FAKE_EV_POWER_UP) == 0,
                 "the refusal is before the part is powered or the bus touched (%zu transactions)",
                 fake_amp_i2c_transactions(&rig.fake));
    chorus_check(fake_amp_count(&rig.fake, FAKE_EV_CLOCK_APPLIED) == 0, "no clock is started");
    chorus_check(strstr(rig.report.detail, "6.000") != NULL &&
                     strstr(rig.report.detail, "0.000") != NULL &&
                     strstr(rig.report.detail, "firmware/config/endpoint.conf") != NULL,
                 "the refusal names the request, the ceiling and the file: %s", rig.report.detail);
}

/* No register-map value is defaulted: each one declared unknown refuses by
 * its own key. */
static void an_unconfigured_register_refuses_by_name(void)
{
    chorus_amp_key_t keys[32];
    size_t n = chorus_amp_keys(&committed.amp, keys, sizeof(keys) / sizeof(keys[0]));
    size_t refused = 0;
    for (size_t i = 0; i < n; i++) {
        rig_t rig;
        rig_init(&rig);
        chorus_amp_key_t mine[32];
        (void)chorus_amp_keys(&rig.config, mine, sizeof(mine) / sizeof(mine[0]));
        ((chorus_amp_byte_t *)mine[i].byte)->known = 0;
        chorus_amp_status_t status = rig_bring_up(&rig);
        int ok = status == CHORUS_AMP_REGISTER_NOT_CONFIGURED &&
                 strstr(rig.report.detail, keys[i].key) != NULL && rig.fake.pdn == FAKE_PDN_LOW &&
                 fake_amp_i2c_transactions(&rig.fake) == 0 &&
                 fake_amp_count(&rig.fake, FAKE_EV_POWER_UP) == 0;
        if (!ok) {
            printf("     %s: %s (%s)\n", keys[i].key, chorus_amp_status_name(status),
                   rig.report.detail);
        }
        refused += ok ? 1u : 0u;
    }
    chorus_check(refused == n,
                 "each of the %zu register-map keys, declared unknown, refuses by its own name "
                 "with PDN low, the part unpowered and the bus untouched (%zu did)",
                 n, refused);

    rig_t rig;
    rig_init(&rig);
    rig.gain.code_known = 0;
    chorus_check(rig_bring_up(&rig) == CHORUS_AMP_REGISTER_NOT_CONFIGURED &&
                     strstr(rig.report.detail, "amp_analog_gain_code") != NULL,
                 "an unknown gain code refuses by name (%s)", rig.report.detail);
}

/* A clock configuration that breaks a platform rule, or a ratio the part does
 * not accept, never reaches the controller. */
static void a_refused_clock_configuration_never_reaches_the_controller(void)
{
    const struct {
        uint32_t wire;
        uint32_t mclk;
        const char *rule;
        const char *what;
    } cases[] = {
        {32, 256, "mclk-multiple-not-divisible-by-three", "an MCLK multiple of 256 at 24 bits"},
        {24, 384, "bclk-ratio-unsupported-by-amplifier",
         "24-bit slots on the wire (48 bit clocks per frame)"},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        rig_t rig;
        rig_init(&rig);
        rig.clock.wire_slot_bit_width = cases[i].wire;
        rig.clock.mclk_multiple = cases[i].mclk;
        chorus_amp_status_t status = rig_bring_up(&rig);
        chorus_check(status == CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED,
                     "%s is refused before the controller sees it (%s)", cases[i].what,
                     chorus_amp_status_name(status));
        chorus_check(fake_amp_count(&rig.fake, FAKE_EV_CLOCK_APPLIED) == 0 &&
                         fake_amp_count(&rig.fake, FAKE_EV_POWER_UP) == 0 &&
                         rig.fake.pdn == FAKE_PDN_LOW,
                     "%s: no clock, the part unpowered, PDN low", cases[i].what);
        chorus_check(rig.report.finding_count > 0 &&
                         strcmp(rig.report.findings[0].rule, cases[i].rule) == 0,
                     "%s: the finding names the rule: %s", cases[i].what,
                     rig.report.finding_count > 0 ? rig.report.findings[0].rule : "<none>");
    }
}

/* AC-5: a fault during playback stops the audio rather than being logged
 * beside it, and the unwind kills the output before it stops the clock. */
static void a_fault_during_playback_stops_the_audio(void)
{
    rig_t rig;
    rig_init(&rig);
    chorus_check(rig_bring_up(&rig) == CHORUS_AMP_OK && fake_amp_switching(&rig.fake),
                 "the part is playing");
    chorus_amp_status_t status =
        chorus_amp_poll_fault(&rig.config, &rig.bus, &rig.stage, &rig.controller, &rig.report);
    chorus_check(status == CHORUS_AMP_OK && fake_amp_switching(&rig.fake),
                 "a healthy poll leaves the output switching");

    rig.fake.event_count = 0;
    rig.fake.persistent_fault[2] = 0x01;
    status = chorus_amp_poll_fault(&rig.config, &rig.bus, &rig.stage, &rig.controller, &rig.report);
    fake_amp_print(&rig.fake);
    chorus_check(status == CHORUS_AMP_REPORTS_FAULT, "an over-temperature shutdown is %s",
                 chorus_amp_status_name(status));
    int low = fake_amp_first(&rig.fake, FAKE_EV_HIGH_IMPEDANCE);
    int stopped = fake_amp_first(&rig.fake, FAKE_EV_CLOCK_STOPPED);
    chorus_check(low >= 0 && stopped > low && rig.fake.pdn == FAKE_PDN_LOW &&
                     !rig.report.clock_started,
                 "PDN low (event %d) before the clock stops (event %d)", low, stopped);
    chorus_check(rig.report.fault_read && rig.report.fault_register == FAKE_TAS_REG_GLOBAL_FAULT2 &&
                     rig.report.fault_bits == 0x01,
                 "the fault register and bits reach the report (0x%02x: 0x%02x)",
                 rig.report.fault_register, rig.report.fault_bits);

    rig_t silent;
    rig_init(&silent);
    chorus_check(rig_bring_up(&silent) == CHORUS_AMP_OK, "a second part is playing");
    silent.fake.answer = CHORUS_I2C_NACK;
    status = chorus_amp_poll_fault(&silent.config, &silent.bus, &silent.stage, &silent.controller,
                                   &silent.report);
    chorus_check(status == CHORUS_AMP_DID_NOT_ANSWER && silent.fake.pdn == FAKE_PDN_LOW &&
                     !silent.fake.clock_changed_while_switching,
                 "an amplifier that stops answering mid-run stops the audio too (%s)",
                 chorus_amp_status_name(status));

    rig_t unknown;
    rig_init(&unknown);
    chorus_check(rig_bring_up(&unknown) == CHORUS_AMP_OK, "a third part is playing");
    unknown.config.reg_fault[0].known = 0;
    unknown.fake.event_count = 0;
    status = chorus_amp_poll_fault(&unknown.config, &unknown.bus, &unknown.stage,
                                   &unknown.controller, &unknown.report);
    chorus_check(status == CHORUS_AMP_REGISTER_NOT_CONFIGURED &&
                     strstr(unknown.report.detail, "amp_reg_fault_channel") != NULL &&
                     fake_amp_i2c_transactions(&unknown.fake) == 0 &&
                     unknown.fake.pdn == FAKE_PDN_LOW &&
                     !unknown.fake.clock_changed_while_switching,
                 "a poll against an unknown fault register refuses by name without the bus");
}

/* The datasheet's shutdown procedure (p. 43). */
static void a_shutdown_goes_hiz_waits_then_drops_pdn_and_the_clock(void)
{
    rig_t rig;
    rig_init(&rig);
    chorus_check(rig_bring_up(&rig) == CHORUS_AMP_OK, "the part is playing");
    rig.fake.event_count = 0;
    int rc = chorus_amp_shut_down(&rig.config, &rig.bus, &rig.stage, &rig.controller);
    fake_amp_print(&rig.fake);
    int hiz = fake_amp_first_write_of(&rig.fake, FAKE_TAS_REG_DEVICE_CTRL2, 0x02);
    uint32_t ms = 0;
    int waited = wait_after(&rig.fake, hiz, &ms);
    int low = fake_amp_first(&rig.fake, FAKE_EV_HIGH_IMPEDANCE);
    int stopped = fake_amp_first(&rig.fake, FAKE_EV_CLOCK_STOPPED);
    chorus_check(rc == 0 && hiz == 0 && waited > hiz && ms >= 6 && low > waited && stopped > low,
                 "Hi-Z (%d), a %u ms wait (%d), PDN low (%d), then the clock (%d)", hiz,
                 (unsigned)ms, waited, low, stopped);
    chorus_check(!rig.fake.clock_changed_while_switching && !fake_amp_switching(&rig.fake),
                 "the clock stopped into a dead output");

    rig_t silent;
    rig_init(&silent);
    chorus_check(rig_bring_up(&silent) == CHORUS_AMP_OK, "a second part is playing");
    silent.fake.answer = CHORUS_I2C_NACK;
    rc = chorus_amp_shut_down(&silent.config, &silent.bus, &silent.stage, &silent.controller);
    chorus_check(rc != 0 && silent.fake.pdn == FAKE_PDN_LOW &&
                     !silent.fake.clock_changed_while_switching,
                 "a part that does not answer the Hi-Z write still gets PDN low before the clock");
}

/* The one condition worse than every other: a stage that will not go dead,
 * and its cousin, a power-down line that will not release. */
static void a_stage_that_will_not_go_dead_stops_everything(void)
{
    rig_t rig;
    rig_init(&rig);
    rig.fake.stage_refuses_high_impedance = 1;
    chorus_amp_status_t status = rig_bring_up(&rig);
    chorus_check(status == CHORUS_AMP_OUTPUT_STAGE_REFUSED, "a stage that refuses is %s",
                 chorus_amp_status_name(status));
    chorus_check(fake_amp_i2c_transactions(&rig.fake) == 0 &&
                     fake_amp_count(&rig.fake, FAKE_EV_POWER_UP) == 0 &&
                     fake_amp_count(&rig.fake, FAKE_EV_CLOCK_APPLIED) == 0,
                 "nothing else is attempted");

    rig_t stuck;
    rig_init(&stuck);
    stuck.fake.stage_refuses_power_up = 1;
    status = rig_bring_up(&stuck);
    chorus_check(status == CHORUS_AMP_OUTPUT_STAGE_REFUSED && stuck.fake.pdn == FAKE_PDN_LOW &&
                     fake_amp_i2c_transactions(&stuck.fake) == 0,
                 "a power-down line that will not release is %s, with PDN low and no I2C",
                 chorus_amp_status_name(status));
}

/* The subwoofer's line DAC (chorus/line_dac.h): no control bus, so no I2C at
 * all; the mute low before the clock, the clock into a muted part, the mute
 * released only when asked (the writer runs), and stopped mute first. */
static void a_line_dac_is_muted_clocked_then_unmuted_with_no_i2c(void)
{
    rig_t rig;
    rig_init(&rig);
    chorus_amp_status_t status =
        chorus_line_dac_start(&rig.clock, &rig.stage, &rig.controller, &rig.report);
    chorus_check(status == CHORUS_AMP_OK, "the committed clock starts the line DAC (%s: %s)",
                 chorus_amp_status_name(status), rig.report.detail);
    int low = fake_amp_first(&rig.fake, FAKE_EV_HIGH_IMPEDANCE);
    int clock = fake_amp_first(&rig.fake, FAKE_EV_CLOCK_APPLIED);
    chorus_check(low == 0 && clock > low,
                 "the mute goes low first (event %d), the clock after (%d)", low, clock);
    chorus_check(rig.fake.pdn == FAKE_PDN_LOW && fake_amp_count(&rig.fake, FAKE_EV_POWER_UP) == 0 &&
                     rig.report.output_in_high_impedance && rig.report.clock_started,
                 "the clock runs into a muted part: the mute stays low until the writer runs");
    chorus_check(fake_amp_i2c_transactions(&rig.fake) == 0 && !rig.report.fault_read,
                 "no I2C transaction and no fault register: the PCM5102A has no control bus");

    status = chorus_line_dac_unmute(&rig.stage, &rig.report);
    chorus_check(status == CHORUS_AMP_OK && rig.fake.pdn == FAKE_PDN_HIGH &&
                     !rig.report.output_in_high_impedance,
                 "unmuted when the writer runs: the mute line high (%s)",
                 chorus_amp_status_name(status));

    size_t before = rig.fake.event_count;
    chorus_line_dac_stop(&rig.stage, &rig.controller);
    int stop_low = -1;
    int stop_clock = -1;
    for (size_t i = before; i < rig.fake.event_count; i++) {
        if (rig.fake.events[i].kind == FAKE_EV_HIGH_IMPEDANCE && stop_low < 0) {
            stop_low = (int)i;
        }
        if (rig.fake.events[i].kind == FAKE_EV_CLOCK_STOPPED && stop_clock < 0) {
            stop_clock = (int)i;
        }
    }
    chorus_check(stop_low >= 0 && stop_clock > stop_low && rig.fake.pdn == FAKE_PDN_LOW,
                 "stopping mutes first (event %d) and stops the clock after (%d)", stop_low,
                 stop_clock);
    chorus_check(fake_amp_i2c_transactions(&rig.fake) == 0, "still no I2C transaction");
}

static void a_line_dac_refuses_what_it_cannot_run_from(void)
{
    /* 24-bit slots on the wire are 48 bit clocks a frame: the PCM5102A's PLL
     * runs from 32 or 64 (SLAS859C Table 11, p. 25). */
    rig_t rig;
    rig_init(&rig);
    rig.clock.wire_slot_bit_width = 24;
    chorus_amp_status_t status =
        chorus_line_dac_start(&rig.clock, &rig.stage, &rig.controller, &rig.report);
    chorus_check(status == CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED && rig.report.finding_count > 0 &&
                     strcmp(rig.report.findings[0].rule, "bclk-ratio-unsupported-by-line-dac") == 0,
                 "48 bit clocks a frame is refused as %s",
                 rig.report.finding_count > 0 ? rig.report.findings[0].rule : "<none>");
    chorus_check(fake_amp_count(&rig.fake, FAKE_EV_CLOCK_APPLIED) == 0 &&
                     rig.fake.pdn == FAKE_PDN_LOW,
                 "no clock, the mute low");
    chorus_check(chorus_line_dac_bck_ratio_ok(32) && chorus_line_dac_bck_ratio_ok(64) &&
                     !chorus_line_dac_bck_ratio_ok(48),
                 "32 and 64 bit clocks a frame are the PLL's, 48 is not");

    rig_t refused;
    rig_init(&refused);
    refused.fake.controller_refuses_clock = 1;
    status =
        chorus_line_dac_start(&refused.clock, &refused.stage, &refused.controller, &refused.report);
    chorus_check(status == CHORUS_AMP_CLOCK_REFUSED && refused.fake.pdn == FAKE_PDN_LOW,
                 "a controller that refuses the clock is %s, the mute low",
                 chorus_amp_status_name(status));

    rig_t dead;
    rig_init(&dead);
    dead.fake.stage_refuses_high_impedance = 1;
    status = chorus_line_dac_start(&dead.clock, &dead.stage, &dead.controller, &dead.report);
    chorus_check(status == CHORUS_AMP_OUTPUT_STAGE_REFUSED &&
                     fake_amp_count(&dead.fake, FAKE_EV_CLOCK_APPLIED) == 0,
                 "a mute that will not go low is %s and no clock starts",
                 chorus_amp_status_name(status));

    rig_t stuck;
    rig_init(&stuck);
    stuck.fake.stage_refuses_power_up = 1;
    (void)chorus_line_dac_start(&stuck.clock, &stuck.stage, &stuck.controller, &stuck.report);
    status = chorus_line_dac_unmute(&stuck.stage, &stuck.report);
    chorus_check(status == CHORUS_AMP_OUTPUT_STAGE_REFUSED && stuck.fake.pdn == FAKE_PDN_LOW,
                 "a mute that will not release is %s, driven low again",
                 chorus_amp_status_name(status));
}

int main(void)
{
    load_committed();

    chorus_section("the committed register map");
    the_committed_map_is_complete();

    chorus_section("the datasheet's startup procedure, as the simulated part saw it");
    the_committed_map_brings_the_datasheet_part_to_play();

    chorus_section("a part that does not answer");
    a_part_that_does_not_answer_leaves_the_stage_dead();

    chorus_section("a part that answers reporting a fault, or as another part");
    a_part_that_reports_a_fault_at_bring_up_starts_no_clock();

    chorus_section("a refusal after the clock");
    a_fault_or_a_refusal_after_the_clock_unwinds_in_order();

    chorus_section("a gain above the ceiling");
    a_gain_above_the_ceiling_is_refused_before_anything_happens();

    chorus_section("a register-map value declared unknown");
    an_unconfigured_register_refuses_by_name();

    chorus_section("a clock configuration that breaks a rule");
    a_refused_clock_configuration_never_reaches_the_controller();

    chorus_section("a fault during playback");
    a_fault_during_playback_stops_the_audio();

    chorus_section("the datasheet's shutdown procedure");
    a_shutdown_goes_hiz_waits_then_drops_pdn_and_the_clock();

    chorus_section("an output stage that will not go dead");
    a_stage_that_will_not_go_dead_stops_everything();

    chorus_section("the subwoofer's line DAC: no control bus, the mute on the power-down pin");
    a_line_dac_is_muted_clocked_then_unmuted_with_no_i2c();
    a_line_dac_refuses_what_it_cannot_run_from();

    return chorus_test_report("endpoint amplifier bring-up");
}
