/* The room's volume on the endpoint, graded on a host (goal 11, K81, I10;
 * docs/decisions/0074-room-volume-on-the-audio-wire.md).
 *
 * firmware/src/volume.c alone first: the conversion, the clamp to the limit
 * and to the ceiling, the ramp's two endpoints and its monotonicity (checked
 * frame by frame against the closed form from + (to - from) * k / N), the
 * skip that advances it over silence, mute, and the configuration's parse.
 * Then through the playout path (firmware/src/playout.c), on the frames the
 * writer hands the I2S driver: frames in == frames out whatever the gain,
 * and the committed vector fixtures/protocol/v2/room_volume_above_limit.hex,
 * decoded by the endpoint's own decoder, plays at its limit. And the
 * committed endpoint.conf carries a max_volume the reader takes.
 *
 * And the server's own sequence: fixtures/volume/room-volume-sequence.hex is
 * every room_volume the real chorus-server sent one player while a test drove
 * every volume path (crates/server/tests/limits_hold_for_every_volume_path.rs
 * captured it), fed here through this endpoint's decoder and volume path, so
 * both endpoint kinds are shown on the same sequence: at every message the
 * applied gain is min(gain, limit, ceiling), never above the limit.
 *
 * Deterministic: no clock, no sleep. Every number here is arithmetic on
 * committed inputs, not a timing claim (BRIEF section 3.1 rule 3). */

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "chorus/endpoint_config.h"
#include "chorus/playout.h"
#include "chorus/protocol.h"
#include "chorus/protocol_v2.h"
#include "chorus/sync_conf.h"
#include "chorus/volume.h"
#include "fixture_text.h"
#include "harness.h"

#define RATE 48000u
#define OUT_BYTES 3u
#define FRAME_OUT (2u * OUT_BYTES)
#define CHUNK_FRAMES 960u

/* --- helpers ------------------------------------------------------------------ */

static uint32_t q(uint32_t thousandths)
{
    return chorus_volume_q16_from_thousandths(thousandths);
}

/* A signed slot of `bytes` bytes, little-endian. */
static int32_t slot(const uint8_t *s, uint8_t bytes)
{
    uint32_t raw = 0;
    for (uint8_t i = 0; i < bytes; i++) {
        raw |= (uint32_t)s[i] << (8u * i);
    }
    int64_t v = (int64_t)raw;
    if ((raw >> (8u * bytes - 1u)) & 1u) {
        v -= (int64_t)1 << (8u * bytes);
    }
    return (int32_t)v;
}

static void put_slot(uint8_t *s, uint8_t bytes, int32_t value)
{
    uint32_t v = (uint32_t)value;
    for (uint8_t i = 0; i < bytes; i++) {
        s[i] = (uint8_t)(v >> (8u * i));
    }
}

/* What a sample scaled by `g` (Q16) must be: truncated toward zero. */
static int32_t expect(int32_t value, uint32_t g)
{
    return (int32_t)((int64_t)value * (int64_t)g / (int64_t)CHORUS_VOLUME_UNITY);
}

/* The ramp's closed form at frame k of N, from `from` to `to`. */
static uint32_t closed_form(uint32_t from, uint32_t to, uint64_t k, uint64_t n)
{
    if (k >= n) {
        return to;
    }
    uint64_t span = (to > from) ? to - from : from - to;
    uint32_t moved = (uint32_t)(span * k / n);
    return (to > from) ? from + moved : from - moved;
}

/* --- the unit alone ------------------------------------------------------------ */

