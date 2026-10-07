/* The line DAC's bring-up: the subwoofer's output (`board_audio_output =
 * line-dac`, firmware/boards/devkitc-s3-pcm5102-sub.conf).
 *
 * The subwoofer feeds a bought plate amplifier at line level
 * (docs/hardware/subwoofer.md version 2), through an Adafruit 6250 breakout
 * carrying a TI PCM5102A. Every fact about the part below is from TI's
 * datasheet SLAS859C (May 2012, revised May 2015,
 * https://www.ti.com/lit/ds/symlink/pcm5102a.pdf, read 2026-10-07, sha256
 * a522083606b8e994875883215046fbdb92fd362996ca97719d259fbe9df07a85), page cited:
 *
 *   - It has no control bus: the PCM5102A is configured by its pins (FLT,
 *     DEMP, FMT, XSMT; the pin table, p. 5), so there is no register to write, no identity to
 *     read and no fault register to poll. The amplifier's sequencer
 *     (chorus/amp.h) does all three; this one does none.
 *   - It makes its own system clock from the bit clock: "if BCK and LRCK start
 *     correctly while SCK remains at ground level for 16 successive LRCK
 *     periods, then the internal PLL starts, automatically generating an
 *     internal SCK from the BCK reference" (p. 25), at 32 or 64 BCK per LRCK
 *     at 48 kHz (Table 11, p. 25). The board routes no MCLK; the clock's ratio
 *     is checked against those two before a clock starts.
 *   - XSMT, the breakout's MU pad, is its soft mute: "When the XSMT pin is
 *     shifted from high to low (3.3 V to 0 V), a soft digital attenuation ramp
 *     begins ... The soft attenuation ramp takes 104 samples", and from low to
 *     high "a soft digital un-mute is started ... The un-mute takes 104
 *     samples" (p. 16). The board wires it to pin_amp_power_down, so the
 *     output stage's two calls keep their meaning: high_impedance drives it
 *     low (muted, the outputs at ground) and power_up drives it high.
 *   - Its normal interpolation filter (FLT low, the breakout's default) has a
 *     group delay of 22 tS (Table 4, p. 17): 22 frames at any rate, the
 *     profile's board_output_delay_frames. The datasheet's summary table says
 *     "Normal 8x oversampling digital filter latency 20tS" (p. 4); the
 *     filter's own table, the specific one, is taken, and the two frames
 *     between them (42 us at 48 kHz) are what a bench reading settles.
 *
 * The order: the mute low before anything; the clock's ratio checked; the
 * clock started into a muted part; the mute held low until the playout writer
 * runs, then released (chorus_line_dac_unmute). Stopping is the mute first,
 * the clock after it. A report is the amplifier's (chorus_amp_report_t), so
 * telemetry and the console read either output one way; no fault register is
 * ever read, and the report says so.
 *
 * Pure and host-graded (firmware/tests/test_amp.c, on the same simulated
 * output stage and controller as the amplifier). */

#ifndef CHORUS_LINE_DAC_H
#define CHORUS_LINE_DAC_H

#include <stdint.h>

#include "chorus/amp.h"
#include "chorus/i2s.h"

/* The PCM5102A's normal x8 interpolation filter's group delay, in frames
 * (SLAS859C Table 4, p. 17: "Filter group delay 22 tS"). A datasheet figure,
 * not measured here; endpoint.conf's validation holds a line-dac profile's
 * board_output_delay_frames to it. */
#define CHORUS_LINE_DAC_GROUP_DELAY_FRAMES 22u

/* Whether `bck_per_frame` is a ratio the PCM5102A's PLL runs from (SLAS859C
 * Table 11, p. 25: 32 or 64). */
int chorus_line_dac_bck_ratio_ok(uint64_t bck_per_frame);

/* The mute low, the clock's rules, the clock on; the part is left muted.
 * CHORUS_AMP_OK, or the amplifier's status for the same failure:
 * OUTPUT_STAGE_REFUSED, CLOCK_CONFIGURATION_REFUSED (with findings) or
 * CLOCK_REFUSED. Every failure leaves the mute low and no clock running. */
chorus_amp_status_t chorus_line_dac_start(const chorus_i2s_clock_t *clock,
                                          chorus_output_stage_t *stage,
                                          chorus_i2s_controller_t *controller,
                                          chorus_amp_report_t *report);

/* The mute released, once the playout writer runs. On failure the mute is
 * driven low again and OUTPUT_STAGE_REFUSED is reported. */
chorus_amp_status_t chorus_line_dac_unmute(chorus_output_stage_t *stage,
                                           chorus_amp_report_t *report);

/* The mute low, then the clock stopped. */
void chorus_line_dac_stop(chorus_output_stage_t *stage, chorus_i2s_controller_t *controller);

#endif /* CHORUS_LINE_DAC_H */
