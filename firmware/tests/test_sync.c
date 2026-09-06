/* The endpoint's sync core, held to FOUNDATION-1's committed fixtures.
 *
 * Two assertions, and they are different assertions:
 *
 *   1. Every committed scenario under fixtures/sync drives the modelled
 *      playout error below that scenario's declared bound by its declared
 *      deadline, and HOLDS it for the whole rest of the run. That is the
 *      phase's own acceptance, run against the C core.
 *   2. For every one of those scenarios the C core reproduces, exchange by
 *      exchange, exactly what the Rust core did: the same round trip, the same
 *      raw estimate, the same SELECTED sample out of the window, the same
 *      filtered offset and the same servo decision. That is the assertion that
 *      makes the first one mean something, because two implementations can
 *      converge for different reasons and only one of them is a mirror.
 *
 * The comparison is exact. Doubles cross in the shortest decimal that
 * round-trips, so both sides hold the identical bit pattern, and the endpoint
 * is compiled with -ffp-contract=off so a fused multiply-add cannot round
 * differently from the two operations the Rust side compiles to. */

#include "chorus/sync.h"
#include "harness.h"

#include <dirent.h>
#include <inttypes.h>
#include <math.h>

#define MAX_SCENARIOS 32
#define MAX_EXCHANGES 512

/* Generous, because a directory entry may be 255 bytes and snprintf truncating
 * a fixture path silently is exactly the kind of quiet wrong answer this whole
 * suite exists to avoid. */
typedef struct {
    char file[256];
    char path[1024];
    char vector[1024];
} scenario_file_t;

static int compare_names(const void *a, const void *b)
{
    return strcmp(((const scenario_file_t *)a)->file, ((const scenario_file_t *)b)->file);
}

static size_t committed_scenarios(scenario_file_t *out, size_t capacity)
{
    char dir_path[512];
    chorus_repo_path(dir_path, sizeof(dir_path), "fixtures/sync");
    DIR *dir = opendir(dir_path);
    if (dir == NULL) {
        return 0;
    }
    size_t count = 0;
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL && count < capacity) {
        const char *dot = strrchr(entry->d_name, '.');
        if (dot == NULL || strcmp(dot, ".cfg") != 0) {
            continue;
        }
        snprintf(out[count].file, sizeof(out[count].file), "%s", entry->d_name);
        snprintf(out[count].path, sizeof(out[count].path), "%s/%s", dir_path, entry->d_name);
        char stem[256];
        snprintf(stem, sizeof(stem), "%s", entry->d_name);
        char *stem_dot = strrchr(stem, '.');
        if (stem_dot != NULL) {
            *stem_dot = '\0';
        }
        snprintf(out[count].vector, sizeof(out[count].vector), "%s/crosscheck/%s.expected",
                 dir_path, stem);
        count++;
    }
    closedir(dir);
    qsort(out, count, sizeof(scenario_file_t), compare_names);
    return count;
}

/* --- the committed cross-check vector --------------------------------------- */

typedef struct {
    uint32_t index;
    double at_ns;
    double rtt_ns;
    double offset_estimate_ns;
    double filtered_offset_ns;
    double selected_at_ns;
    double selected_rtt_ns;
    double selected_offset_ns;
    char tier[16];
    double correction_or_step;
} expected_exchange_t;

typedef struct {
    char scenario[128];
    int64_t error_bound_ns;
    uint64_t steps;
    uint64_t exchanges;
    uint32_t hard_resyncs;
    double final_correction_ppm;
    int settled;
    uint64_t settle_index;
    uint64_t settle_time_ns;
    int64_t peak_after_settle_ns;
    int64_t max_abs_error_ns;
    int64_t first_error_ns;
    int64_t last_error_ns;
    expected_exchange_t exchanges_list[MAX_EXCHANGES];
    size_t exchange_count;
} expected_vector_t;

