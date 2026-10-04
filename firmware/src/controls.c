#include "chorus/controls.h"

#include <string.h>

/* The classes, K67-K70. A class lists exactly the inputs the owner's decision
 * gives it; an input it does not list is refused by name at the seam. */
#define IN(x) (1u << (x))

/* The controller_command values of docs/protocol.md ("0x32 controller
 * command"); test_controls.c holds each to the catalog's name for it. */
enum {
    CMD_TOGGLE = 3,
    CMD_NEXT = 4,
    CMD_PREVIOUS = 5,
    CMD_VOLUME_STEP = 7,
    CMD_JOIN = 9,
    CMD_LEAVE = 10
};
#define BUTTONS                                                                                    \
    (IN(CHORUS_INPUT_PLAY_PAUSE) | IN(CHORUS_INPUT_VOLUME_UP) | IN(CHORUS_INPUT_VOLUME_DOWN) |     \
     IN(CHORUS_INPUT_NEXT) | IN(CHORUS_INPUT_PREVIOUS))

static const chorus_controls_profile_t PROFILES[CHORUS_CLASS_COUNT] = {
    /* K67: buttons or touch, status LED, mic + hardware mute switch. */
    {CHORUS_CLASS_COMPACT, BUTTONS | IN(CHORUS_INPUT_MIC_MUTE_SWITCH), CHORUS_LED_STATUS, 1, 1},
    /* K68: a hidden pairing button and a rear status light only. */
    {CHORUS_CLASS_TWO_WAY, IN(CHORUS_INPUT_PAIRING), CHORUS_LED_REAR_STATUS, 0, 0},
    /* K69: pairing button, status LED, level + phase knobs. */
    {CHORUS_CLASS_SUBWOOFER,
     IN(CHORUS_INPUT_PAIRING) | IN(CHORUS_INPUT_SUB_LEVEL_KNOB) | IN(CHORUS_INPUT_SUB_PHASE_KNOB),
     CHORUS_LED_STATUS, 0, 1},
    /* K70: pairing button + LED, front buttons. */
    {CHORUS_CLASS_STREAMING_AMP, BUTTONS | IN(CHORUS_INPUT_PAIRING), CHORUS_LED_STATUS, 0, 1},
};

static const char *const CLASS_NAMES[CHORUS_CLASS_COUNT] = {"compact", "two-way", "subwoofer",
                                                            "streaming-amp"};

static const char *const INPUT_NAMES[CHORUS_INPUT_COUNT] = {
    "play-pause", "volume-up",       "volume-down",    "next",          "previous",
    "pairing",    "mic-mute-switch", "sub-level-knob", "sub-phase-knob"};

const char *chorus_speaker_class_name(chorus_speaker_class_t speaker_class)
{
    return (unsigned)speaker_class < CHORUS_CLASS_COUNT ? CLASS_NAMES[speaker_class] : NULL;
}

int chorus_speaker_class_from_name(const char *name, chorus_speaker_class_t *out)
{
    for (unsigned i = 0; i < CHORUS_CLASS_COUNT; i++) {
        if (strcmp(name, CLASS_NAMES[i]) == 0) {
            *out = (chorus_speaker_class_t)i;
            return 0;
        }
    }
    return -1;
}

const char *chorus_control_input_name(chorus_control_input_t input)
{
    return (unsigned)input < CHORUS_INPUT_COUNT ? INPUT_NAMES[input] : NULL;
}

const chorus_controls_profile_t *chorus_controls_profile(chorus_speaker_class_t speaker_class)
{
    return (unsigned)speaker_class < CHORUS_CLASS_COUNT ? &PROFILES[speaker_class] : NULL;
}

static int has_input(const chorus_controls_t *c, chorus_control_input_t input)
{
    return (unsigned)input < CHORUS_INPUT_COUNT && (c->profile->inputs & IN(input)) != 0;
}

static int copy_text(char *out, size_t cap, const char *text)
{
    size_t len = strlen(text);
    if (len >= cap || len > CHORUS_V2_MAX_SHORT_TEXT) {
        return -1;
    }
    memcpy(out, text, len + 1);
    return 0;
}

