#include "chorus/line_dac.h"

#include <stdio.h>
#include <string.h>

int chorus_line_dac_bck_ratio_ok(uint64_t bck_per_frame)
{
    return bck_per_frame == 32u || bck_per_frame == 64u;
}

/* The mute low (XSMT, on the power-down pin) and recorded as such. */
static int mute(chorus_output_stage_t *stage, chorus_amp_report_t *report)
{
    if (stage->high_impedance(stage->ctx) != 0) {
        report->output_in_high_impedance = 0;
        report->status = CHORUS_AMP_OUTPUT_STAGE_REFUSED;
        snprintf(report->detail, sizeof(report->detail),
                 "the line DAC's mute line could not be driven low; nothing further is safe");
        return -1;
    }
    report->output_in_high_impedance = 1;
    return 0;
}

chorus_amp_status_t chorus_line_dac_start(const chorus_i2s_clock_t *clock,
                                          chorus_output_stage_t *stage,
                                          chorus_i2s_controller_t *controller,
                                          chorus_amp_report_t *report)
{
    memset(report, 0, sizeof(*report));
    report->status = CHORUS_AMP_OK;

    /* 1. The mute low, before anything else. */
    if (mute(stage, report) != 0) {
        return report->status;
    }

    /* 2. The clock's own rules, and a ratio the DAC's PLL runs from. */
    const size_t capacity = sizeof(report->findings) / sizeof(report->findings[0]);
    chorus_i2s_validate_clock(clock, report->findings, capacity, &report->finding_count);
    uint64_t per_frame =
        (clock->sample_rate_hz == 0) ? 0 : chorus_i2s_bclk_hz(clock) / clock->sample_rate_hz;
    if (!chorus_line_dac_bck_ratio_ok(per_frame) && report->finding_count < capacity) {
        chorus_finding_t *f = &report->findings[report->finding_count++];
        snprintf(f->rule, sizeof(f->rule), "bclk-ratio-unsupported-by-line-dac");
        snprintf(f->detail, sizeof(f->detail),
                 "the I2S configuration gives %llu bit clocks per frame; the PCM5102A's PLL "
                 "runs from 32 or 64 (SLAS859C Table 11, p. 25)",
                 (unsigned long long)per_frame);
    }
    if (report->finding_count > 0) {
        snprintf(report->detail, sizeof(report->detail),
                 "the I2S clock configuration is refused (%.63s). No clock is started and the line "
                 "DAC stays muted.",
                 report->findings[0].rule);
        report->status = CHORUS_AMP_CLOCK_CONFIGURATION_REFUSED;
        return report->status;
    }

    /* 3. The clock, into a muted part: its PLL locks from BCK while XSMT is
     * low, and the writer's first frames reach a part that is already
     * running. */
    if (controller->apply_clock(controller->ctx, clock) != 0) {
        snprintf(report->detail, sizeof(report->detail),
                 "the I2S controller refused the clock configuration; the line DAC stays muted");
        report->status = CHORUS_AMP_CLOCK_REFUSED;
        return report->status;
    }
    report->clock_started = 1;
    snprintf(report->detail, sizeof(report->detail),
             "the line DAC (PCM5102A, no control bus: no register written and no fault "
             "register read) has its clock at %llu bit clocks per frame and is held muted until "
             "the playout writer runs",
             (unsigned long long)per_frame);
    return report->status;
}

chorus_amp_status_t chorus_line_dac_unmute(chorus_output_stage_t *stage,
                                           chorus_amp_report_t *report)
{
    if (stage->power_up(stage->ctx) != 0) {
        (void)mute(stage, report);
        report->status = CHORUS_AMP_OUTPUT_STAGE_REFUSED;
        snprintf(report->detail, sizeof(report->detail),
                 "the line DAC's mute line could not be released; it is driven low again");
        return report->status;
    }
    report->output_in_high_impedance = 0;
    report->status = CHORUS_AMP_OK;
    snprintf(report->detail, sizeof(report->detail),
             "the line DAC is unmuted (XSMT high, a 104-sample soft ramp, SLAS859C p. 16)");
    return report->status;
}

void chorus_line_dac_stop(chorus_output_stage_t *stage, chorus_i2s_controller_t *controller)
{
    (void)stage->high_impedance(stage->ctx);
    (void)controller->stop_clock(controller->ctx);
}
