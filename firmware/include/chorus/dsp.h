/* The DSP library, the C mirror of crates/dsp (goal 12; docs/dsp.md;
 * docs/decisions/ "the DSP library").
 *
 * The same blocks, the same algorithms and the same state-update order as the
 * Rust crate, so the two are held to one set of committed fixtures
 * (fixtures/dsp/, read here by firmware/tests/test_dsp.c and there by
 * crates/dsp/tests/shared_fixtures.rs):
 *
 *   1. biquads: the RBJ cookbook designs in double, run in float as
 *      Transposed Direct Form II, and the double response evaluator;
 *   2. the Linkwitz-Riley 4th-order (LR4) split;
 *   3. whole-sample delay lines over caller storage, with a fixed maximum;
 *   4. the look-ahead limiter whose ceiling is never exceeded;
 *   5. the channel-linked night compressor (Giannoulis et al. 2012);
 *   6. ISO 226:2003 loudness compensation;
 *   7. speech enhancement;
 *   8.-10. the chain: bass management by role, the endpoint's two-way split,
 *      per-output delay, the room gain and the limiter, configured by the
 *      wire `sound` message's fields and the endpoint's own configuration.
 *
 * Samples are float; coefficients are designed in double and rounded to float
 * once (BRIEF section 5.6). The unit is compiled with -ffp-contract=off and
 * -fno-fast-math, so a*b + c is two roundings here as it is in Rust.
 *
 * No heap: every structure has fixed maximums (8 channels, 8 outputs, a
 * delay pool of two 50 ms-at-96 kHz lines, a 768-frame look-ahead), so a
 * chorus_dsp_chain_t is one static object of about 70 KB and nothing on the
 * audio path allocates. No clock, no I/O, no ESP-IDF header. */

#ifndef CHORUS_DSP_H
#define CHORUS_DSP_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* --- bounds, the Rust crate's constants ------------------------------------ */

#define CHORUS_DSP_MIN_RATE_HZ 8000u
#define CHORUS_DSP_MAX_RATE_HZ 384000u
#define CHORUS_DSP_MAX_CHANNELS 8u
#define CHORUS_DSP_MAX_OUTPUTS 8u
/* One delay line's maximum: 50 ms at 96 kHz. */
#define CHORUS_DSP_DELAY_MAX_FRAMES 4800u
/* What a chain's outputs may hold between them: two lines at the maximum. */
#define CHORUS_DSP_DELAY_POOL_FRAMES (2u * CHORUS_DSP_DELAY_MAX_FRAMES)
/* The longest look-ahead: 2 ms at 384 kHz. */
#define CHORUS_DSP_LIMITER_MAX_LOOKAHEAD 768u
/* The chain's look-ahead, us. ASSUMED: 2 ms (docs/dsp.md). */
#define CHORUS_DSP_LIMITER_LOOKAHEAD_US 2000u
/* The chain's limiter release, ms. ASSUMED: 100 ms. */
#define CHORUS_DSP_LIMITER_RELEASE_MS 100.0
/* Below this distance from unity the release lands on 1.0. ASSUMED: 1e-6. */
#define CHORUS_DSP_LIMITER_RELEASE_SNAP 1e-6f

/* The wire `sound` message's bounds (docs/protocol.md, 0x39). */
#define CHORUS_DSP_TONE_MIN_DB (-10)
#define CHORUS_DSP_TONE_MAX_DB 10
#define CHORUS_DSP_CROSSOVER_MIN_HZ 40u
#define CHORUS_DSP_CROSSOVER_MAX_HZ 200u
/* "The most common crossover frequency recommended (and the THX standard) is
 * 80 Hz" (SVS, read 2026-10-01; docs/dsp.md). */