static int load_vector(const char *path, expected_vector_t *out)
{
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return -1;
    }
    memset(out, 0, sizeof(*out));
    char line[1024];
    int in_exchanges = 0;
    while (fgets(line, sizeof(line), file) != NULL) {
        char *hash = strchr(line, '#');
        if (hash != NULL) {
            *hash = '\0';
        }
        char *cursor = line;
        while (*cursor == ' ' || *cursor == '\t') {
            cursor++;
        }
        size_t len = strlen(cursor);
        while (len > 0 && (cursor[len - 1] == '\n' || cursor[len - 1] == '\r' ||
                           cursor[len - 1] == ' ')) {
            cursor[--len] = '\0';
        }
        if (len == 0) {
            continue;
        }
        if (strcmp(cursor, "[exchanges]") == 0) {
            in_exchanges = 1;
            continue;
        }
        if (!in_exchanges) {
            char key[64];
            char value[128];
            if (sscanf(cursor, "%63s = %127s", key, value) != 2) {
                continue;
            }
            if (strcmp(key, "scenario") == 0) {
                snprintf(out->scenario, sizeof(out->scenario), "%s", value);
            } else if (strcmp(key, "error_bound_ns") == 0) {
                out->error_bound_ns = strtoll(value, NULL, 10);
            } else if (strcmp(key, "steps") == 0) {
                out->steps = strtoull(value, NULL, 10);
            } else if (strcmp(key, "exchanges") == 0) {
                out->exchanges = strtoull(value, NULL, 10);
            } else if (strcmp(key, "hard_resyncs") == 0) {
                out->hard_resyncs = (uint32_t)strtoul(value, NULL, 10);
            } else if (strcmp(key, "final_correction_ppm") == 0) {
                out->final_correction_ppm = strtod(value, NULL);
            } else if (strcmp(key, "settled") == 0) {
                out->settled = (strcmp(value, "yes") == 0) ? 1 : 0;
            } else if (strcmp(key, "settle_index") == 0) {
                out->settle_index = strtoull(value, NULL, 10);
            } else if (strcmp(key, "settle_time_ns") == 0) {
                out->settle_time_ns = strtoull(value, NULL, 10);
            } else if (strcmp(key, "peak_after_settle_ns") == 0) {
                out->peak_after_settle_ns = strtoll(value, NULL, 10);
            } else if (strcmp(key, "max_abs_error_ns") == 0) {
                out->max_abs_error_ns = strtoll(value, NULL, 10);
            } else if (strcmp(key, "first_error_ns") == 0) {
                out->first_error_ns = strtoll(value, NULL, 10);
            } else if (strcmp(key, "last_error_ns") == 0) {
                out->last_error_ns = strtoll(value, NULL, 10);
            }
            continue;
        }
        if (out->exchange_count >= MAX_EXCHANGES) {
            fclose(file);
            return -2;
        }
        expected_exchange_t *record = &out->exchanges_list[out->exchange_count];
        char at[64], rtt[64], estimate[64], filtered[64], sel_at[64], sel_rtt[64], sel_offset[64],
            value[64];
        if (sscanf(cursor, "%" SCNu32 " %63s %63s %63s %63s %63s %63s %63s %15s %63s",
                   &record->index, at, rtt, estimate, filtered, sel_at, sel_rtt, sel_offset,
                   record->tier, value) != 10) {
            fclose(file);
            return -3;
        }
        record->at_ns = strtod(at, NULL);
        record->rtt_ns = strtod(rtt, NULL);
        record->offset_estimate_ns = strtod(estimate, NULL);
        record->filtered_offset_ns = strtod(filtered, NULL);
        record->selected_at_ns = strtod(sel_at, NULL);
        record->selected_rtt_ns = strtod(sel_rtt, NULL);
        record->selected_offset_ns = strtod(sel_offset, NULL);
        record->correction_or_step = strtod(value, NULL);
        out->exchange_count++;
    }
    fclose(file);
    return 0;
}

/* --- the two assertions ------------------------------------------------------ */