static void test_conversion_and_parse(void)
{
    chorus_section("thousandths to Q16, and max_volume as endpoint.conf writes it");
    chorus_check(q(0) == 0 && q(1000) == CHORUS_VOLUME_UNITY && q(1001) == CHORUS_VOLUME_UNITY,
                 "0 is 0, 1000 is unity (%u) exactly, above 1000 is unity", q(1000));
    int monotone = 1;
    for (uint32_t t = 1; t <= 1000; t++) {
        if (q(t) < q(t - 1)) {
            monotone = 0;
        }
    }
    chorus_check(monotone, "the conversion is monotone over 0 to 1000");

    static const struct {
        const char *text;
        int ok;
        uint32_t value;
    } cases[] = {
        {"1.000", 1, 1000}, {"1", 1, 1000},  {"0", 1, 0},     {"0.5", 1, 500},
        {"0.25", 1, 250},   {"0.001", 1, 1}, {"1.001", 0, 0}, {"0.0001", 0, 0},
        {"-0.5", 0, 0},     {"2", 0, 0},     {"1.", 0, 0},    {".5", 0, 0},
        {"0.5x", 0, 0},     {"1e0", 0, 0},   {"", 0, 0},      {"unknown", 0, 0},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        uint32_t got = 12345;
        int rc = chorus_volume_parse(cases[i].text, &got);
        int ok = cases[i].ok ? (rc == 0 && got == cases[i].value) : (rc != 0 && got == 12345);
        if (cases[i].ok) {
            chorus_check(ok, "max_volume = '%s' is taken as %u thousandths", cases[i].text, got);
        } else {
            chorus_check(ok, "max_volume = '%s' is refused", cases[i].text);
        }
    }
}

static void test_startup_and_clamps(void)
{
    chorus_section("the startup gain, and the clamp to the limit and to the ceiling");
    chorus_volume_t v;
    chorus_volume_init(&v, CHORUS_VOLUME_DEFAULT_CEILING);
    chorus_check(chorus_volume_applied_q16(&v) == CHORUS_VOLUME_UNITY && !chorus_volume_ramping(&v),
                 "before any room_volume, at the default ceiling 1.000: unity (as before goal 11)");
    uint8_t pcm[2 * FRAME_OUT];
    put_slot(pcm + 0, OUT_BYTES, 8388607);
    put_slot(pcm + 3, OUT_BYTES, -8388608);
    put_slot(pcm + 6, OUT_BYTES, -1);
    put_slot(pcm + 9, OUT_BYTES, 12345);
    uint8_t before[sizeof(pcm)];
    memcpy(before, pcm, sizeof(pcm));
    chorus_volume_apply(&v, pcm, 2, 2, OUT_BYTES);
    chorus_check(memcmp(pcm, before, sizeof(pcm)) == 0,
                 "unity is the identity on every byte, full scale included");

    chorus_volume_init(&v, 500);
    chorus_check(chorus_volume_applied_q16(&v) == q(500),
                 "with max_volume 0.500 and no room_volume yet: the ceiling, %u",
                 chorus_volume_applied_q16(&v));

    /* The limit. */
    chorus_volume_init(&v, 1000);
    chorus_volume_set(&v, 900, 600, 0, RATE);
    chorus_check(chorus_volume_applied_q16(&v) == q(600) &&
                     chorus_volume_applied_thousandths(&v) == 600u,
                 "gain 900 under limit 600: plays at the limit, %u (Q16) = %u thousandths",
                 chorus_volume_applied_q16(&v), chorus_volume_applied_thousandths(&v));
    chorus_volume_set(&v, 300, 600, 0, RATE);
    chorus_check(chorus_volume_applied_q16(&v) == q(300), "gain 300 under limit 600: the gain");
    chorus_volume_set(&v, 300, 100, 0, RATE);
    chorus_check(chorus_volume_applied_q16(&v) == q(100),
                 "a limit lowered to 100 below the gain 300 applies at once");

    /* The ceiling. Nothing on the wire raises it. */
    chorus_volume_init(&v, 300);
    chorus_volume_set(&v, 1000, 1000, 0, RATE);
    chorus_check(chorus_volume_applied_q16(&v) == q(300),
                 "max_volume 0.300 and room_volume gain 1000 limit 1000: the ceiling, %u",
                 chorus_volume_applied_q16(&v));
    chorus_volume_set(&v, 1000, 200, 0, RATE);
    chorus_check(chorus_volume_applied_q16(&v) == q(200),
                 "under the ceiling, a lower limit wins: min(1000, 200, 300) = 200");

    /* The scaled samples are the products, truncated toward zero. */
    chorus_volume_init(&v, 1000);
    chorus_volume_set(&v, 900, 600, 0, RATE);
    put_slot(pcm + 0, OUT_BYTES, 8388607);
    put_slot(pcm + 3, OUT_BYTES, -8388608);
    put_slot(pcm + 6, OUT_BYTES, -1);
    put_slot(pcm + 9, OUT_BYTES, 12345);
    chorus_volume_apply(&v, pcm, 2, 2, OUT_BYTES);
    int32_t got[4] = {slot(pcm, 3), slot(pcm + 3, 3), slot(pcm + 6, 3), slot(pcm + 9, 3)};
    chorus_check(got[0] == expect(8388607, q(600)) && got[1] == expect(-8388608, q(600)) &&
                     got[2] == 0 && got[3] == expect(12345, q(600)),
                 "s24 slots at the limit: %" PRId32 " %" PRId32 " %" PRId32 " %" PRId32
                 " (truncated toward zero; the extremes keep their sign)",
                 got[0], got[1], got[2], got[3]);
    uint8_t s16[4];
    put_slot(s16, 2, -32768);
    put_slot(s16 + 2, 2, 32767);
    chorus_volume_apply(&v, s16, 1, 2, 2);
    uint8_t s32[8];
    put_slot(s32, 4, INT32_MIN);
    put_slot(s32 + 4, 4, INT32_MAX);
    chorus_volume_apply(&v, s32, 1, 2, 4);
    chorus_check(
        slot(s16, 2) == expect(-32768, q(600)) && slot(s16 + 2, 2) == expect(32767, q(600)) &&
            slot(s32, 4) == expect(INT32_MIN, q(600)) &&
            slot(s32 + 4, 4) == expect(INT32_MAX, q(600)),
        "2-byte and 4-byte slots too, never wrapping at either extreme (%" PRId32 ", %" PRId32 ")",
        slot(s32, 4), slot(s32 + 4, 4));
}

