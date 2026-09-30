#include "chorus/sync.h"

#include <math.h>
#include <stdlib.h>
#include <string.h>

/* The simulator: two virtual clocks, a network, an exchange, a servo and the
 * modelled playout-error series that comes out. Mirrored from
 * crates/sync/src/sim.rs.
 *
 * Nothing here reads a clock, so two runs of one configuration are the same
 * run - the property the whole cross-check rests on. */

const char *chorus_sim_status_name(chorus_sim_status_t status)
{
    switch (status) {
    case CHORUS_SIM_OK:
        return "ok";
    case CHORUS_SIM_ERR_ZERO_DURATION:
        return "zero-duration";
    case CHORUS_SIM_ERR_INVALID_STEP:
        return "invalid-step";
    case CHORUS_SIM_ERR_ZERO_SYNC_INTERVAL:
        return "zero-sync-interval";
    case CHORUS_SIM_ERR_SKEW_OUT_OF_RANGE:
        return "skew-out-of-range";
    case CHORUS_SIM_ERR_DELAY_OUT_OF_RANGE:
        return "delay-out-of-range";
    case CHORUS_SIM_ERR_INVALID_SERVO_PARAMETER:
        return "invalid-servo-parameter";
    case CHORUS_SIM_ERR_TOO_MANY_STEPS:
        return "too-many-steps";
    case CHORUS_SIM_ERR_OUT_OF_MEMORY:
        return "out-of-memory";
    case CHORUS_SIM_ERR_ASYMMETRY_OUT_OF_RANGE:
        return "asymmetry-out-of-range";
    case CHORUS_SIM_ERR_INVALID_JITTER_PARAMETER:
        return "invalid-jitter-parameter";
    case CHORUS_SIM_ERR_WANDER_OUT_OF_RANGE:
        return "wander-out-of-range";
    }
    return "unknown-status";
}

uint64_t chorus_sim_steps(const chorus_sim_config_t *config)
{
    if (config->step_ms == 0) {
        return 0;
    }
    return config->duration_ms / config->step_ms;
}

static int chorus_finite(double value)
{
    return isfinite(value) ? 1 : 0;
}

static chorus_sim_status_t check_skew(double ppm)
{
    double magnitude = (ppm < 0.0) ? -ppm : ppm;
    if (!chorus_finite(ppm) || magnitude > CHORUS_MAX_SKEW_PPM) {
        return CHORUS_SIM_ERR_SKEW_OUT_OF_RANGE;
    }
    return CHORUS_SIM_OK;
}

static chorus_sim_status_t check_delay(double value_us)
{
    if (!chorus_finite(value_us) || value_us < 0.0 || value_us > CHORUS_MAX_DELAY_US) {
        return CHORUS_SIM_ERR_DELAY_OUT_OF_RANGE;
    }
    return CHORUS_SIM_OK;
}

static chorus_sim_status_t check_asymmetry(double base_us, double asymmetry_us)
{
    double return_us = base_us + asymmetry_us;
    if (!chorus_finite(asymmetry_us) || return_us < 0.0 || return_us > CHORUS_MAX_DELAY_US) {
        return CHORUS_SIM_ERR_ASYMMETRY_OUT_OF_RANGE;
    }
    return CHORUS_SIM_OK;
}

static chorus_sim_status_t check_jitter(const chorus_jitter_t *jitter)
{
    if (jitter->kind != CHORUS_JITTER_BURST) {
        return CHORUS_SIM_OK;
    }
    /* Mirrored from check_jitter in config.rs: a burst that can never end is
     * not a link, and a tail index at or below 1 has no finite mean. */
    double enter = jitter->burst_enter_prob;
    double exit_prob = jitter->burst_exit_prob;
    double scale = jitter->burst_scale_us;
    double shape = jitter->burst_shape;
    if (!chorus_finite(enter) || enter < 0.0 || enter > 1.0) {
        return CHORUS_SIM_ERR_INVALID_JITTER_PARAMETER;
    }
    if (!chorus_finite(exit_prob) || exit_prob <= 0.0 || exit_prob > 1.0) {
        return CHORUS_SIM_ERR_INVALID_JITTER_PARAMETER;
    }
    if (!chorus_finite(scale) || scale <= 0.0 || scale > CHORUS_MAX_DELAY_US) {
        return CHORUS_SIM_ERR_INVALID_JITTER_PARAMETER;
    }
    if (!chorus_finite(shape) || shape <= 1.0 || shape > 100.0) {
        return CHORUS_SIM_ERR_INVALID_JITTER_PARAMETER;
    }
    return CHORUS_SIM_OK;
}

