/* The DSP library, the C mirror of crates/dsp (goal 12). Every function here
 * is the Rust function of the same name, written in the same order with the
 * same roundings: float operations carry the f suffix so nothing is promoted
 * to double (one float operation rounded once is the Rust f32 operation),
 * and the designs are double throughout, rounded to float once. What each
 * block does and every citation: the Rust modules' comments and docs/dsp.md;
 * this file says only where C differs in form. */

#include "chorus/dsp.h"

#include <math.h>
#include <string.h>

#define PI 3.14159265358979323846

double chorus_dsp_db_to_gain(double db)
{
    return pow(10.0, db / 20.0);
}

/* --- 1. biquads ---------------------------------------------------------- */

const chorus_dsp_coeffs_t chorus_dsp_identity = {1.0, 0.0, 0.0, 0.0, 0.0};

int chorus_dsp_kind_from_name(const char *name, chorus_dsp_kind_t *kind)
{
    static const struct {
        const char *name;
        chorus_dsp_kind_t kind;
    } names[] = {
        {"lowpass", CHORUS_DSP_LOWPASS},   {"highpass", CHORUS_DSP_HIGHPASS},
        {"bandpass", CHORUS_DSP_BANDPASS}, {"notch", CHORUS_DSP_NOTCH},
        {"allpass", CHORUS_DSP_ALLPASS},   {"peaking", CHORUS_DSP_PEAKING},
        {"lowshelf", CHORUS_DSP_LOWSHELF}, {"highshelf", CHORUS_DSP_HIGHSHELF},
    };
    for (size_t i = 0; i < sizeof(names) / sizeof(names[0]); i++) {
        if (strcmp(name, names[i].name) == 0) {
            *kind = names[i].kind;
            return 0;
        }
    }
    return -1;
}

chorus_dsp_err_t chorus_dsp_biquad_design(chorus_dsp_kind_t kind, double rate_hz, double f0_hz,
                                          double q, double gain_db, chorus_dsp_coeffs_t *out)
{
    if (!(isfinite(rate_hz) && rate_hz > 0.0)) {
        return CHORUS_DSP_ERR_RATE;
    }
    if (!(isfinite(f0_hz) && f0_hz > 0.0 && f0_hz < rate_hz / 2.0)) {
        return CHORUS_DSP_ERR_FREQUENCY;
    }
    if (!(isfinite(q) && q > 0.0)) {
        return CHORUS_DSP_ERR_Q;
    }
    if (!isfinite(gain_db)) {
        return CHORUS_DSP_ERR_GAIN;
    }
    double a = pow(10.0, gain_db / 40.0);
    double w0 = 2.0 * PI * f0_hz / rate_hz;
    double cos_w0 = cos(w0);
    double sin_w0 = sin(w0);
    double alpha = sin_w0 / (2.0 * q);
    double b0, b1, b2, a0, a1, a2;
    double sa = 2.0 * sqrt(a) * alpha;
    switch (kind) {
    case CHORUS_DSP_LOWPASS:
        b0 = (1.0 - cos_w0) / 2.0;
        b1 = 1.0 - cos_w0;
        b2 = (1.0 - cos_w0) / 2.0;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    case CHORUS_DSP_HIGHPASS:
        b0 = (1.0 + cos_w0) / 2.0;
        b1 = -(1.0 + cos_w0);
        b2 = (1.0 + cos_w0) / 2.0;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    case CHORUS_DSP_BANDPASS:
        b0 = alpha;
        b1 = 0.0;
        b2 = -alpha;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    case CHORUS_DSP_NOTCH:
        b0 = 1.0;
        b1 = -2.0 * cos_w0;
        b2 = 1.0;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    case CHORUS_DSP_ALLPASS:
        b0 = 1.0 - alpha;
        b1 = -2.0 * cos_w0;
        b2 = 1.0 + alpha;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    case CHORUS_DSP_PEAKING:
        b0 = 1.0 + alpha * a;
        b1 = -2.0 * cos_w0;
        b2 = 1.0 - alpha * a;
        a0 = 1.0 + alpha / a;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha / a;
        break;
    case CHORUS_DSP_LOWSHELF:
        b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + sa);
        b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - sa);
        a0 = (a + 1.0) + (a - 1.0) * cos_w0 + sa;
        a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        a2 = (a + 1.0) + (a - 1.0) * cos_w0 - sa;
        break;
    case CHORUS_DSP_HIGHSHELF:
        b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + sa);
        b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - sa);
        a0 = (a + 1.0) - (a - 1.0) * cos_w0 + sa;
        a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        a2 = (a + 1.0) - (a - 1.0) * cos_w0 - sa;
        break;
    default:
        return CHORUS_DSP_ERR_FREQUENCY;
    }
    out->b0 = b0 / a0;
    out->b1 = b1 / a0;
    out->b2 = b2 / a0;
    out->a1 = a1 / a0;
    out->a2 = a2 / a0;
    return CHORUS_DSP_OK;
}

static void complex_mul(double ar, double ai, double br, double bi, double *re, double *im)
{
    *re = ar * br - ai * bi;
    *im = ar * bi + ai * br;
}

static void complex_div(double ar, double ai, double br, double bi, double *re, double *im)
{
    double d = br * br + bi * bi;
    *re = (ar * br + ai * bi) / d;
    *im = (ai * br - ar * bi) / d;
}

void chorus_dsp_response(const chorus_dsp_coeffs_t *c, double freq_hz, double rate_hz, double *re,
                         double *im)
{
    double w = 2.0 * PI * freq_hz / rate_hz;
    double c1 = cos(w), s1 = -sin(w);
    double c2 = cos(2.0 * w), s2 = -sin(2.0 * w);
    double nr = c->b0 + c->b1 * c1 + c->b2 * c2, ni = c->b1 * s1 + c->b2 * s2;
    double dr = 1.0 + c->a1 * c1 + c->a2 * c2, di = c->a1 * s1 + c->a2 * s2;
    complex_div(nr, ni, dr, di, re, im);
}