static void test_ramp(void)
{
    chorus_section("the ramp: linear in amplitude, its endpoints exact, monotone, in frames");
    chorus_volume_t v;
    chorus_volume_init(&v, 1000);
    /* Down from unity to 0.200 over 10 ms = 480 frames at 48 kHz. */
    chorus_volume_set(&v, 200, 1000, 10, RATE);
    uint64_t n = v.ramp_frames;
    chorus_check(n == 480u && chorus_volume_ramping(&v),
                 "ramp_ms 10 at %u Hz is %" PRIu64 " frames", RATE, n);
    uint32_t first = chorus_volume_applied_q16(&v);
    int exact = 1;
    int monotone = 1;
    uint32_t prev = first;
    static uint8_t frame[FRAME_OUT];
    for (uint64_t k = 0; k <= n + 5; k++) {
        uint32_t g = chorus_volume_applied_q16(&v);
        if (g != closed_form(q(1000), q(200), k, n)) {
            exact = 0;
        }
        if (g > prev) {
            monotone = 0;
        }
        prev = g;
        memset(frame, 0x11, sizeof(frame));
        chorus_volume_apply(&v, frame, 1, 2, OUT_BYTES);
    }
    chorus_check(first == CHORUS_VOLUME_UNITY,
                 "the first frame plays at the gain being applied when it arrived (unity)");
    chorus_check(exact, "every frame k is from + (to - from) * k / N exactly (truncated)");
    chorus_check(monotone, "non-increasing at every frame on the way down");
    chorus_check(chorus_volume_applied_q16(&v) == q(200) && !chorus_volume_ramping(&v),
                 "after N frames it is the target %u exactly, and stays there", q(200));

    /* Up from silence over 1 s (an alarm's ramp, shortened). */
    chorus_volume_init(&v, 1000);
    chorus_volume_set(&v, 0, 1000, 0, RATE);
    chorus_check(chorus_volume_applied_q16(&v) == 0, "a step to 0 (mute) is at once");
    chorus_volume_set(&v, 1000, 1000, 1000, RATE);
    n = v.ramp_frames;
    exact = 1;
    monotone = 1;
    prev = 0;
    for (uint64_t k = 0; k <= n; k++) {
        uint32_t g = chorus_volume_applied_q16(&v);
        if (g != closed_form(0, q(1000), k, n)) {
            exact = 0;
        }
        if (g < prev) {
            monotone = 0;
        }
        prev = g;
        memset(frame, 0x11, sizeof(frame));
        chorus_volume_apply(&v, frame, 1, 2, OUT_BYTES);
    }
    chorus_check(exact && monotone && prev == CHORUS_VOLUME_UNITY && n == RATE,
                 "up from 0 to unity over %" PRIu64 " frames: exact, non-decreasing, ending at "
                 "unity",
                 n);

    /* Skipping over silence lands where the frames would have. */
    chorus_volume_t a;
    chorus_volume_t b;
    chorus_volume_init(&a, 1000);
    chorus_volume_set(&a, 333, 1000, 250, RATE);
    b = a;
    static uint8_t block[777 * FRAME_OUT];
    chorus_volume_apply(&a, block, 777, 2, OUT_BYTES);
    chorus_volume_skip(&b, 777);
    chorus_check(a.ramp_q16 == b.ramp_q16 && a.done_frames == b.done_frames && a.acc == b.acc,
                 "skip(777) and 777 scaled frames leave the ramp in the same place (%u)",
                 a.ramp_q16);
    chorus_volume_skip(&b, 1000000);
    chorus_check(b.ramp_q16 == q(333) && !chorus_volume_ramping(&b),
                 "a long silence ends the ramp on its target");

    /* A ramp under a lower limit is capped all the way, and a new message
     * starts from what is heard. */
    chorus_volume_init(&v, 1000);
    chorus_volume_set(&v, 800, 400, 0, RATE);
    chorus_volume_set(&v, 1000, 400, 100, RATE);
    int capped = 1;
    for (int k = 0; k < 4800; k++) {
        if (chorus_volume_applied_q16(&v) != q(400)) {
            capped = 0;
        }
        chorus_volume_skip(&v, 1);
    }
    chorus_check(capped, "a ramp up under limit 400 never plays above 400");
    chorus_volume_set(&v, 0, 400, 100, RATE);
    chorus_check(v.from_q16 == q(400) && chorus_volume_applied_q16(&v) == q(400),
                 "a fade starts from what is heard (the limit, %u), not from the gain held "
                 "above it",
                 v.from_q16);
    chorus_volume_set(&v, 1000, 1000, 60000, RATE);
    chorus_check(v.ramp_frames == 60u * RATE, "the longest ramp, 60 s, is %" PRIu64 " frames",
                 v.ramp_frames);
}

