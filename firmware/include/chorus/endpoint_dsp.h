/* The endpoint's sound chain in the playout path (goal 12, done-when line C;
 * docs/decisions/, "the DSP chain on the endpoints").
 *
 * firmware/include/chorus/dsp.h is the library: pure blocks and one chain,
 * held to the shared fixtures. This unit is what puts that chain between the
 * jitter buffer and the I2S write, and decides what configures it:
 *
 *   - the room's `sound` (docs/protocol.md, "0x39 sound"), as the session
 *     decodes it: tone, loudness, night, speech, room EQ, and the endpoint's
 *     role in the room's bonded set with the set's crossover, sub level and
 *     sub polarity;
 *   - this endpoint's own two-way split, from firmware/config/endpoint.conf
 *     (`two_way*`), which is the speaker's drivers and not the room's;
 *   - the subwoofer class's level and phase knobs (goal 9, ADR 0063), which
 *     are local sound settings beside the catalog's and combine with them;
 *   - the stream's rate and channel map, from stream_format.
 *
 * When it runs. The chain ENGAGES when the endpoint knows its stream's layout
 * and either a `sound` has arrived or a two-way is configured. Until then the
 * playout path is byte for byte what it was before goal 12 (the integer room
 * gain, no added latency), so an endpoint whose server sends no `sound` plays
 * exactly as it did. Once engaged it stays engaged: a new stream re-initialises
 * the chain for its rate and map, a new `sound` reconfigures it in place
 * (chorus_dsp_chain_set_sound keeps running filters' state), and a setting the
 * library refuses keeps the previous one and is counted. A chain the library
 * cannot build for a stream (a two-way crossover above 0.45 x a low rate) is
 * not engaged and the endpoint plays as before, the refusal counted: never
 * silence, never a path without the room's gain.
 *
 * The room gain goes INTO the chain (the library's order: everything, then
 * the gain, then the limiter at min(1, the effective limit), so no DSP boost
 * lifts a room above its limit, K81, I10). The ramped gain is the volume
 * unit's (chorus/volume.h), read at the first frame of each sub-block of at
 * most CHORUS_ENDPOINT_DSP_BLOCK_FRAMES and advanced over it, so a ramp is a
 * staircase of 32-frame steps (0.67 ms at 48 kHz) while it moves. The
 * effective limit is the least of the room's last limit and this endpoint's
 * own ceiling (`max_volume`).
 *
 * The chain adds a fixed latency, the limiter's look-ahead
 * (chorus_dsp_chain_latency_frames, 2 ms). The playout path adds it to the
 * device delay its sync error is formed from, so content is written that much
 * earlier and is heard at the sync target; the Linux client's sink reports the
 * same latency on top of its device's delay. Both endpoints account it one way.
 *
 * Pure and host-graded: no ESP-IDF header, no allocation, no lock (the
 * playout path holds its own around every call). The object is large (the
 * chain is a 72680-byte static object, chorus/dsp.h) and is one static per
 * image. firmware/tests/test_endpoint_dsp.c grades it alone and through the
 * playout path; firmware/tests/dsp-session.sh grades it end to end against
 * the real server. */

#ifndef CHORUS_ENDPOINT_DSP_H
#define CHORUS_ENDPOINT_DSP_H

#include <stdbool.h>
#include <stdint.h>

#include "chorus/dsp.h"
#include "chorus/protocol_v2.h"
#include "chorus/volume.h"

/* The most frames one chain call takes, which is also the step of a gain
 * ramp through the chain. ASSUMED: 32 frames, short enough that a ramp's
 * staircase is inaudible and long enough that the per-call work (the loudness
 * attenuation, a log10) is a small share of the block. */
#define CHORUS_ENDPOINT_DSP_BLOCK_FRAMES 32u

/* The I2S slots a frame carries (chorus/playout.h's CHORUS_PLAYOUT_MAX_CHANNELS). */
#define CHORUS_ENDPOINT_DSP_SLOTS 2u

/* The subwoofer knobs' ranges (chorus/controls.h): a cut only, in tenths of a
 * dB, and the phase in degrees. */
#define CHORUS_ENDPOINT_DSP_KNOB_LEVEL_MIN_TENTHS (-120)
#define CHORUS_ENDPOINT_DSP_KNOB_PHASE_MAX_DEG 180

