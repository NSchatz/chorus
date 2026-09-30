#include "chorus/sync.h"

#include <math.h>
#include <string.h>

void chorus_rng_init(chorus_rng_t *rng, uint64_t seed)
{
    rng->state = seed;
}

uint64_t chorus_rng_next_u64(chorus_rng_t *rng)
{
    rng->state += 0x9E3779B97F4A7C15ull;
    uint64_t z = rng->state;
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
    return z ^ (z >> 31);
}

double chorus_rng_next_f64(chorus_rng_t *rng)
{
    /* The Rust mirror is `(next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as
     * f64)`. The scale is computed the same way here so the two are the same
     * double and not merely the same value on paper. */
    const double scale = 1.0 / (double)(1ull << 53);
    return (double)(chorus_rng_next_u64(rng) >> 11) * scale;
}

const char *chorus_jitter_name(chorus_jitter_kind_t kind)
{
    switch (kind) {
    case CHORUS_JITTER_NONE:
        return "none";
    case CHORUS_JITTER_UNIFORM:
        return "uniform";
    case CHORUS_JITTER_EXPONENTIAL:
        return "exponential";
    case CHORUS_JITTER_BURST:
        return "burst";
    }
    return "unknown";
}

chorus_jitter_kind_t chorus_jitter_from_name(const char *name, int *ok)
{
    *ok = 1;
    if (name[0] == 'n' && name[1] == 'o' && name[2] == 'n' && name[3] == 'e' && name[4] == '\0') {
        return CHORUS_JITTER_NONE;
    }
    if (name[0] == 'u') {
        const char *uniform = "uniform";
        for (int i = 0;; i++) {
            if (uniform[i] != name[i]) {
                break;
            }
            if (uniform[i] == '\0') {
                return CHORUS_JITTER_UNIFORM;
            }
        }
    }
    if (name[0] == 'e') {
        const char *exponential = "exponential";
        for (int i = 0;; i++) {
            if (exponential[i] != name[i]) {
                break;
            }
            if (exponential[i] == '\0') {
                return CHORUS_JITTER_EXPONENTIAL;
            }
        }
    }
    if (strcmp(name, "burst") == 0) {
        return CHORUS_JITTER_BURST;
    }
    *ok = 0;
    return CHORUS_JITTER_NONE;
}

/* Inverse-transform exponential draw, capped at CHORUS_JITTER_TAIL_CAP_MULTIPLE
 * times the mean, in nanoseconds. Mirrored from exponential_ns in jitter.rs. */
static double exponential_ns(double mean_us, chorus_rng_t *rng)
{
    /* next_f64 is in [0, 1), so 1 - it is in (0, 1] and the logarithm is
     * always finite. */
    double u = 1.0 - chorus_rng_next_f64(rng);
    double sample_us = -mean_us * log(u);
    double cap_us = mean_us * CHORUS_JITTER_TAIL_CAP_MULTIPLE;
    double capped = (sample_us < cap_us) ? sample_us : cap_us;
    return capped * 1000.0;
}

/* Inverse-transform Lomax draw, scale * (u^(-1/shape) - 1), capped at
 * CHORUS_JITTER_BURST_CAP_US, in nanoseconds. Mirrored from lomax_ns. */
static double lomax_ns(double scale_us, double shape, chorus_rng_t *rng)
{
    double u = 1.0 - chorus_rng_next_f64(rng);
    double sample_us = scale_us * (pow(u, -1.0 / shape) - 1.0);
    double capped = (sample_us < CHORUS_JITTER_BURST_CAP_US) ? sample_us
                                                              : CHORUS_JITTER_BURST_CAP_US;
    return capped * 1000.0;
}

double chorus_jitter_sample_ns(const chorus_jitter_t *jitter, chorus_rng_t *rng)
{
    switch (jitter->kind) {
    case CHORUS_JITTER_NONE:
        return 0.0;
    case CHORUS_JITTER_UNIFORM:
        return chorus_rng_next_f64(rng) * jitter->scale_us * 1000.0;
    case CHORUS_JITTER_EXPONENTIAL:
    case CHORUS_JITTER_BURST:
        return exponential_ns(jitter->scale_us, rng);
    }
    return 0.0;
}

void chorus_jitter_process_init(chorus_jitter_process_t *process, const chorus_jitter_t *model)
{
    process->model = *model;
    process->in_burst = 0;
}

double chorus_jitter_process_sample_ns(chorus_jitter_process_t *process, chorus_rng_t *rng)
{
    const chorus_jitter_t *model = &process->model;
    if (model->kind != CHORUS_JITTER_BURST) {
        return chorus_jitter_sample_ns(model, rng);
    }
    double transition = chorus_rng_next_f64(rng);
    if (process->in_burst) {
        if (transition < model->burst_exit_prob) {
            process->in_burst = 0;
        }
    } else if (transition < model->burst_enter_prob) {
        process->in_burst = 1;
    }
    if (process->in_burst) {
        return lomax_ns(model->burst_scale_us, model->burst_shape, rng);
    }
    return exponential_ns(model->scale_us, rng);
}