#define CHORUS_DSP_CROSSOVER_DEFAULT_HZ 80u
#define CHORUS_DSP_SUB_LEVEL_MIN_CDB (-1200)
#define CHORUS_DSP_SUB_LEVEL_MAX_CDB 600
/* THE room-correction bounds (crates/control's ROOM_EQ_* are the same). */
#define CHORUS_DSP_ROOM_EQ_MAX_FILTERS 8u
#define CHORUS_DSP_ROOM_EQ_FREQ_MIN_HZ 20u
#define CHORUS_DSP_ROOM_EQ_FREQ_MAX_HZ 1000u
#define CHORUS_DSP_ROOM_EQ_GAIN_MIN_CDB (-1200)
#define CHORUS_DSP_ROOM_EQ_GAIN_MAX_CDB 300
#define CHORUS_DSP_ROOM_EQ_Q_MIN_MILLI 500u
#define CHORUS_DSP_ROOM_EQ_Q_MAX_MILLI 10000u
/* A two-way driver's lowest trim, centi-dB. ASSUMED: -24 dB. */
#define CHORUS_DSP_DRIVER_TRIM_MIN_CDB (-2400)

/* Channel positions (docs/protocol.md "The channel map"). */
#define CHORUS_DSP_POS_MONO 0u
#define CHORUS_DSP_POS_FL 1u
#define CHORUS_DSP_POS_FR 2u
#define CHORUS_DSP_POS_FC 3u
#define CHORUS_DSP_POS_LFE 4u
#define CHORUS_DSP_POS_BL 5u
#define CHORUS_DSP_POS_BR 6u
#define CHORUS_DSP_POS_SL 10u
#define CHORUS_DSP_POS_SR 11u
#define CHORUS_DSP_POS_MAX 18u

typedef enum {
    CHORUS_DSP_OK = 0,
    CHORUS_DSP_ERR_RATE,
    CHORUS_DSP_ERR_FREQUENCY,
    CHORUS_DSP_ERR_Q,
    CHORUS_DSP_ERR_GAIN,
    CHORUS_DSP_ERR_DELAY_TOO_LONG,
    CHORUS_DSP_ERR_LOOKAHEAD,
    CHORUS_DSP_ERR_TIME_CONSTANT,
    CHORUS_DSP_ERR_CHANNEL_MAP,
    CHORUS_DSP_ERR_SETTING,
    CHORUS_DSP_ERR_ENDPOINT,
    CHORUS_DSP_ERR_BUFFER,
} chorus_dsp_err_t;

double chorus_dsp_db_to_gain(double db);

/* --- 1. biquads ---------------------------------------------------------- */

typedef enum {
    CHORUS_DSP_LOWPASS,
    CHORUS_DSP_HIGHPASS,
    CHORUS_DSP_BANDPASS, /* constant 0 dB peak gain */
    CHORUS_DSP_NOTCH,
    CHORUS_DSP_ALLPASS,
    CHORUS_DSP_PEAKING,
    CHORUS_DSP_LOWSHELF,
    CHORUS_DSP_HIGHSHELF,
} chorus_dsp_kind_t;

/* A fixture's name for a design; returns 0 and sets `kind`, or -1. */
int chorus_dsp_kind_from_name(const char *name, chorus_dsp_kind_t *kind);

/* A designed section, normalised so a0 = 1. */
typedef struct {
    double b0, b1, b2, a1, a2;
} chorus_dsp_coeffs_t;

extern const chorus_dsp_coeffs_t chorus_dsp_identity;

/* One cookbook design; refused (and `out` untouched) as the Rust design is. */
chorus_dsp_err_t chorus_dsp_biquad_design(chorus_dsp_kind_t kind, double rate_hz, double f0_hz,
                                          double q, double gain_db, chorus_dsp_coeffs_t *out);
void chorus_dsp_response(const chorus_dsp_coeffs_t *c, double freq_hz, double rate_hz, double *re,
                         double *im);
double chorus_dsp_magnitude_db(const chorus_dsp_coeffs_t *c, double freq_hz, double rate_hz);

typedef struct {
    float b0, b1, b2, a1, a2;
    float z1, z2;
} chorus_dsp_biquad_t;

void chorus_dsp_biquad_init(chorus_dsp_biquad_t *q, const chorus_dsp_coeffs_t *c);
void chorus_dsp_biquad_set(chorus_dsp_biquad_t *q, const chorus_dsp_coeffs_t *c);
void chorus_dsp_biquad_reset(chorus_dsp_biquad_t *q);
float chorus_dsp_biquad_process(chorus_dsp_biquad_t *q, float x);

/* --- 2. LR4 -------------------------------------------------------------- */

typedef struct {
    chorus_dsp_coeffs_t low, high;
} chorus_dsp_lr4_design_t;