static void test_mute(void)
{
    chorus_section("mute: zeros, and the same number of frames");
    chorus_volume_t v;
    chorus_volume_init(&v, 1000);
    chorus_volume_set(&v, 0, 1000, 0, RATE);
    static uint8_t pcm[CHUNK_FRAMES * FRAME_OUT];
    memset(pcm, 0x5A, sizeof(pcm));
    chorus_volume_apply(&v, pcm, CHUNK_FRAMES, 2, OUT_BYTES);
    int zero = 1;
    for (size_t i = 0; i < sizeof(pcm); i++) {
        if (pcm[i] != 0) {
            zero = 0;
        }
    }
    chorus_check(zero, "a muted room's %u frames are every byte zero", CHUNK_FRAMES);
    chorus_volume_set(&v, 1000, 0, 0, RATE);
    chorus_check(chorus_volume_applied_q16(&v) == 0, "a limit of 0 silences a gain of 1000");
}

/* --- through the playout path ------------------------------------------------- */

static uint64_t fake_now;
static uint64_t fake_clock(void)
{
    return fake_now;
}

static chorus_sync_conf_t sync_conf;
static uint8_t ring_storage[RATE * 300u / 1000u * FRAME_OUT];
static chorus_playout_chunk_t chunk_storage[128];

/* A stereo s24le chunk: left +A, right -A. */
static void constant_chunk(uint8_t *pcm, int32_t a)
{
    for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
        put_slot(pcm + f * 6, 3, a);
        put_slot(pcm + f * 6 + 3, 3, -a);
    }
}

static int setup(chorus_playout_t *p, uint32_t ceiling)
{
    chorus_playout_config_t c =
        chorus_playout_config_from(&sync_conf, RATE, 24, 240, CHORUS_PLAYOUT_BUFFER_MS);
    c.max_volume_thousandths = ceiling;
    fake_now = 1000000000ull;
    if (chorus_playout_init(p, &c, ring_storage, chunk_storage, fake_clock) != 0) {
        return -1;
    }
    p->acquired = 1; /* placed on the timeline, as by a zero-error tick */
    return 0;
}