double chorus_dsp_magnitude_db(const chorus_dsp_coeffs_t *c, double freq_hz, double rate_hz)
{
    double re, im;
    chorus_dsp_response(c, freq_hz, rate_hz, &re, &im);
    return 10.0 * log10(re * re + im * im);
}

void chorus_dsp_biquad_set(chorus_dsp_biquad_t *q, const chorus_dsp_coeffs_t *c)
{
    q->b0 = (float)c->b0;
    q->b1 = (float)c->b1;
    q->b2 = (float)c->b2;
    q->a1 = (float)c->a1;
    q->a2 = (float)c->a2;
}

void chorus_dsp_biquad_reset(chorus_dsp_biquad_t *q)
{
    q->z1 = 0.0f;
    q->z2 = 0.0f;
}

void chorus_dsp_biquad_init(chorus_dsp_biquad_t *q, const chorus_dsp_coeffs_t *c)
{
    chorus_dsp_biquad_set(q, c);
    chorus_dsp_biquad_reset(q);
}

float chorus_dsp_biquad_process(chorus_dsp_biquad_t *q, float x)
{
    float y = q->b0 * x + q->z1;
    q->z1 = q->b1 * x - q->a1 * y + q->z2;
    q->z2 = q->b2 * x - q->a2 * y;
    return y;
}

/* --- 2. LR4 -------------------------------------------------------------- */

#define BUTTERWORTH_Q 0.70710678118654752440

chorus_dsp_err_t chorus_dsp_lr4_design(double rate_hz, double crossover_hz,
                                       chorus_dsp_lr4_design_t *out)
{
    chorus_dsp_lr4_design_t d;
    chorus_dsp_err_t rc = chorus_dsp_biquad_design(CHORUS_DSP_LOWPASS, rate_hz, crossover_hz,
                                                   BUTTERWORTH_Q, 0.0, &d.low);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    rc = chorus_dsp_biquad_design(CHORUS_DSP_HIGHPASS, rate_hz, crossover_hz, BUTTERWORTH_Q, 0.0,
                                  &d.high);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    *out = d;
    return CHORUS_DSP_OK;
}

void chorus_dsp_lr4_response_low(const chorus_dsp_lr4_design_t *d, double freq_hz, double rate_hz,
                                 double *re, double *im)
{
    double r, i;
    chorus_dsp_response(&d->low, freq_hz, rate_hz, &r, &i);
    complex_mul(r, i, r, i, re, im);
}

void chorus_dsp_lr4_response_high(const chorus_dsp_lr4_design_t *d, double freq_hz, double rate_hz,
                                  double *re, double *im)
{
    double r, i;
    chorus_dsp_response(&d->high, freq_hz, rate_hz, &r, &i);
    complex_mul(r, i, r, i, re, im);
}

void chorus_dsp_lr4_set(chorus_dsp_lr4_t *s, const chorus_dsp_lr4_design_t *d)
{
    for (int k = 0; k < 2; k++) {
        chorus_dsp_biquad_set(&s->low[k], &d->low);
        chorus_dsp_biquad_set(&s->high[k], &d->high);
    }
}

void chorus_dsp_lr4_reset(chorus_dsp_lr4_t *s)
{
    for (int k = 0; k < 2; k++) {
        chorus_dsp_biquad_reset(&s->low[k]);
        chorus_dsp_biquad_reset(&s->high[k]);
    }
}

void chorus_dsp_lr4_init(chorus_dsp_lr4_t *s, const chorus_dsp_lr4_design_t *d)
{
    chorus_dsp_lr4_set(s, d);
    chorus_dsp_lr4_reset(s);
}

float chorus_dsp_lr4_low(chorus_dsp_lr4_t *s, float x)
{
    float v = chorus_dsp_biquad_process(&s->low[0], x);
    return chorus_dsp_biquad_process(&s->low[1], v);
}

float chorus_dsp_lr4_high(chorus_dsp_lr4_t *s, float x)
{
    float v = chorus_dsp_biquad_process(&s->high[0], x);
    return chorus_dsp_biquad_process(&s->high[1], v);
}

void chorus_dsp_lr4_split(chorus_dsp_lr4_t *s, float x, float *low, float *high)
{
    *low = chorus_dsp_lr4_low(s, x);
    *high = chorus_dsp_lr4_high(s, x);
}

/* --- 3. delay ------------------------------------------------------------ */

uint64_t chorus_dsp_frames_for_us(uint32_t delay_us, uint32_t rate_hz)
{
    return ((uint64_t)delay_us * (uint64_t)rate_hz + 500000u) / 1000000u;
}

chorus_dsp_err_t chorus_dsp_delay_init(chorus_dsp_delay_t *d, float *storage, uint32_t capacity,
                                       uint32_t frames)
{
    if (frames > CHORUS_DSP_DELAY_MAX_FRAMES || frames > capacity) {
        return CHORUS_DSP_ERR_DELAY_TOO_LONG;
    }
    d->line = storage;
    d->frames = frames;
    chorus_dsp_delay_reset(d);
    return CHORUS_DSP_OK;
}

void chorus_dsp_delay_reset(chorus_dsp_delay_t *d)
{
    for (uint32_t i = 0; i < d->frames; i++) {
        d->line[i] = 0.0f;
    }
    d->pos = 0;
}

float chorus_dsp_delay_process(chorus_dsp_delay_t *d, float x)
{
    if (d->frames == 0) {
        return x;
    }
    float y = d->line[d->pos];
    d->line[d->pos] = x;
    d->pos++;
    if (d->pos == d->frames) {
        d->pos = 0;
    }
    return y;
}

/* --- 4. limiter ---------------------------------------------------------- */

