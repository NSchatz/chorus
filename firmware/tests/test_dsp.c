/* The DSP library's C mirror against the shared fixtures (goal 12;
 * docs/dsp.md, fixtures/README.md "dsp/").
 *
 * Walks fixtures/dsp and runs every file through firmware/src/dsp.c, the
 * same files crates/dsp/tests/shared_fixtures.rs runs through the Rust crate,
 * with the same checks: the cited worked examples (ITU-R BS.1770's printed
 * K-weighting coefficients, the RBJ cookbook's magnitudes, Linkwitz-Riley's
 * -6.02 dB, in-phase and flat sum, the delay line's definition, the
 * brickwall limiter's ceiling, Giannoulis et al.'s static curve and
 * MathWorks' time constants, the ISO 226:2003 table and formula) and the
 * chain fixtures, whose `samples.<o>` lines are the Rust chain's output, so
 * the two chains cannot drift apart. A kind this reader does not know is a
 * failure. Then the refusals and the fixed maximums, which are C's own.
 *
 * Deterministic: no clock, no device. Every number is arithmetic on committed
 * inputs, not a timing claim. */

#include <dirent.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "chorus/dsp.h"
#include "chorus/protocol_v2.h"
#include "fixture_text.h"
#include "harness.h"

#define MAX_FRAMES 48000u
#define MAX_LIST 128u
#define VALUE_CAP 4096u

static char text[65536];
static char value[VALUE_CAP];
static const char *fixture_name;

/* --- reading a fixture --------------------------------------------------------- */

static const char *get(const char *key)
{
    return fixture_field(text, key, value, sizeof(value));
}

static int num(const char *key, double *out)
{
    const char *v = get(key);
    if (v == NULL) {
        return -1;
    }
    char *end = NULL;
    *out = strtod(v, &end);
    return (end != v && *end == '\0') ? 0 : -1;
}

static double num_or(const char *key, double fallback)
{
    double v;
    return (num(key, &v) == 0) ? v : fallback;
}

/* A space-separated list; returns its length, or -1. */
static int list(const char *key, double *out, unsigned cap)
{
    const char *v = get(key);
    if (v == NULL) {
        return -1;
    }
    unsigned count = 0;
    const char *at = v;
    for (;;) {
        while (*at == ' ' || *at == '\t') {
            at++;
        }
        if (*at == '\0') {
            break;
        }
        char *end = NULL;
        double d = strtod(at, &end);
        if (end == at || count >= cap) {
            return -1;
        }
        out[count++] = d;
        at = end;
    }
    return (int)count;
}

static uint64_t lcg_state;

static double lcg(void)
{
    lcg_state = lcg_state * 6364136223846793005ull + 1442695040888963407ull;
    return ((double)(lcg_state >> 40) / 16777216.0) * 2.0 - 1.0;
}

/* A signal recipe (crates/dsp/src/fixture.rs), summed in double, rounded to
 * float once per sample. Returns 0, or -1 for a recipe it cannot read. */
static double acc[MAX_FRAMES];

static int signal(const char *recipe, uint32_t frames, double rate, float *out)
{
    if (frames > MAX_FRAMES) {
        return -1;
    }
    for (uint32_t n = 0; n < frames; n++) {
        acc[n] = 0.0;
    }
    char copy[512];
    snprintf(copy, sizeof(copy), "%s", recipe);
    char *save = NULL;
    for (char *term = strtok_r(copy, "+", &save); term != NULL; term = strtok_r(NULL, "+", &save)) {
        char word[16] = {0};
        double p[4];
        int got = sscanf(term, " %15s %lf %lf %lf %lf", word, &p[0], &p[1], &p[2], &p[3]);
        int k = got - 1;
        const double tau = 2.0 * 3.14159265358979323846;
        if (strcmp(word, "sine") == 0 && k == 2) {
            for (uint32_t n = 0; n < frames; n++) {
                acc[n] += p[1] * sin(tau * p[0] * (double)n / rate);
            }
        } else if (strcmp(word, "burst") == 0 && k == 4) {
            for (uint32_t n = 0; n < frames; n++) {
                if ((double)n >= p[2] && (double)n < p[3]) {
                    acc[n] += p[1] * sin(tau * p[0] * (double)n / rate);
                }
            }
        } else if (strcmp(word, "impulse") == 0 && k == 1) {
            if (frames > 0) {
                acc[0] += p[0];
            }
        } else if (strcmp(word, "step") == 0 && k == 1) {
            for (uint32_t n = 0; n < frames; n++) {
                acc[n] += p[0];
            }
        } else if (strcmp(word, "ramp") == 0 && k == 1) {
            for (uint32_t n = 0; n < frames; n++) {
                acc[n] += p[0] * ((double)n + 1.0);
            }
        } else if (strcmp(word, "noise") == 0 && k == 2) {
            lcg_state = (uint64_t)p[0];
            for (uint32_t n = 0; n < frames; n++) {
                acc[n] += p[1] * lcg();
            }
        } else if (strcmp(word, "silence") == 0 && k == 0) {
            /* nothing */
        } else {
            return -1;
        }
    }
    for (uint32_t n = 0; n < frames; n++) {
        out[n] = (float)acc[n];
    }
    return 0;
}

static int fail_parse(const char *what)
{
    chorus_check(0, "%s: %s", fixture_name, what);
    return -1;
}

/* --- block kinds ------------------------------------------------------------- */

static int design(chorus_dsp_coeffs_t *c)
{
    chorus_dsp_kind_t kind;
    double rate, f0, q, g;
    const char *type = get("type");
    if (type == NULL || chorus_dsp_kind_from_name(type, &kind) != 0 || num("rate_hz", &rate) ||
        num("f0_hz", &f0) || num("q", &q) || num("gain_db", &g)) {
        return fail_parse("an unreadable design");
    }
    if (chorus_dsp_biquad_design(kind, rate, f0, q, g, c) != CHORUS_DSP_OK) {
        return fail_parse("the design was refused");
    }
    return 0;
}

