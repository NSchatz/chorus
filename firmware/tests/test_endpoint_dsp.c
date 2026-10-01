/* The endpoint's sound chain in the playout path (goal 12, done-when line C;
 * firmware/include/chorus/endpoint_dsp.h).
 *
 * The library (firmware/src/dsp.c) is held to the shared fixtures by
 * test_dsp.c. What is graded here is the seam: the wire's `sound` and the
 * subwoofer knobs turned into the chain's settings, when the chain is in the
 * path and when it is not, and what the playout path writes with it: the room
 * gain inside the chain, the latency counted in the device delay, bass
 * management and the two-way split at the levels the LR4 design predicts, and
 * the limiter holding every written sample at or under the room's limit.
 *
 * Deterministic: a fake clock, no device, no sleeps. Levels are arithmetic
 * on synthetic tones, not timing evidence. */

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "chorus/endpoint_dsp.h"
#include "chorus/playout.h"
#include "chorus/protocol.h"
#include "chorus/sync_conf.h"
#include "harness.h"

#define RATE 48000u
#define CHUNK_FRAMES 960u
#define SLOT_BYTES 3u
#define FRAME_OUT (2u * SLOT_BYTES)
#define LATENCY 96u /* 2 ms at 48 kHz: CHORUS_DSP_LIMITER_LOOKAHEAD_US */
#define PI 3.14159265358979323846

static uint64_t fake_now;
static uint64_t fake_clock(void)
{
    return fake_now;
}

static chorus_sync_conf_t sync_conf;
static uint8_t ring_storage[RATE * 300u / 1000u * FRAME_OUT];
static chorus_playout_chunk_t chunk_storage[128];
static chorus_endpoint_dsp_t dsp;
static chorus_playout_t p;

static int32_t slot(const uint8_t *s)
{
    uint32_t raw = (uint32_t)s[0] | (uint32_t)s[1] << 8 | (uint32_t)s[2] << 16;
    int32_t v = (int32_t)raw;
    if (raw & 0x800000u) {
        v -= 0x1000000;
    }
    return v;
}

static void put_slot(uint8_t *s, int32_t value)
{
    uint32_t v = (uint32_t)value;
    s[0] = (uint8_t)v;
    s[1] = (uint8_t)(v >> 8);
    s[2] = (uint8_t)(v >> 16);
}

static chorus_v2_sound_t flat_wire(void)
{
    chorus_v2_sound_t w;
    memset(&w, 0, sizeof(w));
    w.crossover_hz = 80;
    return w;
}

static const uint8_t STEREO[2] = {CHORUS_DSP_POS_FL, CHORUS_DSP_POS_FR};

/* A fresh playout path with the chain handed in, on the timeline. */
static int setup(const chorus_endpoint_two_way_t *two_way)
{
    chorus_playout_config_t c =
        chorus_playout_config_from(&sync_conf, RATE, 24, 240, CHORUS_PLAYOUT_BUFFER_MS);
    fake_now = 1000000000ull;
    if (chorus_playout_init(&p, &c, ring_storage, chunk_storage, fake_clock) != 0) {
        return -1;
    }
    chorus_endpoint_dsp_init(&dsp, two_way);
    chorus_playout_set_dsp(&p, &dsp);
    chorus_playout_set_stream_layout(&p, 2, STEREO);
    p.acquired = 1; /* placed on the timeline, as by a zero-error tick */
    return 0;
}

/* --- tones through the playout path --------------------------------------- */

#define TONES_MAX 4
typedef struct {
    double freq[TONES_MAX];
    double amp[TONES_MAX];
    int count;
} tones_t;

/* Plays `seconds` of the tones (L = R) through offer and fill, and keeps the
 * last half second of both written slots. */
#define KEEP (RATE / 2u)
static int32_t kept[2][KEEP];
static int32_t peak_abs;