chorus_dsp_err_t chorus_dsp_limiter_init(chorus_dsp_limiter_t *l, uint32_t channels,
                                         uint32_t lookahead_frames, double release_ms,
                                         double rate_hz)
{
    if (lookahead_frames > CHORUS_DSP_LIMITER_MAX_LOOKAHEAD) {
        return CHORUS_DSP_ERR_LOOKAHEAD;
    }
    if (channels > CHORUS_DSP_MAX_OUTPUTS) {
        return CHORUS_DSP_ERR_BUFFER;
    }
    if (!(isfinite(release_ms) && release_ms > 0.0 && rate_hz > 0.0)) {
        return CHORUS_DSP_ERR_TIME_CONSTANT;
    }
    l->release = (float)exp(-1.0 / (release_ms / 1000.0 * rate_hz));
    l->channels = channels;
    l->len = lookahead_frames + 1u;
    for (uint32_t c = 0; c < channels; c++) {
        (void)chorus_dsp_delay_init(&l->lines[c], l->storage[c], CHORUS_DSP_LIMITER_MAX_LOOKAHEAD,
                                    lookahead_frames);
    }
    for (uint32_t d = 0; d < l->len; d++) {
        l->reciprocal[d] = 1.0f / ((float)d + 1.0f);
    }
    l->ceiling = 1.0f;
    chorus_dsp_limiter_reset(l);
    return CHORUS_DSP_OK;
}

void chorus_dsp_limiter_set_ceiling(chorus_dsp_limiter_t *l, float ceiling)
{
    l->ceiling = (ceiling > 0.0f) ? ceiling : 0.0f;
}

void chorus_dsp_limiter_reset(chorus_dsp_limiter_t *l)
{
    for (uint32_t c = 0; c < l->channels; c++) {
        chorus_dsp_delay_reset(&l->lines[c]);
    }
    for (uint32_t d = 0; d < l->len; d++) {
        l->required[d] = 1.0f;
    }
    l->newest = 0;
    l->gain = 1.0f;
    l->gap = 0.0f;
}

void chorus_dsp_limiter_process_frame(chorus_dsp_limiter_t *l, float *frame)
{
    float ceiling = l->ceiling;
    float peak = 0.0f;
    for (uint32_t c = 0; c < l->channels; c++) {
        float a = fabsf(frame[c]);
        if (a > peak) {
            peak = a;
        }
    }
    float r = (peak > ceiling) ? ceiling / peak : 1.0f;
    uint32_t len = l->len;
    l->newest++;
    if (l->newest == len) {
        l->newest = 0;
    }
    l->required[l->newest] = r;

    float previous = l->gain;
    float gap = l->gap * l->release;
    if (gap < CHORUS_DSP_LIMITER_RELEASE_SNAP) {
        gap = 0.0f;
    }
    float release = 1.0f - gap;
    float g = release;
    uint32_t idx = l->newest + 1u;
    if (idx == len) {
        idx = 0;
    }
    if (l->required[idx] < g) {
        g = l->required[idx];
    }
    for (uint32_t d = 1; d < len; d++) {
        idx++;
        if (idx == len) {
            idx = 0;
        }
        float req = l->required[idx];
        if (req < previous) {
            float term = previous + (req - previous) * l->reciprocal[d];
            if (term < g) {
                g = term;
            }
        }
    }
    l->gain = g;
    l->gap = (g < release) ? 1.0f - g : gap;

    for (uint32_t c = 0; c < l->channels; c++) {
        float y = chorus_dsp_delay_process(&l->lines[c], frame[c]) * g;
        if (y > ceiling) {
            y = ceiling;
        } else if (y < -ceiling) {
            y = -ceiling;
        } else if (isnan(y)) {
            y = 0.0f;
        }
        frame[c] = y;
    }
}

/* --- 5. compressor ------------------------------------------------------- */

const chorus_dsp_compressor_params_t chorus_dsp_night = {
    .threshold_db = -24.0,
    .ratio = 3.0,
    .knee_db = 12.0,
    .attack_ms = 10.0,
    .release_ms = 500.0,
    .makeup_db = 6.0,
};

#define LN10_OVER_20 0.11512925f
#define LEVEL_FLOOR 1e-10f

double chorus_dsp_static_curve_db(double x_db, double threshold_db, double ratio, double knee_db)
{
    double over = x_db - threshold_db;
    if (2.0 * over < -knee_db) {
        return x_db;
    }
    if (knee_db > 0.0 && 2.0 * fabs(over) <= knee_db) {
        double t = over + knee_db / 2.0;
        return x_db + (1.0 / ratio - 1.0) * t * t / (2.0 * knee_db);
    }
    return threshold_db + over / ratio;
}

static float static_curve_db_f32(float x_db, float threshold_db, float ratio, float knee_db)
{
    float over = x_db - threshold_db;
    if (2.0f * over < -knee_db) {
        return x_db;
    }
    if (knee_db > 0.0f && 2.0f * fabsf(over) <= knee_db) {
        float t = over + knee_db / 2.0f;
        return x_db + (1.0f / ratio - 1.0f) * t * t / (2.0f * knee_db);
    }
    return threshold_db + over / ratio;
}

chorus_dsp_err_t chorus_dsp_ballistics(double time_ms, double rate_hz, float *out)
{
    if (!(isfinite(time_ms) && time_ms > 0.0 && rate_hz > 0.0)) {
        return CHORUS_DSP_ERR_TIME_CONSTANT;
    }
    *out = (float)exp(-log(9.0) / (rate_hz * time_ms / 1000.0));
    return CHORUS_DSP_OK;
}

chorus_dsp_err_t chorus_dsp_compressor_init(chorus_dsp_compressor_t *c,
                                            const chorus_dsp_compressor_params_t *p, double rate_hz)
{
    if (!(isfinite(p->ratio) && p->ratio >= 1.0)) {
        return CHORUS_DSP_ERR_GAIN;
    }
    if (!(isfinite(p->threshold_db) && isfinite(p->knee_db) && p->knee_db >= 0.0 &&
          isfinite(p->makeup_db))) {
        return CHORUS_DSP_ERR_GAIN;
    }
    float attack, release;
    chorus_dsp_err_t rc = chorus_dsp_ballistics(p->attack_ms, rate_hz, &attack);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    rc = chorus_dsp_ballistics(p->release_ms, rate_hz, &release);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    c->threshold_db = (float)p->threshold_db;
    c->ratio = (float)p->ratio;
    c->knee_db = (float)p->knee_db;
    c->makeup_db = (float)p->makeup_db;
    c->attack = attack;
    c->release = release;
    c->smoothed_db = 0.0f;
    return CHORUS_DSP_OK;
}