static void biquad_coefficients(void)
{
    chorus_dsp_coeffs_t c;
    double tol, b[3], a[2];
    if (design(&c) || num("tolerance", &tol) || list("expect_a", a, 2) != 2) {
        fail_parse("an unreadable biquad_coefficients fixture");
        return;
    }
    if (get("expect_b") != NULL) {
        list("expect_b", b, 3);
        double got[3] = {c.b0, c.b1, c.b2};
        for (int i = 0; i < 3; i++) {
            chorus_check(fabs(got[i] - b[i]) <= tol, "%s: b%d %.14f vs printed %.14f", fixture_name,
                         i, got[i], b[i]);
        }
    }
    if (get("expect_b_ratio") != NULL) {
        double tr = num_or("tolerance_ratio", 0.0);
        list("expect_b_ratio", b, 3);
        double got[3] = {1.0, c.b1 / c.b0, c.b2 / c.b0};
        for (int i = 0; i < 3; i++) {
            chorus_check(fabs(got[i] - b[i]) <= tr, "%s: b%d/b0 %.14f vs printed %.14f",
                         fixture_name, i, got[i], b[i]);
        }
    }
    double got[2] = {c.a1, c.a2};
    for (int i = 0; i < 2; i++) {
        chorus_check(fabs(got[i] - a[i]) <= tol, "%s: a%d %.14f vs printed %.14f", fixture_name,
                     i + 1, got[i], a[i]);
    }
}

static void biquad_magnitude(void)
{
    chorus_dsp_coeffs_t c;
    double rate, tol, freqs[MAX_LIST], expect[MAX_LIST];
    if (design(&c) || num("rate_hz", &rate) || num("tolerance_db", &tol)) {
        return;
    }
    int n = list("freqs_hz", freqs, MAX_LIST);
    if (n < 0 || list("expect_db", expect, MAX_LIST) != n) {
        fail_parse("freqs_hz and expect_db");
        return;
    }
    for (int i = 0; i < n; i++) {
        double got = chorus_dsp_magnitude_db(&c, freqs[i], rate);
        int ok = isinf(expect[i]) ? got < -100.0 : fabs(got - expect[i]) <= tol;
        chorus_check(ok, "%s: %.1f Hz is %.9f dB, want %.9f", fixture_name, freqs[i], got,
                     expect[i]);
    }
}

static void biquad_reference(void)
{
    chorus_dsp_coeffs_t c;
    double rate, tol, rb[3], ra[2], freqs[MAX_LIST], frames_d, tol_i;
    if (design(&c) || num("rate_hz", &rate) || num("tolerance_db", &tol) ||
        list("ref_b", rb, 3) != 3 || list("ref_a", ra, 2) != 2 ||
        num("impulse_frames", &frames_d) || num("tolerance_impulse", &tol_i)) {
        fail_parse("an unreadable biquad_reference fixture");
        return;
    }
    chorus_dsp_coeffs_t r = {rb[0], rb[1], rb[2], ra[0], ra[1]};
    int n = list("freqs_hz", freqs, MAX_LIST);
    for (int i = 0; i < n; i++) {
        double got = chorus_dsp_magnitude_db(&c, freqs[i], rate);
        double want = chorus_dsp_magnitude_db(&r, freqs[i], rate);
        chorus_check(fabs(got - want) <= tol, "%s: %.0f Hz is %.6f dB, the printed filter %.6f",
                     fixture_name, freqs[i], got, want);
    }
    chorus_dsp_biquad_t q;
    chorus_dsp_biquad_init(&q, &c);
    double x1 = 0, x2 = 0, y1 = 0, y2 = 0, worst = 0;
    for (uint32_t k = 0; k < (uint32_t)frames_d; k++) {
        double x = (k == 0) ? 1.0 : 0.0;
        double y = r.b0 * x + r.b1 * x1 + r.b2 * x2 - r.a1 * y1 - r.a2 * y2;
        double got = (double)chorus_dsp_biquad_process(&q, (float)x);
        if (fabs(got - y) > worst) {
            worst = fabs(got - y);
        }
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
    }
    chorus_check(worst <= tol_i, "%s: the impulse response is within %.2e of the printed filter's",
                 fixture_name, worst);
}

static double db_of(double re, double im)
{
    return 10.0 * log10(re * re + im * im);
}

static double amplitude(const float *y, uint32_t n)
{
    double ms = 0.0;
    for (uint32_t i = 0; i < n; i++) {
        ms += (double)y[i] * (double)y[i];
    }
    return sqrt(2.0 * ms / (double)n);
}

static float sig[CHORUS_DSP_MAX_CHANNELS][MAX_FRAMES];
static float outs[CHORUS_DSP_MAX_OUTPUTS][MAX_FRAMES];