static void test_playout_path(void)
{
    chorus_section("the playout path: frames in == frames out, the gain on what is written");
    static chorus_playout_t p;
    chorus_playout_config_t bad =
        chorus_playout_config_from(&sync_conf, RATE, 24, 240, CHORUS_PLAYOUT_BUFFER_MS);
    chorus_check(bad.max_volume_thousandths == CHORUS_VOLUME_DEFAULT_CEILING,
                 "chorus_playout_config_from defaults the ceiling to %u",
                 bad.max_volume_thousandths);
    bad.max_volume_thousandths = 1001;
    chorus_check(chorus_playout_init(&p, &bad, ring_storage, chunk_storage, fake_clock) != 0,
                 "a ceiling above 1.000 is not a playable configuration");

    static uint8_t pcm[CHUNK_FRAMES * 6];
    static uint8_t out[CHUNK_FRAMES * FRAME_OUT];
    const int32_t a = 4000000;
    constant_chunk(pcm, a);
    struct {
        const char *what;
        uint16_t gain;
        uint16_t limit;
        uint16_t ramp_ms;
        uint32_t ceiling;
        uint32_t plays;
    } cases[] = {
        {"no room_volume yet, default ceiling", 0xFFFF, 0, 0, 1000, 1000},
        {"gain 400 under limit 750", 400, 750, 0, 1000, 400},
        {"gain 900 above limit 600", 900, 600, 0, 1000, 600},
        {"gain 1000 above ceiling 0.250", 1000, 1000, 0, 250, 250},
        {"muted (gain 0)", 0, 1000, 0, 1000, 0},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        if (setup(&p, cases[i].ceiling) != 0) {
            chorus_check(0, "%s: the playout initialises", cases[i].what);
            continue;
        }
        if (cases[i].gain != 0xFFFF) {
            chorus_playout_set_room_volume(&p, cases[i].gain, cases[i].limit, cases[i].ramp_ms);
        }
        uint32_t in_frames = 0;
        uint32_t out_frames = 0;
        uint32_t audio_frames = 0;
        int scaled = 1;
        for (uint32_t c = 0; c < 4; c++) {
            if (chorus_playout_offer(&p, 2000000000ull + c * 20000000ull, c + 1u,
                                     CHORUS_FMT_PCM_S24LE, 2, RATE, pcm,
                                     CHUNK_FRAMES) == CHORUS_PLAYOUT_QUEUED) {
                in_frames += CHUNK_FRAMES;
            }
        }
        for (uint32_t b = 0; b < 4; b++) {
            audio_frames += chorus_playout_fill(&p, out, CHUNK_FRAMES);
            out_frames += CHUNK_FRAMES;
            for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
                uint32_t g = q(cases[i].plays);
                if (slot(out + f * FRAME_OUT, 3) != expect(a, g) ||
                    slot(out + f * FRAME_OUT + 3, 3) != expect(-a, g)) {
                    scaled = 0;
                }
            }
        }
        chorus_playout_stats_t st;
        chorus_playout_stats(&p, &st);
        chorus_check(in_frames == 4u * CHUNK_FRAMES && audio_frames == in_frames &&
                         out_frames == in_frames && st.written_frames == out_frames,
                     "%s: %u frames in, %u written, %u of them audio", cases[i].what, in_frames,
                     out_frames, audio_frames);
        chorus_check(scaled && st.applied_volume_thousandths == cases[i].plays,
                     "%s: every sample plays at %u thousandths (+-%" PRId32 " of +-%" PRId32 ")",
                     cases[i].what, cases[i].plays, expect(a, q(cases[i].plays)), a);
    }

    /* A ramp across blocks and a silence: frames out still equal frames
     * written, and the gain falls monotonically to its target. */
    chorus_section("the playout path: a fade across blocks and an underrun");
    (void)setup(&p, 1000);
    chorus_playout_set_room_volume(&p, 0, 1000, 30);
    for (uint32_t c = 0; c < 2; c++) {
        (void)chorus_playout_offer(&p, 2000000000ull + c * 20000000ull, c + 1u,
                                   CHORUS_FMT_PCM_S24LE, 2, RATE, pcm, CHUNK_FRAMES);
    }
    int monotone = 1;
    int32_t prev = a;
    uint32_t written = 0;
    for (uint32_t b = 0; b < 3; b++) {
        (void)chorus_playout_fill(&p, out, CHUNK_FRAMES);
        written += CHUNK_FRAMES;
        for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
            int32_t left = slot(out + f * FRAME_OUT, 3);
            if (left > prev || left < 0) {
                monotone = 0;
            }
            prev = left;
        }
    }
    chorus_playout_stats_t st;
    chorus_playout_stats(&p, &st);
    chorus_check(monotone && prev == 0 && st.written_frames == written &&
                     st.underrun_frames == CHUNK_FRAMES,
                 "a 30 ms fade (1440 frames) over 1920 frames of audio then %u of underrun: the "
                 "level never rises, ends at 0, and %" PRIu64 " frames were written for %u",
                 CHUNK_FRAMES, st.written_frames, written);

    chorus_section("kept across a new stream");
    (void)setup(&p, 1000);
    chorus_playout_set_room_volume(&p, 250, 500, 0);
    chorus_playout_reset_stream(&p);
    chorus_playout_stats(&p, &st);
    chorus_check(st.applied_volume_thousandths == 250u && st.room_volume_messages == 1,
                 "a new stream does not reset the gain upward (%u thousandths after the reset)",
                 st.applied_volume_thousandths);
}