int chorus_controls_init(chorus_controls_t *controls, chorus_speaker_class_t speaker_class,
                         const char *room, const char *join_target)
{
    memset(controls, 0, sizeof(*controls));
    controls->profile = chorus_controls_profile(speaker_class);
    if (controls->profile == NULL || copy_text(controls->room, sizeof(controls->room), room) != 0 ||
        copy_text(controls->join_target, sizeof(controls->join_target), join_target) != 0) {
        return -1;
    }
    /* Until the server says otherwise, the endpoint plays its own room. */
    memcpy(controls->group, controls->room, sizeof(controls->group));
    /* The microphone gate starts CLOSED: until the switch has been read, the
     * endpoint does not know the owner has not muted it. */
    controls->mic_live = 0;
    return 0;
}

static void push(chorus_controls_t *c, const chorus_control_action_t *action)
{
    if (c->pending_count >= CHORUS_CONTROLS_MAX_ACTIONS) {
        c->dropped_actions++;
        return;
    }
    c->pending[c->pending_count++] = *action;
}

static void push_simple(chorus_controls_t *c, chorus_control_action_kind_t kind,
                        chorus_control_input_t input, int32_t value, uint64_t now_ns)
{
    chorus_control_action_t a;
    memset(&a, 0, sizeof(a));
    a.kind = kind;
    a.input = input;
    a.value = value;
    a.at_ns = now_ns;
    push(c, &a);
}

static void push_command(chorus_controls_t *c, chorus_control_input_t input, uint8_t command,
                         int16_t value, const char *target, uint64_t now_ns)
{
    chorus_control_action_t a;
    memset(&a, 0, sizeof(a));
    a.kind = CHORUS_ACTION_COMMAND;
    a.input = input;
    a.at_ns = now_ns;
    a.value = value;
    a.command.command = command;
    a.command.value = value;
    size_t len = strlen(target);
    memcpy(a.target_text, target, len + 1);
    /* `command.target` is pointed at the action's own copy of the text when
     * the action is handed out, since the action is copied until then. */
    push(c, &a);
}

int chorus_controls_level(chorus_controls_t *controls, chorus_control_input_t input, int level,
                          uint64_t now_ns)
{
    if (!has_input(controls, input) || input == CHORUS_INPUT_SUB_LEVEL_KNOB ||
        input == CHORUS_INPUT_SUB_PHASE_KNOB) {
        controls->refused_inputs++;
        return -1;
    }
    chorus_control_input_state_t *s = &controls->inputs[input];
    uint8_t v = level ? 1u : 0u;
    if (!s->seen || v != s->raw) {
        s->raw = v;
        s->raw_since_ns = now_ns;
        s->seen = 1;
    }
    return 0;
}

/* The join/leave a long press of play/pause asks for: leave when grouped with
 * other rooms, else join the configured target. */
static void long_press_group(chorus_controls_t *c, uint64_t now_ns)
{
    if (strcmp(c->group, c->room) != 0) {
        push_command(c, CHORUS_INPUT_PLAY_PAUSE, CMD_LEAVE, 0, "", now_ns);
    } else if (c->join_target[0] != '\0') {
        push_command(c, CHORUS_INPUT_PLAY_PAUSE, CMD_JOIN, 0, c->join_target, now_ns);
    } else {
        push_simple(c, CHORUS_ACTION_NO_JOIN_TARGET, CHORUS_INPUT_PLAY_PAUSE, 0, now_ns);
    }
}

static void on_press(chorus_controls_t *c, chorus_control_input_t input, uint64_t at_ns)
{
    chorus_control_input_state_t *s = &c->inputs[input];
    s->pressed_at_ns = at_ns;
    s->long_fired = 0;
    switch (input) {
    case CHORUS_INPUT_VOLUME_UP:
    case CHORUS_INPUT_VOLUME_DOWN:
        push_command(c, input, CMD_VOLUME_STEP,
                     (int16_t)(input == CHORUS_INPUT_VOLUME_UP ? CHORUS_CONTROLS_VOLUME_STEP
                                                               : -CHORUS_CONTROLS_VOLUME_STEP),
                     "", at_ns);
        s->next_repeat_ns = at_ns + CHORUS_CONTROLS_REPEAT_DELAY_NS;
        break;
    case CHORUS_INPUT_NEXT:
        push_command(c, input, CMD_NEXT, 0, "", at_ns);
        break;
    case CHORUS_INPUT_PREVIOUS:
        push_command(c, input, CMD_PREVIOUS, 0, "", at_ns);
        break;
    case CHORUS_INPUT_PAIRING:
        push_simple(c, CHORUS_ACTION_PAIRING, input, 1, at_ns);
        break;
    default:
        /* play/pause decides on release or at the long-press mark. */
        break;
    }
}