chorus_dsp_err_t chorus_dsp_lr4_design(double rate_hz, double crossover_hz,
                                       chorus_dsp_lr4_design_t *out);
void chorus_dsp_lr4_response_low(const chorus_dsp_lr4_design_t *d, double freq_hz, double rate_hz,
                                 double *re, double *im);
void chorus_dsp_lr4_response_high(const chorus_dsp_lr4_design_t *d, double freq_hz, double rate_hz,
                                  double *re, double *im);

typedef struct {
    chorus_dsp_biquad_t low[2], high[2];
} chorus_dsp_lr4_t;

void chorus_dsp_lr4_init(chorus_dsp_lr4_t *s, const chorus_dsp_lr4_design_t *d);
void chorus_dsp_lr4_set(chorus_dsp_lr4_t *s, const chorus_dsp_lr4_design_t *d);
void chorus_dsp_lr4_reset(chorus_dsp_lr4_t *s);
float chorus_dsp_lr4_low(chorus_dsp_lr4_t *s, float x);
float chorus_dsp_lr4_high(chorus_dsp_lr4_t *s, float x);
void chorus_dsp_lr4_split(chorus_dsp_lr4_t *s, float x, float *low, float *high);

/* --- 3. delay ------------------------------------------------------------ */

typedef struct {
    float *line;
    uint32_t frames;
    uint32_t pos;
} chorus_dsp_delay_t;

/* Microseconds to whole frames, rounded to nearest (halves up). */
uint64_t chorus_dsp_frames_for_us(uint32_t delay_us, uint32_t rate_hz);
/* A line of `frames` over `storage` (at least `frames` floats), zeroed.
 * Refused above CHORUS_DSP_DELAY_MAX_FRAMES or above `capacity`. */
chorus_dsp_err_t chorus_dsp_delay_init(chorus_dsp_delay_t *d, float *storage, uint32_t capacity,
                                       uint32_t frames);
void chorus_dsp_delay_reset(chorus_dsp_delay_t *d);
float chorus_dsp_delay_process(chorus_dsp_delay_t *d, float x);

/* --- 4. limiter ---------------------------------------------------------- */

typedef struct {
    float storage[CHORUS_DSP_MAX_OUTPUTS][CHORUS_DSP_LIMITER_MAX_LOOKAHEAD];
    chorus_dsp_delay_t lines[CHORUS_DSP_MAX_OUTPUTS];
    float required[CHORUS_DSP_LIMITER_MAX_LOOKAHEAD + 1u];
    float reciprocal[CHORUS_DSP_LIMITER_MAX_LOOKAHEAD + 1u];
    uint32_t channels;
    uint32_t len; /* lookahead + 1 */
    uint32_t newest;
    float gain;
    float gap;
    float release;
    float ceiling;
} chorus_dsp_limiter_t;

chorus_dsp_err_t chorus_dsp_limiter_init(chorus_dsp_limiter_t *l, uint32_t channels,
                                         uint32_t lookahead_frames, double release_ms,
                                         double rate_hz);
void chorus_dsp_limiter_set_ceiling(chorus_dsp_limiter_t *l, float ceiling);
void chorus_dsp_limiter_reset(chorus_dsp_limiter_t *l);
/* One frame of `channels` samples, limited in place. */
void chorus_dsp_limiter_process_frame(chorus_dsp_limiter_t *l, float *frame);

/* --- 5. compressor ------------------------------------------------------- */

typedef struct {
    double threshold_db, ratio, knee_db, attack_ms, release_ms, makeup_db;
} chorus_dsp_compressor_params_t;

/* The night mode's settings, every one ASSUMED (docs/dsp.md). */
extern const chorus_dsp_compressor_params_t chorus_dsp_night;

typedef struct {
    float threshold_db, ratio, knee_db, makeup_db;
    float attack, release;
    float smoothed_db;
} chorus_dsp_compressor_t;

double chorus_dsp_static_curve_db(double x_db, double threshold_db, double ratio, double knee_db);
chorus_dsp_err_t chorus_dsp_ballistics(double time_ms, double rate_hz, float *out);
chorus_dsp_err_t chorus_dsp_compressor_init(chorus_dsp_compressor_t *c,
                                            const chorus_dsp_compressor_params_t *p,
                                            double rate_hz);