static void scenario_holds_its_bound(const scenario_file_t *file)
{
    chorus_scenario_t scenario;
    char detail[512];
    detail[0] = '\0';
    chorus_scenario_status_t status =
        chorus_scenario_load(&scenario, file->path, detail, sizeof(detail));
    chorus_check(status == CHORUS_SCENARIO_OK, "%s parses (%s %s)", file->file,
                 chorus_scenario_status_name(status), detail);
    if (status != CHORUS_SCENARIO_OK) {
        return;
    }

    chorus_sim_result_t result;
    chorus_sim_status_t run_status = chorus_sim_run(&scenario.config, &result);
    chorus_check(run_status == CHORUS_SIM_OK, "%s: the committed configuration runs (%s)",
                 file->file, chorus_sim_status_name(run_status));
    if (run_status != CHORUS_SIM_OK) {
        return;
    }

    chorus_check(result.sample_count == chorus_sim_steps(&scenario.config),
                 "%s: one sample per step (%zu of %" PRIu64 ")", file->file, result.sample_count,
                 chorus_sim_steps(&scenario.config));

    int found = 0;
    uint64_t settled = chorus_sim_settle_time_ns(&result, scenario.error_bound_ns, &found);
    chorus_check(found,
                 "%s: the error comes inside %" PRId64 " ns for the rest of the run (peak %" PRId64
                 " ns)",
                 file->file, scenario.error_bound_ns, chorus_sim_max_abs_error(&result));
    if (!found) {
        chorus_sim_result_free(&result);
        return;
    }
    uint64_t deadline = chorus_scenario_settle_deadline_ns(&scenario);
    chorus_check(settled <= deadline, "%s: settled at %llu ms against a deadline of %llu ms",
                 file->file, (unsigned long long)(settled / 1000000ull),
                 (unsigned long long)(deadline / 1000000ull));

    size_t index = chorus_sim_settle_index(&result, scenario.error_bound_ns, &found);
    int64_t held = chorus_sim_max_abs_error_after(&result, index);
    chorus_check(held < scenario.error_bound_ns,
                 "%s: after settling the peak error is %" PRId64 " ns against a bound of %" PRId64
                 " ns",
                 file->file, held, scenario.error_bound_ns);

    /* "Below the bound and hold it for the rest of the run" is not "below the
     * bound at the end": check the tail sample by sample rather than trusting
     * the settle index that produced it. */
    int every_sample = 1;
    for (size_t i = index; i < result.sample_count; i++) {
        int64_t error = result.samples[i].error_ns;
        if (error < 0) {
            error = -error;
        }
        if (error >= scenario.error_bound_ns) {
            every_sample = 0;
            break;
        }
    }
    chorus_check(every_sample, "%s: every sample after settling is inside the bound", file->file);
    chorus_check(result.sample_count - index > result.sample_count / 2,
                 "%s: settling consumed less than half the run", file->file);

    int64_t steady = chorus_sim_max_abs_error_after(&result, result.sample_count / 2);
    printf("     %s: settled at %llu ms, peak |error| %" PRId64 " ns after settling and "
           "%" PRId64 " ns over the back half, %zu exchanges, %" PRIu32 " hard resyncs, final "
           "correction %.2f ppm\n",
           file->file, (unsigned long long)(settled / 1000000ull), held, steady,
           result.exchange_count, result.hard_resyncs, result.final_correction_ppm);

    chorus_sim_result_free(&result);
}