void chorus_dsp_compressor_reset(chorus_dsp_compressor_t *c)
{
    c->smoothed_db = 0.0f;
}

float chorus_dsp_compressor_smooth(chorus_dsp_compressor_t *c, float gc_db)
{
    float a = (gc_db <= c->smoothed_db) ? c->attack : c->release;
    c->smoothed_db = a * c->smoothed_db + (1.0f - a) * gc_db;
    return c->smoothed_db;
}

void chorus_dsp_compressor_process_frame(chorus_dsp_compressor_t *c, float *frame, uint32_t n)
{
    float peak = LEVEL_FLOOR;
    for (uint32_t i = 0; i < n; i++) {
        float a = fabsf(frame[i]);
        if (a > peak) {
            peak = a;
        }
    }
    float x_db = 20.0f * log10f(peak);
    float y_db = static_curve_db_f32(x_db, c->threshold_db, c->ratio, c->knee_db);
    float gs = chorus_dsp_compressor_smooth(c, y_db - x_db);
    float g = expf((gs + c->makeup_db) * LN10_OVER_20);
    for (uint32_t i = 0; i < n; i++) {
        frame[i] *= g;
    }
}

/* --- 6. loudness ----------------------------------------------------------- */

/* The ISO 226:2003 parameter table, as crates/dsp/src/loudness.rs cites it. */
const double chorus_dsp_iso226_freqs[CHORUS_DSP_ISO226_ROWS] = {
    20.0,   25.0,   31.5,   40.0,   50.0,   63.0,   80.0,   100.0,   125.0,   160.0,
    200.0,  250.0,  315.0,  400.0,  500.0,  630.0,  800.0,  1000.0,  1250.0,  1600.0,
    2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0, 8000.0, 10000.0, 12500.0,
};
const double chorus_dsp_iso226_af[CHORUS_DSP_ISO226_ROWS] = {
    0.532, 0.506, 0.480, 0.455, 0.432, 0.409, 0.387, 0.367, 0.349, 0.330,
    0.315, 0.301, 0.288, 0.276, 0.267, 0.259, 0.253, 0.250, 0.246, 0.244,
    0.243, 0.243, 0.243, 0.242, 0.242, 0.245, 0.254, 0.271, 0.301,
};
const double chorus_dsp_iso226_lu[CHORUS_DSP_ISO226_ROWS] = {
    -31.6, -27.2, -23.0, -19.1, -15.9, -13.0, -10.3, -8.1,  -6.2, -4.5,
    -3.1,  -2.0,  -1.1,  -0.4,  0.0,   0.3,   0.5,   0.0,   -2.7, -4.1,
    -1.0,  1.7,   2.5,   1.2,   -2.1,  -7.1,  -11.2, -10.7, -3.1,
};
const double chorus_dsp_iso226_tf[CHORUS_DSP_ISO226_ROWS] = {
    78.5, 68.7, 59.5, 51.1, 44.0, 37.5, 31.5, 26.5, 22.1, 17.9, 14.4, 11.4, 8.6,  6.2,  4.4,
    3.0,  2.2,  2.4,  3.5,  1.7,  -1.3, -4.2, -6.0, -5.4, -1.5, 6.0,  12.6, 13.9, 12.3,
};

/* The loudness constants, every one ASSUMED (crates/dsp/src/loudness.rs). */
#define REFERENCE_PHON 80.0
#define MIN_PHON 20.0
#define LOW_EVAL_HZ 50.0
#define HIGH_EVAL_HZ 10000.0
#define LOW_SHELF_HZ 100.0
#define HIGH_SHELF_HZ 8000.0
#define SHELF_Q 0.70710678118654752440
#define LOW_CAP_DB 12.0
#define HIGH_CAP_DB 6.0
#define STEP_DB 0.5

static double clamp(double v, double lo, double hi)
{
    if (v < lo) {
        return lo;
    }
    if (v > hi) {
        return hi;
    }
    return v;
}

int chorus_dsp_iso226_index(double freq_hz)
{
    for (unsigned i = 0; i < CHORUS_DSP_ISO226_ROWS; i++) {
        if (chorus_dsp_iso226_freqs[i] == freq_hz) {
            return (int)i;
        }
    }
    return -1;
}

int chorus_dsp_iso226_spl_db(double freq_hz, double phon, double *out)
{
    int i = chorus_dsp_iso226_index(freq_hz);
    if (i < 0) {
        return -1;
    }
    double af =
        4.47e-3 * (pow(10.0, 0.025 * phon) - 1.15) +
        pow(0.4 * pow(10.0, (chorus_dsp_iso226_tf[i] + chorus_dsp_iso226_lu[i]) / 10.0 - 9.0),
            chorus_dsp_iso226_af[i]);
    *out = (10.0 / chorus_dsp_iso226_af[i]) * log10(af) - chorus_dsp_iso226_lu[i] + 94.0;
    return 0;
}

static double needed_db(double freq_hz, double att_db)
{
    double level = clamp(REFERENCE_PHON - att_db, MIN_PHON, REFERENCE_PHON);
    double f_at, k_at, f_ref, k_ref;
    if (chorus_dsp_iso226_spl_db(freq_hz, level, &f_at) != 0 ||
        chorus_dsp_iso226_spl_db(1000.0, level, &k_at) != 0 ||
        chorus_dsp_iso226_spl_db(freq_hz, REFERENCE_PHON, &f_ref) != 0 ||
        chorus_dsp_iso226_spl_db(1000.0, REFERENCE_PHON, &k_ref) != 0) {
        return 0.0;
    }
    return (f_at - k_at) - (f_ref - k_ref);
}