static chorus_sim_status_t check_servo(const chorus_servo_config_t *servo)
{
    /* Zero is a legitimate gain: a servo with both gains at zero is the control
     * case that proves a regression is asserting something. */
    const double non_negative[2] = {servo->kp, servo->ki};
    for (int i = 0; i < 2; i++) {
        if (!chorus_finite(non_negative[i]) || non_negative[i] < 0.0) {
            return CHORUS_SIM_ERR_INVALID_SERVO_PARAMETER;
        }
    }
    const double positive[2] = {servo->max_correction_ppm, servo->hard_resync_threshold_ns};
    for (int i = 0; i < 2; i++) {
        if (!chorus_finite(positive[i]) || positive[i] <= 0.0) {
            return CHORUS_SIM_ERR_INVALID_SERVO_PARAMETER;
        }
    }
    if (!chorus_finite(servo->smoothing_alpha) || servo->smoothing_alpha <= 0.0 ||
        servo->smoothing_alpha > 1.0) {
        return CHORUS_SIM_ERR_INVALID_SERVO_PARAMETER;
    }
    if (servo->filter_window == 0) {
        return CHORUS_SIM_ERR_INVALID_SERVO_PARAMETER;
    }
    return CHORUS_SIM_OK;
}

chorus_sim_status_t chorus_sim_validate(const chorus_sim_config_t *config)
{
    if (config->duration_ms == 0) {
        return CHORUS_SIM_ERR_ZERO_DURATION;
    }
    if (config->step_ms == 0 || config->step_ms > config->duration_ms) {
        return CHORUS_SIM_ERR_INVALID_STEP;
    }
    if (config->sync_interval_ms == 0) {
        return CHORUS_SIM_ERR_ZERO_SYNC_INTERVAL;
    }
    chorus_sim_status_t status = check_skew(config->server_ppm);
    if (status != CHORUS_SIM_OK) {
        return status;
    }
    status = check_skew(config->client_ppm);
    if (status != CHORUS_SIM_OK) {
        return status;
    }
    status = check_delay(config->base_one_way_delay_us);
    if (status != CHORUS_SIM_OK) {
        return status;
    }
    status = check_asymmetry(config->base_one_way_delay_us, config->path_asymmetry_us);
    if (status != CHORUS_SIM_OK) {
        return status;
    }
    status = check_delay(config->jitter.kind == CHORUS_JITTER_NONE ? 0.0 : config->jitter.scale_us);
    if (status != CHORUS_SIM_OK) {
        return status;
    }
    status = check_jitter(&config->jitter);
    if (status != CHORUS_SIM_OK) {
        return status;
    }
    double wander = config->client_wander_ppm_per_sqrt_s;
    if (!chorus_finite(wander) || wander < 0.0 || wander > CHORUS_MAX_WANDER_PPM_PER_SQRT_S) {
        return CHORUS_SIM_ERR_WANDER_OUT_OF_RANGE;
    }
    status = check_servo(&config->servo);
    if (status != CHORUS_SIM_OK) {
        return status;
    }

    uint64_t steps = chorus_sim_steps(config);
    if (steps == 0) {
        return CHORUS_SIM_ERR_ZERO_DURATION;
    }
    if (steps > CHORUS_MAX_STEPS) {
        return CHORUS_SIM_ERR_TOO_MANY_STEPS;
    }
    return CHORUS_SIM_OK;
}