static void lr4(void)
{
    double rate, fc, at, tol, sum_tol, phase_tol, frames_d, sine_tol, freqs[MAX_LIST];
    if (num("rate_hz", &rate) || num("crossover_hz", &fc) || num("at_crossover_db", &at) ||
        num("tolerance_db", &tol) || num("sum_tolerance_db", &sum_tol) ||
        num("phase_tolerance_deg", &phase_tol) || num("sine_frames", &frames_d) ||
        num("sine_tolerance", &sine_tol)) {
        fail_parse("an unreadable lr4 fixture");
        return;
    }
    chorus_dsp_lr4_design_t d;
    chorus_dsp_lr4_design(rate, fc, &d);
    double lr, li, hr, hi;
    chorus_dsp_lr4_response_low(&d, fc, rate, &lr, &li);
    chorus_dsp_lr4_response_high(&d, fc, rate, &hr, &hi);
    chorus_check(fabs(db_of(lr, li) - at) <= tol, "%s: low %.9f dB at fc", fixture_name,
                 db_of(lr, li));
    chorus_check(fabs(db_of(hr, hi) - at) <= tol, "%s: high %.9f dB at fc", fixture_name,
                 db_of(hr, hi));
    int n = list("freqs_hz", freqs, MAX_LIST);
    for (int i = 0; i < n; i++) {
        chorus_dsp_lr4_response_low(&d, freqs[i], rate, &lr, &li);
        chorus_dsp_lr4_response_high(&d, freqs[i], rate, &hr, &hi);
        double s = db_of(lr + hr, li + hi);
        chorus_check(fabs(s) <= sum_tol, "%s: the sum is %.2e dB at %.1f Hz", fixture_name, s,
                     freqs[i]);
        double diff = (atan2(li, lr) - atan2(hi, hr)) * 180.0 / 3.14159265358979323846;
        diff = fmod(diff + 540.0, 360.0);
        if (diff < 0) {
            diff += 360.0;
        }
        diff -= 180.0;
        chorus_check(fabs(diff) <= phase_tol, "%s: the branches are %.2e degrees apart at %.1f Hz",
                     fixture_name, diff, freqs[i]);
    }
    uint32_t frames = (uint32_t)frames_d;
    char recipe[64];
    snprintf(recipe, sizeof(recipe), "sine %.17g 1", fc);
    if (signal(recipe, frames, rate, sig[0]) != 0) {
        fail_parse("the sine");
        return;
    }
    chorus_dsp_lr4_t s;
    chorus_dsp_lr4_init(&s, &d);
    for (uint32_t k = 0; k < frames; k++) {
        chorus_dsp_lr4_split(&s, sig[0][k], &outs[0][k], &outs[1][k]);
    }
    uint32_t from = frames - frames / 10;
    double al = amplitude(&outs[0][from], frames - from);
    double ah = amplitude(&outs[1][from], frames - from);
    chorus_check(fabs(al - 0.5) <= sine_tol, "%s: the running low branch's amplitude is %.6f",
                 fixture_name, al);
    chorus_check(fabs(ah - 0.5) <= sine_tol, "%s: the running high branch's amplitude is %.6f",
                 fixture_name, ah);
}

static float delay_storage[CHORUS_DSP_DELAY_MAX_FRAMES + 1u];

static void delay(void)
{
    double m_d, frames_d, rate, max_d, refuse_d;
    const char *recipe = get("input");
    char copy[512];
    if (recipe == NULL) {
        fail_parse("no input");
        return;
    }
    snprintf(copy, sizeof(copy), "%s", recipe);
    if (num("delay_frames", &m_d) || num("frames", &frames_d) || num("rate_hz", &rate) ||
        num("max_frames", &max_d) || num("refuse_frames", &refuse_d) ||
        signal(copy, (uint32_t)frames_d, rate, sig[0]) != 0) {
        fail_parse("an unreadable delay fixture");
        return;
    }
    uint32_t m = (uint32_t)m_d, frames = (uint32_t)frames_d;
    chorus_dsp_delay_t d;
    chorus_dsp_delay_init(&d, delay_storage, CHORUS_DSP_DELAY_MAX_FRAMES + 1u, m);
    int exact = 1;
    for (uint32_t k = 0; k < frames; k++) {
        float y = chorus_dsp_delay_process(&d, sig[0][k]);
        float want = (k >= m) ? sig[0][k - m] : 0.0f;
        exact &= memcmp(&y, &want, sizeof(y)) == 0;
    }
    chorus_check(exact, "%s: y(n) = x(n - %u), bit for bit", fixture_name, m);
    chorus_check((uint32_t)max_d == CHORUS_DSP_DELAY_MAX_FRAMES &&
                     chorus_dsp_delay_init(&d, delay_storage, CHORUS_DSP_DELAY_MAX_FRAMES + 1u,
                                           (uint32_t)max_d) == CHORUS_DSP_OK,
                 "%s: %u frames, the maximum, is taken", fixture_name, (uint32_t)max_d);
    chorus_check(chorus_dsp_delay_init(&d, delay_storage, CHORUS_DSP_DELAY_MAX_FRAMES + 1u,
                                       (uint32_t)refuse_d) == CHORUS_DSP_ERR_DELAY_TOO_LONG,
                 "%s: %u frames is refused", fixture_name, (uint32_t)refuse_d);
}

static chorus_dsp_limiter_t limiter_under_test;

static void limiter(void)
{
    double rate, ceiling, la_d, release, frames_d, tail_d;
    if (num("rate_hz", &rate) || num("ceiling", &ceiling) || num("lookahead_frames", &la_d) ||
        num("release_ms", &release) || num("frames", &frames_d) ||
        num("unity_tail_frames", &tail_d)) {
        fail_parse("an unreadable limiter fixture");
        return;
    }
    uint32_t frames = (uint32_t)frames_d, la = (uint32_t)la_d, tail = (uint32_t)tail_d;
    uint32_t n = 0;
    for (; n < CHORUS_DSP_MAX_CHANNELS; n++) {
        char key[16];
        snprintf(key, sizeof(key), "input.%u", n);
        const char *r = get(key);
        if (r == NULL) {
            break;
        }
        char copy[512];
        snprintf(copy, sizeof(copy), "%s", r);
        if (signal(copy, frames, rate, sig[n]) != 0) {
            fail_parse("an unreadable input");
            return;
        }
    }
    const char *check = get("check");
    if (check == NULL || (strcmp(check, "ceiling") != 0 && strcmp(check, "unity") != 0)) {
        fail_parse("an unknown limiter check");
        return;
    }
    chorus_dsp_limiter_t *l = &limiter_under_test;
    chorus_dsp_limiter_init(l, n, la, release, rate);
    chorus_dsp_limiter_set_ceiling(l, (float)ceiling);
    int within = 1, exact = 1;
    float c = (float)ceiling;
    for (uint32_t i = 0; i < frames; i++) {
        float frame[CHORUS_DSP_MAX_CHANNELS];
        for (uint32_t ch = 0; ch < n; ch++) {
            frame[ch] = sig[ch][i];
        }
        chorus_dsp_limiter_process_frame(l, frame);
        for (uint32_t ch = 0; ch < n; ch++) {
            within &= fabsf(frame[ch]) <= c;
            if (i >= frames - tail) {
                float want = (i >= la) ? sig[ch][i - la] : 0.0f;
                exact &= memcmp(&frame[ch], &want, sizeof(float)) == 0;
            }
        }
    }
    chorus_check(within, "%s: no output sample exceeds the ceiling %g", fixture_name, ceiling);
    if (tail > 0) {
        chorus_check(exact, "%s: the last %u frames are the input delayed by %u, bit for bit",
                     fixture_name, tail, la);
    }
}