/* The phase knob is quantised to a polarity: at or past this many degrees the
 * sub's feed is inverted. ASSUMED: 90, the midpoint, until a variable-phase
 * all-pass is designed (a follow-up); 0 and 180, the knob's ends, are then
 * exactly right. */
#define CHORUS_ENDPOINT_DSP_KNOB_INVERT_DEG 90

/* The endpoint's two-way, as endpoint.conf declares it. */
typedef struct {
    bool enabled;
    uint32_t crossover_hz;
    /* Which I2S slot (0 or 1) each driver's amplifier channel is on. */
    uint8_t woofer_slot;
    uint8_t tweeter_slot;
} chorus_endpoint_two_way_t;

typedef struct {
    chorus_dsp_chain_t chain;
    chorus_dsp_endpoint_t endpoint;
    uint8_t woofer_slot;
    uint8_t tweeter_slot;

    /* The last `sound`, as it arrived. */
    bool have_sound;
    chorus_v2_sound_t wire;
    /* The subwoofer knobs. */
    int32_t knob_level_tenths_db;
    int32_t knob_phase_deg;

    /* The stream's layout, from stream_format. */
    bool have_stream;
    uint32_t rate_hz;
    uint32_t channels;
    uint8_t map[CHORUS_DSP_MAX_CHANNELS];

    /* Whether the chain is in the path (see the top of this file). */
    bool engaged;

    /* Counters: settings the chain took, and settings or streams it
     * refused, with the field the last refusal named. */
    uint32_t sounds_applied;
    uint32_t refusals;
    const char *last_refused;

    float in[CHORUS_ENDPOINT_DSP_BLOCK_FRAMES * CHORUS_DSP_MAX_CHANNELS];
    float out[CHORUS_ENDPOINT_DSP_BLOCK_FRAMES * CHORUS_DSP_MAX_OUTPUTS];
} chorus_endpoint_dsp_t;

/* The wire's `sound` and the knobs as the chain's settings: every field one
 * for one, the flags bit by bit, and the knobs combined with the catalog's
 * sub level and polarity (level: the sum, held to the wire's -12..+6 dB;
 * polarity: inverted when exactly one of the catalog and the knob says so). */
void chorus_endpoint_dsp_settings(const chorus_v2_sound_t *wire, int32_t knob_level_tenths_db,
                                  int32_t knob_phase_deg, chorus_dsp_sound_t *out);

/* Start disengaged, with no stream and no sound. `two_way` NULL or disabled
 * is a full-range endpoint. */
void chorus_endpoint_dsp_init(chorus_endpoint_dsp_t *d, const chorus_endpoint_two_way_t *two_way);

/* The stream's rate and channel positions (stream_format). A new stream is a
 * new chain. */
void chorus_endpoint_dsp_set_stream(chorus_endpoint_dsp_t *d, uint32_t rate_hz, uint32_t channels,
                                    const uint8_t *map);

/* A `sound` from the session (already held to its ranges by the decoder). */
void chorus_endpoint_dsp_set_sound(chorus_endpoint_dsp_t *d, const chorus_v2_sound_t *wire);

/* The subwoofer knobs: level -120..0 tenths of a dB, phase 0..180 degrees;
 * values past either end are taken at the end. */
void chorus_endpoint_dsp_set_sub_knobs(chorus_endpoint_dsp_t *d, int32_t level_tenths_db,
                                       int32_t phase_deg);

/* Frames every output is late by while engaged (the limiter's look-ahead);
 * 0 while disengaged. */
uint32_t chorus_endpoint_dsp_latency_frames(const chorus_endpoint_dsp_t *d);

/* Run `frames` frames of an I2S block (CHORUS_ENDPOINT_DSP_SLOTS slots of
 * `sample_bytes` bytes, little-endian, signed) through the chain in place,
 * with the room's gain and limit from `volume`, which is advanced over them.
 * The chain's input is the stream's channels (a mono stream from slot 0); its
 * outputs go back onto the slots: one output on both, a two-way's woofer and
 * tweeter on their configured slots, two outputs otherwise on slots 0 and 1.
 * Only called while engaged. */
void chorus_endpoint_dsp_process(chorus_endpoint_dsp_t *d, uint8_t *block, uint32_t frames,
                                 uint8_t sample_bytes, chorus_volume_t *volume);

#endif /* CHORUS_ENDPOINT_DSP_H */