static void play(const tones_t *t, double seconds)
{
    static uint8_t pcm[CHUNK_FRAMES * 6];
    static uint8_t out[CHUNK_FRAMES * FRAME_OUT];
    uint32_t chunks = (uint32_t)(seconds * RATE / CHUNK_FRAMES);
    uint64_t n = 0;
    uint64_t total = (uint64_t)chunks * CHUNK_FRAMES;
    peak_abs = 0;
    for (uint32_t c = 0; c < chunks; c++) {
        for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
            double v = 0.0;
            double tt = (double)(c * CHUNK_FRAMES + f) / RATE;
            for (int k = 0; k < t->count; k++) {
                v += t->amp[k] * sin(2.0 * PI * t->freq[k] * tt);
            }
            int32_t s = (int32_t)lrint(v * 8388607.0);
            put_slot(pcm + f * 6, s);
            put_slot(pcm + f * 6 + 3, s);
        }
        (void)chorus_playout_offer(&p, 2000000000ull + (uint64_t)c * 20000000ull, c + 1u,
                                   CHORUS_FMT_PCM_S24LE, 2, RATE, pcm, CHUNK_FRAMES);
        (void)chorus_playout_fill(&p, out, CHUNK_FRAMES);
        for (uint32_t f = 0; f < CHUNK_FRAMES; f++, n++) {
            for (int s = 0; s < 2; s++) {
                int32_t v = slot(out + f * FRAME_OUT + s * SLOT_BYTES);
                int32_t a = v < 0 ? -v : v;
                if (a > peak_abs) {
                    peak_abs = a;
                }
                if (n >= total - KEEP) {
                    kept[s][n - (total - KEEP)] = v;
                }
            }
        }
    }
}

/* A kept slot's level at `freq` (a whole number of cycles in the half
 * second), in dB of full scale, or the level of slot0 + slot1 with `sum`. */
static double level_db(int s, double freq, int sum)
{
    double re = 0.0, im = 0.0;
    for (uint32_t i = 0; i < KEEP; i++) {
        double v = sum ? (double)kept[0][i] + (double)kept[1][i] : (double)kept[s][i];
        double ph = 2.0 * PI * freq * (double)i / RATE;
        re += v * cos(ph);
        im += v * sin(ph);
    }
    double a = 2.0 * sqrt(re * re + im * im) / KEEP / 8388607.0;
    return 20.0 * log10(a + 1e-30);
}

static double lr4_db(double fc, double f, int high)
{
    chorus_dsp_lr4_design_t d;
    (void)chorus_dsp_lr4_design(RATE, fc, &d);
    double re, im;
    if (high) {
        chorus_dsp_lr4_response_high(&d, f, RATE, &re, &im);
    } else {
        chorus_dsp_lr4_response_low(&d, f, RATE, &re, &im);
    }
    return 10.0 * log10(re * re + im * im);
}

/* --- the tests ------------------------------------------------------------- */

static void test_settings(void)
{
    chorus_section("the wire's sound and the knobs as the chain's settings");
    chorus_v2_sound_t w = flat_wire();
    w.bass_db = 3;
    w.treble_db = -2;
    w.flags = 0x1F;
    w.role = 4;
    w.sub_present = 1;
    w.crossover_hz = 100;
    w.sub_level_cdb = -350;
    w.eq_count = 1;
    w.filters[0].freq_hz = 42;
    w.filters[0].gain_cdb = -600;
    w.filters[0].q_milli = 4500;
    chorus_dsp_sound_t s;
    chorus_endpoint_dsp_settings(&w, 0, 0, &s);
    chorus_check(s.bass_db == 3 && s.treble_db == -2 && s.loudness && s.night && s.speech &&
                     s.room_eq_enabled && s.sub_polarity_inverted && s.role == 4 && s.sub_present &&
                     s.crossover_hz == 100 && s.sub_level_cdb == -350 && s.eq_count == 1 &&
                     s.eq[0].freq_hz == 42 && s.eq[0].gain_cdb == -600 && s.eq[0].q_milli == 4500,
                 "every field one for one, every flag bit by bit");
    chorus_endpoint_dsp_settings(&w, -60, 180, &s);
    chorus_check(s.sub_level_cdb == -950 && !s.sub_polarity_inverted,
                 "knob -6.0 dB adds to -3.50 dB (%d cdB); inverted twice is normal",
                 s.sub_level_cdb);
    w.sub_level_cdb = -1000;
    chorus_endpoint_dsp_settings(&w, -120, 0, &s);
    chorus_check(s.sub_level_cdb == -1200, "the sum is held to -12 dB (%d)", s.sub_level_cdb);
    w.flags = 0;
    chorus_endpoint_dsp_settings(&w, 0, 89, &s);
    int at89 = s.sub_polarity_inverted;
    chorus_endpoint_dsp_settings(&w, 0, 90, &s);
    chorus_check(!at89 && s.sub_polarity_inverted,
                 "the phase knob inverts from 90 degrees (ASSUMED quantisation)");
}

