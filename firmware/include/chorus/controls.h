/* The endpoint's physical controls, its status LED and its microphone gate
 * (brief section 13 item 2; K65, K67-K70; docs/decisions/0063-*).
 *
 * Pure logic, graded on a host by firmware/tests/test_controls.c. Nothing here
 * reads a clock or a pin: the binding hands in a level or an ADC reading with
 * the monotonic time it was sampled at (BRIEF.md guardrail 4), and takes back
 * actions. What leaves the endpoint is a protocol v2 `controller_command`,
 * encoded by the same encoder the session uses, so a button is the controller
 * role of docs/protocol.md and not a second control plane: the server decides
 * what the request changes and clamps it by the room's limits (K81, I10).
 *
 * Per class (K67-K70):
 *   compact       buttons or touch (play/pause, volume up and down, next,
 *                 previous), a status LED, a microphone behind a hardware
 *                 mute switch;
 *   two-way       a hidden pairing button and a rear status light only;
 *   subwoofer     a pairing button, a status LED, level and phase knobs;
 *   streaming amp a pairing button and LED, front buttons (play/pause, volume
 *                 up and down, next, previous). Its line-in, optical in and
 *                 line/sub out are inputs and outputs (the source role, goal
 *                 17), not controls, and are not modelled here.
 *
 * Every time constant below is ASSUMED: chosen here, not measured, until a
 * bench session on the owner's hardware says otherwise. */

#ifndef CHORUS_CONTROLS_H
#define CHORUS_CONTROLS_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/protocol_v2.h"

/* ASSUMED: a contact is believed once it has held one level this long. */
#define CHORUS_CONTROLS_DEBOUNCE_NS 20000000ull
/* ASSUMED: a press held this long is a long press. */
#define CHORUS_CONTROLS_LONG_PRESS_NS 1000000000ull
/* ASSUMED: a held volume button repeats after this, then every period. */
#define CHORUS_CONTROLS_REPEAT_DELAY_NS 600000000ull
#define CHORUS_CONTROLS_REPEAT_PERIOD_NS 300000000ull
/* ASSUMED: one volume press asks for this many points of 100. */
#define CHORUS_CONTROLS_VOLUME_STEP 5
/* The knobs' ADC span (12 bits, the ESP32-S3's SAR ADC width). */
#define CHORUS_CONTROLS_KNOB_MAX 4095u
/* The subwoofer's level knob is a CUT, never a boost: -12.0 dB to 0.0 dB in
 * tenths, so no knob position can take the sub past the level the room's
 * limit allows (K81, I10). */
#define CHORUS_CONTROLS_SUB_LEVEL_MIN_TENTHS_DB (-120)
#define CHORUS_CONTROLS_SUB_LEVEL_STEP_TENTHS_DB 5
/* The phase knob: 0 to 180 degrees in 15 degree steps. */
#define CHORUS_CONTROLS_SUB_PHASE_MAX_DEG 180
#define CHORUS_CONTROLS_SUB_PHASE_STEP_DEG 15
/* ASSUMED: ADC codes of hysteresis before a knob is believed to have moved
 * to a neighbouring step, so noise at a step edge does not chatter. */
#define CHORUS_CONTROLS_KNOB_HYSTERESIS 24u
/* ASSUMED: a visualizer stream older than this is over; the LED returns to
 * its steady playing colour. */
#define CHORUS_LED_VISUALIZER_STALE_NS 500000000ull
/* ASSUMED: a beat at least this strong flashes the LED to full brightness. */
#define CHORUS_LED_BEAT_THRESHOLD 128u
/* Frames the LED holds ahead of the moment they are heard. */
#define CHORUS_LED_QUEUE 16u

typedef enum {
    CHORUS_CLASS_COMPACT = 0,
    CHORUS_CLASS_TWO_WAY,
    CHORUS_CLASS_SUBWOOFER,
    CHORUS_CLASS_STREAMING_AMP,
    CHORUS_CLASS_COUNT
} chorus_speaker_class_t;

const char *chorus_speaker_class_name(chorus_speaker_class_t speaker_class);

/* Returns 0 and sets *out, or -1 for a name that is not a class. */
int chorus_speaker_class_from_name(const char *name, chorus_speaker_class_t *out);