static void compressor_static(void)
{
    double t, r, w, tol, xs[MAX_LIST], ys[MAX_LIST];
    if (num("threshold_db", &t) || num("ratio", &r) || num("knee_db", &w) ||
        num("tolerance_db", &tol)) {
        fail_parse("an unreadable compressor_static fixture");
        return;
    }
    int n = list("inputs_db", xs, MAX_LIST);
    if (n < 0 || list("expect_db", ys, MAX_LIST) != n) {
        fail_parse("inputs_db and expect_db");
        return;
    }
    for (int i = 0; i < n; i++) {
        double got = chorus_dsp_static_curve_db(xs[i], t, r, w);
        chorus_check(fabs(got - ys[i]) <= tol, "%s: y(%g) = %.9f, want %.9f", fixture_name, xs[i],
                     got, ys[i]);
    }
}

static void compressor_timing(void)
{
    double rate, at, rel, step, tol;
    if (num("rate_hz", &rate) || num("attack_ms", &at) || num("release_ms", &rel) ||
        num("step_db", &step) || num("tolerance_frames", &tol)) {
        fail_parse("an unreadable compressor_timing fixture");
        return;
    }
    for (int phase = 0; phase < 2; phase++) {
        float from = phase == 0 ? 0.0f : (float)step;
        float to = phase == 0 ? (float)step : 0.0f;
        double ms = phase == 0 ? at : rel;
        chorus_dsp_compressor_params_t p = chorus_dsp_night;
        p.attack_ms = at;
        p.release_ms = rel;
        chorus_dsp_compressor_t c;
        chorus_dsp_compressor_init(&c, &p, rate);
        uint32_t settle = (uint32_t)rate * 10u;
        for (uint32_t i = 0; i < settle; i++) {
            chorus_dsp_compressor_smooth(&c, from);
        }
        float a = from + 0.1f * (to - from), b = from + 0.9f * (to - from);
        long t10 = -1, t90 = -1;
        for (uint32_t i = 0; i < settle; i++) {
            float g = chorus_dsp_compressor_smooth(&c, to);
            int past_a = (to < from) ? g <= a : g >= a;
            int past_b = (to < from) ? g <= b : g >= b;
            if (t10 < 0 && past_a) {
                t10 = (long)i;
            }
            if (t90 < 0 && past_b) {
                t90 = (long)i;
                break;
            }
        }
        double want = ms * rate / 1000.0;
        double got = (t10 >= 0 && t90 >= 0) ? (double)(t90 - t10) : NAN;
        chorus_check(fabs(got - want) <= tol, "%s: %s 10-90 %% in %.0f frames, want %.1f",
                     fixture_name, phase == 0 ? "attack" : "release", got, want);
    }
}

static void iso226(void)
{
    double rows[MAX_LIST], af[MAX_LIST], lu[MAX_LIST], tf[MAX_LIST], tol;
    int n = list("table_freqs_hz", rows, MAX_LIST);
    if (n < 0 || list("af", af, MAX_LIST) != n || list("lu", lu, MAX_LIST) != n ||
        list("tf", tf, MAX_LIST) != n || num("tolerance_db", &tol)) {
        fail_parse("an unreadable iso226 fixture");
        return;
    }
    for (int k = 0; k < n; k++) {
        int i = chorus_dsp_iso226_index(rows[k]);
        int ok = i >= 0 && fabs(chorus_dsp_iso226_af[i] - af[k]) <= 1e-12 &&
                 fabs(chorus_dsp_iso226_lu[i] - lu[k]) <= 1e-12 &&
                 fabs(chorus_dsp_iso226_tf[i] - tf[k]) <= 1e-12;
        chorus_check(ok, "%s: the table row at %g Hz is the cited one", fixture_name, rows[k]);
    }
    double freqs[MAX_LIST], phon[MAX_LIST], expect[MAX_LIST];
    int m = list("freqs_hz", freqs, MAX_LIST);
    if (m < 0 || list("phon", phon, MAX_LIST) != m ||
        list("expect_spl_db", expect, MAX_LIST) != m) {
        fail_parse("freqs_hz, phon and expect_spl_db");
        return;
    }
    for (int k = 0; k < m; k++) {
        double got = NAN;
        chorus_dsp_iso226_spl_db(freqs[k], phon[k], &got);
        chorus_check(fabs(got - expect[k]) <= tol, "%s: Lp(%g Hz, %g phon) = %.9f, want %.9f",
                     fixture_name, freqs[k], phon[k], got, expect[k]);
    }
}