static void test_engagement(void)
{
    chorus_section("when the chain is in the path");
    chorus_v2_sound_t w = flat_wire();
    chorus_endpoint_dsp_init(&dsp, NULL);
    chorus_endpoint_dsp_set_stream(&dsp, RATE, 2, STEREO);
    chorus_check(!dsp.engaged && chorus_endpoint_dsp_latency_frames(&dsp) == 0,
                 "a stream and no sound: not engaged, no latency");
    chorus_endpoint_dsp_set_sound(&dsp, &w);
    chorus_check(dsp.engaged && chorus_endpoint_dsp_latency_frames(&dsp) == LATENCY &&
                     dsp.sounds_applied == 1,
                 "the first sound engages it, latency %u frames",
                 chorus_endpoint_dsp_latency_frames(&dsp));

    chorus_endpoint_dsp_init(&dsp, NULL);
    chorus_endpoint_dsp_set_sound(&dsp, &w);
    chorus_check(!dsp.engaged, "a sound and no stream: not engaged yet");
    chorus_endpoint_dsp_set_stream(&dsp, RATE, 2, STEREO);
    chorus_check(dsp.engaged, "then the stream engages it");

    chorus_endpoint_two_way_t tw = {true, 2000, 0, 1};
    chorus_endpoint_dsp_init(&dsp, &tw);
    chorus_endpoint_dsp_set_stream(&dsp, RATE, 2, STEREO);
    chorus_check(dsp.engaged && chorus_dsp_chain_out_channels(&dsp.chain) == 2,
                 "a two-way alone engages it, with two outputs");

    /* A refused setting keeps the previous one. The decoder never passes a
     * crossover of 30 Hz; the chain refuses it by name. */
    chorus_endpoint_dsp_init(&dsp, NULL);
    chorus_endpoint_dsp_set_stream(&dsp, RATE, 2, STEREO);
    w.bass_db = 4;
    chorus_endpoint_dsp_set_sound(&dsp, &w);
    chorus_v2_sound_t bad = w;
    bad.bass_db = 0;
    bad.crossover_hz = 30;
    chorus_endpoint_dsp_set_sound(&dsp, &bad);
    chorus_check(dsp.engaged && dsp.refusals == 1 && dsp.last_refused != NULL &&
                     strcmp(dsp.last_refused, "crossover_hz") == 0 && dsp.chain.sound.bass_db == 4,
                 "a refused sound is counted (%s) and the previous settings stay",
                 dsp.last_refused ? dsp.last_refused : "none");
}

static void test_flat_playout(void)
{
    chorus_section("a flat chain in the playout path: the input, times the gain, 2 ms late");
    static uint8_t pcm[CHUNK_FRAMES * 6];
    static uint8_t out[CHUNK_FRAMES * FRAME_OUT];
    if (setup(NULL) != 0) {
        chorus_check(0, "the playout initialises");
        return;
    }
    /* Before any sound: today's path, byte for byte, and no latency. */
    for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
        put_slot(pcm + f * 6, (int32_t)(f * 2u + 2u));
        put_slot(pcm + f * 6 + 3, -(int32_t)(f * 2u + 2u));
    }
    (void)chorus_playout_offer(&p, 2000000000ull, 1, CHORUS_FMT_PCM_S24LE, 2, RATE, pcm,
                               CHUNK_FRAMES);
    (void)chorus_playout_fill(&p, out, CHUNK_FRAMES);
    chorus_check(memcmp(out, pcm, sizeof(pcm)) == 0 && !dsp.engaged,
                 "no sound yet: the frames are written as they came");
    double before = 0.0, ns = 0.0;
    chorus_playout_fifo(&p, &before, &ns);

    chorus_v2_sound_t w = flat_wire();
    chorus_playout_set_sound(&p, &w);
    chorus_playout_set_room_volume(&p, 500, 1000, 0);
    double after = 0.0;
    chorus_playout_fifo(&p, &after, &ns);
    chorus_check(after - before == (double)LATENCY,
                 "engaged, the device delay grows by the chain's latency (%.0f frames)",
                 after - before);
    int exact = 1;
    for (uint32_t c = 0; c < 3; c++) {
        (void)chorus_playout_offer(&p, 2020000000ull + c * 20000000ull, c + 2u,
                                   CHORUS_FMT_PCM_S24LE, 2, RATE, pcm, CHUNK_FRAMES);
        (void)chorus_playout_fill(&p, out, CHUNK_FRAMES);
        for (uint32_t f = 0; f < CHUNK_FRAMES; f++) {
            /* Frame f of this block is input frame f - LATENCY (of the same
             * repeating chunk after the first block), times 0.5 exactly. */
            int32_t want;
            if (c == 0 && f < LATENCY) {
                want = 0;
            } else {
                uint32_t src = (f + CHUNK_FRAMES - LATENCY) % CHUNK_FRAMES;
                want = (int32_t)(src * 2u + 2u) / 2;
            }
            if (slot(out + f * FRAME_OUT) != want || slot(out + f * FRAME_OUT + 3) != -want) {
                exact = 0;
            }
        }
    }
    chorus_check(exact, "every written sample is the input times 0.500, %u frames late", LATENCY);
    chorus_playout_stats_t st;
    chorus_playout_stats(&p, &st);
    chorus_check(st.dsp_engaged == 1 && st.dsp_latency_frames == LATENCY &&
                     st.sounds_applied == 1 && st.dsp_refusals == 0,
                 "the stats say so: engaged=%u latency=%u sounds=%u refusals=%u", st.dsp_engaged,
                 st.dsp_latency_frames, st.sounds_applied, st.dsp_refusals);
}

