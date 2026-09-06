/* The endpoint's sync core: the same offset filter, the same correction law
 * and the same deterministic simulator as `crates/sync`.
 *
 * This is the second implementation the roadmap phase's scope note asks for:
 * "The sync and protocol cores arrive here as C mirrors validated against
 * FOUNDATION-1's fixtures, which is the whole reason those fixtures are
 * files." It is held to the `.cfg` scenarios under fixtures/sync scenario for
 * scenario, and to the `.expected` files under fixtures/sync/crosscheck
 * exchange for exchange, so the two
 * implementations cannot drift apart without a red test.
 *
 * Every arithmetic step below is in the same order as the Rust one, on IEEE-754
 * doubles, because "selects the same sample as the Rust implementation given
 * the same inputs" is an assertion about arithmetic and not about intent. */

#ifndef CHORUS_SYNC_H
#define CHORUS_SYNC_H

#include <stddef.h>
#include <stdint.h>

/* --- SplitMix64 -------------------------------------------------------------
 *
 * Mirrored from crates/sync/src/rng.rs, which says in its own words that "the
 * same 20 lines have to be mirrored in C when the firmware wants to replay a
 * scenario". Reference: Steele, Lea and Flood, "Fast splittable pseudorandom
 * number generators" (2014). */

typedef struct {
    uint64_t state;
} chorus_rng_t;

void chorus_rng_init(chorus_rng_t *rng, uint64_t seed);
uint64_t chorus_rng_next_u64(chorus_rng_t *rng);
/* A double in [0, 1), from the top 53 bits so every value is exactly
 * representable. */
double chorus_rng_next_f64(chorus_rng_t *rng);

/* --- jitter -----------------------------------------------------------------
 *
 * Queuing delay, so never negative: a packet can be held up, it cannot arrive
 * before it was sent. That sign is what makes the minimum-round-trip filter
 * work at all. */

#define CHORUS_JITTER_TAIL_CAP_MULTIPLE 10.0

typedef enum {
    CHORUS_JITTER_NONE = 0,
    CHORUS_JITTER_UNIFORM,
    CHORUS_JITTER_EXPONENTIAL
} chorus_jitter_kind_t;

typedef struct {
    chorus_jitter_kind_t kind;
    double scale_us;
} chorus_jitter_t;

double chorus_jitter_sample_ns(const chorus_jitter_t *jitter, chorus_rng_t *rng);
const char *chorus_jitter_name(chorus_jitter_kind_t kind);
/* CHORUS_JITTER_NONE for a name the format does not define; `ok` distinguishes
 * that from the real `none`. */
chorus_jitter_kind_t chorus_jitter_from_name(const char *name, int *ok);

/* --- the offset filter ------------------------------------------------------ */

/* Largest offset drift the filter will believe, in ppm. */
#define CHORUS_MAX_TRACKED_DRIFT_PPM 2500.0

/* Deepest window the endpoint carries. config/sync.conf's filter_window is 64;
 * this is the ceiling a scenario may ask for, and a deeper one is refused
 * rather than silently clamped. */
#define CHORUS_MAX_FILTER_WINDOW 256

/* One exchange in the window. */
typedef struct {
    double at_ns;
    double rtt_ns;
    double offset_ns;
} chorus_sample_t;

typedef struct {
    size_t capacity;
    double alpha;
    chorus_sample_t window[CHORUS_MAX_FILTER_WINDOW];
    size_t len;
    int has_smoothed;
    double smoothed;
    double smoothed_at;
    double drift;
    int has_selected;
    chorus_sample_t selected;
} chorus_offset_filter_t;

/* A filter over `capacity` exchanges, smoothing with the given weight.
 * `capacity` is clamped up to 1 and down to CHORUS_MAX_FILTER_WINDOW. */
void chorus_offset_filter_init(chorus_offset_filter_t *filter, size_t capacity, double alpha);

/* Add one exchange and return the filtered offset estimate as of `at_ns`. */
double chorus_offset_filter_push(chorus_offset_filter_t *filter, double at_ns, double rtt_ns,
                                 double offset_ns);

/* Estimated rate at which the offset is moving, in ppm. */
double chorus_offset_filter_drift_ppm(const chorus_offset_filter_t *filter);

/* The sample the last push selected out of the window, or NULL when nothing
 * has been pushed. This is the exchange the offset in use came FROM, which is
 * what a caller publishing an error bound needs. */
const chorus_sample_t *chorus_offset_filter_selected(const chorus_offset_filter_t *filter);

void chorus_offset_filter_reset(chorus_offset_filter_t *filter);

/* --- the correction law ----------------------------------------------------- */

typedef struct {
    double kp;
    double ki;
    double max_correction_ppm;
    double hard_resync_threshold_ns;
    size_t filter_window;
    double smoothing_alpha;
} chorus_servo_config_t;

/* The defaults crates/sync/src/servo.rs carries, which are the values every
 * committed scenario is run with. */
chorus_servo_config_t chorus_servo_config_default(void);

typedef enum {
    /* Slew: apply this rate correction until the next exchange. */
    CHORUS_SERVO_FINE = 0,
    /* Step: the error is too large to slew away in reasonable time. */
    CHORUS_SERVO_HARD_RESYNC
} chorus_servo_tier_t;

typedef struct {
    chorus_servo_tier_t tier;
    /* Correction in ppm when the tier is FINE, already clamped. */
    double correction_ppm;
    /* How far the playout pointer moves when the tier is HARD_RESYNC. */
    double step_ns;
} chorus_servo_action_t;

typedef struct {
    chorus_servo_config_t config;
    double integral_ppm;
    double correction_ppm;
    uint32_t hard_resyncs;
    uint32_t updates;
    int clamped;
} chorus_servo_t;