static void on_release(chorus_controls_t *c, chorus_control_input_t input, uint64_t at_ns)
{
    chorus_control_input_state_t *s = &c->inputs[input];
    if (input == CHORUS_INPUT_PLAY_PAUSE && !s->long_fired) {
        push_command(c, input, CMD_TOGGLE, 0, "", at_ns);
    }
}

size_t chorus_controls_poll(chorus_controls_t *controls, uint64_t now_ns,
                            chorus_control_action_t *out, size_t max)
{
    for (unsigned i = 0; i < CHORUS_INPUT_COUNT; i++) {
        chorus_control_input_t input = (chorus_control_input_t)i;
        chorus_control_input_state_t *s = &controls->inputs[i];
        if (!has_input(controls, input) || !s->seen || input == CHORUS_INPUT_SUB_LEVEL_KNOB ||
            input == CHORUS_INPUT_SUB_PHASE_KNOB) {
            continue;
        }
        /* A level is believed once it has held for the debounce time; the
         * event is dated when it became stable, not when it was polled. */
        uint64_t settled_at = s->raw_since_ns + CHORUS_CONTROLS_DEBOUNCE_NS;
        if (s->raw != s->stable && now_ns >= settled_at) {
            s->stable = s->raw;
            if (input == CHORUS_INPUT_MIC_MUTE_SWITCH) {
                controls->mic_live = s->stable ? 0 : 1;
                push_simple(controls, CHORUS_ACTION_MIC_MUTE, input, s->stable, settled_at);
            } else if (s->stable) {
                on_press(controls, input, settled_at);
            } else {
                on_release(controls, input, settled_at);
            }
        } else if (input == CHORUS_INPUT_MIC_MUTE_SWITCH && s->raw == 0 && s->stable == 0 &&
                   !controls->mic_live && now_ns >= settled_at) {
            /* The first stable reading of a switch in the live position
             * opens the gate. */
            controls->mic_live = 1;
            push_simple(controls, CHORUS_ACTION_MIC_MUTE, input, 0, settled_at);
        }
        if (!s->stable) {
            continue;
        }
        if (input == CHORUS_INPUT_PLAY_PAUSE && !s->long_fired &&
            now_ns >= s->pressed_at_ns + CHORUS_CONTROLS_LONG_PRESS_NS) {
            s->long_fired = 1;
            long_press_group(controls, s->pressed_at_ns + CHORUS_CONTROLS_LONG_PRESS_NS);
        }
        if (input == CHORUS_INPUT_VOLUME_UP || input == CHORUS_INPUT_VOLUME_DOWN) {
            while (now_ns >= s->next_repeat_ns) {
                push_command(controls, input, CMD_VOLUME_STEP,
                             (int16_t)(input == CHORUS_INPUT_VOLUME_UP
                                           ? CHORUS_CONTROLS_VOLUME_STEP
                                           : -CHORUS_CONTROLS_VOLUME_STEP),
                             "", s->next_repeat_ns);
                s->next_repeat_ns += CHORUS_CONTROLS_REPEAT_PERIOD_NS;
            }
        }
    }
    size_t n = controls->pending_count < max ? controls->pending_count : max;
    for (size_t i = 0; i < n; i++) {
        out[i] = controls->pending[i];
        if (out[i].kind == CHORUS_ACTION_COMMAND) {
            out[i].command.target.data = (const uint8_t *)out[i].target_text;
            out[i].command.target.len = strlen(out[i].target_text);
        }
    }
    memmove(controls->pending, controls->pending + n,
            (controls->pending_count - n) * sizeof(controls->pending[0]));
    controls->pending_count -= n;
    return n;
}

/* The step a knob code maps to, with hysteresis: the code has to go past the
 * step's edge by CHORUS_CONTROLS_KNOB_HYSTERESIS codes before the step changes. */
static int32_t knob_step(uint32_t code, int32_t steps, int32_t current, int have_current)
{
    int32_t span = (int32_t)CHORUS_CONTROLS_KNOB_MAX + 1;
    int32_t step = (int32_t)(((int64_t)code * steps) / span);
    if (step >= steps) {
        step = steps - 1;
    }
    if (!have_current || step == current) {
        return step;
    }
    /* Where the current step's edge toward `step` lies, in codes. */
    int32_t edge = (step > current) ? (int32_t)(((int64_t)(current + 1) * span) / steps)
                                    : (int32_t)(((int64_t)current * span) / steps);
    int32_t distance = (int32_t)code - edge;
    if (distance < 0) {
        distance = -distance;
    }
    return distance >= (int32_t)CHORUS_CONTROLS_KNOB_HYSTERESIS ? step : current;
}