double chorus_dsp_loudness_attenuation_db(float room_gain)
{
    double g = (double)room_gain;
    double att = (g > 0.0) ? -20.0 * log10(g) : REFERENCE_PHON;
    att = clamp(att, 0.0, REFERENCE_PHON - MIN_PHON);
    return floor(att / STEP_DB + 0.5) * STEP_DB;
}

void chorus_dsp_loudness_shelf_gains_db(double att_db, double *low_db, double *high_db)
{
    if (att_db <= 0.0) {
        *low_db = 0.0;
        *high_db = 0.0;
        return;
    }
    *low_db = clamp(needed_db(LOW_EVAL_HZ, att_db), 0.0, LOW_CAP_DB);
    *high_db = clamp(needed_db(HIGH_EVAL_HZ, att_db), 0.0, HIGH_CAP_DB);
}

/* --- 7. speech ----------------------------------------------------------- */

/* ASSUMED centre and Q; the gain is Geiger et al.'s 3.8 dB rounded
 * (crates/dsp/src/speech.rs). */
#define SPEECH_CENTRE_HZ 2000.0
#define SPEECH_Q 0.667
#define SPEECH_GAIN_DB 4.0

static double corner(double freq_hz, double rate_hz)
{
    double top = 0.45 * rate_hz;
    return (freq_hz > top) ? top : freq_hz;
}

chorus_dsp_err_t chorus_dsp_speech_design(double rate_hz, chorus_dsp_coeffs_t *out)
{
    return chorus_dsp_biquad_design(CHORUS_DSP_PEAKING, rate_hz, corner(SPEECH_CENTRE_HZ, rate_hz),
                                    SPEECH_Q, SPEECH_GAIN_DB, out);
}

/* --- 8.-10. settings and the chain ------------------------------------------ */

/* The tone shelves, ASSUMED (crates/dsp/src/chain.rs). */
#define BASS_HZ 100.0
#define TREBLE_HZ 8000.0
#define TONE_Q 0.70710678118654752440
#define LFE_GAIN_DB 10.0

void chorus_dsp_sound_default(chorus_dsp_sound_t *s)
{
    memset(s, 0, sizeof(*s));
    s->crossover_hz = CHORUS_DSP_CROSSOVER_DEFAULT_HZ;
}

void chorus_dsp_endpoint_default(chorus_dsp_endpoint_t *e)
{
    memset(e, 0, sizeof(*e));
}

static chorus_dsp_err_t refuse(const char **field, const char *name, chorus_dsp_err_t rc)
{
    if (field != NULL) {
        *field = name;
    }
    return rc;
}

chorus_dsp_err_t chorus_dsp_sound_validate(const chorus_dsp_sound_t *s, const char **field)
{
    const chorus_dsp_err_t bad = CHORUS_DSP_ERR_SETTING;
    if (s->bass_db < CHORUS_DSP_TONE_MIN_DB || s->bass_db > CHORUS_DSP_TONE_MAX_DB) {
        return refuse(field, "bass_db", bad);
    }
    if (s->treble_db < CHORUS_DSP_TONE_MIN_DB || s->treble_db > CHORUS_DSP_TONE_MAX_DB) {
        return refuse(field, "treble_db", bad);
    }
    if (s->role > CHORUS_DSP_POS_MAX) {
        return refuse(field, "role", bad);
    }
    if (s->crossover_hz < CHORUS_DSP_CROSSOVER_MIN_HZ ||
        s->crossover_hz > CHORUS_DSP_CROSSOVER_MAX_HZ) {
        return refuse(field, "crossover_hz", bad);
    }
    if (s->sub_level_cdb < CHORUS_DSP_SUB_LEVEL_MIN_CDB ||
        s->sub_level_cdb > CHORUS_DSP_SUB_LEVEL_MAX_CDB) {
        return refuse(field, "sub_level_cdb", bad);
    }
    if (s->eq_count > CHORUS_DSP_ROOM_EQ_MAX_FILTERS) {
        return refuse(field, "eq_count", bad);
    }
    for (unsigned i = 0; i < s->eq_count; i++) {
        const chorus_dsp_eq_filter_t *f = &s->eq[i];
        if (f->freq_hz < CHORUS_DSP_ROOM_EQ_FREQ_MIN_HZ ||
            f->freq_hz > CHORUS_DSP_ROOM_EQ_FREQ_MAX_HZ) {
            return refuse(field, "freq_hz", bad);
        }
        if (f->gain_cdb < CHORUS_DSP_ROOM_EQ_GAIN_MIN_CDB ||
            f->gain_cdb > CHORUS_DSP_ROOM_EQ_GAIN_MAX_CDB) {
            return refuse(field, "gain_cdb", bad);
        }
        if (f->q_milli < CHORUS_DSP_ROOM_EQ_Q_MIN_MILLI ||
            f->q_milli > CHORUS_DSP_ROOM_EQ_Q_MAX_MILLI) {
            return refuse(field, "q_milli", bad);
        }
    }
    return CHORUS_DSP_OK;
}

static chorus_dsp_err_t endpoint_validate(const chorus_dsp_endpoint_t *e, uint32_t rate_hz,
                                          const char **field)
{
    if (e->two_way) {
        if (e->two_way_hz < 20u || (double)e->two_way_hz > 0.45 * (double)rate_hz) {
            return refuse(field, "crossover_hz", CHORUS_DSP_ERR_ENDPOINT);
        }
        const chorus_dsp_driver_t *d[2] = {&e->woofer, &e->tweeter};
        for (int k = 0; k < 2; k++) {
            if (d[k]->trim_cdb < CHORUS_DSP_DRIVER_TRIM_MIN_CDB || d[k]->trim_cdb > 0) {
                return refuse(field, "trim_cdb", CHORUS_DSP_ERR_ENDPOINT);
            }
        }
    }
    return CHORUS_DSP_OK;
}

static bool is_main(uint8_t p)
{
    return p != CHORUS_DSP_POS_LFE;
}

static int find(const chorus_dsp_chain_t *c, uint8_t position)
{
    for (uint32_t i = 0; i < c->channels; i++) {
        if (c->map[i] == position) {
            return (int)i;
        }
    }
    return -1;
}