static void loudness(void)
{
    double atts[MAX_LIST], lows[MAX_LIST], highs[MAX_LIST], tol, gains[MAX_LIST], qs[MAX_LIST];
    int n = list("attenuations_db", atts, MAX_LIST);
    if (n < 0 || list("expect_low_db", lows, MAX_LIST) != n ||
        list("expect_high_db", highs, MAX_LIST) != n || num("tolerance_db", &tol)) {
        fail_parse("an unreadable loudness fixture");
        return;
    }
    for (int k = 0; k < n; k++) {
        double lo, hi;
        chorus_dsp_loudness_shelf_gains_db(atts[k], &lo, &hi);
        chorus_check(fabs(lo - lows[k]) <= tol && fabs(hi - highs[k]) <= tol,
                     "%s: %g dB down gives shelves of %.6f and %.6f dB", fixture_name, atts[k], lo,
                     hi);
    }
    int m = list("room_gains", gains, MAX_LIST);
    if (m < 0 || list("expect_attenuation_db", qs, MAX_LIST) != m) {
        fail_parse("room_gains and expect_attenuation_db");
        return;
    }
    for (int k = 0; k < m; k++) {
        double got = chorus_dsp_loudness_attenuation_db((float)gains[k]);
        chorus_check(got == qs[k], "%s: room gain %g is %g dB down", fixture_name, gains[k], got);
    }
}

/* --- the chain ------------------------------------------------------------------ */

static chorus_dsp_chain_t chain_under_test;
static float interleaved_in[MAX_FRAMES * CHORUS_DSP_MAX_CHANNELS];
static float interleaved_out[MAX_FRAMES * CHORUS_DSP_MAX_OUTPUTS];
static float first_run[MAX_FRAMES * CHORUS_DSP_MAX_OUTPUTS];

static int read_driver(const char *key, chorus_dsp_driver_t *d)
{
    double v[3];
    if (list(key, v, 3) != 3) {
        return -1;
    }
    d->trim_cdb = (int16_t)v[0];
    d->delay_us = (uint32_t)v[1];
    d->inverted = v[2] != 0.0;
    return 0;
}

/* Runs the fixture's chain in blocks of `block`; returns the output count, or
 * 0 on a refusal. */
static uint32_t run_chain(uint32_t block, uint32_t *frames_out, double *room_out, double *limit_out)
{
    chorus_dsp_sound_t s;
    chorus_dsp_endpoint_t e;
    chorus_dsp_sound_default(&s);
    chorus_dsp_endpoint_default(&e);
    s.bass_db = (int8_t)num_or("bass_db", 0);
    s.treble_db = (int8_t)num_or("treble_db", 0);
    s.loudness = num_or("loudness", 0) != 0;
    s.night = num_or("night", 0) != 0;
    s.speech = num_or("speech", 0) != 0;
    s.room_eq_enabled = num_or("room_eq_enabled", 0) != 0;
    s.sub_polarity_inverted = num_or("sub_polarity_inverted", 0) != 0;
    s.role = (uint8_t)num_or("role", 0);
    s.sub_present = num_or("sub_present", 0) != 0;
    s.crossover_hz = (uint16_t)num_or("crossover_hz", 80);
    s.sub_level_cdb = (int16_t)num_or("sub_level_cdb", 0);
    double eq[3 * CHORUS_DSP_ROOM_EQ_MAX_FILTERS];
    int ne = list("room_eq", eq, 3 * CHORUS_DSP_ROOM_EQ_MAX_FILTERS);
    if (ne > 0) {
        s.eq_count = (uint8_t)(ne / 3);
        for (int k = 0; k < ne / 3; k++) {
            s.eq[k].freq_hz = (uint16_t)eq[3 * k];
            s.eq[k].gain_cdb = (int16_t)eq[3 * k + 1];
            s.eq[k].q_milli = (uint16_t)eq[3 * k + 2];
        }
    }
    if (num_or("two_way", 0) != 0) {
        e.two_way = true;
        e.two_way_hz = (uint32_t)num_or("two_way_hz", 0);
        if (read_driver("woofer", &e.woofer) || read_driver("tweeter", &e.tweeter)) {
            fail_parse("woofer and tweeter");
            return 0;
        }
    }
    double delays[CHORUS_DSP_MAX_OUTPUTS];
    int nd = list("output_delay_us", delays, CHORUS_DSP_MAX_OUTPUTS);
    for (int k = 0; k < nd; k++) {
        e.output_delay_us[k] = (uint32_t)delays[k];
    }
    double rate, frames_d, room, limit, map_d[CHORUS_DSP_MAX_CHANNELS];
    int n = list("channel_map", map_d, CHORUS_DSP_MAX_CHANNELS);
    if (n <= 0 || num("rate_hz", &rate) || num("frames", &frames_d) || num("room_gain", &room) ||
        num("limit_gain", &limit) || frames_d > MAX_FRAMES) {
        fail_parse("an unreadable chain fixture");
        return 0;
    }
    uint8_t map[CHORUS_DSP_MAX_CHANNELS];
    for (int k = 0; k < n; k++) {
        map[k] = (uint8_t)map_d[k];
    }
    const char *field = NULL;
    chorus_dsp_chain_t *c = &chain_under_test;
    if (chorus_dsp_chain_init(c, &s, &e, map, (uint32_t)n, (uint32_t)rate, &field) !=
        CHORUS_DSP_OK) {
        fail_parse("the chain was refused");
        return 0;
    }
    uint32_t frames = (uint32_t)frames_d;
    for (int ch = 0; ch < n; ch++) {
        char key[16], copy[512];
        snprintf(key, sizeof(key), "input.%d", ch);
        const char *r = get(key);
        if (r == NULL) {
            fail_parse("an input is missing");
            return 0;
        }
        snprintf(copy, sizeof(copy), "%s", r);
        if (signal(copy, frames, rate, sig[ch]) != 0) {
            fail_parse("an unreadable input");
            return 0;
        }
        for (uint32_t i = 0; i < frames; i++) {
            interleaved_in[(size_t)i * (uint32_t)n + (uint32_t)ch] = sig[ch][i];
        }
    }
    uint32_t o = chorus_dsp_chain_out_channels(c);
    for (uint32_t at = 0; at < frames; at += block) {
        uint32_t k = (frames - at < block) ? frames - at : block;
        chorus_dsp_chain_process(c, &interleaved_in[(size_t)at * (uint32_t)n],
                                 &interleaved_out[(size_t)at * o], k, (float)room, (float)limit);
    }
    *frames_out = frames;
    *room_out = room;
    *limit_out = limit;
    return o;
}