/* The committed vector, decoded by this endpoint's decoder, into the playout
 * path: a gain above the limit plays at the limit. */
static void test_committed_vector_plays_at_its_limit(void)
{
    chorus_section("fixtures/protocol/v2/room_volume_above_limit.hex: plays at its limit");
    char path[512];
    static char text[4096];
    uint8_t frame[64];
    chorus_repo_path(path, sizeof(path), "fixtures/protocol/v2/room_volume_above_limit.hex");
    long len = (fixture_read(path, text, sizeof(text)) < 0)
                   ? -1
                   : fixture_parse_hex(text, frame, sizeof(frame));
    chorus_v2_frame_t d = chorus_v2_decode_frame(frame, (size_t)(len < 0 ? 0 : len));
    int decoded =
        len > 0 && d.outcome == CHORUS_FRAME_DECODED && d.message.type == CHORUS_V2_ROOM_VOLUME;
    chorus_check(decoded, "the vector decodes as room_volume (gain %u, limit %u, ramp %u ms)",
                 (unsigned)d.message.as.room_volume.gain, (unsigned)d.message.as.room_volume.limit,
                 (unsigned)d.message.as.room_volume.ramp_ms);
    if (!decoded) {
        return;
    }
    const chorus_v2_room_volume_t *rv = &d.message.as.room_volume;
    static chorus_playout_t p;
    (void)setup(&p, 1000);
    chorus_playout_set_room_volume(&p, rv->gain, rv->limit, rv->ramp_ms);
    static uint8_t pcm[CHUNK_FRAMES * 6];
    static uint8_t out[CHUNK_FRAMES * FRAME_OUT];
    constant_chunk(pcm, 8000000);
    (void)chorus_playout_offer(&p, 2000000000ull, 1, CHORUS_FMT_PCM_S24LE, 2, RATE, pcm,
                               CHUNK_FRAMES);
    uint32_t audio = chorus_playout_fill(&p, out, CHUNK_FRAMES);
    int at_limit = audio == CHUNK_FRAMES;
    for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
        if (slot(out + f * FRAME_OUT, 3) != expect(8000000, q(rv->limit))) {
            at_limit = 0;
        }
    }
    chorus_check(rv->gain > rv->limit && at_limit,
                 "gain %u above limit %u: every frame plays at the limit (%" PRId32
                 " for 8000000), never at the gain (%" PRId32 ")",
                 (unsigned)rv->gain, (unsigned)rv->limit, expect(8000000, q(rv->limit)),
                 expect(8000000, q(rv->gain)));
}

/* The real server's room_volume sequence over every volume path (captured
 * by the server's test), through this endpoint's decoder and volume path,
 * at the default ceiling and at a lowered one. */
