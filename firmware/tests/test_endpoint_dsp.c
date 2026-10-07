/* The endpoint's sound chain in the playout path (goal 12, done-when line C;
 * firmware/include/chorus/endpoint_dsp.h).
 *
 * The library (firmware/src/dsp.c) is held to the shared fixtures by
 * test_dsp.c. What is graded here is the seam: the wire's `sound` and the
 * subwoofer knobs turned into the chain's settings, when the chain is in the
 * path and when it is not, and what the playout path writes with it: the room
 * gain inside the chain, the latency counted in the device delay, bass
 * management and the two-way split at the levels the LR4 design predicts, and
 * the limiter holding every written sample at or under the room's limit; and
 * the subwoofer's board (devkitc-s3-pcm5102-sub): its LFE feed on both I2S
 * slots, its line DAC's delay in the device delay, and its knobs' ADC codes
 * through chorus_controls_knob to the chain's sub level and polarity.
 *
 * Deterministic: a fake clock, no device, no sleeps. Levels are arithmetic
 * on synthetic tones, not timing evidence. */

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "chorus/controls.h"
#include "chorus/endpoint_config.h"
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

/* The output part's delay the next setup() configures (0 but for the
 * subwoofer's board). */
static uint32_t setup_output_delay;

/* A fresh playout path with the chain handed in, on the timeline. */
static int setup(const chorus_endpoint_two_way_t *two_way)
{
    chorus_playout_config_t c =
        chorus_playout_config_from(&sync_conf, RATE, 24, 240, CHORUS_PLAYOUT_BUFFER_MS);
    c.output_delay_frames = setup_output_delay;
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

/* The subwoofer's board (firmware/boards/devkitc-s3-pcm5102-sub.conf): what
 * its image hands the playout path, and what the path does with the LFE role
 * and the knobs. */
#define KNOB_STEP_CODES(steps, step) ((uint32_t)(((step) * 4096u + 4096u / 2u) / (steps)))

/* A knob's code, through the controls the board's ADC binding feeds
 * (esp_knobs.c: chorus_controls_knob, then chorus_controls_poll, then
 * chorus_playout_set_sub_knobs with both knobs' settings). */
static void turn_knobs(uint32_t level_code, uint32_t phase_code)
{
    chorus_controls_t c;
    (void)chorus_controls_init(&c, CHORUS_CLASS_SUBWOOFER, "", "");
    (void)chorus_controls_knob(&c, CHORUS_INPUT_SUB_LEVEL_KNOB, level_code, 1);
    (void)chorus_controls_knob(&c, CHORUS_INPUT_SUB_PHASE_KNOB, phase_code, 1);
    chorus_control_action_t actions[CHORUS_CONTROLS_MAX_ACTIONS];
    size_t n = chorus_controls_poll(&c, 2, actions, CHORUS_CONTROLS_MAX_ACTIONS);
    int moved = 0;
    for (size_t i = 0; i < n; i++) {
        moved |= actions[i].kind == CHORUS_ACTION_SUB_LEVEL ||
                 actions[i].kind == CHORUS_ACTION_SUB_PHASE;
    }
    if (moved) {
        chorus_playout_set_sub_knobs(&p, c.sub_level_tenths_db, c.sub_phase_deg);
    }
}

static int sub_setup(uint32_t delay)
{
    setup_output_delay = delay;
    int rc = setup(NULL);
    setup_output_delay = 0;
    if (rc == 0) {
        chorus_v2_sound_t w = flat_wire();
        w.role = CHORUS_DSP_POS_LFE;
        w.sub_present = 1;
        chorus_playout_set_sound(&p, &w);
    }
    return rc;
}

static void test_subwoofer_board(void)
{
    chorus_section("the subwoofer's board: the LFE feed on both slots, the DAC's delay, the knobs");
    static chorus_endpoint_config_t board;
    char detail[512];
    detail[0] = '\0';
    int loaded = chorus_endpoint_config_load_profile(
                     &board, CHORUS_REPO_ROOT "/firmware/config/endpoint.conf",
                     CHORUS_REPO_ROOT "/firmware/boards/devkitc-s3-pcm5102-sub.conf", detail,
                     sizeof(detail)) == 0;
    chorus_check(loaded && board.board.audio_output == CHORUS_AUDIO_OUTPUT_LINE_DAC,
                 "devkitc-s3-pcm5102-sub loads and plays through %s (%s)",
                 chorus_audio_output_name(board.board.audio_output), detail[0] ? detail : "ok");
    if (!loaded) {
        return;
    }
    const uint32_t delay = board.board.output_delay_frames;

    /* The LFE role's one output, written to both slots sample for sample, so
     * both of the plate amplifier's inputs are driven. */
    if (sub_setup(delay) != 0) {
        chorus_check(0, "the playout initialises");
        return;
    }
    tones_t t = {{40.0, 1000.0}, {0.2, 0.2}, 2};
    play(&t, 1.5);
    uint32_t same = 0;
    for (uint32_t i = 0; i < KEEP; i++) {
        same += kept[0][i] == kept[1][i];
    }
    const double in_db = 20.0 * log10(0.2);
    double low = level_db(0, 40.0, 0) - in_db;
    double high = level_db(0, 1000.0, 0) - in_db;
    double want = lr4_db(80.0, 40.0, 0) + 20.0 * log10(2.0);
    chorus_check(same == KEEP && fabs(low - want) < 0.05 && high < -40.0,
                 "the LFE feed is on both slots (%u of %u frames identical): 40 Hz at %.2f dB "
                 "(LR4 predicts %.2f), 1 kHz at %.1f dB",
                 (unsigned)same, (unsigned)KEEP, low, want, high);
    double level_0db = level_db(0, 40.0, 0);
    int32_t normal[64];
    memcpy(normal, kept[0], sizeof(normal));

    /* The DAC's filter delay in the device delay: the same frames written,
     * the same DMA, and the device delay that much further from the pins. */
    double with_delay = 0.0, delay_ns = 0.0, without = 0.0, ns = 0.0;
    chorus_playout_fifo(&p, &with_delay, &delay_ns);
    if (sub_setup(0) == 0) {
        play(&t, 1.5);
        chorus_playout_fifo(&p, &without, &ns);
    }
    chorus_check(delay == 22u && fabs(with_delay - without - (double)delay) < 1e-9,
                 "the device delay carries the PCM5102A's %u-frame filter delay (%.0f against "
                 "%.0f frames, %.1f us)",
                 (unsigned)delay, with_delay, without, (delay_ns - ns) / 1000.0);

    /* The knobs: 12-bit codes onto sub_level_cdb and the polarity. */
    const uint32_t level_steps = 25u, phase_steps = 13u;
    const struct {
        uint32_t level_code;
        uint32_t phase_code;
        int16_t sub_level_cdb;
        bool inverted;
        const char *what;
    } turns[] = {
        {0u, 0u, -1200, false, "both fully anticlockwise: the full cut, normal"},
        {4095u, 0u, 0, false, "level fully clockwise: 0 dB, never a boost"},
        {KNOB_STEP_CODES(level_steps, 12u), 0u, -600, false, "level at its middle step: -6 dB"},
        {4095u, KNOB_STEP_CODES(phase_steps, 5u), 0, false, "phase at 75 degrees: normal"},
        {4095u, KNOB_STEP_CODES(phase_steps, 6u), 0, true, "phase at 90 degrees: inverted"},
        {4095u, 4095u, 0, true, "phase fully clockwise, 180 degrees: inverted"},
    };
    for (size_t i = 0; i < sizeof(turns) / sizeof(turns[0]); i++) {
        if (sub_setup(delay) != 0) {
            chorus_check(0, "the playout initialises");
            return;
        }
        turn_knobs(turns[i].level_code, turns[i].phase_code);
        chorus_check(dsp.chain.sound.sub_level_cdb == turns[i].sub_level_cdb &&
                         dsp.chain.sound.sub_polarity_inverted == turns[i].inverted,
                     "codes %u and %u, %s: sub_level_cdb %d, polarity %s", turns[i].level_code,
                     turns[i].phase_code, turns[i].what, dsp.chain.sound.sub_level_cdb,
                     dsp.chain.sound.sub_polarity_inverted ? "inverted" : "normal");
    }

    /* And they are heard: the full cut is 12 dB down, and the half turn
     * inverts the feed sample for sample. */
    if (sub_setup(delay) == 0) {
        turn_knobs(0u, 0u);
        play(&t, 1.5);
        double cut = level_db(0, 40.0, 0) - level_0db;
        chorus_check(fabs(cut + 12.0) < 0.05, "the level knob's full cut plays %.2f dB", cut);
    }
    if (sub_setup(delay) == 0) {
        turn_knobs(4095u, 4095u);
        play(&t, 1.5);
        int negated = 1;
        for (size_t i = 0; i < sizeof(normal) / sizeof(normal[0]); i++) {
            int32_t d = kept[0][i] + normal[i];
            negated &= d >= -1 && d <= 1;
        }
        chorus_check(negated, "the phase knob's half turn plays the feed inverted");
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
    test_subwoofer_board();
    return chorus_test_report("test_endpoint_dsp");
}