static void test_bass_management(void)
{
    chorus_section("a 2.1 set through the playout path: LR4 at the crossover");
    tones_t t = {{40.0, 80.0, 160.0, 1000.0}, {0.1, 0.1, 0.1, 0.1}, 4};
    const double in_db = 20.0 * log10(0.1);
    for (int role = 1; role <= 4; role += 3) {
        if (setup(NULL) != 0) {
            chorus_check(0, "the playout initialises");
            return;
        }
        chorus_v2_sound_t w = flat_wire();
        w.role = (uint8_t)role;
        w.sub_present = 1;
        chorus_playout_set_sound(&p, &w);
        play(&t, 1.5);
        for (int k = 0; k < t.count; k++) {
            double got = level_db(0, t.freq[k], 0) - in_db;
            /* The sub plays the low branch of FL + FR, here 2 x the tone. */
            double want = (role == 4) ? lr4_db(80.0, t.freq[k], 0) + 20.0 * log10(2.0)
                                      : lr4_db(80.0, t.freq[k], 1);
            double other = level_db(1, t.freq[k], 0) - in_db;
            int ok = (want < -40.0) ? got < -40.0 : fabs(got - want) < 0.05;
            chorus_check(ok && fabs(other - got) < 1e-9,
                         "%s at %.0f Hz: %.2f dB (LR4 predicts %.2f), on both slots",
                         role == 4 ? "the sub" : "a main", t.freq[k], got, want);
        }
    }
}

static void test_limiter(void)
{
    chorus_section("the limiter: bass +10 dB never lifts the room above its limit");
    if (setup(NULL) != 0) {
        chorus_check(0, "the playout initialises");
        return;
    }
    chorus_v2_sound_t w = flat_wire();
    w.bass_db = 10;
    chorus_playout_set_sound(&p, &w);
    chorus_playout_set_room_volume(&p, 1000, 500, 0);
    tones_t t = {{50.0}, {0.4}, 1};
    play(&t, 1.0);
    int32_t limit = (int32_t)(0.5 * 8388608.0);
    chorus_check(peak_abs <= limit && peak_abs > limit * 9 / 10,
                 "every written sample is at or under the limit 0.500 (peak %.4f full scale)",
                 peak_abs / 8388608.0);
}

static void test_two_way(void)
{
    chorus_section("a two-way: woofer and tweeter on their slots, summing flat");
    chorus_endpoint_two_way_t tw = {true, 2000, 1, 0}; /* swapped slots */
    if (setup(&tw) != 0) {
        chorus_check(0, "the playout initialises");
        return;
    }
    chorus_check(dsp.engaged, "engaged by the two-way alone, before any sound");
    tones_t t = {{500.0, 2000.0, 8000.0}, {0.1, 0.1, 0.1}, 3};
    const double in_db = 20.0 * log10(0.1);
    play(&t, 1.0);
    for (int k = 0; k < t.count; k++) {
        double woofer = level_db(1, t.freq[k], 0) - in_db;
        double tweeter = level_db(0, t.freq[k], 0) - in_db;
        double sum = level_db(0, t.freq[k], 1) - in_db;
        chorus_check(fabs(woofer - lr4_db(2000.0, t.freq[k], 0)) < 0.05 &&
                         fabs(tweeter - lr4_db(2000.0, t.freq[k], 1)) < 0.05 && fabs(sum) < 0.05,
                     "%.0f Hz: woofer (slot 1) %.2f dB, tweeter (slot 0) %.2f dB, sum %.3f dB",
                     t.freq[k], woofer, tweeter, sum);
    }
}

int main(void)
{
    char detail[256];
    if (chorus_sync_conf_load(&sync_conf, CHORUS_REPO_ROOT "/config/sync.conf", detail,
                              sizeof(detail)) != 0) {
        printf("FAIL %s\n", detail);
        return 1;
    }
    test_settings();
    test_engagement();
    test_flat_playout();
    test_bass_management();
    test_limiter();
    test_two_way();
    return chorus_test_report("test_endpoint_dsp");
}