static void scenario_matches_the_rust_implementation(const scenario_file_t *file)
{
    expected_vector_t expected;
    int loaded = load_vector(file->vector, &expected);
    chorus_check(loaded == 0, "%s has a readable committed cross-check vector (status %d)",
                 file->file, loaded);
    if (loaded != 0) {
        return;
    }

    chorus_scenario_t scenario;
    char detail[512];
    if (chorus_scenario_load(&scenario, file->path, detail, sizeof(detail)) !=
        CHORUS_SCENARIO_OK) {
        chorus_check(0, "%s parses", file->file);
        return;
    }
    chorus_sim_result_t result;
    if (chorus_sim_run(&scenario.config, &result) != CHORUS_SIM_OK) {
        chorus_check(0, "%s runs", file->file);
        return;
    }

    chorus_check(strcmp(expected.scenario, scenario.name) == 0,
                 "%s: the vector is for scenario %s and the file names %s", file->file,
                 expected.scenario, scenario.name);
    chorus_check(expected.error_bound_ns == scenario.error_bound_ns,
                 "%s: the vector's bound is the scenario's bound", file->file);
    chorus_check(result.sample_count == expected.steps,
                 "%s: %zu steps against the vector's %" PRIu64, file->file, result.sample_count,
                 expected.steps);
    chorus_check(result.exchange_count == expected.exchanges,
                 "%s: %zu exchanges against the vector's %" PRIu64, file->file,
                 result.exchange_count, expected.exchanges);
    chorus_check(result.hard_resyncs == expected.hard_resyncs,
                 "%s: %" PRIu32 " hard resyncs against the vector's %" PRIu32, file->file,
                 result.hard_resyncs, expected.hard_resyncs);
    chorus_check(result.final_correction_ppm == expected.final_correction_ppm,
                 "%s: final correction %.17g against the vector's %.17g", file->file,
                 result.final_correction_ppm, expected.final_correction_ppm);
    chorus_check(chorus_sim_max_abs_error(&result) == expected.max_abs_error_ns,
                 "%s: peak |error| %" PRId64 " against the vector's %" PRId64, file->file,
                 chorus_sim_max_abs_error(&result), expected.max_abs_error_ns);
    chorus_check(result.samples[0].error_ns == expected.first_error_ns &&
                     result.samples[result.sample_count - 1].error_ns == expected.last_error_ns,
                 "%s: the first and last modelled errors match the vector", file->file);

    int found = 0;
    size_t index = chorus_sim_settle_index(&result, scenario.error_bound_ns, &found);
    chorus_check(found == expected.settled, "%s: settled=%d against the vector's %d", file->file,
                 found, expected.settled);
    if (found && expected.settled) {
        chorus_check(index == expected.settle_index,
                     "%s: settle index %zu against the vector's %" PRIu64, file->file, index,
                     expected.settle_index);
        chorus_check(chorus_sim_max_abs_error_after(&result, index) ==
                         expected.peak_after_settle_ns,
                     "%s: peak after settling matches the vector", file->file);
    }

    /* The assertion the phase's scope note is really about: the same sample,
     * exchange by exchange. */
    size_t mismatches = 0;
    size_t selection_mismatches = 0;
    size_t compared = result.exchange_count < expected.exchange_count ? result.exchange_count
                                                                      : expected.exchange_count;
    for (size_t i = 0; i < compared; i++) {
        const chorus_exchange_record_t *got = &result.exchanges[i];
        const expected_exchange_t *want = &expected.exchanges_list[i];
        int selection_same = got->selected.at_ns == want->selected_at_ns &&
                             got->selected.rtt_ns == want->selected_rtt_ns &&
                             got->selected.offset_ns == want->selected_offset_ns;
        if (!selection_same) {
            selection_mismatches++;
        }
        const char *tier = (got->tier == CHORUS_SERVO_FINE) ? "fine" : "hard-resync";
        double value = (got->tier == CHORUS_SERVO_FINE) ? got->correction_ppm : got->step_ns;
        int same = selection_same && got->index == want->index && got->at_ns == want->at_ns &&
                   got->rtt_ns == want->rtt_ns &&
                   got->offset_estimate_ns == want->offset_estimate_ns &&
                   got->filtered_offset_ns == want->filtered_offset_ns &&
                   strcmp(tier, want->tier) == 0 && value == want->correction_or_step;
        if (!same) {
            if (mismatches == 0) {
                printf("     %s: first divergence at exchange %zu\n", file->file, i);
                printf("       rust: at %.17g rtt %.17g estimate %.17g filtered %.17g selected "
                       "(%.17g, %.17g, %.17g) %s %.17g\n",
                       want->at_ns, want->rtt_ns, want->offset_estimate_ns,
                       want->filtered_offset_ns, want->selected_at_ns, want->selected_rtt_ns,
                       want->selected_offset_ns, want->tier, want->correction_or_step);
                printf("       c   : at %.17g rtt %.17g estimate %.17g filtered %.17g selected "
                       "(%.17g, %.17g, %.17g) %s %.17g\n",
                       got->at_ns, got->rtt_ns, got->offset_estimate_ns, got->filtered_offset_ns,
                       got->selected.at_ns, got->selected.rtt_ns, got->selected.offset_ns, tier,
                       value);
            }
            mismatches++;
        }
    }
    chorus_check(selection_mismatches == 0,
                 "%s: the endpoint selects the same sample as the Rust implementation in all %zu "
                 "exchanges (%zu differ)",
                 file->file, compared, selection_mismatches);
    chorus_check(mismatches == 0,
                 "%s: every one of %zu exchanges matches the committed vector exactly (%zu "
                 "differ)",
                 file->file, compared, mismatches);

    chorus_sim_result_free(&result);
}

