#include "chorus/dsp_cost.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The room gain every configuration runs at: -12 dB, so loudness
 * compensation has an attenuation to compensate and the limiter a ceiling
 * the boosted all-on chain can reach. ASSUMED: a representative listening
 * level, not a measured one; the cost of the chain does not depend on it
 * beyond which stages engage. */
#define ROOM_GAIN 0.25f
/* The effective limit: the room's own (unity, no limit below it). */
#define LIMIT_GAIN 1.0f

const char *const chorus_dsp_cost_names[CHORUS_DSP_COST_CONFIGS] = {"flat", "all-on", "sub",
                                                                    "two-way"};

/* The host's sums for each configuration, from firmware/tests/test_console.c
 * on an x86-64 host with the firmware Makefile's flags (-O2
 * -ffp-contract=off -fno-fast-math, glibc's libm). That test recomputes them
 * on every run, prints the lines to paste here when they are not bit exact,
 * and fails if the chain, the signal or a configuration moves them past
 * CHORUS_DSP_COST_TOLERANCE. */
const chorus_dsp_cost_sums_t chorus_dsp_cost_reference[CHORUS_DSP_COST_CONFIGS] = {
    {0x8ac41bf1470a3139ull,
     498.97646463275032,
     {6.7444576919078827, 20.804299622774124, 30.472570538520813, -8.8038040101528168}}, /* flat */
    {0x8ed1b826779f99adull,
     128.62233056526827,
     {7.301863530636723, 2.3602974048659178, 20.671187314222465, -9.3511388349596416}}, /* all-on */
    {0x70f2740745e59c0full,
     0.70398347894668067,
     {-0.35794201522518343, 1.6284587792380636, -0.83200891856765447,
      -1.1177214366150947}}, /* sub */
    {0x53f840c37909a91aull,
     63.44995482920163,
     {-1.0161112184374588, 5.6074814887969069, 5.0035155615702251,
      -3.9647841270572144}}, /* two-way */
};

/* The everything-on configuration's eight room-EQ filters: below 1 kHz,
 * mostly cuts, two small boosts, Q from 1 to 8, inside the bounds. ASSUMED:
 * shaped like a fit (crates/dsp/src/roomfit.rs), not taken from a room. */
static const chorus_dsp_eq_filter_t ALL_ON_EQ[CHORUS_DSP_ROOM_EQ_MAX_FILTERS] = {
    {35, -600, 4000},  {52, -300, 2500},   {78, -900, 6000}, {110, 200, 1500},
    {160, -450, 3000}, {240, -1200, 8000}, {420, 150, 1000}, {800, -200, 2000},
};

int chorus_dsp_cost_configure(size_t config, chorus_dsp_sound_t *sound,
                              chorus_dsp_endpoint_t *endpoint, const char **name)
{
    if (config >= CHORUS_DSP_COST_CONFIGS) {
        return -1;
    }
    chorus_dsp_sound_default(sound);
    chorus_dsp_endpoint_default(endpoint);
    *name = chorus_dsp_cost_names[config];
    switch (config) {
    case 0:
        /* Flat: every stage bypassed, an unbonded stereo endpoint. */
        break;
    case 1:
        /* Everything the room's sound can switch on at once, unbonded. */
        sound->bass_db = 4;
        sound->treble_db = -3;
        sound->loudness = true;
        sound->night = true;
        sound->speech = true;
        sound->room_eq_enabled = true;
        sound->eq_count = (uint8_t)CHORUS_DSP_ROOM_EQ_MAX_FILTERS;
        memcpy(sound->eq, ALL_ON_EQ, sizeof(ALL_ON_EQ));
        break;
    case 2:
        /* The LFE member of a 2.1 set: the LR4 low branch of the mains' sum
         * at the default crossover, the sub level a little down. */
        sound->role = (uint8_t)CHORUS_DSP_POS_LFE;
        sound->sub_present = true;
        sound->sub_level_cdb = -300;
        break;
    default:
        /* An unbonded two-way speaker: the mono downmix split at the
         * envelope's ASSUMED 2 kHz into woofer and tweeter, the tweeter
         * trimmed and delayed a little (ASSUMED example values). */
        endpoint->two_way = true;
        endpoint->two_way_hz = 2000;
        endpoint->tweeter.trim_cdb = -300;
        endpoint->tweeter.delay_us = 100;
        break;
    }
    return 0;
}

/* xorshift32 (Marsaglia 2003): integer only, so the signal and the
 * projections are the same bits on every platform. */
static uint32_t xorshift32(uint32_t *s)
{
    uint32_t x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *s = x;
    return x;
}

/* A sample in [-0.5, 0.5): a 24-bit integer over 2^24, exact in float. */
static float noise_sample(uint32_t *s)
{
    int32_t v = (int32_t)xorshift32(s) >> 8;
    return (float)v / 16777216.0f;
}