void chorus_dsp_compressor_reset(chorus_dsp_compressor_t *c);
float chorus_dsp_compressor_smooth(chorus_dsp_compressor_t *c, float gc_db);
void chorus_dsp_compressor_process_frame(chorus_dsp_compressor_t *c, float *frame, uint32_t n);

/* --- 6. loudness (ISO 226:2003) -------------------------------------------- */

#define CHORUS_DSP_ISO226_ROWS 29u
extern const double chorus_dsp_iso226_freqs[CHORUS_DSP_ISO226_ROWS];
extern const double chorus_dsp_iso226_af[CHORUS_DSP_ISO226_ROWS];
extern const double chorus_dsp_iso226_lu[CHORUS_DSP_ISO226_ROWS];
extern const double chorus_dsp_iso226_tf[CHORUS_DSP_ISO226_ROWS];

/* The row of a table frequency, or -1. */
int chorus_dsp_iso226_index(double freq_hz);
/* Lp at a table frequency; returns 0, or -1 for a frequency not in the table. */
int chorus_dsp_iso226_spl_db(double freq_hz, double phon, double *out);
/* The attenuation below the reference, quantised (docs/dsp.md). */
double chorus_dsp_loudness_attenuation_db(float room_gain);
void chorus_dsp_loudness_shelf_gains_db(double att_db, double *low_db, double *high_db);

/* --- 7. speech ----------------------------------------------------------- */

chorus_dsp_err_t chorus_dsp_speech_design(double rate_hz, chorus_dsp_coeffs_t *out);

/* --- 8.-10. settings and the chain ------------------------------------------ */

typedef struct {
    uint16_t freq_hz;
    int16_t gain_cdb;
    uint16_t q_milli;
} chorus_dsp_eq_filter_t;

/* `tv_upmix` (goal 13): what a surround role plays from a stream with no
 * surround channel. Off (silence) is the default, ASSUMED; ambient is the
 * passive matrix surround (crates/dsp/src/chain.rs, docs/dsp.md). */
#define CHORUS_DSP_TV_UPMIX_OFF 0u
#define CHORUS_DSP_TV_UPMIX_AMBIENT 1u
#define CHORUS_DSP_TV_UPMIX_MAX CHORUS_DSP_TV_UPMIX_AMBIENT
/* The most terms one mix row holds: a 7.1 map's seven main channels. */
#define CHORUS_DSP_MIX_TERMS 7u

/* Exactly the wire `sound` message's fields. */
typedef struct {
    int8_t bass_db;
    int8_t treble_db;
    bool loudness;
    bool night;
    bool speech;
    bool room_eq_enabled;
    bool sub_polarity_inverted;
    uint8_t role;
    bool sub_present;
    uint16_t crossover_hz;
    int16_t sub_level_cdb;
    uint8_t eq_count;
    chorus_dsp_eq_filter_t eq[CHORUS_DSP_ROOM_EQ_MAX_FILTERS];
    /* Goal 13: the theater maps. */
    uint8_t tv_upmix;
    bool fold_centre;   /* the set has no FC: a front role folds FC in */
    bool fold_surround; /* the set has no surrounds: a front role folds its side's */
} chorus_dsp_sound_t;

/* Flat: every stage bypassed, not in a set, the default crossover. */
void chorus_dsp_sound_default(chorus_dsp_sound_t *s);
/* CHORUS_DSP_OK, or CHORUS_DSP_ERR_SETTING with `*field` naming the field
 * (the Rust names: "bass_db", "eq_count", "q_milli", ...). */
chorus_dsp_err_t chorus_dsp_sound_validate(const chorus_dsp_sound_t *s, const char **field);

typedef struct {
    int16_t trim_cdb; /* cut only, CHORUS_DSP_DRIVER_TRIM_MIN_CDB..0 */
    uint32_t delay_us;
    bool inverted;
} chorus_dsp_driver_t;

/* The endpoint's own DSP: its drivers, not the room's sound. */
typedef struct {
    bool two_way;
    uint32_t two_way_hz;
    chorus_dsp_driver_t woofer, tweeter;
    uint32_t output_delay_us[CHORUS_DSP_MAX_OUTPUTS];
    /* Goal 13: one stereo speaker; not in a set and not two-way, a stream
     * with a centre or surrounds is downmixed to two outputs (BS.775-4). */
    bool stereo_downmix;
} chorus_dsp_endpoint_t;