/* --- the pieces, mirrored from the Rust unit tests --------------------------- */

static void the_generator_reproduces_its_stream(void)
{
    chorus_rng_t a, b;
    chorus_rng_init(&a, 0x5EED0001);
    chorus_rng_init(&b, 0x5EED0001);
    int same = 1;
    for (int i = 0; i < 1000; i++) {
        if (chorus_rng_next_u64(&a) != chorus_rng_next_u64(&b)) {
            same = 0;
        }
    }
    chorus_check(same, "a seed reproduces its stream");

    chorus_rng_init(&a, 1);
    chorus_rng_init(&b, 2);
    int overlaps = 0;
    for (int i = 0; i < 1000; i++) {
        if (chorus_rng_next_u64(&a) == chorus_rng_next_u64(&b)) {
            overlaps++;
        }
    }
    chorus_check(overlaps == 0, "two seeds produce no overlapping draws");

    chorus_rng_init(&a, 42);
    int inside = 1;
    for (int i = 0; i < 10000; i++) {
        double x = chorus_rng_next_f64(&a);
        if (!(x >= 0.0 && x < 1.0)) {
            inside = 0;
        }
    }
    chorus_check(inside, "every double is in [0, 1)");

    /* SplitMix64's first draw from seed 0 is a documented constant of the
     * algorithm, so this catches a mirror that transcribed a constant wrong
     * without needing the Rust side to be running. */
    chorus_rng_init(&a, 0);
    chorus_check(chorus_rng_next_u64(&a) == 0xE220A8397B1DCDAFull,
                 "SplitMix64 from seed 0 produces its documented first draw");
}

static void jitter_is_never_negative_and_is_capped(void)
{
    chorus_jitter_t uniform = {CHORUS_JITTER_UNIFORM, 150.0};
    chorus_rng_t rng;
    chorus_rng_init(&rng, 9);
    double max_seen = 0.0;
    int ok = 1;
    for (int i = 0; i < 10000; i++) {
        double sample = chorus_jitter_sample_ns(&uniform, &rng);
        if (sample < 0.0 || sample > 150000.0) {
            ok = 0;
        }
        if (sample > max_seen) {
            max_seen = sample;
        }
    }
    chorus_check(ok && max_seen > 140000.0,
                 "uniform jitter stays inside its bound and reaches the whole range (%.1f ns)",
                 max_seen);

    chorus_jitter_t exponential = {CHORUS_JITTER_EXPONENTIAL, 200.0};
    chorus_rng_init(&rng, 11);
    double total = 0.0;
    double cap_ns = 200.0 * CHORUS_JITTER_TAIL_CAP_MULTIPLE * 1000.0;
    int capped = 1;
    const int draws = 100000;
    for (int i = 0; i < draws; i++) {
        double sample = chorus_jitter_sample_ns(&exponential, &rng);
        if (sample < 0.0 || sample > cap_ns) {
            capped = 0;
        }
        total += sample;
    }
    double mean_us = total / draws / 1000.0;
    chorus_check(capped && mean_us > 180.0 && mean_us < 210.0,
                 "exponential jitter is non-negative, capped, and means %.1f us against a "
                 "configured 200 us",
                 mean_us);

    chorus_jitter_t none = {CHORUS_JITTER_NONE, 0.0};
    chorus_rng_init(&rng, 1);
    int zero = 1;
    for (int i = 0; i < 100; i++) {
        if (chorus_jitter_sample_ns(&none, &rng) != 0.0) {
            zero = 0;
        }
    }
    chorus_check(zero, "no jitter means no jitter");
}