static void test_server_sequence_holds_the_clamp(void)
{
    chorus_section("fixtures/volume/room-volume-sequence.hex: the server's sequence, clamped");
    char path[512];
    static char text[16384];
    static uint8_t bytes[4096];
    chorus_repo_path(path, sizeof(path), "fixtures/volume/room-volume-sequence.hex");
    long len = (fixture_read(path, text, sizeof(text)) < 0)
                   ? -1
                   : fixture_parse_hex(text, bytes, sizeof(bytes));
    chorus_check(len > 0, "the sequence reads (%ld bytes)", len);
    if (len <= 0) {
        return;
    }
    chorus_volume_t full;
    chorus_volume_t low;
    chorus_volume_init(&full, 1000);
    chorus_volume_init(&low, 250);
    size_t at = 0;
    unsigned messages = 0;
    int clamped = 1;
    int decoded_all = 1;
    while (at < (size_t)len) {
        chorus_v2_frame_t d = chorus_v2_decode_frame(bytes + at, (size_t)len - at);
        if (d.consumed == 0 || d.outcome != CHORUS_FRAME_DECODED ||
            d.message.type != CHORUS_V2_ROOM_VOLUME) {
            decoded_all = 0;
            break;
        }
        at += d.consumed;
        const chorus_v2_room_volume_t *rv = &d.message.as.room_volume;
        chorus_volume_set(&full, rv->gain, rv->limit, rv->ramp_ms, RATE);
        chorus_volume_set(&low, rv->gain, rv->limit, rv->ramp_ms, RATE);
        /* The server sends every step at once (ramp_ms 0), so each applies
         * from the next frame. */
        uint32_t wanted = rv->gain < rv->limit ? rv->gain : rv->limit;
        uint32_t low_wanted = wanted < 250u ? wanted : 250u;
        if (rv->ramp_ms != 0 || chorus_volume_applied_q16(&full) != q(wanted) ||
            chorus_volume_applied_q16(&full) > q(rv->limit) ||
            chorus_volume_applied_q16(&low) != q(low_wanted)) {
            clamped = 0;
            printf("  message %u: gain %u limit %u ramp %u applied %u (ceiling 250: %u)\n",
                   messages, (unsigned)rv->gain, (unsigned)rv->limit, (unsigned)rv->ramp_ms,
                   chorus_volume_applied_thousandths(&full),
                   chorus_volume_applied_thousandths(&low));
        }
        messages++;
    }
    chorus_check(decoded_all && messages >= 8, "every frame decodes as room_volume (%u messages)",
                 messages);
    chorus_check(clamped,
                 "at every message the applied gain is min(gain, limit), never above the limit, "
                 "and under a ceiling of 250 never above 250");
    chorus_check(full.messages == messages && low.messages == messages,
                 "every message was taken (%u)", (unsigned)full.messages);
}

static void test_committed_configuration(void)
{
    chorus_section("firmware/config/endpoint.conf carries max_volume");
    chorus_endpoint_config_t config;
    char detail[512];
    int rc = chorus_endpoint_config_load(&config, chorus_endpoint_config_default_path(), detail,
                                         sizeof(detail));
    chorus_check(rc == 0 && config.max_volume_thousandths == CHORUS_VOLUME_DEFAULT_CEILING,
                 "the committed max_volume is read as %u thousandths (the ASSUMED default)",
                 config.max_volume_thousandths);
    /* The committed file with max_volume above full scale, then with none. */
    static char text[65536];
    static char edited[65536];
    long len = fixture_read(chorus_endpoint_config_default_path(), text, sizeof(text));
    const char *at = (len < 0) ? NULL : strstr(text, "\nmax_volume = 1.000\n");
    chorus_check(at != NULL, "endpoint.conf has the line max_volume = 1.000");
    if (at == NULL) {
        return;
    }
    static const char *const replacements[] = {"\nmax_volume = 1.5\n", "\nmax_volume = -1\n", "\n"};
    for (size_t i = 0; i < 3; i++) {
        size_t head = (size_t)(at - text);
        snprintf(edited, sizeof(edited), "%.*s%s%s", (int)head, text, replacements[i],
                 at + strlen("\nmax_volume = 1.000\n"));
        chorus_endpoint_config_t other;
        rc = chorus_endpoint_config_parse(&other, "endpoint.conf, edited", edited, detail,
                                          sizeof(detail));
        chorus_check(rc != 0 && strstr(detail, "max_volume") != NULL, "refused by name: %s",
                     detail);
    }
}

int main(void)
{
    char path[512];
    char detail[256];
    chorus_repo_path(path, sizeof(path), "config/sync.conf");
    if (chorus_sync_conf_load(&sync_conf, path, detail, sizeof(detail)) != 0) {
        printf("FAIL config/sync.conf: %s\n", detail);
        return 1;
    }
    test_conversion_and_parse();
    test_startup_and_clamps();
    test_ramp();
    test_mute();
    test_playout_path();
    test_committed_vector_plays_at_its_limit();
    test_server_sequence_holds_the_clamp();
    test_committed_configuration();
    return chorus_test_report("test_volume");
}