static float driver_trim(const chorus_dsp_driver_t *d)
{
    float g = (float)chorus_dsp_db_to_gain((double)d->trim_cdb / 100.0);
    return d->inverted ? -g : g;
}

/* Rust's Chain::apply, step for step. */
static chorus_dsp_err_t apply(chorus_dsp_chain_t *c, const chorus_dsp_sound_t *s, bool first)
{
    double rate = (double)c->rate_hz;
    uint32_t n = c->channels;
    chorus_dsp_err_t rc;

    /* 1. Room EQ: the non-zero filters, in order. */
    chorus_dsp_coeffs_t designs[CHORUS_DSP_ROOM_EQ_MAX_FILTERS];
    uint32_t count = 0;
    if (s->room_eq_enabled) {
        for (unsigned i = 0; i < s->eq_count; i++) {
            const chorus_dsp_eq_filter_t *f = &s->eq[i];
            if (f->gain_cdb == 0) {
                continue;
            }
            rc = chorus_dsp_biquad_design(
                CHORUS_DSP_PEAKING, rate, corner((double)f->freq_hz, rate),
                (double)f->q_milli / 1000.0, (double)f->gain_cdb / 100.0, &designs[count]);
            if (rc != CHORUS_DSP_OK) {
                return rc;
            }
            count++;
        }
    }
    /* 2. Tone. */
    chorus_dsp_coeffs_t tone[2];
    rc = chorus_dsp_biquad_design(CHORUS_DSP_LOWSHELF, rate, corner(BASS_HZ, rate), TONE_Q,
                                  (double)s->bass_db, &tone[0]);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    rc = chorus_dsp_biquad_design(CHORUS_DSP_HIGHSHELF, rate, corner(TREBLE_HZ, rate), TONE_Q,
                                  (double)s->treble_db, &tone[1]);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    bool tone_on[2] = {s->bass_db != 0, s->treble_db != 0};

    /* 6. The output's source. */
    chorus_dsp_source_t source;
    uint32_t sa = 0, sb = 0;
    bool highpass = false;
    if (s->role == 0) {
        if (c->endpoint.two_way) {
            int l = find(c, CHORUS_DSP_POS_FL), r = find(c, CHORUS_DSP_POS_FR);
            if (l >= 0 && r >= 0) {
                source = CHORUS_DSP_SOURCE_PAIR;
                sa = (uint32_t)l;
                sb = (uint32_t)r;
            } else if (n == 1) {
                source = CHORUS_DSP_SOURCE_CHANNEL;
            } else {
                source = CHORUS_DSP_SOURCE_MEAN;
            }
        } else {
            source = CHORUS_DSP_SOURCE_PASS;
        }
    } else if (s->role == CHORUS_DSP_POS_LFE) {
        source = CHORUS_DSP_SOURCE_SUB;
    } else {
        int at = find(c, s->role);
        if (at < 0 && n == 1 && c->map[0] == CHORUS_DSP_POS_MONO) {
            at = 0;
        }
        if (at >= 0) {
            source = CHORUS_DSP_SOURCE_CHANNEL;
            sa = (uint32_t)at;
        } else {
            source = CHORUS_DSP_SOURCE_SILENCE;
        }
        highpass = s->sub_present;
    }
    uint32_t base = (source == CHORUS_DSP_SOURCE_PASS) ? n : 1u;
    uint32_t outputs = c->endpoint.two_way ? 2u : base;

    /* 8. Delays: refused before anything changes. */
    uint32_t frames[CHORUS_DSP_MAX_OUTPUTS] = {0};
    uint64_t pool = 0;
    for (uint32_t o = 0; o < outputs; o++) {
        uint64_t us = c->endpoint.output_delay_us[o];
        if (c->endpoint.two_way) {
            us += (o == 0) ? c->endpoint.woofer.delay_us : c->endpoint.tweeter.delay_us;
        }
        if (us > UINT32_MAX) {
            return CHORUS_DSP_ERR_DELAY_TOO_LONG;
        }
        uint64_t f = chorus_dsp_frames_for_us((uint32_t)us, c->rate_hz);
        if (f > CHORUS_DSP_DELAY_MAX_FRAMES) {
            return CHORUS_DSP_ERR_DELAY_TOO_LONG;
        }
        frames[o] = (uint32_t)f;
        pool += f;
    }
    if (pool > CHORUS_DSP_DELAY_POOL_FRAMES) {
        return CHORUS_DSP_ERR_DELAY_TOO_LONG;
    }
    chorus_dsp_lr4_design_t bass;
    rc = chorus_dsp_lr4_design(rate, corner((double)s->crossover_hz, rate), &bass);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }

    /* Commit. */
    for (uint32_t ch = 0; ch < n; ch++) {
        for (uint32_t k = 0; k < count; k++) {
            if (k >= c->eq_count) {
                chorus_dsp_biquad_reset(&c->eq[ch][k]);
            }
            chorus_dsp_biquad_set(&c->eq[ch][k], &designs[k]);
        }
        for (int k = 0; k < 2; k++) {
            if (tone_on[k] && !c->tone_on[k]) {
                chorus_dsp_biquad_reset(&c->tone[ch][k]);
            }
            chorus_dsp_biquad_set(&c->tone[ch][k], &tone[k]);
        }
    }
    c->eq_count = count;
    c->tone_on[0] = tone_on[0];
    c->tone_on[1] = tone_on[1];
    if (!s->loudness) {
        c->loud_on[0] = false;
        c->loud_on[1] = false;
        c->loud_att = 0.0;
    }
    if (s->night && !c->sound.night) {
        chorus_dsp_compressor_reset(&c->night);
    }
    if (s->speech && !c->sound.speech) {
        chorus_dsp_biquad_reset(&c->speech);
    }
    bool changed = first || source != c->source || sa != c->source_a || sb != c->source_b ||
                   highpass != c->highpass || outputs != c->outputs ||
                   s->crossover_hz != c->sound.crossover_hz;
    for (uint32_t o = 0; o < outputs && !changed; o++) {
        if (frames[o] != c->delays[o].frames) {
            changed = true;
        }
    }
    chorus_dsp_lr4_set(&c->bass, &bass);
    float sub = (float)chorus_dsp_db_to_gain((double)s->sub_level_cdb / 100.0);
    c->sub_gain = s->sub_polarity_inverted ? -sub : sub;
    if (changed) {
        c->source = source;
        c->source_a = sa;
        c->source_b = sb;
        c->highpass = highpass;
        chorus_dsp_lr4_reset(&c->bass);
        chorus_dsp_lr4_reset(&c->two_way);
        uint32_t at = 0;
        for (uint32_t o = 0; o < outputs; o++) {
            (void)chorus_dsp_delay_init(&c->delays[o], &c->delay_pool[at],
                                        CHORUS_DSP_DELAY_POOL_FRAMES - at, frames[o]);
            at += frames[o];
        }
        rc = chorus_dsp_limiter_init(&c->limiter, outputs, c->lookahead,
                                     CHORUS_DSP_LIMITER_RELEASE_MS, rate);
        if (rc != CHORUS_DSP_OK) {
            return rc;
        }
        c->outputs = outputs;
    }
    c->sound = *s;
    return CHORUS_DSP_OK;
}