static void the_filter_and_the_servo_behave_as_the_rust_ones_do(void)
{
    const double tick = 1e9;

    chorus_offset_filter_t filter;
    chorus_offset_filter_init(&filter, 4, 1.0);
    chorus_check(chorus_offset_filter_selected(&filter) == NULL, "nothing has been pushed");
    chorus_offset_filter_push(&filter, tick, 900000.0, 5000.0);
    chorus_offset_filter_push(&filter, 2.0 * tick, 300000.0, 1000.0);
    double estimate = chorus_offset_filter_push(&filter, 3.0 * tick, 1500000.0, 90000.0);
    chorus_check(estimate == 1000.0, "the 300 us round trip is the trustworthy sample (%.1f)",
                 estimate);
    chorus_check(chorus_offset_filter_drift_ppm(&filter) == 0.0, "three samples is not a baseline");
    const chorus_sample_t *selected = chorus_offset_filter_selected(&filter);
    chorus_check(selected != NULL && selected->rtt_ns == 300000.0 &&
                     selected->offset_ns == 1000.0 && selected->at_ns == 2.0 * tick,
                 "the filter says which exchange the estimate came from");
    chorus_offset_filter_reset(&filter);
    chorus_check(chorus_offset_filter_selected(&filter) == NULL,
                 "a reset forgets the selection too");

    chorus_offset_filter_init(&filter, 2, 1.0);
    chorus_offset_filter_push(&filter, tick, 100.0, 1.0);
    chorus_offset_filter_push(&filter, 2.0 * tick, 200.0, 2.0);
    chorus_check(chorus_offset_filter_push(&filter, 3.0 * tick, 300.0, 3.0) == 2.0,
                 "the window slides");

    chorus_offset_filter_init(&filter, 1, 0.25);
    chorus_check(chorus_offset_filter_push(&filter, tick, 100.0, 0.0) == 0.0 &&
                     chorus_offset_filter_push(&filter, 2.0 * tick, 100.0, 100.0) == 25.0 &&
                     chorus_offset_filter_push(&filter, 3.0 * tick, 100.0, 100.0) == 43.75,
                 "smoothing moves part of the way");

    /* Two crystals 100 ppm apart with every exchange equally queued: the
     * minimum-round-trip rule keeps selecting the oldest sample, which is the
     * worst case for staleness. Without the forward projection the estimate
     * would be up to seven seconds, and 700 us, out of date. */
    chorus_offset_filter_init(&filter, 8, 0.5);
    for (int exchange = 1; exchange <= 16; exchange++) {
        double offset = 10000.0 + 100.0 * 1000.0 * exchange;
        chorus_offset_filter_push(&filter, exchange * tick, 900000.0, offset);
    }
    double truth = 10000.0 + 100.0 * 1000.0 * 16.0;
    double aged = filter.smoothed;
    chorus_check(fabs(aged - truth) < 1000.0,
                 "a stale sample is projected forward: %.1f ns against a true %.1f ns", aged,
                 truth);
    chorus_check(fabs(chorus_offset_filter_drift_ppm(&filter) - 100.0) < 1.0,
                 "drift is estimated at %.3f ppm against a true 100 ppm",
                 chorus_offset_filter_drift_ppm(&filter));

    chorus_servo_t servo;
    chorus_servo_init(&servo, chorus_servo_config_default());
    chorus_servo_action_t action = chorus_servo_update(&servo, 50000.0, 1.0);
    chorus_check(action.tier == CHORUS_SERVO_FINE && fabs(action.correction_ppm + 24.0) < 1e-9,
                 "50 us over 1 s is 50 ppm, and kp 0.4 plus ki 0.08 gives -24 ppm (%.9f)",
                 action.correction_ppm);
    chorus_check(!chorus_servo_last_correction_was_clamped(&servo),
                 "a correction inside the clamp is not reported as clamped");

    action = chorus_servo_update(&servo, 2999999.0, 1.0);
    chorus_check(action.tier == CHORUS_SERVO_FINE && action.correction_ppm == -500.0 &&
                     chorus_servo_last_correction_was_clamped(&servo),
                 "just under the threshold demands more than the clamp allows, and says so");

    action = chorus_servo_update(&servo, 12345000.0, 1.0);
    chorus_check(action.tier == CHORUS_SERVO_HARD_RESYNC && action.step_ns == -12345000.0 &&
                     chorus_servo_hard_resyncs(&servo) == 1 &&
                     chorus_servo_correction_ppm(&servo) == 0.0 &&
                     !chorus_servo_last_correction_was_clamped(&servo),
                 "a large error steps instead of slewing, and clamps nothing");

    /* The integral learns a constant skew, which is the term that makes the
     * loop hold rather than merely acquire. */
    chorus_servo_init(&servo, chorus_servo_config_default());
    double error_ns = 0.0;
    for (int i = 0; i < 200; i++) {
        chorus_servo_action_t step = chorus_servo_update(&servo, error_ns, 1.0);
        if (step.tier != CHORUS_SERVO_FINE) {
            chorus_check(0, "no step should be needed while learning a 50 ppm skew");
            return;
        }
        error_ns += (50.0 + step.correction_ppm) * 1000.0;
    }
    chorus_check(fabs(chorus_servo_correction_ppm(&servo) + 50.0) < 0.5 && fabs(error_ns) < 1000.0,
                 "the integral settles at %.3f ppm against a 50 ppm skew, residual %.1f ns",
                 chorus_servo_correction_ppm(&servo), error_ns);
}

