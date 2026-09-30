/* What the endpoint's decoders cost, timed where they run.
 *
 * Goal 6 measured the host's decode cost (docs/measurements/
 * codec-decode-cost-host.md, source host) and could not measure the S3's:
 * that needs an endpoint command. This is it. A fixture from fixtures/codec
 * (NAME.fields and NAME.chunks, the same files firmware/tests/test_codec.c and
 * the Linux client read) is decoded chunk by chunk through the endpoint's own
 * codec seam, the whole decode is timed on the monotonic clock, and the result
 * says how many frames per second that is and what fraction of one core real
 * time playback would take. The decode is also hashed and compared with the
 * fixture's `decode_fnv1a64`, so a figure is only ever about a decode that
 * produced the right audio.
 *
 * A figure taken this way on an ESP32-S3 is a hardware measurement only when
 * the bench script (tools/decode-cost-run.sh) takes it and writes its report;
 * on a host it is host evidence and nothing more. */

#ifndef CHORUS_DECODE_COST_H
#define CHORUS_DECODE_COST_H

#include <stddef.h>
#include <stdint.h>

typedef uint64_t (*chorus_clock_fn)(void);

/* One fixture, as the image carries it. */
typedef struct {
    const char *name;
    const char *fields;
    size_t fields_len;
    const uint8_t *chunks;
    size_t chunks_len;
} chorus_decode_fixture_t;

typedef struct {
    uint8_t codec;
    uint32_t sample_rate_hz;
    uint8_t channels;
    uint32_t chunks;
    uint64_t frames;
    uint64_t elapsed_ns;
    /* Decoded frames per second of wall clock on this core. */
    double frames_per_s;
    /* Seconds of decoding per second of audio: 0.25 means real-time playback
     * would take a quarter of one core. */
    double cpu_fraction;
    /* Whether the decode hashed to the fixture's decode_fnv1a64. */
    int decode_matches;
} chorus_decode_cost_t;

/* Decode `fixture` once, timed on `now_ns`. Returns 0 with `out` filled, or
 * -1 with `detail` naming what was wrong (a fixture that does not parse, a
 * chunk that does not decode, no memory). */
int chorus_decode_cost_run(const chorus_decode_fixture_t *fixture, chorus_clock_fn now_ns,
                           chorus_decode_cost_t *out, char *detail, size_t detail_len);

#endif /* CHORUS_DECODE_COST_H */