void chorus_servo_init(chorus_servo_t *servo, chorus_servo_config_t config);
chorus_servo_action_t chorus_servo_update(chorus_servo_t *servo, double error_ns,
                                          double interval_s);
double chorus_servo_correction_ppm(const chorus_servo_t *servo);
int chorus_servo_last_correction_was_clamped(const chorus_servo_t *servo);
uint32_t chorus_servo_hard_resyncs(const chorus_servo_t *servo);

/* --- one simulator run ------------------------------------------------------ */

typedef struct {
    uint64_t seed;
    uint64_t duration_ms;
    uint64_t step_ms;
    uint64_t sync_interval_ms;
    double server_ppm;
    double client_ppm;
    int64_t initial_offset_ns;
    double base_one_way_delay_us;
    chorus_jitter_t jitter;
    chorus_servo_config_t servo;
} chorus_sim_config_t;

#define CHORUS_MAX_SKEW_PPM 1000.0
#define CHORUS_MAX_DELAY_US 100000.0
#define CHORUS_MAX_STEPS 10000000ull
#define CHORUS_SERVER_TURNAROUND_NS 50000.0

typedef enum {
    CHORUS_SIM_OK = 0,
    CHORUS_SIM_ERR_ZERO_DURATION,
    CHORUS_SIM_ERR_INVALID_STEP,
    CHORUS_SIM_ERR_ZERO_SYNC_INTERVAL,
    CHORUS_SIM_ERR_SKEW_OUT_OF_RANGE,
    CHORUS_SIM_ERR_DELAY_OUT_OF_RANGE,
    CHORUS_SIM_ERR_INVALID_SERVO_PARAMETER,
    CHORUS_SIM_ERR_TOO_MANY_STEPS,
    CHORUS_SIM_ERR_OUT_OF_MEMORY
} chorus_sim_status_t;

const char *chorus_sim_status_name(chorus_sim_status_t status);

chorus_sim_status_t chorus_sim_validate(const chorus_sim_config_t *config);
uint64_t chorus_sim_steps(const chorus_sim_config_t *config);

/* One sample of the modelled playout error. `error_ns` is playout position
 * minus the true server timeline; no participant in the model can observe it,
 * which is the whole reason a simulator is worth having. */
typedef struct {
    uint64_t t_ns;
    int64_t error_ns;
} chorus_playout_sample_t;

/* What one exchange did, recorded so the two implementations can be held to
 * the same selection rather than only to the same summary. */
typedef struct {
    uint32_t index;
    double at_ns;
    double rtt_ns;
    double offset_estimate_ns;
    double filtered_offset_ns;
    /* The sample the filter selected out of the window for this exchange. */
    chorus_sample_t selected;
    chorus_servo_tier_t tier;
    double correction_ppm;
    double step_ns;
} chorus_exchange_record_t;

typedef struct {
    chorus_playout_sample_t *samples;
    size_t sample_count;
    chorus_exchange_record_t *exchanges;
    size_t exchange_count;
    uint32_t hard_resyncs;
    double final_correction_ppm;
} chorus_sim_result_t;

/* Run one simulation. The caller frees with chorus_sim_result_free. */
chorus_sim_status_t chorus_sim_run(const chorus_sim_config_t *config, chorus_sim_result_t *out);
void chorus_sim_result_free(chorus_sim_result_t *result);

/* Index of the first sample from which the error stays inside `bound_ns` for
 * the whole rest of the run. Returns 0 and sets `found` to 0 when it never
 * does, which is the failure this exists to catch. */
size_t chorus_sim_settle_index(const chorus_sim_result_t *result, int64_t bound_ns, int *found);
uint64_t chorus_sim_settle_time_ns(const chorus_sim_result_t *result, int64_t bound_ns, int *found);
int64_t chorus_sim_max_abs_error_after(const chorus_sim_result_t *result, size_t index);
int64_t chorus_sim_max_abs_error(const chorus_sim_result_t *result);

/* --- a committed scenario --------------------------------------------------- */

#define CHORUS_SCENARIO_NAME_MAX 64

typedef struct {
    char name[CHORUS_SCENARIO_NAME_MAX];
    chorus_sim_config_t config;
    uint64_t settle_deadline_ms;
    int64_t error_bound_ns;
} chorus_scenario_t;

typedef enum {
    CHORUS_SCENARIO_OK = 0,
    CHORUS_SCENARIO_ERR_MALFORMED,
    CHORUS_SCENARIO_ERR_DUPLICATE_KEY,
    CHORUS_SCENARIO_ERR_UNKNOWN_KEY,
    CHORUS_SCENARIO_ERR_MISSING_KEY,
    CHORUS_SCENARIO_ERR_BAD_VALUE,
    CHORUS_SCENARIO_ERR_INVALID_CONFIG,
    CHORUS_SCENARIO_ERR_UNREADABLE
} chorus_scenario_status_t;

const char *chorus_scenario_status_name(chorus_scenario_status_t status);

/* Parse a scenario file's text. Unknown keys are an error rather than a shrug,
 * exactly as the Rust parser has it: a typo in a committed fixture that
 * silently fell back to a default would quietly weaken the regression. */
chorus_scenario_status_t chorus_scenario_parse(chorus_scenario_t *out, const char *text,
                                               char *detail, size_t detail_len);
chorus_scenario_status_t chorus_scenario_load(chorus_scenario_t *out, const char *path,
                                              char *detail, size_t detail_len);
uint64_t chorus_scenario_settle_deadline_ns(const chorus_scenario_t *scenario);

#endif /* CHORUS_SYNC_H */
