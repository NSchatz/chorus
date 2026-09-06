#include "chorus/sync.h"

#include <string.h>

/* Offset filtering and the two-tier correction law, mirrored step for step
 * from crates/sync/src/servo.rs.
 *
 * The one thing here that is not in BRIEF.md and that the simulator found: a
 * sliding window hands back an offset measured up to a window ago, and the
 * offset between two crystals is moving the whole time, so the filter projects
 * the sample it selects forward to now at a drift rate it estimates from the
 * offsets themselves. Dropping that projection here would make the endpoint
 * agree with the Rust implementation on paper and disagree with it by hundreds
 * of microseconds in a run, which is exactly what the cross-check fixtures
 * exist to catch. */

static chorus_sample_t best_of(const chorus_sample_t *samples, size_t len)
{
    chorus_sample_t best = samples[0];
    for (size_t i = 1; i < len; i++) {
        if (samples[i].rtt_ns < best.rtt_ns) {
            best = samples[i];
        }
    }
    return best;
}

static double clamp_double(double value, double low, double high)
{
    if (value < low) {
        return low;
    }
    if (value > high) {
        return high;
    }
    return value;
}

void chorus_offset_filter_init(chorus_offset_filter_t *filter, size_t capacity, double alpha)
{
    memset(filter, 0, sizeof(*filter));
    if (capacity < 1) {
        capacity = 1;
    }
    if (capacity > CHORUS_MAX_FILTER_WINDOW) {
        capacity = CHORUS_MAX_FILTER_WINDOW;
    }
    filter->capacity = capacity;
    filter->alpha = alpha;
}

/* Slope between the least queued sample of the older half of the window and
 * the least queued sample of the newer half. Zero until there are enough
 * samples for the two halves to be a baseline worth measuring across. */
static double estimate_drift(const chorus_offset_filter_t *filter)
{
    if (filter->len < 4) {
        return 0.0;
    }
    size_t middle = filter->len / 2;
    chorus_sample_t older = best_of(filter->window, middle);
    chorus_sample_t newer = best_of(filter->window + middle, filter->len - middle);
    double span_ns = newer.at_ns - older.at_ns;
    if (span_ns <= 0.0) {
        return 0.0;
    }
    double drift = (newer.offset_ns - older.offset_ns) / span_ns;
    return clamp_double(drift, -CHORUS_MAX_TRACKED_DRIFT_PPM * 1e-6,
                        CHORUS_MAX_TRACKED_DRIFT_PPM * 1e-6);
}

double chorus_offset_filter_push(chorus_offset_filter_t *filter, double at_ns, double rtt_ns,
                                 double offset_ns)
{
    if (filter->len == filter->capacity) {
        memmove(filter->window, filter->window + 1, (filter->len - 1) * sizeof(chorus_sample_t));
        filter->len--;
    }
    filter->window[filter->len].at_ns = at_ns;
    filter->window[filter->len].rtt_ns = rtt_ns;
    filter->window[filter->len].offset_ns = offset_ns;
    filter->len++;

    filter->drift = estimate_drift(filter);

    /* The least queued exchange in the window is the most trustworthy
     * measurement, and it is also usually not the most recent one, so it is
     * projected forward to now before it is used. */
    chorus_sample_t best = best_of(filter->window, filter->len);
    filter->selected = best;
    filter->has_selected = 1;
    double aged = best.offset_ns + filter->drift * (at_ns - best.at_ns);

    double next;
    if (!filter->has_smoothed) {
        next = aged;
    } else {
        /* Smoothing a quantity that is itself moving would reintroduce the
         * same staleness through the back door, so the previous estimate is
         * carried forward at the drift rate before it is blended. */
        double predicted = filter->smoothed + filter->drift * (at_ns - filter->smoothed_at);
        next = predicted + filter->alpha * (aged - predicted);
    }
    filter->smoothed = next;
    filter->smoothed_at = at_ns;
    filter->has_smoothed = 1;
    return next;
}

double chorus_offset_filter_drift_ppm(const chorus_offset_filter_t *filter)
{
    return filter->drift * 1e6;
}

const chorus_sample_t *chorus_offset_filter_selected(const chorus_offset_filter_t *filter)
{
    return filter->has_selected ? &filter->selected : NULL;
}

void chorus_offset_filter_reset(chorus_offset_filter_t *filter)
{
    filter->len = 0;
    filter->has_smoothed = 0;
    filter->smoothed = 0.0;
    filter->smoothed_at = 0.0;
    filter->drift = 0.0;
    filter->has_selected = 0;
}

chorus_servo_config_t chorus_servo_config_default(void)
{
    chorus_servo_config_t config;
    config.kp = 0.4;
    config.ki = 0.08;
    config.max_correction_ppm = 500.0;
    config.hard_resync_threshold_ns = 3000000.0;
    config.filter_window = 8;
    config.smoothing_alpha = 0.25;
    return config;
}

void chorus_servo_init(chorus_servo_t *servo, chorus_servo_config_t config)
{
    servo->config = config;
    servo->integral_ppm = 0.0;
    servo->correction_ppm = 0.0;
    servo->hard_resyncs = 0;
    servo->updates = 0;
    servo->clamped = 0;
}

chorus_servo_action_t chorus_servo_update(chorus_servo_t *servo, double error_ns, double interval_s)
{
    chorus_servo_action_t action;
    memset(&action, 0, sizeof(action));
    servo->updates++;

    double magnitude = (error_ns < 0.0) ? -error_ns : error_ns;
    if (magnitude >= servo->config.hard_resync_threshold_ns) {
        servo->integral_ppm = 0.0;
        servo->correction_ppm = 0.0;
        servo->clamped = 0;
        servo->hard_resyncs++;
        action.tier = CHORUS_SERVO_HARD_RESYNC;
        action.step_ns = -error_ns;
        return action;
    }

    /* The ppm that would produce this error over one interval. */
    double normalised_ppm = error_ns / (interval_s * 1000.0);
    servo->integral_ppm += normalised_ppm;

    double raw = -(servo->config.kp * normalised_ppm + servo->config.ki * servo->integral_ppm);
    double clamped =
        clamp_double(raw, -servo->config.max_correction_ppm, servo->config.max_correction_ppm);
    servo->clamped = (clamped != raw) ? 1 : 0;
    if (servo->clamped) {
        /* Anti-windup: while the output is pinned, the integral does not get
         * to keep charging. */
        servo->integral_ppm -= normalised_ppm;
    }

    servo->correction_ppm = clamped;
    action.tier = CHORUS_SERVO_FINE;
    action.correction_ppm = clamped;
    return action;
}

double chorus_servo_correction_ppm(const chorus_servo_t *servo)
{
    return servo->correction_ppm;
}

int chorus_servo_last_correction_was_clamped(const chorus_servo_t *servo)
{
    return servo->clamped;
}

uint32_t chorus_servo_hard_resyncs(const chorus_servo_t *servo)
{
    return servo->hard_resyncs;
}
