/* What the endpoint publishes about itself.
 *
 * Three things, because those are the three that go wrong: the link, the sync
 * health, and the amplifier. The line format is `key=value` separated by
 * spaces, the same shape the Linux client's delay log uses, so one grep works
 * across both endpoints.
 *
 * The rule this file exists to enforce is AC-5's: an amplifier fault is
 * SURFACED, never swallowed. A published line always carries an `amp=` field,
 * and when the amplifier is in a fault the line says which fault by name and
 * says that audio stopped. There is no state of this struct that prints a
 * healthy line while a fault is set. */

#ifndef CHORUS_TELEMETRY_H
#define CHORUS_TELEMETRY_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/amp.h"

typedef enum {
    CHORUS_LINK_DOWN = 0,
    CHORUS_LINK_CONNECTING,
    CHORUS_LINK_UP
} chorus_link_state_t;

const char *chorus_link_state_name(chorus_link_state_t state);

typedef enum {
    /* Playing, or ready to. */
    CHORUS_AUDIO_RUNNING = 0,
    /* Stopped because the amplifier reported a fault or stopped answering.
     * docs/decisions/0015 records why a fault stops audio rather than only
     * being logged. */
    CHORUS_AUDIO_STOPPED_ON_AMP_FAULT,
    /* Stopped because there is nothing to play: the link is down. */
    CHORUS_AUDIO_IDLE
} chorus_audio_state_t;

const char *chorus_audio_state_name(chorus_audio_state_t state);

typedef struct {
    chorus_link_state_t link;
    /* How many times the link came back after going away. The first connection
     * is not a rejoin. */
    uint32_t rejoins;
    uint32_t connect_attempts;

    /* Sync health. `offset_known` is 0 until an exchange has completed, and a
     * line published then reads `none` rather than zero, because a bound of
     * zero is a claim and an absent bound is not. */
    int offset_known;
    int64_t offset_ns;
    uint64_t round_trip_ns;
    uint64_t bound_ns;
    uint32_t exchanges;

    /* Playout. */
    chorus_audio_state_t audio;
    uint64_t chunks_played;
    uint64_t frames_played;
    int have_sequence;
    uint32_t last_sequence;
    /* Frames of unassigned message types stepped over, which is not an error
     * and is counted so that it is visible when it happens. */
    uint64_t skipped_frames;

    /* The amplifier. */
    chorus_amp_status_t amp;
    int amp_fault_read;
    uint8_t amp_fault_bits;
} chorus_telemetry_t;

void chorus_telemetry_init(chorus_telemetry_t *telemetry);

/* Render one published line into `out`. */
void chorus_telemetry_line(const chorus_telemetry_t *telemetry, char *out, size_t out_len);

/* Record an amplifier report. A non-OK status stops audio, which is what makes
 * "rather than continue playing silently" true in the struct rather than only
 * in the caller. */
void chorus_telemetry_record_amp(chorus_telemetry_t *telemetry, const chorus_amp_report_t *report);

#endif /* CHORUS_TELEMETRY_H */