int chorus_controls_knob(chorus_controls_t *controls, chorus_control_input_t input, uint32_t code,
                         uint64_t now_ns)
{
    if ((input != CHORUS_INPUT_SUB_LEVEL_KNOB && input != CHORUS_INPUT_SUB_PHASE_KNOB) ||
        !has_input(controls, input) || code > CHORUS_CONTROLS_KNOB_MAX) {
        controls->refused_inputs++;
        return -1;
    }
    chorus_control_input_state_t *s = &controls->inputs[input];
    int level = input == CHORUS_INPUT_SUB_LEVEL_KNOB;
    int32_t steps =
        level ? (-CHORUS_CONTROLS_SUB_LEVEL_MIN_TENTHS_DB /
                 CHORUS_CONTROLS_SUB_LEVEL_STEP_TENTHS_DB) +
                    1
              : (CHORUS_CONTROLS_SUB_PHASE_MAX_DEG / CHORUS_CONTROLS_SUB_PHASE_STEP_DEG) + 1;
    int32_t step = knob_step(code, steps, s->knob_step, s->seen);
    int changed = !s->seen || step != s->knob_step;
    s->seen = 1;
    s->knob_code = code;
    s->knob_step = step;
    if (!changed) {
        return 0;
    }
    if (level) {
        /* Step 0 is the full cut, the last step 0 dB: never above 0 dB. */
        int32_t tenths = CHORUS_CONTROLS_SUB_LEVEL_MIN_TENTHS_DB +
                         step * CHORUS_CONTROLS_SUB_LEVEL_STEP_TENTHS_DB;
        if (tenths > 0) {
            tenths = 0;
        }
        controls->sub_level_tenths_db = tenths;
        push_simple(controls, CHORUS_ACTION_SUB_LEVEL, input, tenths, now_ns);
    } else {
        int32_t deg = step * CHORUS_CONTROLS_SUB_PHASE_STEP_DEG;
        controls->sub_phase_deg = deg;
        push_simple(controls, CHORUS_ACTION_SUB_PHASE, input, deg, now_ns);
    }
    return 0;
}

void chorus_controls_state(chorus_controls_t *controls, const chorus_v2_controller_state_t *state)
{
    size_t len = state->group.len;
    if (len > CHORUS_V2_MAX_SHORT_TEXT) {
        return;
    }
    if (len == 0) {
        memcpy(controls->group, controls->room, sizeof(controls->group));
    } else {
        memcpy(controls->group, state->group.data, len);
        controls->group[len] = '\0';
    }
    controls->state_known = 1;
}

int chorus_controls_mic_live(const chorus_controls_t *controls)
{
    return controls->profile->has_microphone && controls->mic_live;
}

size_t chorus_controls_mic_pass(const chorus_controls_t *controls, const int16_t *in, size_t count,
                                int16_t *out)
{
    if (!chorus_controls_mic_live(controls)) {
        return 0;
    }
    memcpy(out, in, count * sizeof(in[0]));
    return count;
}

chorus_encode_status_t chorus_controls_encode(const chorus_control_action_t *action, uint8_t *out,
                                              size_t out_len, size_t *written)
{
    if (action->kind != CHORUS_ACTION_COMMAND) {
        return CHORUS_ENCODE_INVALID_FIELD;
    }
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_CONTROLLER_COMMAND;
    m.as.controller_command = action->command;
    m.as.controller_command.target.data = (const uint8_t *)action->target_text;
    m.as.controller_command.target.len = strlen(action->target_text);
    return chorus_v2_encode(&m, out, out_len, written, NULL);
}

/* --- the status LED ------------------------------------------------------- */

static const char *const LED_STATE_NAMES[] = {"boot",  "link-down", "idle",  "playing",
                                              "muted", "pairing",   "fault", "listening"};

const char *chorus_led_state_name(chorus_led_state_t state)
{
    return (unsigned)state <= CHORUS_LED_LISTENING ? LED_STATE_NAMES[state] : NULL;
}