int chorus_dsp_cost_measure(const char *name, const chorus_dsp_sound_t *sound,
                            const chorus_dsp_endpoint_t *endpoint,
                            const chorus_dsp_cost_sums_t *reference, chorus_clock_fn now_ns,
                            chorus_dsp_cost_alloc_fn alloc, chorus_dsp_cost_free_fn release,
                            chorus_dsp_cost_t *out, char *detail, size_t detail_len)
{
    memset(out, 0, sizeof(*out));
    out->name = name;
    chorus_dsp_chain_t *chain =
        (alloc != NULL) ? alloc(sizeof(chorus_dsp_chain_t)) : malloc(sizeof(chorus_dsp_chain_t));
    if (chain == NULL) {
        snprintf(detail, detail_len, "no memory for a %zu byte chain", sizeof(chorus_dsp_chain_t));
        return -1;
    }
    const uint8_t map[2] = {(uint8_t)CHORUS_DSP_POS_FL, (uint8_t)CHORUS_DSP_POS_FR};
    const char *field = NULL;
    if (chorus_dsp_chain_init(chain, sound, endpoint, map, 2u, CHORUS_DSP_COST_RATE_HZ, &field) !=
        CHORUS_DSP_OK) {
        snprintf(detail, detail_len, "the library refused the %s chain at %s", name,
                 field != NULL ? field : "an unnamed field");
        if (release != NULL) {
            release(chain);
        } else {
            free(chain);
        }
        return -1;
    }
    const uint32_t outputs = chorus_dsp_chain_out_channels(chain);

    uint32_t signal = 0x2545f491u;
    uint32_t weights[CHORUS_DSP_COST_PROJECTIONS] = {0x9e3779b9u, 0x85ebca6bu, 0xc2b2ae35u,
                                                     0x27d4eb2fu};
    uint64_t fnv = 0xcbf29ce484222325ull;
    double energy = 0.0;
    double projection[CHORUS_DSP_COST_PROJECTIONS] = {0.0, 0.0, 0.0, 0.0};
    float in[CHORUS_DSP_COST_BLOCK * 2u];
    float o[CHORUS_DSP_COST_BLOCK * CHORUS_DSP_MAX_OUTPUTS];
    /* Only the chain calls are timed: generating the signal and summing the
     * output are the harness's cost, not the chain's. */
    uint64_t elapsed = 0;
    uint64_t frames = 0;
    while (frames < CHORUS_DSP_COST_FRAMES) {
        uint32_t n = CHORUS_DSP_COST_BLOCK;
        if (CHORUS_DSP_COST_FRAMES - frames < n) {
            n = (uint32_t)(CHORUS_DSP_COST_FRAMES - frames);
        }
        for (uint32_t i = 0; i < 2u * n; i++) {
            in[i] = noise_sample(&signal);
        }
        uint64_t started = now_ns();
        (void)chorus_dsp_chain_process(chain, in, o, n, ROOM_GAIN, LIMIT_GAIN);
        elapsed += now_ns() - started;
        for (uint32_t i = 0; i < n * outputs; i++) {
            uint32_t bits;
            memcpy(&bits, &o[i], sizeof(bits));
            for (int b = 0; b < 4; b++) {
                fnv = (fnv ^ ((bits >> (8 * b)) & 0xffu)) * 0x100000001b3ull;
            }
            double x = (double)o[i];
            energy += x * x;
            for (size_t k = 0; k < CHORUS_DSP_COST_PROJECTIONS; k++) {
                projection[k] += (xorshift32(&weights[k]) & 0x80000000u) ? x : -x;
            }
        }
        frames += n;
    }
    if (release != NULL) {
        release(chain);
    } else {
        free(chain);
    }

    out->outputs = outputs;
    out->frames = frames;
    out->elapsed_ns = elapsed;
    double seconds = (double)elapsed / 1e9;
    double audio_seconds = (double)frames / (double)CHORUS_DSP_COST_RATE_HZ;
    out->frames_per_s = (seconds > 0.0) ? (double)frames / seconds : 0.0;
    out->cpu_fraction = (audio_seconds > 0.0) ? seconds / audio_seconds : 0.0;
    out->sums.fnv1a64 = fnv;
    out->sums.energy = energy;
    memcpy(out->sums.projection, projection, sizeof(projection));

    /* The deviation: energy against the reference's, each projection against
     * the reference's RMS norm. A reference of no energy matches only an
     * output of none. */
    double deviation;
    if (reference->energy > 0.0) {
        double norm = sqrt(reference->energy);
        deviation = fabs(energy - reference->energy) / reference->energy;
        for (size_t k = 0; k < CHORUS_DSP_COST_PROJECTIONS; k++) {
            double d = fabs(projection[k] - reference->projection[k]) / norm;
            if (d > deviation) {
                deviation = d;
            }
        }
    } else {
        deviation = (energy == 0.0) ? 0.0 : HUGE_VAL;
    }
    out->deviation = deviation;
    out->output_matches = deviation <= CHORUS_DSP_COST_TOLERANCE;
    out->bit_exact = fnv == reference->fnv1a64;
    return 0;
}

int chorus_dsp_cost_run(size_t config, chorus_clock_fn now_ns, chorus_dsp_cost_alloc_fn alloc,
                        chorus_dsp_cost_free_fn release, chorus_dsp_cost_t *out, char *detail,
                        size_t detail_len)
{
    chorus_dsp_sound_t sound;
    chorus_dsp_endpoint_t endpoint;
    const char *name = NULL;
    if (chorus_dsp_cost_configure(config, &sound, &endpoint, &name) != 0) {
        memset(out, 0, sizeof(*out));
        snprintf(detail, detail_len, "there is no configuration %zu", config);
        return -1;
    }
    return chorus_dsp_cost_measure(name, &sound, &endpoint, &chorus_dsp_cost_reference[config],
                                   now_ns, alloc, release, out, detail, detail_len);
}