chorus_dsp_err_t chorus_dsp_chain_init(chorus_dsp_chain_t *c, const chorus_dsp_sound_t *sound,
                                       const chorus_dsp_endpoint_t *endpoint, const uint8_t *map,
                                       uint32_t channels, uint32_t rate_hz, const char **field)
{
    if (rate_hz < CHORUS_DSP_MIN_RATE_HZ || rate_hz > CHORUS_DSP_MAX_RATE_HZ) {
        return CHORUS_DSP_ERR_RATE;
    }
    if (channels == 0 || channels > CHORUS_DSP_MAX_CHANNELS) {
        return CHORUS_DSP_ERR_CHANNEL_MAP;
    }
    for (uint32_t i = 0; i < channels; i++) {
        if (map[i] > CHORUS_DSP_POS_MAX || (map[i] == CHORUS_DSP_POS_MONO && channels != 1)) {
            return CHORUS_DSP_ERR_CHANNEL_MAP;
        }
        for (uint32_t j = 0; j < i; j++) {
            if (map[j] == map[i]) {
                return CHORUS_DSP_ERR_CHANNEL_MAP;
            }
        }
    }
    chorus_dsp_err_t rc = endpoint_validate(endpoint, rate_hz, field);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    rc = chorus_dsp_sound_validate(sound, field);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    memset(c, 0, sizeof(*c));
    double rate = (double)rate_hz;
    c->rate_hz = rate_hz;
    c->channels = channels;
    memcpy(c->map, map, channels);
    c->endpoint = *endpoint;
    chorus_dsp_sound_default(&c->sound);
    for (uint32_t ch = 0; ch < channels; ch++) {
        for (unsigned k = 0; k < CHORUS_DSP_ROOM_EQ_MAX_FILTERS; k++) {
            chorus_dsp_biquad_init(&c->eq[ch][k], &chorus_dsp_identity);
        }
        for (int k = 0; k < 2; k++) {
            chorus_dsp_biquad_init(&c->tone[ch][k], &chorus_dsp_identity);
            chorus_dsp_biquad_init(&c->loud[ch][k], &chorus_dsp_identity);
        }
    }
    chorus_dsp_coeffs_t sp;
    rc = chorus_dsp_speech_design(rate, &sp);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    chorus_dsp_biquad_init(&c->speech, &sp);
    int fc = find(c, CHORUS_DSP_POS_FC), fl = find(c, CHORUS_DSP_POS_FL),
        fr = find(c, CHORUS_DSP_POS_FR);
    if (fc >= 0) {
        c->speech_mode = 1;
        c->speech_a = (uint32_t)fc;
    } else if (fl >= 0 && fr >= 0) {
        c->speech_mode = 2;
        c->speech_a = (uint32_t)fl;
        c->speech_b = (uint32_t)fr;
    } else if (channels == 1) {
        c->speech_mode = 1;
        c->speech_a = 0;
    } else {
        c->speech_mode = 0;
    }
    rc = chorus_dsp_compressor_init(&c->night, &chorus_dsp_night, rate);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    c->source = CHORUS_DSP_SOURCE_PASS;
    c->sub_gain = 1.0f;
    c->lfe_gain = (float)chorus_dsp_db_to_gain(LFE_GAIN_DB);
    c->lfe = find(c, CHORUS_DSP_POS_LFE);
    c->lookahead = (uint32_t)chorus_dsp_frames_for_us(CHORUS_DSP_LIMITER_LOOKAHEAD_US, rate_hz);
    if (c->lookahead > CHORUS_DSP_LIMITER_MAX_LOOKAHEAD) {
        return CHORUS_DSP_ERR_LOOKAHEAD;
    }
    if (endpoint->two_way) {
        chorus_dsp_lr4_design_t d;
        rc = chorus_dsp_lr4_design(rate, (double)endpoint->two_way_hz, &d);
        if (rc != CHORUS_DSP_OK) {
            return rc;
        }
        chorus_dsp_lr4_init(&c->two_way, &d);
        c->trims[0] = driver_trim(&endpoint->woofer);
        c->trims[1] = driver_trim(&endpoint->tweeter);
    }
    return apply(c, sound, true);
}

chorus_dsp_err_t chorus_dsp_chain_set_sound(chorus_dsp_chain_t *c, const chorus_dsp_sound_t *sound,
                                            const char **field)
{
    chorus_dsp_err_t rc = chorus_dsp_sound_validate(sound, field);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    return apply(c, sound, false);
}

uint32_t chorus_dsp_chain_out_channels(const chorus_dsp_chain_t *c)
{
    return c->outputs;
}

uint32_t chorus_dsp_chain_latency_frames(const chorus_dsp_chain_t *c)
{
    return c->lookahead;
}