static void chain(void)
{
    uint32_t frames = 0;
    double room = 0, limit = 0;
    uint32_t o = run_chain(37, &frames, &room, &limit);
    if (o == 0) {
        return;
    }
    memcpy(first_run, interleaved_out, (size_t)frames * o * sizeof(float));
    o = run_chain(4096, &frames, &room, &limit);
    chorus_check(memcmp(first_run, interleaved_out, (size_t)frames * o * sizeof(float)) == 0,
                 "%s: the output does not depend on the block size", fixture_name);
    for (uint32_t k = 0; k < o; k++) {
        for (uint32_t i = 0; i < frames; i++) {
            outs[k][i] = interleaved_out[(size_t)i * o + k];
        }
    }
    chorus_dsp_chain_t *c = &chain_under_test;
    double want_outs;
    if (num("out_channels", &want_outs) == 0) {
        chorus_check(o == (uint32_t)want_outs, "%s: %u outputs", fixture_name, o);
    }
    uint32_t lat = chorus_dsp_chain_latency_frames(c);
    double want_lat;
    if (num("latency_frames", &want_lat) == 0) {
        chorus_check(lat == (uint32_t)want_lat, "%s: latency %u frames", fixture_name, lat);
    }
    for (uint32_t k = 0; k < o; k++) {
        char key[32];
        double v;
        snprintf(key, sizeof(key), "exact.%u", k);
        if (num(key, &v) == 0) {
            uint32_t ch = (uint32_t)v;
            int ok = 1;
            for (uint32_t i = 0; i < frames; i++) {
                float want = (i >= lat) ? sig[ch][i - lat] * (float)room : 0.0f;
                ok &= memcmp(&outs[k][i], &want, sizeof(float)) == 0;
            }
            chorus_check(ok, "%s: output %u is input %u times the gain, delayed, bit for bit",
                         fixture_name, k, ch);
        }
        snprintf(key, sizeof(key), "amplitude.%u", k);
        if (num(key, &v) == 0) {
            uint32_t from = (uint32_t)num_or("amplitude_from", 0);
            double tol = num_or("amplitude_tolerance", 0);
            double got = amplitude(&outs[k][from], frames - from);
            chorus_check(fabs(got - v) <= tol, "%s: output %u has amplitude %.6f, want %.6f",
                         fixture_name, k, got, v);
        }
        snprintf(key, sizeof(key), "samples.%u", k);
        if (get(key) != NULL) {
            double want[MAX_LIST];
            int m = list(key, want, MAX_LIST);
            uint32_t from = (uint32_t)num_or("samples_from", 0);
            double tol = num_or("samples_tolerance", 0);
            double worst = 0;
            for (int i = 0; i < m; i++) {
                double d = fabs((double)outs[k][from + (uint32_t)i] - want[i]);
                if (d > worst) {
                    worst = d;
                }
            }
            chorus_check(m > 0 && worst <= tol,
                         "%s: output %u is within %.2e of the Rust chain's %d samples",
                         fixture_name, k, worst, m);
        }
    }
    double want_sum;
    if (num("sum_amplitude", &want_sum) == 0) {
        uint32_t from = (uint32_t)num_or("amplitude_from", 0);
        double tol = num_or("amplitude_tolerance", 0);
        for (uint32_t i = from; i < frames; i++) {
            float s = 0.0f;
            for (uint32_t k = 0; k < o; k++) {
                s += outs[k][i];
            }
            sig[0][i - from] = s;
        }
        double got = amplitude(sig[0], frames - from);
        chorus_check(fabs(got - want_sum) <= tol, "%s: the outputs' sum has amplitude %.6f",
                     fixture_name, got);
    }
    if (get("ceiling_holds") != NULL) {
        float ceiling = (limit < 1.0) ? (float)limit : 1.0f;
        int ok = 1;
        for (uint32_t k = 0; k < o; k++) {
            for (uint32_t i = 0; i < frames; i++) {
                ok &= fabsf(outs[k][i]) <= ceiling;
            }
        }
        chorus_check(ok, "%s: no output sample exceeds %g", fixture_name, (double)ceiling);
    }
}

/* --- the walk --------------------------------------------------------------------- */

static int compare_names(const void *a, const void *b)
{
    return strcmp((const char *)a, (const char *)b);
}

static void run_fixtures(void)
{
    char dir_path[1024];
    chorus_repo_path(dir_path, sizeof(dir_path), "fixtures/dsp");
    chorus_section("fixtures/dsp: every fixture, against the C mirror");
    DIR *dir = opendir(dir_path);
    chorus_check(dir != NULL, "%s opens", dir_path);
    if (dir == NULL) {
        return;
    }
    static char names[256][256];
    unsigned count = 0;
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL && count < 256) {
        size_t len = strlen(entry->d_name);
        if (entry->d_name[0] == '.' || len >= sizeof(names[0])) {
            continue;
        }
        memcpy(names[count++], entry->d_name, len + 1);
    }
    closedir(dir);
    qsort(names, count, sizeof(names[0]), compare_names);
    chorus_check(count > 0, "fixtures/dsp holds %u fixtures", count);
    for (unsigned i = 0; i < count; i++) {
        char path[1400];
        size_t dl = strlen(dir_path), nl = strlen(names[i]);
        if (dl + 1 + nl + 1 > sizeof(path)) {
            chorus_check(0, "%s: the path fits", names[i]);
            continue;
        }
        memcpy(path, dir_path, dl);
        path[dl] = '/';
        memcpy(path + dl + 1, names[i], nl + 1);
        fixture_name = names[i];
        if (fixture_read(path, text, sizeof(text)) < 0) {
            chorus_check(0, "%s reads", names[i]);
            continue;
        }
        chorus_check(get("source") != NULL && get("read") != NULL, "%s: cites a source and a date",
                     names[i]);
        const char *kind = get("kind");
        char k[64];
        snprintf(k, sizeof(k), "%s", kind == NULL ? "" : kind);
        if (strcmp(k, "biquad_coefficients") == 0) {
            biquad_coefficients();
        } else if (strcmp(k, "biquad_magnitude") == 0) {
            biquad_magnitude();
        } else if (strcmp(k, "biquad_reference") == 0) {
            biquad_reference();
        } else if (strcmp(k, "lr4") == 0) {
            lr4();
        } else if (strcmp(k, "delay") == 0) {
            delay();
        } else if (strcmp(k, "limiter") == 0) {
            limiter();
        } else if (strcmp(k, "compressor_static") == 0) {
            compressor_static();
        } else if (strcmp(k, "compressor_timing") == 0) {
            compressor_timing();
        } else if (strcmp(k, "iso226") == 0) {
            iso226();
        } else if (strcmp(k, "loudness") == 0) {
            loudness();
        } else if (strcmp(k, "chain") == 0) {
            chain();
        } else {
            chorus_check(0, "%s: unknown kind '%s'", names[i], k);
        }
    }
}