void chorus_sim_result_free(chorus_sim_result_t *result)
{
    free(result->samples);
    free(result->exchanges);
    result->samples = NULL;
    result->exchanges = NULL;
    result->sample_count = 0;
    result->exchange_count = 0;
}

chorus_sim_status_t chorus_sim_run(const chorus_sim_config_t *config, chorus_sim_result_t *out)
{
    memset(out, 0, sizeof(*out));
    chorus_sim_status_t status = chorus_sim_validate(config);
    if (status != CHORUS_SIM_OK) {
        return status;
    }

    chorus_rng_t rng;
    chorus_rng_init(&rng, config->seed);
    chorus_jitter_process_t jitter;
    chorus_jitter_process_init(&jitter, &config->jitter);
    chorus_rng_t wander_rng;
    chorus_rng_init(&wander_rng, config->seed + CHORUS_WANDER_STREAM);
    chorus_offset_filter_t filter;
    chorus_offset_filter_init(&filter, config->servo.filter_window, config->servo.smoothing_alpha);
    chorus_servo_t servo;
    chorus_servo_init(&servo, config->servo);

    uint64_t steps = chorus_sim_steps(config);
    double step_ns = (double)config->step_ms * 1000000.0;
    double sync_interval_ns = (double)config->sync_interval_ms * 1000000.0;
    double interval_s = (double)config->sync_interval_ms / 1000.0;
    double base_delay_ns = config->base_one_way_delay_us * 1000.0;
    double return_base_delay_ns = base_delay_ns + config->path_asymmetry_us * 1000.0;

    /* A uniform increment on [-w, w] has standard deviation w / sqrt(3). */
    int wander_enabled = config->client_wander_ppm_per_sqrt_s > 0.0;
    double wander_half_width_ppm =
        config->client_wander_ppm_per_sqrt_s * sqrt(step_ns * 1e-9) * sqrt(3.0);

    double server_rate = 1.0 + config->server_ppm * 1e-6;
    double client_rate = 1.0 + config->client_ppm * 1e-6;
    double epoch_offset_ns = (double)config->initial_offset_ns;

    /* The client starts playing out on its own clock, knowing nothing about
     * the server timeline. Acquiring it is the servo's first job. */
    double playout_ns = epoch_offset_ns;
    double correction_ppm = 0.0;
    double next_sync_ns = sync_interval_ns;
    /* Both stay exactly 0.0 without wander, and adding 0.0 changes no bit,
     * so a scenario without wander runs the arithmetic it always ran. */
    double wander_ppm = 0.0;
    double wander_phase_ns = 0.0;

    out->samples = calloc((size_t)steps, sizeof(chorus_playout_sample_t));
    if (out->samples == NULL) {
        return CHORUS_SIM_ERR_OUT_OF_MEMORY;
    }
    size_t exchange_capacity = (size_t)(config->duration_ms / config->sync_interval_ms) + 4;
    out->exchanges = calloc(exchange_capacity, sizeof(chorus_exchange_record_t));
    if (out->exchanges == NULL) {
        chorus_sim_result_free(out);
        return CHORUS_SIM_ERR_OUT_OF_MEMORY;
    }

    for (uint64_t step = 0; step < steps; step++) {
        double t_ns = (double)(step + 1) * step_ns;

        if (wander_enabled) {
            wander_ppm += wander_half_width_ppm * (2.0 * chorus_rng_next_f64(&wander_rng) - 1.0);
        }

        /* Playout advances at the client's crystal rate, plus whatever the
         * servo is currently correcting by. */
        playout_ns += step_ns * (1.0 + (config->client_ppm + wander_ppm + correction_ppm) * 1e-6);
        wander_phase_ns += step_ns * wander_ppm * 1e-6;

        while (t_ns >= next_sync_ns) {
            double client_now_ns = t_ns * client_rate + epoch_offset_ns + wander_phase_ns;
            double forward_ns = base_delay_ns + chorus_jitter_process_sample_ns(&jitter, &rng);
            double return_ns =
                return_base_delay_ns + chorus_jitter_process_sample_ns(&jitter, &rng);

            /* RFC 5905 section 8. Every timestamp is taken on the clock of the
             * device that took it. */
            double t0 = client_now_ns;
            double t1 = (t_ns + forward_ns) * server_rate;
            double t2 = (t_ns + forward_ns + CHORUS_SERVER_TURNAROUND_NS) * server_rate;
            double t3 =
                (t_ns + forward_ns + CHORUS_SERVER_TURNAROUND_NS + return_ns) * client_rate +
                epoch_offset_ns + wander_phase_ns;

            double offset_estimate = ((t1 - t0) + (t2 - t3)) / 2.0;
            double rtt = (t3 - t0) - (t2 - t1);
            double filtered_offset =
                chorus_offset_filter_push(&filter, client_now_ns, rtt, offset_estimate);

            /* What the client believes the server timeline reads right now. */
            double server_estimate_ns = client_now_ns + filtered_offset;
            double observed_error_ns = playout_ns - server_estimate_ns;

            chorus_servo_action_t action =
                chorus_servo_update(&servo, observed_error_ns, interval_s);

            if (out->exchange_count < exchange_capacity) {
                chorus_exchange_record_t *record = &out->exchanges[out->exchange_count];
                record->index = (uint32_t)out->exchange_count;
                record->at_ns = client_now_ns;
                record->rtt_ns = rtt;
                record->offset_estimate_ns = offset_estimate;
                record->filtered_offset_ns = filtered_offset;
                const chorus_sample_t *selected = chorus_offset_filter_selected(&filter);
                record->selected = *selected;
                record->tier = action.tier;
                record->correction_ppm = action.correction_ppm;
                record->step_ns = action.step_ns;
            }
            out->exchange_count++;

            if (action.tier == CHORUS_SERVO_FINE) {
                correction_ppm = action.correction_ppm;
            } else {
                playout_ns += action.step_ns;
                correction_ppm = 0.0;
            }

            next_sync_ns += sync_interval_ns;
        }

        double server_timeline_ns = t_ns * server_rate;
        out->samples[step].t_ns = (uint64_t)t_ns;
        out->samples[step].error_ns = (int64_t)round(playout_ns - server_timeline_ns);
    }

    out->sample_count = (size_t)steps;
    out->hard_resyncs = chorus_servo_hard_resyncs(&servo);
    out->final_correction_ppm = chorus_servo_correction_ppm(&servo);
    return CHORUS_SIM_OK;
}