static chorus_dsp_err_t update_loudness(chorus_dsp_chain_t *c, float room_gain)
{
    if (!c->sound.loudness) {
        return CHORUS_DSP_OK;
    }
    double att = chorus_dsp_loudness_attenuation_db(room_gain);
    if (att == c->loud_att) {
        return CHORUS_DSP_OK;
    }
    double rate = (double)c->rate_hz;
    double low, high;
    chorus_dsp_loudness_shelf_gains_db(att, &low, &high);
    chorus_dsp_coeffs_t designs[2];
    chorus_dsp_err_t rc = chorus_dsp_biquad_design(
        CHORUS_DSP_LOWSHELF, rate, corner(LOW_SHELF_HZ, rate), SHELF_Q, low, &designs[0]);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    rc = chorus_dsp_biquad_design(CHORUS_DSP_HIGHSHELF, rate, corner(HIGH_SHELF_HZ, rate), SHELF_Q,
                                  high, &designs[1]);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    bool on[2] = {low != 0.0, high != 0.0};
    for (uint32_t ch = 0; ch < c->channels; ch++) {
        for (int k = 0; k < 2; k++) {
            if (on[k] && !c->loud_on[k]) {
                chorus_dsp_biquad_reset(&c->loud[ch][k]);
            }
            chorus_dsp_biquad_set(&c->loud[ch][k], &designs[k]);
        }
    }
    c->loud_on[0] = on[0];
    c->loud_on[1] = on[1];
    c->loud_att = att;
    return CHORUS_DSP_OK;
}

chorus_dsp_err_t chorus_dsp_chain_process(chorus_dsp_chain_t *c, const float *in, float *out,
                                          uint32_t frames, float room_gain, float limit_gain)
{
    uint32_t n = c->channels;
    uint32_t outs = c->outputs;
    chorus_dsp_err_t rc = update_loudness(c, room_gain);
    if (rc != CHORUS_DSP_OK) {
        return rc;
    }
    chorus_dsp_limiter_set_ceiling(&c->limiter, (limit_gain < 1.0f) ? limit_gain : 1.0f);
    int lfe = c->lfe;
    float *x = c->frame;
    for (uint32_t f = 0; f < frames; f++) {
        memcpy(x, &in[(size_t)f * n], n * sizeof(float));
        /* 1-3. Room EQ, tone, loudness, every channel but LFE. */
        for (uint32_t ch = 0; ch < n; ch++) {
            if ((int)ch == lfe) {
                continue;
            }
            float v = x[ch];
            for (uint32_t k = 0; k < c->eq_count; k++) {
                v = chorus_dsp_biquad_process(&c->eq[ch][k], v);
            }
            for (int k = 0; k < 2; k++) {
                if (c->tone_on[k]) {
                    v = chorus_dsp_biquad_process(&c->tone[ch][k], v);
                }
            }
            for (int k = 0; k < 2; k++) {
                if (c->loud_on[k]) {
                    v = chorus_dsp_biquad_process(&c->loud[ch][k], v);
                }
            }
            x[ch] = v;
        }
        /* 4. Speech. */
        if (c->sound.speech) {
            if (c->speech_mode == 1) {
                x[c->speech_a] = chorus_dsp_biquad_process(&c->speech, x[c->speech_a]);
            } else if (c->speech_mode == 2) {
                uint32_t l = c->speech_a, r = c->speech_b;
                float m = (x[l] + x[r]) * 0.5f;
                float s = (x[l] - x[r]) * 0.5f;
                m = chorus_dsp_biquad_process(&c->speech, m);
                x[l] = m + s;
                x[r] = m - s;
            }
        }
        /* 5. Night. */
        if (c->sound.night) {
            chorus_dsp_compressor_process_frame(&c->night, x, n);
        }
        /* 6. Bass management and the source. */
        switch (c->source) {
        case CHORUS_DSP_SOURCE_PASS:
            memcpy(c->out, x, n * sizeof(float));
            break;
        case CHORUS_DSP_SOURCE_CHANNEL:
        case CHORUS_DSP_SOURCE_SILENCE: {
            float v = (c->source == CHORUS_DSP_SOURCE_CHANNEL) ? x[c->source_a] : 0.0f;
            if (c->highpass) {
                v = chorus_dsp_lr4_high(&c->bass, v);
            }
            c->out[0] = v;
            break;
        }
        case CHORUS_DSP_SOURCE_SUB: {
            float sum = 0.0f;
            for (uint32_t ch = 0; ch < n; ch++) {
                if (is_main(c->map[ch])) {
                    sum += x[ch];
                }
            }
            float v = chorus_dsp_lr4_low(&c->bass, sum);
            if (lfe >= 0) {
                v += x[lfe] * c->lfe_gain;
            }
            c->out[0] = v * c->sub_gain;
            break;
        }
        case CHORUS_DSP_SOURCE_PAIR:
            c->out[0] = (x[c->source_a] + x[c->source_b]) * 0.5f;
            break;
        case CHORUS_DSP_SOURCE_MEAN: {
            float sum = 0.0f;
            uint32_t count = 0;
            for (uint32_t ch = 0; ch < n; ch++) {
                if (is_main(c->map[ch])) {
                    sum += x[ch];
                    count++;
                }
            }
            c->out[0] = (count > 0) ? sum * (1.0f / (float)count) : 0.0f;
            break;
        }
        }
        /* 7. Two-way. */
        if (c->endpoint.two_way) {
            float lo, hi;
            chorus_dsp_lr4_split(&c->two_way, c->out[0], &lo, &hi);
            c->out[0] = lo * c->trims[0];
            c->out[1] = hi * c->trims[1];
        }
        /* 8-9. Delay, volume. */
        for (uint32_t o = 0; o < outs; o++) {
            c->out[o] = chorus_dsp_delay_process(&c->delays[o], c->out[o]) * room_gain;
        }
        /* 10. Limiter. */
        chorus_dsp_limiter_process_frame(&c->limiter, c->out);
        memcpy(&out[(size_t)f * outs], c->out, outs * sizeof(float));
    }
    return CHORUS_DSP_OK;
}