typedef enum {
    CHORUS_INPUT_PLAY_PAUSE = 0,
    CHORUS_INPUT_VOLUME_UP,
    CHORUS_INPUT_VOLUME_DOWN,
    CHORUS_INPUT_NEXT,
    CHORUS_INPUT_PREVIOUS,
    CHORUS_INPUT_PAIRING,
    /* A latching switch, not a button: its level IS the mute state. */
    CHORUS_INPUT_MIC_MUTE_SWITCH,
    /* Knobs, read as ADC codes through chorus_controls_knob. */
    CHORUS_INPUT_SUB_LEVEL_KNOB,
    CHORUS_INPUT_SUB_PHASE_KNOB,
    CHORUS_INPUT_COUNT
} chorus_control_input_t;

const char *chorus_control_input_name(chorus_control_input_t input);

typedef enum {
    CHORUS_LED_NONE = 0,
    /* A status LED a listener sees (compact, subwoofer, streaming amp). */
    CHORUS_LED_STATUS,
    /* The two-way's rear status light (K68): status only, never the
     * visualizer, so a clean front stays clean. */
    CHORUS_LED_REAR_STATUS
} chorus_led_kind_t;

/* What a class carries. */
typedef struct {
    chorus_speaker_class_t speaker_class;
    /* Bit (1u << input) for every input the class has. */
    uint32_t inputs;
    chorus_led_kind_t led;
    int has_microphone;
    int led_follows_visualizer;
} chorus_controls_profile_t;

/* The class's profile; never NULL for a defined class. */
const chorus_controls_profile_t *chorus_controls_profile(chorus_speaker_class_t speaker_class);

typedef enum {
    /* `command` is a controller_command for the server. */
    CHORUS_ACTION_COMMAND = 1,
    /* The pairing button. Adoption is trust-on-first-use (K92) and has no
     * protocol message in the v2 catalog; goal 14 (adoption and provisioning)
     * decides what a press asks for. Until then it is a local event: the LED
     * shows pairing and the console reports it. */
    CHORUS_ACTION_PAIRING,
    /* The microphone mute switch moved; `value` is 1 muted, 0 live. */
    CHORUS_ACTION_MIC_MUTE,
    /* A local sound setting (K69, alongside the app's): `value` is the sub
     * level in tenths of a dB (a cut, -120 to 0), or its phase in degrees. */
    CHORUS_ACTION_SUB_LEVEL,
    CHORUS_ACTION_SUB_PHASE,
    /* A long press asked to join a group, but no join target is configured.
     * Nothing is sent; the reason is surfaced instead of guessed. */
    CHORUS_ACTION_NO_JOIN_TARGET
} chorus_control_action_kind_t;

typedef struct {
    chorus_control_action_kind_t kind;
    chorus_control_input_t input;
    /* The monotonic time the action was decided at. */
    uint64_t at_ns;
    int32_t value;
    /* For CHORUS_ACTION_COMMAND. `target` points into `target_text`. */
    chorus_v2_controller_command_t command;
    char target_text[CHORUS_V2_MAX_SHORT_TEXT + 1];
} chorus_control_action_t;

#define CHORUS_CONTROLS_MAX_ACTIONS 8u

typedef struct {
    uint8_t raw;
    uint64_t raw_since_ns;
    uint8_t stable;
    int seen;
    uint64_t pressed_at_ns;
    int long_fired;
    uint64_t next_repeat_ns;
    /* Knobs: the last ADC code believed, and the step it maps to. */
    uint32_t knob_code;
    int32_t knob_step;
} chorus_control_input_state_t;

typedef struct {
    const chorus_controls_profile_t *profile;
    chorus_control_input_state_t inputs[CHORUS_INPUT_COUNT];
    /* What the server last said (controller_state), for join and leave. */
    char room[CHORUS_V2_MAX_SHORT_TEXT + 1];
    char group[CHORUS_V2_MAX_SHORT_TEXT + 1];
    char join_target[CHORUS_V2_MAX_SHORT_TEXT + 1];
    int state_known;
    /* The microphone gate: closed until the switch has been read. */
    int mic_live;
    int32_t sub_level_tenths_db;
    int32_t sub_phase_deg;
    /* An input the class does not have, reported by a binding: counted, and
     * never turned into an action. */
    uint32_t refused_inputs;
    /* Actions decided and not yet taken. */
    chorus_control_action_t pending[CHORUS_CONTROLS_MAX_ACTIONS];
    size_t pending_count;
    uint32_t dropped_actions;
} chorus_controls_t;

/* `room` is this endpoint's own room (leave returns it there); `join_target`
 * is the group a long press of play/pause joins, or "" for none. Returns -1
 * for an undefined class or a name that does not fit. */
int chorus_controls_init(chorus_controls_t *controls, chorus_speaker_class_t speaker_class,
                         const char *room, const char *join_target);