static int64_t saturating_abs(int64_t value)
{
    if (value == INT64_MIN) {
        return INT64_MAX;
    }
    return (value < 0) ? -value : value;
}

size_t chorus_sim_settle_index(const chorus_sim_result_t *result, int64_t bound_ns, int *found)
{
    size_t settle = 0;
    for (size_t index = 0; index < result->sample_count; index++) {
        if (saturating_abs(result->samples[index].error_ns) >= bound_ns) {
            settle = index + 1;
        }
    }
    if (settle >= result->sample_count) {
        *found = 0;
        return 0;
    }
    *found = 1;
    return settle;
}

uint64_t chorus_sim_settle_time_ns(const chorus_sim_result_t *result, int64_t bound_ns, int *found)
{
    size_t index = chorus_sim_settle_index(result, bound_ns, found);
    if (!*found) {
        return 0;
    }
    return result->samples[index].t_ns;
}

int64_t chorus_sim_max_abs_error_after(const chorus_sim_result_t *result, size_t index)
{
    if (index > result->sample_count) {
        index = result->sample_count;
    }
    int64_t peak = 0;
    for (size_t i = index; i < result->sample_count; i++) {
        int64_t magnitude = saturating_abs(result->samples[i].error_ns);
        if (magnitude > peak) {
            peak = magnitude;
        }
    }
    return peak;
}

int64_t chorus_sim_max_abs_error(const chorus_sim_result_t *result)
{
    return chorus_sim_max_abs_error_after(result, 0);
}