void chorus_dsp_endpoint_default(chorus_dsp_endpoint_t *e);

typedef enum {
    CHORUS_DSP_SOURCE_PASS,
    CHORUS_DSP_SOURCE_CHANNEL,
    CHORUS_DSP_SOURCE_SILENCE,
    CHORUS_DSP_SOURCE_SUB,
    CHORUS_DSP_SOURCE_PAIR,
    CHORUS_DSP_SOURCE_MEAN,
    CHORUS_DSP_SOURCE_MIX,
    CHORUS_DSP_SOURCE_DOWNMIX,
    CHORUS_DSP_SOURCE_AMBIENT,
} chorus_dsp_source_t;

/* One output's weighted sum of stream channels, summed in term order (Rust's
 * `Mix`, term for term). */
typedef struct {
    uint32_t at[CHORUS_DSP_MIX_TERMS];
    float gain[CHORUS_DSP_MIX_TERMS];
    uint32_t n;
} chorus_dsp_mix_t;

typedef struct {
    uint32_t rate_hz;
    uint32_t channels;
    uint8_t map[CHORUS_DSP_MAX_CHANNELS];
    chorus_dsp_endpoint_t endpoint;
    chorus_dsp_sound_t sound;
    chorus_dsp_biquad_t eq[CHORUS_DSP_MAX_CHANNELS][CHORUS_DSP_ROOM_EQ_MAX_FILTERS];
    uint32_t eq_count;
    chorus_dsp_biquad_t tone[CHORUS_DSP_MAX_CHANNELS][2];
    bool tone_on[2];
    chorus_dsp_biquad_t loud[CHORUS_DSP_MAX_CHANNELS][2];
    bool loud_on[2];
    double loud_att;
    chorus_dsp_biquad_t speech;
    int speech_mode; /* 0 none, 1 channel, 2 mid/side */
    uint32_t speech_a, speech_b;
    chorus_dsp_compressor_t night;
    chorus_dsp_source_t source;
    uint32_t source_a, source_b;
    chorus_dsp_mix_t mix[2];
    chorus_dsp_biquad_t ambient[2];
    float ambient_gain;
    bool highpass;
    chorus_dsp_lr4_t bass;
    float sub_gain;
    float lfe_gain;
    int lfe; /* the LFE channel's index, or -1 */
    chorus_dsp_lr4_t two_way;
    float trims[2];
    float delay_pool[CHORUS_DSP_DELAY_POOL_FRAMES];
    chorus_dsp_delay_t delays[CHORUS_DSP_MAX_OUTPUTS];
    uint32_t lookahead;
    chorus_dsp_limiter_t limiter;
    uint32_t outputs;
    float frame[CHORUS_DSP_MAX_CHANNELS];
    float out[CHORUS_DSP_MAX_OUTPUTS];
} chorus_dsp_chain_t;

/* A chain for a stream of `channels` positions `map` at `rate_hz`. On a
 * refusal `*field` (when not NULL) names the setting or endpoint field. */
chorus_dsp_err_t chorus_dsp_chain_init(chorus_dsp_chain_t *c, const chorus_dsp_sound_t *sound,
                                       const chorus_dsp_endpoint_t *endpoint, const uint8_t *map,
                                       uint32_t channels, uint32_t rate_hz, const char **field);
/* New room settings; running filters keep their state when only gains
 * change. A refusal changes nothing. */
chorus_dsp_err_t chorus_dsp_chain_set_sound(chorus_dsp_chain_t *c, const chorus_dsp_sound_t *sound,
                                            const char **field);
uint32_t chorus_dsp_chain_out_channels(const chorus_dsp_chain_t *c);
uint32_t chorus_dsp_chain_latency_frames(const chorus_dsp_chain_t *c);
/* `frames` interleaved frames of the stream in, as many of the outputs out.
 * The limiter's ceiling is min(1, limit_gain). */
chorus_dsp_err_t chorus_dsp_chain_process(chorus_dsp_chain_t *c, const float *in, float *out,
                                          uint32_t frames, float room_gain, float limit_gain);

#endif /* CHORUS_DSP_H */