/* A button or switch level (1 pressed / muted, 0 released / live), sampled at
 * `now_ns`. Returns -1 when the class has no such input. */
int chorus_controls_level(chorus_controls_t *controls, chorus_control_input_t input, int level,
                          uint64_t now_ns);

/* A knob's ADC code, 0 to CHORUS_CONTROLS_KNOB_MAX. Returns -1 when the class
 * has no such knob or the code is out of range. */
int chorus_controls_knob(chorus_controls_t *controls, chorus_control_input_t input, uint32_t code,
                         uint64_t now_ns);

/* The server's controller_state, which is what join and leave are decided
 * against. */
void chorus_controls_state(chorus_controls_t *controls, const chorus_v2_controller_state_t *state);

/* Advance to `now_ns`: settle debounced levels, fire long presses and
 * repeats, and hand back up to `max` actions in the order they were decided.
 * Returns the count. */
size_t chorus_controls_poll(chorus_controls_t *controls, uint64_t now_ns,
                            chorus_control_action_t *out, size_t max);

/* Whether microphone audio may leave the endpoint now. */
int chorus_controls_mic_live(const chorus_controls_t *controls);

/* The only way microphone samples reach the voice path: copies `count`
 * samples to `out` and returns `count` when the gate is open, and returns 0
 * and copies nothing when it is closed. A class without a microphone is
 * always closed. */
size_t chorus_controls_mic_pass(const chorus_controls_t *controls, const int16_t *in, size_t count,
                                int16_t *out);

/* Encode a COMMAND action as one v2 frame. Returns the encoder's status. */
chorus_encode_status_t chorus_controls_encode(const chorus_control_action_t *action, uint8_t *out,
                                              size_t out_len, size_t *written);

/* --- the status LED --------------------------------------------------------- */

typedef enum {
    CHORUS_LED_BOOT = 0,
    CHORUS_LED_LINK_DOWN,
    CHORUS_LED_IDLE,
    CHORUS_LED_PLAYING,
    CHORUS_LED_MUTED,
    CHORUS_LED_PAIRING,
    CHORUS_LED_FAULT
} chorus_led_state_t;

const char *chorus_led_state_name(chorus_led_state_t state);

/* What the endpoint knows, from which the LED's state is decided. */
typedef struct {
    int booted;
    int link_up;
    int adopted;
    int playing;
    int mic_muted;
    int pairing;
    int fault;
} chorus_led_inputs_t;

/* The state those inputs mean: a fault first, then pairing, then a muted
 * microphone, then the link, then boot, then playing, else idle. */
chorus_led_state_t chorus_led_decide(const chorus_led_inputs_t *inputs);

typedef struct {
    uint8_t red;
    uint8_t green;
    uint8_t blue;
    uint8_t brightness;
} chorus_led_output_t;

typedef struct {
    uint64_t timestamp_ns;
    uint8_t is_color;
    uint8_t beat;
    uint8_t peak;
    uint8_t red;
    uint8_t green;
    uint8_t blue;
    uint8_t brightness;
} chorus_led_event_t;

typedef struct {
    chorus_led_kind_t kind;
    int follows_visualizer;
    chorus_led_event_t queue[CHORUS_LED_QUEUE];
    size_t queued;
    uint32_t dropped;
    /* The colour the visualizer paints with (the last `color` message heard),
     * and the last frame heard. */
    chorus_led_output_t base;
    int have_frame;
    uint64_t frame_at_ns;
    uint8_t beat;
    uint8_t peak;
} chorus_led_t;

void chorus_led_init(chorus_led_t *led, const chorus_controls_profile_t *profile);

/* Queue a visualizer_frame or color message (other types return -1). Frames
 * are held until they are heard: `timestamp_ns` is on the server timeline,
 * the one the playout path schedules audio on. */
int chorus_led_offer(chorus_led_t *led, const chorus_v2_message_t *message);

/* What the LED shows at `server_now_ns` in `state`. While PLAYING, a LED that
 * follows the visualizer takes the last colour heard, scaled by the last
 * frame's peak, and a beat at or above CHORUS_LED_BEAT_THRESHOLD flashes it to
 * that colour's full brightness; with no frame heard for
 * CHORUS_LED_VISUALIZER_STALE_NS it shows the steady playing colour. Every
 * other state has one fixed colour (ASSUMED palette, docs/hardware/controls.md). */
chorus_led_output_t chorus_led_render(chorus_led_t *led, chorus_led_state_t state,
                                      uint64_t server_now_ns);

#endif /* CHORUS_CONTROLS_H */
