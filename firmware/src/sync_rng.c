#include "chorus/sync.h"

#include <math.h>

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
    *ok = 0;
    return CHORUS_JITTER_NONE;
}

double chorus_jitter_sample_ns(const chorus_jitter_t *jitter, chorus_rng_t *rng)
{
    switch (jitter->kind) {
    case CHORUS_JITTER_NONE:
        return 0.0;
    case CHORUS_JITTER_UNIFORM:
        return chorus_rng_next_f64(rng) * jitter->scale_us * 1000.0;
    case CHORUS_JITTER_EXPONENTIAL: {
        /* Inverse transform. next_f64 is in [0, 1), so 1 - it is in (0, 1] and
         * the logarithm is always finite. */
        double u = 1.0 - chorus_rng_next_f64(rng);
        double sample_us = -jitter->scale_us * log(u);
        double cap_us = jitter->scale_us * CHORUS_JITTER_TAIL_CAP_MULTIPLE;
        double capped = (sample_us < cap_us) ? sample_us : cap_us;
        return capped * 1000.0;
    }
    }
    return 0.0;
}
