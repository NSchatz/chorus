/* The room's volume, enforced on the endpoint (goal 11, K81, I10, brief
 * section 4.8; docs/decisions/0074-room-volume-on-the-audio-wire.md).
 *
 * The server sends `room_volume` (docs/protocol.md, "0x38 room volume"): a
 * gain, the room's effective limit and a ramp. What this endpoint plays at,
 * at every frame, is
 *
 *     applied = min(ramped gain, last limit received, this endpoint's ceiling)
 *
 * so a gain above the limit plays at the limit, and nothing on the wire can
 * take the endpoint above `max_volume` in firmware/config/endpoint.conf. The
 * limit and the ceiling are applied at once; only the gain is ramped, linear
 * in amplitude, from the gain being applied when the message arrived to the
 * gain it asks for. The ramp is counted in FRAMES at the stream's rate, which
 * is the endpoint's monotonic clock by construction (a frame is written for
 * every 1/rate of the DMA's own clock), so this unit reads no clock at all.
 *
 * Integer only. Gains are Q16 fractions of full amplitude (65536 is unity),
 * so a thousandth converts exactly at both ends (0 is 0, 1000 is 65536) and
 * unity is the identity on every sample, bit for bit: the existing playout
 * tests, which read a frame counter back out of the PCM, see no change. A
 * scaled sample is truncated toward zero, so it is within one unit in the
 * last place of the exact product, in the direction of silence: the rule the
 * Linux client's zone gain follows (crates/client-linux/src/zone.rs).
 *
 * A gain never changes how many frames are written. Apply scales the frames
 * it is handed in place and skip advances the ramp over frames that are
 * silence anyway; neither adds nor removes one.
 *
 * Pure and host-graded: no ESP-IDF header, no allocation, no lock (the
 * playout path holds its own around every call). firmware/tests/test_volume.c
 * grades it, alone and inside the playout path. */

#ifndef CHORUS_VOLUME_H
#define CHORUS_VOLUME_H

#include <stddef.h>
#include <stdint.h>

/* Full amplitude in the wire's unit, thousandths (docs/protocol.md). */
#define CHORUS_VOLUME_FULL 1000u

/* Full amplitude in the unit samples are scaled by: Q16. */
#define CHORUS_VOLUME_UNITY 65536u

/* The endpoint's own ceiling when its configuration names none: full scale,
 * so an endpoint behaves as it did before this unit existed. ASSUMED: a
 * policy default, not a measurement (ADR 0074); an owner who wants a speaker
 * never to play above some level sets `max_volume` below it. */
#define CHORUS_VOLUME_DEFAULT_CEILING CHORUS_VOLUME_FULL

typedef struct {
    /* The endpoint's ceiling and the last limit received, Q16. Before any
     * room_volume the limit is unity: the ceiling is the only bound. */
    uint32_t ceiling_q16;
    uint32_t limit_q16;
    /* The ramp: from `from_q16` to `to_q16` over `ramp_frames` frames, of
     * which `done_frames` have been written. `ramp_q16` is the ramped gain
     * at the next frame, kept by a remainder accumulator (`step_q16`,
     * `step_rem`, `acc`) that reproduces from + (to - from) * k / N exactly,
     * truncated, at every k, with no division per frame. */
    uint32_t from_q16;
    uint32_t to_q16;
    uint32_t ramp_q16;
    uint64_t ramp_frames;
    uint64_t done_frames;
    uint32_t step_q16;
    uint64_t step_rem;
    uint64_t acc;
    /* room_volume messages taken, for a status line and the test. */
    uint32_t messages;
} chorus_volume_t;

/* A thousandths value as Q16: exact at 0 and at 1000, and never above unity
 * (a value above 1000 is taken as 1000). */
uint32_t chorus_volume_q16_from_thousandths(uint32_t thousandths);

/* Start at the ceiling, with no limit received yet. The startup gain is the
 * ceiling (ADR 0074): an endpoint whose server never sends room_volume plays
 * as it did before goal 11, bounded by its own ceiling. A ceiling above 1000
 * is taken as 1000. */
void chorus_volume_init(chorus_volume_t *v, uint32_t ceiling_thousandths);

/* A room_volume, decoded and validated (chorus/protocol_v2.h). The new ramp
 * starts from the gain being applied now, after the clamp, so what is heard
 * never jumps up; the limit applies from the next frame. `rate_hz` turns
 * `ramp_ms` into frames; a ramp of 0 frames is a step. Values above their
 * wire ranges are taken at the range's end: never reached from the decoder,
 * which rejects them, and safe if a caller ever passed one. */
void chorus_volume_set(chorus_volume_t *v, uint32_t gain_thousandths, uint32_t limit_thousandths,
                       uint32_t ramp_ms, uint32_t rate_hz);

/* What the next frame plays at, Q16: min(ramped gain, limit, ceiling). */
uint32_t chorus_volume_applied_q16(const chorus_volume_t *v);

/* The same in thousandths, to the nearest, for a status line. */
uint32_t chorus_volume_applied_thousandths(const chorus_volume_t *v);

/* Whether a ramp is still moving. */
int chorus_volume_ramping(const chorus_volume_t *v);

/* Advance the ramp over `frames` frames that are not scaled (silence the
 * playout path writes anyway: an insertion, an underrun, the hold before the
 * loop acquires). Time passes for the ramp whether or not there is audio. */
void chorus_volume_skip(chorus_volume_t *v, uint64_t frames);

/* Scale `frames` frames in place, each by the gain applied at that frame,
 * and advance the ramp by them. The layout is the I2S buffer's
 * (chorus/playout.h): `channels` slots per frame, each `sample_bytes` bytes
 * (2, 3 or 4) little-endian and signed. Unity with no ramp moving touches no
 * byte; zero writes zeros. The frame count is never changed. */
void chorus_volume_apply(chorus_volume_t *v, uint8_t *pcm, uint32_t frames, uint8_t channels,
                         uint8_t sample_bytes);

/* `max_volume` as endpoint.conf writes it: a decimal from 0 to 1 with at most
 * three places (`1.000`, `0.5`, `0`). Returns 0 and sets `*thousandths`, or
 * -1 for anything else (a sign, an exponent, a fourth place, above 1). */
int chorus_volume_parse(const char *text, uint32_t *thousandths);

#endif /* CHORUS_VOLUME_H */