chorus_led_state_t chorus_led_decide(const chorus_led_inputs_t *in)
{
    if (in->fault) {
        return CHORUS_LED_FAULT;
    }
    if (in->pairing) {
        return CHORUS_LED_PAIRING;
    }
    if (in->mic_muted) {
        return CHORUS_LED_MUTED;
    }
    if (!in->booted) {
        return CHORUS_LED_BOOT;
    }
    if (!in->link_up) {
        return CHORUS_LED_LINK_DOWN;
    }
    if (in->listening) {
        return CHORUS_LED_LISTENING;
    }
    if (in->playing && in->adopted) {
        return CHORUS_LED_PLAYING;
    }
    return CHORUS_LED_IDLE;
}

/* The fixed palette, ASSUMED (docs/hardware/controls.md): a colour per state,
 * dim, since a status light in a living room should not be a lamp. */
static const chorus_led_output_t PALETTE[] = {
    [CHORUS_LED_BOOT] = {255, 255, 255, 32}, [CHORUS_LED_LINK_DOWN] = {255, 160, 0, 48},
    [CHORUS_LED_IDLE] = {255, 255, 255, 8},  [CHORUS_LED_PLAYING] = {255, 255, 255, 24},
    [CHORUS_LED_MUTED] = {255, 64, 0, 48},   [CHORUS_LED_PAIRING] = {0, 96, 255, 64},
    [CHORUS_LED_FAULT] = {255, 0, 0, 96},    [CHORUS_LED_LISTENING] = {0, 255, 160, 64},
};

void chorus_led_init(chorus_led_t *led, const chorus_controls_profile_t *profile)
{
    memset(led, 0, sizeof(*led));
    led->kind = profile->led;
    led->follows_visualizer = profile->led_follows_visualizer && profile->led == CHORUS_LED_STATUS;
    led->base = PALETTE[CHORUS_LED_PLAYING];
}

int chorus_led_offer(chorus_led_t *led, const chorus_v2_message_t *message)
{
    chorus_led_event_t e;
    memset(&e, 0, sizeof(e));
    if (message->type == CHORUS_V2_VISUALIZER_FRAME) {
        const chorus_v2_visualizer_frame_t *v = &message->as.visualizer_frame;
        e.timestamp_ns = v->timestamp_ns;
        e.beat = v->beat;
        e.peak = v->peak;
    } else if (message->type == CHORUS_V2_COLOR) {
        const chorus_v2_color_t *c = &message->as.color;
        e.timestamp_ns = c->timestamp_ns;
        e.is_color = 1;
        e.red = c->red;
        e.green = c->green;
        e.blue = c->blue;
        e.brightness = c->brightness;
    } else {
        return -1;
    }
    if (led->queued >= CHORUS_LED_QUEUE) {
        led->dropped++;
        return -1;
    }
    /* Keep the queue in the order the events are heard. */
    size_t at = led->queued;
    while (at > 0 && led->queue[at - 1].timestamp_ns > e.timestamp_ns) {
        led->queue[at] = led->queue[at - 1];
        at--;
    }
    led->queue[at] = e;
    led->queued++;
    return 0;
}

chorus_led_output_t chorus_led_render(chorus_led_t *led, chorus_led_state_t state,
                                      uint64_t server_now_ns)
{
    /* Take every event that is heard by now, in order. */
    size_t taken = 0;
    while (taken < led->queued && led->queue[taken].timestamp_ns <= server_now_ns) {
        const chorus_led_event_t *e = &led->queue[taken];
        if (e->is_color) {
            led->base.red = e->red;
            led->base.green = e->green;
            led->base.blue = e->blue;
            led->base.brightness = e->brightness;
        } else {
            led->have_frame = 1;
            led->frame_at_ns = e->timestamp_ns;
            led->beat = e->beat;
            led->peak = e->peak;
        }
        taken++;
    }
    memmove(led->queue, led->queue + taken, (led->queued - taken) * sizeof(led->queue[0]));
    led->queued -= taken;

    chorus_led_output_t out = PALETTE[state];
    if (led->kind == CHORUS_LED_NONE) {
        chorus_led_output_t off = {0, 0, 0, 0};
        return off;
    }
    if (state != CHORUS_LED_PLAYING || !led->follows_visualizer || !led->have_frame ||
        server_now_ns - led->frame_at_ns > CHORUS_LED_VISUALIZER_STALE_NS) {
        return out;
    }
    out = led->base;
    if (led->beat < CHORUS_LED_BEAT_THRESHOLD) {
        out.brightness = (uint8_t)(((uint32_t)led->base.brightness * led->peak + 127u) / 255u);
    }
    return out;
}