static void an_unmodellable_configuration_is_refused(void)
{
    chorus_scenario_t scenario;
    char detail[512];
    const char *sample =
        "name = example\nseed = 7\nduration_ms = 30000\nstep_ms = 10\nsync_interval_ms = 1000\n"
        "server_ppm = -12.5\nclient_ppm = 38.0\ninitial_offset_ns = -8000000\n"
        "base_one_way_delay_us = 200.0\njitter_model = exponential\njitter_scale_us = 150.0\n"
        "settle_deadline_ms = 15000\n";
    chorus_check(chorus_scenario_parse(&scenario, sample, detail, sizeof(detail)) ==
                     CHORUS_SCENARIO_OK,
                 "a well formed scenario parses");
    chorus_check(scenario.error_bound_ns == 1000000,
                 "an absent error_bound_ns defaults to the 1 ms FOUNDATION-1 committed");

    char text[1024];
    snprintf(text, sizeof(text), "%sjitter_kind = uniform\n", sample);
    chorus_check(chorus_scenario_parse(&scenario, text, detail, sizeof(detail)) ==
                     CHORUS_SCENARIO_ERR_UNKNOWN_KEY,
                 "an unknown key is an error rather than a shrug");

    snprintf(text, sizeof(text), "%sseed = 9\n", sample);
    chorus_check(chorus_scenario_parse(&scenario, text, detail, sizeof(detail)) ==
                     CHORUS_SCENARIO_ERR_DUPLICATE_KEY,
                 "a duplicate key is an error");

    snprintf(text, sizeof(text), "%s", sample);
    char *skew = strstr(text, "client_ppm = 38.0");
    memcpy(skew, "client_ppm = 9000", 17);
    chorus_check(chorus_scenario_parse(&scenario, text, detail, sizeof(detail)) ==
                     CHORUS_SCENARIO_ERR_INVALID_CONFIG,
                 "a configuration the model does not describe is refused at parse time");
}

int main(void)
{
    scenario_file_t scenarios[MAX_SCENARIOS];
    size_t count = committed_scenarios(scenarios, MAX_SCENARIOS);
    chorus_check(count >= 4, "found %zu committed scenarios under fixtures/sync", count);

    chorus_section("every committed scenario drives the error below its bound and holds it");
    for (size_t i = 0; i < count; i++) {
        scenario_holds_its_bound(&scenarios[i]);
    }

    chorus_section("the endpoint reproduces the Rust implementation exchange by exchange");
    for (size_t i = 0; i < count; i++) {
        scenario_matches_the_rust_implementation(&scenarios[i]);
    }

    chorus_section("the seeded generator");
    the_generator_reproduces_its_stream();

    chorus_section("the jitter models");
    jitter_is_never_negative_and_is_capped();

    chorus_section("the filter and the correction law");
    the_filter_and_the_servo_behave_as_the_rust_ones_do();

    chorus_section("a scenario the model does not describe");
    an_unmodellable_configuration_is_refused();

    return chorus_test_report("endpoint sync core");
}