/* --- C's own: refusals and maximums ----------------------------------------------- */

static void test_refusals(void)
{
    chorus_section("refusals name the field, and change nothing");
    chorus_dsp_sound_t s;
    chorus_dsp_endpoint_t e;
    chorus_dsp_sound_default(&s);
    chorus_dsp_endpoint_default(&e);
    const char *field = NULL;
    static const struct {
        const char *name;
        int which;
    } cases[] = {
        {"bass_db", 0},      {"treble_db", 1},     {"role", 2},
        {"crossover_hz", 3}, {"sub_level_cdb", 4}, {"eq_count", 5},
        {"freq_hz", 6},      {"gain_cdb", 7},      {"q_milli", 8},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        chorus_dsp_sound_t bad = s;
        bad.eq_count = 1;
        bad.eq[0] = (chorus_dsp_eq_filter_t){100, -300, 1000};
        switch (cases[i].which) {
        case 0:
            bad.bass_db = 11;
            break;
        case 1:
            bad.treble_db = -11;
            break;
        case 2:
            bad.role = 19;
            break;
        case 3:
            bad.crossover_hz = 39;
            break;
        case 4:
            bad.sub_level_cdb = 601;
            break;
        case 5:
            bad.eq_count = 9;
            break;
        case 6:
            bad.eq[0].freq_hz = 1001;
            break;
        case 7:
            bad.eq[0].gain_cdb = 301;
            break;
        default:
            bad.eq[0].q_milli = 499;
            break;
        }
        field = NULL;
        chorus_dsp_err_t rc = chorus_dsp_sound_validate(&bad, &field);
        chorus_check(rc == CHORUS_DSP_ERR_SETTING && field != NULL &&
                         strcmp(field, cases[i].name) == 0,
                     "an out-of-range %s is refused by name (%s)", cases[i].name,
                     field == NULL ? "none" : field);
    }
    uint8_t stereo[2] = {1, 2}, twice[2] = {1, 1}, mono_pair[2] = {0, 1};
    chorus_dsp_chain_t *c = &chain_under_test;
    chorus_check(chorus_dsp_chain_init(c, &s, &e, twice, 2, 48000, NULL) ==
                     CHORUS_DSP_ERR_CHANNEL_MAP,
                 "a position twice is refused");
    chorus_check(chorus_dsp_chain_init(c, &s, &e, mono_pair, 2, 48000, NULL) ==
                     CHORUS_DSP_ERR_CHANNEL_MAP,
                 "MONO in a two-channel stream is refused");
    chorus_check(chorus_dsp_chain_init(c, &s, &e, stereo, 2, 7999, NULL) == CHORUS_DSP_ERR_RATE,
                 "a rate below 8000 Hz is refused");
    chorus_dsp_endpoint_t far = e;
    far.output_delay_us[0] = 60000;
    chorus_check(chorus_dsp_chain_init(c, &s, &far, stereo, 2, 96000, NULL) ==
                     CHORUS_DSP_ERR_DELAY_TOO_LONG,
                 "a delay above 50 ms at 96 kHz is refused");
    chorus_dsp_endpoint_t three = e;
    three.output_delay_us[0] = 50000;
    three.output_delay_us[1] = 50000;
    chorus_dsp_sound_t pass = s;
    uint8_t quad[3] = {1, 2, 3};
    three.output_delay_us[2] = 1000;
    chorus_check(chorus_dsp_chain_init(c, &pass, &three, quad, 3, 96000, NULL) ==
                     CHORUS_DSP_ERR_DELAY_TOO_LONG,
                 "delays beyond the pool (%u frames) are refused", CHORUS_DSP_DELAY_POOL_FRAMES);
    chorus_dsp_endpoint_t loud = e;
    loud.two_way = true;
    loud.two_way_hz = 2000;
    loud.tweeter.trim_cdb = 1;
    field = NULL;
    chorus_check(chorus_dsp_chain_init(c, &s, &loud, stereo, 2, 48000, &field) ==
                         CHORUS_DSP_ERR_ENDPOINT &&
                     field != NULL && strcmp(field, "trim_cdb") == 0,
                 "a driver trim above 0 dB is refused (trims cut only)");

    chorus_check(chorus_dsp_chain_init(c, &s, &e, stereo, 2, 48000, NULL) == CHORUS_DSP_OK,
                 "a flat stereo chain initialises");
    chorus_dsp_sound_t bad = s;
    bad.bass_db = 11;
    chorus_check(chorus_dsp_chain_set_sound(c, &bad, NULL) == CHORUS_DSP_ERR_SETTING &&
                     c->sound.bass_db == 0,
                 "a refused set_sound changes nothing");
    chorus_dsp_limiter_t *l = &limiter_under_test;
    chorus_check(chorus_dsp_limiter_init(l, 8, CHORUS_DSP_LIMITER_MAX_LOOKAHEAD, 100.0, 384000.0) ==
                         CHORUS_DSP_OK &&
                     chorus_dsp_limiter_init(l, 8, CHORUS_DSP_LIMITER_MAX_LOOKAHEAD + 1u, 100.0,
                                             384000.0) == CHORUS_DSP_ERR_LOOKAHEAD,
                 "the look-ahead's fixed maximum is %u frames", CHORUS_DSP_LIMITER_MAX_LOOKAHEAD);
    printf("chorus_dsp_chain_t is %zu bytes (fixed; no heap)\n", sizeof(chorus_dsp_chain_t));
}

static void test_gain_change_keeps_state(void)
{
    chorus_section("a gain change keeps every running filter's state");
    chorus_dsp_sound_t s;
    chorus_dsp_endpoint_t e;
    chorus_dsp_sound_default(&s);
    chorus_dsp_endpoint_default(&e);
    s.bass_db = 6;
    uint8_t stereo[2] = {1, 2};
    chorus_dsp_chain_t *c = &chain_under_test;
    chorus_dsp_chain_init(c, &s, &e, stereo, 2, 48000, NULL);
    float in[2 * 64], out[2 * 64];
    for (int i = 0; i < 128; i++) {
        in[i] = 0.25f;
    }
    chorus_dsp_chain_process(c, in, out, 64, 1.0f, 1.0f);
    float z1 = c->tone[0][0].z1;
    s.bass_db = 3;
    chorus_dsp_chain_set_sound(c, &s, NULL);
    chorus_check(c->tone[0][0].z1 == z1 && z1 != 0.0f,
                 "the bass shelf's state survives a bass change (z1 %g)", (double)z1);
}

/* THE room-correction bounds exist twice in C: the library's
 * CHORUS_DSP_ROOM_EQ_* (what the chain designs) and the wire codec's
 * CHORUS_V2_SOUND_EQ_* (what a `sound` message may carry). Equal, or a filter
 * the wire accepts could be one the chain refuses (or the reverse). Asserted
 * at compile time, and counted here so the run says it was checked. The Rust
 * pair is crates/server/tests/room_eq_bounds_agree.rs. */
_Static_assert(CHORUS_DSP_ROOM_EQ_MAX_FILTERS == CHORUS_V2_SOUND_EQ_MAX_FILTERS, "max filters");
_Static_assert(CHORUS_DSP_ROOM_EQ_FREQ_MIN_HZ == CHORUS_V2_SOUND_EQ_FREQ_HZ_MIN, "freq min");
_Static_assert(CHORUS_DSP_ROOM_EQ_FREQ_MAX_HZ == CHORUS_V2_SOUND_EQ_FREQ_HZ_MAX, "freq max");
_Static_assert(CHORUS_DSP_ROOM_EQ_GAIN_MIN_CDB == CHORUS_V2_SOUND_EQ_GAIN_CDB_MIN, "gain min");
_Static_assert(CHORUS_DSP_ROOM_EQ_GAIN_MAX_CDB == CHORUS_V2_SOUND_EQ_GAIN_CDB_MAX, "gain max");
_Static_assert(CHORUS_DSP_ROOM_EQ_Q_MIN_MILLI == CHORUS_V2_SOUND_EQ_Q_MILLI_MIN, "q min");
_Static_assert(CHORUS_DSP_ROOM_EQ_Q_MAX_MILLI == CHORUS_V2_SOUND_EQ_Q_MILLI_MAX, "q max");

static void test_room_eq_bounds_match_the_wire(void)
{
    chorus_section("the library's room-EQ bounds are the wire's");
    chorus_check(CHORUS_DSP_ROOM_EQ_MAX_FILTERS == CHORUS_V2_SOUND_EQ_MAX_FILTERS,
                 "at most %u filters on both", CHORUS_DSP_ROOM_EQ_MAX_FILTERS);
    chorus_check(CHORUS_DSP_ROOM_EQ_FREQ_MIN_HZ == CHORUS_V2_SOUND_EQ_FREQ_HZ_MIN &&
                     CHORUS_DSP_ROOM_EQ_FREQ_MAX_HZ == CHORUS_V2_SOUND_EQ_FREQ_HZ_MAX,
                 "%u..%u Hz on both", CHORUS_DSP_ROOM_EQ_FREQ_MIN_HZ,
                 CHORUS_DSP_ROOM_EQ_FREQ_MAX_HZ);
    chorus_check(CHORUS_DSP_ROOM_EQ_GAIN_MIN_CDB == CHORUS_V2_SOUND_EQ_GAIN_CDB_MIN &&
                     CHORUS_DSP_ROOM_EQ_GAIN_MAX_CDB == CHORUS_V2_SOUND_EQ_GAIN_CDB_MAX,
                 "%d..%d centi-dB on both", CHORUS_DSP_ROOM_EQ_GAIN_MIN_CDB,
                 CHORUS_DSP_ROOM_EQ_GAIN_MAX_CDB);
    chorus_check(CHORUS_DSP_ROOM_EQ_Q_MIN_MILLI == CHORUS_V2_SOUND_EQ_Q_MILLI_MIN &&
                     CHORUS_DSP_ROOM_EQ_Q_MAX_MILLI == CHORUS_V2_SOUND_EQ_Q_MILLI_MAX,
                 "Q %u..%u thousandths on both", CHORUS_DSP_ROOM_EQ_Q_MIN_MILLI,
                 CHORUS_DSP_ROOM_EQ_Q_MAX_MILLI);
}

int main(void)
{
    run_fixtures();
    test_refusals();
    test_gain_change_keeps_state();
    test_room_eq_bounds_match_the_wire();
    return chorus_test_report("test_dsp");
}
