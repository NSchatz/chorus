/* The endpoint's controls per class, its status LED and its microphone gate
 * (brief section 13 item 2; K65, K67-K70; docs/decisions/0063-*).
 *
 * Each class is driven by a timed input script on a fake monotonic clock, and
 * the controller_command frames its controls produce are held byte for byte
 * to the committed fixtures/controls/<class>.hex, which the server's
 * crates/server/tests/controller_role.rs decodes and applies to a room: that
 * pair is the controller role end to end. The LED is fed
 * fixtures/controls/visualizer-sequence.hex and held, moment by moment, to
 * visualizer-sequence.led. The session's voice path (chorus/session.h) is fed
 * by a fake capture source and held to the voice role's own vectors in
 * fixtures/protocol/v2. No pin, no clock and no network. */

#include <stdint.h>
#include <string.h>

#include "chorus/controls.h"
#include "chorus/protocol_v2.h"
#include "chorus/session.h"
#include "fixture_text.h"
#include "harness.h"

#define MS 1000000ull
#define FRAMES_CAP 4096u
#define TEXT_CAP 16384u

/* The frames the controls produced, concatenated, and every action seen. */
typedef struct {
    uint8_t bytes[FRAMES_CAP];
    size_t len;
    chorus_control_action_t actions[64];
    size_t action_count;
} sink_t;

static void drain(chorus_controls_t *c, uint64_t now, sink_t *sink)
{
    chorus_control_action_t out[CHORUS_CONTROLS_MAX_ACTIONS];
    size_t n = chorus_controls_poll(c, now, out, CHORUS_CONTROLS_MAX_ACTIONS);
    for (size_t i = 0; i < n; i++) {
        if (sink->action_count < sizeof(sink->actions) / sizeof(sink->actions[0])) {
            sink->actions[sink->action_count++] = out[i];
        }
        if (out[i].kind != CHORUS_ACTION_COMMAND) {
            continue;
        }
        size_t written = 0;
        chorus_encode_status_t st = chorus_controls_encode(&out[i], sink->bytes + sink->len,
                                                           FRAMES_CAP - sink->len, &written);
        if (st != CHORUS_ENCODE_OK) {
            chorus_check(0, "a controller_command action encodes (status %s)",
                         chorus_encode_status_name(st));
            continue;
        }
        sink->len += written;
    }
}

/* Run the clock from `from` to `to` in 1 ms polls, draining actions. */
static void run(chorus_controls_t *c, uint64_t from, uint64_t to, sink_t *sink)
{
    for (uint64_t t = from; t <= to; t += MS) {
        drain(c, t, sink);
    }
}

/* A clean press of `ms` milliseconds at `at`, then 100 ms of quiet. Returns
 * the time after. */
static uint64_t press(chorus_controls_t *c, chorus_control_input_t input, uint64_t at, uint64_t ms,
                      sink_t *sink)
{
    chorus_controls_level(c, input, 1, at);
    run(c, at, at + ms * MS - MS, sink);
    chorus_controls_level(c, input, 0, at + ms * MS);
    run(c, at + ms * MS, at + ms * MS + 100 * MS, sink);
    return at + ms * MS + 101 * MS;
}

static long load_hex(const char *relative, uint8_t *out, size_t cap)
{
    static char text[TEXT_CAP];
    char path[512];
    chorus_repo_path(path, sizeof(path), relative);
    if (fixture_read(path, text, sizeof(text)) < 0) {
        return -1;
    }
    return fixture_parse_hex(text, out, cap);
}

/* Every frame in the produced bytes decodes as a controller_command. */
static void decode_back(const sink_t *sink, const char *who)
{
    size_t at = 0;
    int frames = 0;
    int all_ok = 1;
    while (at < sink->len) {
        chorus_v2_frame_t f = chorus_v2_decode_frame(sink->bytes + at, sink->len - at);
        if (f.outcome != CHORUS_FRAME_DECODED || f.message_type != CHORUS_V2_CONTROLLER_COMMAND ||
            f.consumed == 0) {
            all_ok = 0;
            break;
        }
        at += f.consumed;
        frames++;
    }
    chorus_check(all_ok, "%s: all %d produced frames decode back as controller_command", who,
                 frames);
}

static void compare_fixture(const sink_t *sink, const char *fixture, const char *who)
{
    static uint8_t expected[FRAMES_CAP];
    long n = load_hex(fixture, expected, sizeof(expected));
    chorus_check(n > 0, "%s: %s is readable (%ld bytes)", who, fixture, n);
    chorus_check(n == (long)sink->len && memcmp(expected, sink->bytes, sink->len) == 0,
                 "%s: the controls produced exactly %s (%zu bytes produced, %ld committed)", who,
                 fixture, sink->len, n);
    decode_back(sink, who);
}

static size_t count_kind(const sink_t *sink, chorus_control_action_kind_t kind)
{
    size_t n = 0;
    for (size_t i = 0; i < sink->action_count; i++) {
        n += sink->actions[i].kind == kind;
    }
    return n;
}

/* --- the command values are the catalog's ---------------------------------- */

static void the_command_values_are_the_catalogs(void)
{
    chorus_section("the command bytes the controls send are the catalog's names");
    const struct {
        uint8_t value;
        const char *name;
    } want[] = {{3, "toggle"},      {4, "next"}, {5, "previous"},
                {7, "volume_step"}, {9, "join"}, {10, "leave"}};
    for (size_t i = 0; i < sizeof(want) / sizeof(want[0]); i++) {
        const char *name = chorus_v2_enum_name(CHORUS_V2_ENUM_COMMAND, want[i].value);
        chorus_check(name != NULL && strcmp(name, want[i].name) == 0,
                     "command %u is `%s` in the catalog (it says `%s`)", want[i].value,
                     want[i].name, name ? name : "(none)");
    }
}

/* --- the profiles, K67-K70 ---------------------------------------------------- */

static void the_profiles_are_the_owners_decisions(void)
{
    chorus_section("per class, the controls and light K67-K70 give it");
    const chorus_controls_profile_t *compact = chorus_controls_profile(CHORUS_CLASS_COMPACT);
    const chorus_controls_profile_t *two_way = chorus_controls_profile(CHORUS_CLASS_TWO_WAY);
    const chorus_controls_profile_t *sub = chorus_controls_profile(CHORUS_CLASS_SUBWOOFER);
    const chorus_controls_profile_t *amp = chorus_controls_profile(CHORUS_CLASS_STREAMING_AMP);
    uint32_t buttons = (1u << CHORUS_INPUT_PLAY_PAUSE) | (1u << CHORUS_INPUT_VOLUME_UP) |
                       (1u << CHORUS_INPUT_VOLUME_DOWN) | (1u << CHORUS_INPUT_NEXT) |
                       (1u << CHORUS_INPUT_PREVIOUS);
    chorus_check(compact->inputs == (buttons | (1u << CHORUS_INPUT_MIC_MUTE_SWITCH)) &&
                     compact->led == CHORUS_LED_STATUS && compact->has_microphone,
                 "K67 compact: buttons or touch, a status LED, a mic behind a mute switch");
    chorus_check(two_way->inputs == (1u << CHORUS_INPUT_PAIRING) &&
                     two_way->led == CHORUS_LED_REAR_STATUS && !two_way->has_microphone &&
                     !two_way->led_follows_visualizer,
                 "K68 two-way: a hidden pairing button and a rear status light only");
    chorus_check(sub->inputs ==
                         ((1u << CHORUS_INPUT_PAIRING) | (1u << CHORUS_INPUT_SUB_LEVEL_KNOB) |
                          (1u << CHORUS_INPUT_SUB_PHASE_KNOB)) &&
                     sub->led == CHORUS_LED_STATUS && !sub->has_microphone,
                 "K69 subwoofer: a pairing button, a status LED, level and phase knobs");
    chorus_check(amp->inputs == (buttons | (1u << CHORUS_INPUT_PAIRING)) &&
                     amp->led == CHORUS_LED_STATUS && !amp->has_microphone,
                 "K70 streaming amp: a pairing button and LED, front buttons");
    chorus_speaker_class_t parsed;
    chorus_check(chorus_speaker_class_from_name("two-way", &parsed) == 0 &&
                     parsed == CHORUS_CLASS_TWO_WAY &&
                     chorus_speaker_class_from_name("soundbar", &parsed) == -1,
                 "class names read back, and an undefined class is refused");
}

/* --- compact ----------------------------------------------------------------- */

static void the_compact_class(void)
{
    chorus_section("compact (K67): each control as the controller role");
    static chorus_controls_t c;
    static sink_t sink;
    memset(&sink, 0, sizeof(sink));
    chorus_check(chorus_controls_init(&c, CHORUS_CLASS_COMPACT, "kitchen", "downstairs") == 0,
                 "the compact class initialises");
    uint64_t t = 1000 * MS;

    /* Contact chatter: five edges 3 ms apart, then held down 80 ms. */
    for (int i = 0; i < 5; i++) {
        chorus_controls_level(&c, CHORUS_INPUT_PLAY_PAUSE, (i % 2) == 0, t + (uint64_t)i * 3 * MS);
        drain(&c, t + (uint64_t)i * 3 * MS, &sink);
    }
    chorus_controls_level(&c, CHORUS_INPUT_PLAY_PAUSE, 1, t + 15 * MS);
    run(&c, t + 15 * MS, t + 95 * MS, &sink);
    chorus_controls_level(&c, CHORUS_INPUT_PLAY_PAUSE, 0, t + 95 * MS);
    run(&c, t + 95 * MS, t + 200 * MS, &sink);
    chorus_check(sink.action_count == 1 && sink.actions[0].command.command == 3,
                 "a bouncing play/pause press is ONE toggle (%zu actions)", sink.action_count);
    chorus_check(sink.action_count == 1 && sink.actions[0].at_ns == t + 115 * MS,
                 "a short press toggles on release, dated when the release settled, 20 ms "
                 "after its edge at +95 ms (at +%llu ms)",
                 sink.action_count ? (unsigned long long)((sink.actions[0].at_ns - t) / MS) : 0ull);
    t += 300 * MS;

    /* A press shorter than the debounce is noise. */
    chorus_controls_level(&c, CHORUS_INPUT_NEXT, 1, t);
    run(&c, t, t + 10 * MS, &sink);
    chorus_controls_level(&c, CHORUS_INPUT_NEXT, 0, t + 11 * MS);
    run(&c, t + 11 * MS, t + 100 * MS, &sink);
    chorus_check(sink.action_count == 1, "an 11 ms blip on next sends nothing");
    t += 200 * MS;

    t = press(&c, CHORUS_INPUT_VOLUME_UP, t, 100, &sink);
    size_t before = sink.action_count;
    t = press(&c, CHORUS_INPUT_VOLUME_DOWN, t, 1000, &sink);
    chorus_check(sink.action_count - before == 3,
                 "volume down held 1.0 s: one step and two repeats (%zu)",
                 sink.action_count - before);
    t = press(&c, CHORUS_INPUT_NEXT, t, 100, &sink);
    t = press(&c, CHORUS_INPUT_PREVIOUS, t, 100, &sink);

    /* Long press while alone: join the configured target. */
    before = sink.action_count;
    t = press(&c, CHORUS_INPUT_PLAY_PAUSE, t, 1500, &sink);
    chorus_check(
        sink.action_count - before == 1 && sink.actions[before].command.command == 9 &&
            strcmp(sink.actions[before].target_text, "downstairs") == 0,
        "a long press while the room plays alone joins \"downstairs\" and does not toggle");

    /* The server says the room now plays in "downstairs"; long press leaves. */
    chorus_v2_controller_state_t state = {40, 0, 1, {(const uint8_t *)"downstairs", 10}};
    chorus_controls_state(&c, &state);
    t = press(&c, CHORUS_INPUT_PLAY_PAUSE, t, 1500, &sink);
    chorus_check(sink.actions[sink.action_count - 1].command.command == 10,
                 "a long press while grouped leaves");

    chorus_check(chorus_controls_level(&c, CHORUS_INPUT_PAIRING, 1, t) == -1 &&
                     chorus_controls_knob(&c, CHORUS_INPUT_SUB_LEVEL_KNOB, 100, t) == -1 &&
                     c.refused_inputs == 2,
                 "an input the compact class does not have is refused and counted");
    compare_fixture(&sink, "fixtures/controls/compact.hex", "compact");
}

/* --- the microphone gate ----------------------------------------------------- */

static void the_mute_switch_cuts_the_microphone(void)
{
    chorus_section("compact (K67): the hardware mute switch cuts the microphone in firmware");
    static chorus_controls_t c;
    static sink_t sink;
    memset(&sink, 0, sizeof(sink));
    chorus_controls_init(&c, CHORUS_CLASS_COMPACT, "kitchen", "");
    int16_t in[4] = {100, -200, 300, -400};
    int16_t out[4] = {0, 0, 0, 0};

    chorus_check(chorus_controls_mic_pass(&c, in, 4, out) == 0 && out[0] == 0,
                 "before the switch has been read, no microphone sample passes");
    chorus_controls_level(&c, CHORUS_INPUT_MIC_MUTE_SWITCH, 0, 0);
    run(&c, 0, 30 * MS, &sink);
    chorus_check(chorus_controls_mic_pass(&c, in, 4, out) == 4 && out[3] == -400,
                 "the switch read live: samples pass unchanged");
    chorus_controls_level(&c, CHORUS_INPUT_MIC_MUTE_SWITCH, 1, 40 * MS);
    run(&c, 40 * MS, 50 * MS, &sink);
    chorus_check(chorus_controls_mic_live(&c) == 1,
                 "a muting edge not yet debounced (10 ms) has not closed the gate");
    run(&c, 51 * MS, 70 * MS, &sink);
    memset(out, 0, sizeof(out));
    chorus_check(!chorus_controls_mic_live(&c) && chorus_controls_mic_pass(&c, in, 4, out) == 0 &&
                     out[0] == 0,
                 "muted: no microphone sample passes, and nothing is copied");
    chorus_check(count_kind(&sink, CHORUS_ACTION_MIC_MUTE) == 2 &&
                     sink.actions[sink.action_count - 1].value == 1,
                 "the switch reports live then muted as local events");
    chorus_check(sink.len == 0, "the mute switch sends no controller_command");

    static chorus_controls_t sub;
    chorus_controls_init(&sub, CHORUS_CLASS_SUBWOOFER, "den", "");
    chorus_check(chorus_controls_mic_pass(&sub, in, 4, out) == 0 &&
                     chorus_controls_level(&sub, CHORUS_INPUT_MIC_MUTE_SWITCH, 0, 0) == -1,
                 "a class with no microphone passes nothing and has no switch");

    chorus_led_inputs_t li = {1, 1, 1, 1, 1, 0, 0, 0};
    chorus_check(chorus_led_decide(&li) == CHORUS_LED_MUTED,
                 "while muted the LED shows mute, even while playing");
}

/* --- two-way ----------------------------------------------------------------- */

static void the_two_way_class(void)
{
    chorus_section("two-way (K68): a hidden pairing button, nothing on the front");
    static chorus_controls_t c;
    static sink_t sink;
    memset(&sink, 0, sizeof(sink));
    chorus_controls_init(&c, CHORUS_CLASS_TWO_WAY, "living", "");
    uint64_t t = press(&c, CHORUS_INPUT_PAIRING, 0, 100, &sink);
    chorus_check(count_kind(&sink, CHORUS_ACTION_PAIRING) == 1 && sink.len == 0,
                 "pairing is a local event (adoption is goal 14's); no frame is sent");
    chorus_check(chorus_controls_level(&c, CHORUS_INPUT_PLAY_PAUSE, 1, t) == -1 &&
                     chorus_controls_level(&c, CHORUS_INPUT_VOLUME_UP, 1, t) == -1,
                 "play/pause and volume do not exist on a two-way");
}

/* --- subwoofer ----------------------------------------------------------------- */

static void the_subwoofer_class(void)
{
    chorus_section("subwoofer (K69): pairing, and level and phase as local settings");
    static chorus_controls_t c;
    static sink_t sink;
    memset(&sink, 0, sizeof(sink));
    chorus_controls_init(&c, CHORUS_CLASS_SUBWOOFER, "den", "");
    press(&c, CHORUS_INPUT_PAIRING, 0, 100, &sink);
    chorus_check(count_kind(&sink, CHORUS_ACTION_PAIRING) == 1,
                 "the pairing button is a local event");

    chorus_controls_knob(&c, CHORUS_INPUT_SUB_LEVEL_KNOB, 0, 0);
    drain(&c, 0, &sink);
    chorus_check(c.sub_level_tenths_db == -120, "level knob fully down: -12.0 dB (%d tenths)",
                 (int)c.sub_level_tenths_db);
    chorus_controls_knob(&c, CHORUS_INPUT_SUB_LEVEL_KNOB, CHORUS_CONTROLS_KNOB_MAX, MS);
    drain(&c, MS, &sink);
    chorus_check(c.sub_level_tenths_db == 0,
                 "level knob fully up: 0.0 dB, a cut never a boost (K81, I10) (%d tenths)",
                 (int)c.sub_level_tenths_db);
    int32_t max_seen = INT32_MIN;
    for (uint32_t code = 0; code <= CHORUS_CONTROLS_KNOB_MAX; code += 7) {
        chorus_controls_knob(&c, CHORUS_INPUT_SUB_LEVEL_KNOB, code, 2 * MS);
        if (c.sub_level_tenths_db > max_seen) {
            max_seen = c.sub_level_tenths_db;
        }
    }
    drain(&c, 2 * MS, &sink);
    chorus_check(max_seen <= 0, "no knob position sets a level above 0 dB (max %d tenths)",
                 (int)max_seen);

    /* Noise at a step edge: +/- 10 codes around the edge between two steps. */
    chorus_controls_knob(&c, CHORUS_INPUT_SUB_PHASE_KNOB, 2000, 3 * MS);
    drain(&c, 3 * MS, &sink);
    int32_t settled = c.sub_phase_deg;
    size_t before = count_kind(&sink, CHORUS_ACTION_SUB_PHASE);
    uint32_t edge = (uint32_t)(((uint64_t)(settled / 15 + 1) * 4096u) / 13u);
    for (int i = 0; i < 20; i++) {
        chorus_controls_knob(&c, CHORUS_INPUT_SUB_PHASE_KNOB, edge + ((i % 2) ? 10u : 0u) - 5u,
                             (4 + (uint64_t)i) * MS);
    }
    drain(&c, 30 * MS, &sink);
    chorus_check(count_kind(&sink, CHORUS_ACTION_SUB_PHASE) == before && c.sub_phase_deg == settled,
                 "ADC noise at a step edge does not chatter the phase (stays %d deg)",
                 (int)settled);
    chorus_controls_knob(&c, CHORUS_INPUT_SUB_PHASE_KNOB, CHORUS_CONTROLS_KNOB_MAX, 40 * MS);
    drain(&c, 40 * MS, &sink);
    chorus_check(c.sub_phase_deg == 180, "phase knob fully up: 180 degrees (%d)",
                 (int)c.sub_phase_deg);
    chorus_check(chorus_controls_knob(&c, CHORUS_INPUT_SUB_PHASE_KNOB, 4096, 41 * MS) == -1,
                 "an ADC code past 12 bits is refused");
    chorus_check(sink.len == 0, "the knobs send no controller_command (local sound settings)");
}

/* --- streaming amp ----------------------------------------------------------- */

static void the_streaming_amp_class(void)
{
    chorus_section("streaming amp (K70): front buttons as the controller role, pairing");
    static chorus_controls_t c;
    static sink_t sink;
    memset(&sink, 0, sizeof(sink));
    chorus_controls_init(&c, CHORUS_CLASS_STREAMING_AMP, "rack", "");
    uint64_t t = 0;
    t = press(&c, CHORUS_INPUT_PLAY_PAUSE, t, 100, &sink);
    t = press(&c, CHORUS_INPUT_VOLUME_UP, t, 100, &sink);
    t = press(&c, CHORUS_INPUT_NEXT, t, 100, &sink);
    t = press(&c, CHORUS_INPUT_PREVIOUS, t, 100, &sink);
    t = press(&c, CHORUS_INPUT_PLAY_PAUSE, t, 1500, &sink);
    chorus_check(count_kind(&sink, CHORUS_ACTION_NO_JOIN_TARGET) == 1,
                 "a long press with no join target configured sends nothing and says why");
    press(&c, CHORUS_INPUT_PAIRING, t, 100, &sink);
    chorus_check(count_kind(&sink, CHORUS_ACTION_PAIRING) == 1,
                 "the pairing button is a local event");
    compare_fixture(&sink, "fixtures/controls/streaming-amp.hex", "streaming amp");
}

/* --- the LED follows the visualizer fixture --------------------------------- */

static void offer_sequence(chorus_led_t *led, int *frames)
{
    static uint8_t bytes[FRAMES_CAP];
    long n = load_hex("fixtures/controls/visualizer-sequence.hex", bytes, sizeof(bytes));
    chorus_check(n > 0, "fixtures/controls/visualizer-sequence.hex is readable (%ld bytes)", n);
    size_t at = 0;
    *frames = 0;
    while (n > 0 && at < (size_t)n) {
        chorus_v2_frame_t f = chorus_v2_decode_frame(bytes + at, (size_t)n - at);
        if (f.outcome != CHORUS_FRAME_DECODED || f.consumed == 0) {
            chorus_check(0, "every frame of the sequence decodes");
            return;
        }
        chorus_check(chorus_led_offer(led, &f.message) == 0, "the LED takes %s at %zu",
                     chorus_v2_type_name(f.message_type), at);
        at += f.consumed;
        (*frames)++;
    }
}

static void the_led_follows_the_visualizer_fixture(void)
{
    chorus_section("the status LED follows fixtures/controls/visualizer-sequence.*");
    static chorus_led_t led;
    chorus_led_init(&led, chorus_controls_profile(CHORUS_CLASS_COMPACT));
    int frames = 0;
    offer_sequence(&led, &frames);
    chorus_check(frames == 7, "seven frames offered (%d)", frames);

    static char text[TEXT_CAP];
    char path[512];
    chorus_repo_path(path, sizeof(path), "fixtures/controls/visualizer-sequence.led");
    chorus_check(fixture_read(path, text, sizeof(text)) > 0, "visualizer-sequence.led is readable");
    int moments = 0;
    const char *line = text;
    while (*line != '\0') {
        const char *eol = strchr(line, '\n');
        size_t len = eol ? (size_t)(eol - line) : strlen(line);
        char buf[256];
        if (len > 0 && line[0] != '#' && len < sizeof(buf)) {
            memcpy(buf, line, len);
            buf[len] = '\0';
            unsigned long long at = 0;
            char state_name[32];
            unsigned r, g, b, br;
            if (sscanf(buf, "%llu %31s %u %u %u %u", &at, state_name, &r, &g, &b, &br) == 6) {
                chorus_led_state_t state = CHORUS_LED_IDLE;
                for (int s = CHORUS_LED_BOOT; s <= CHORUS_LED_FAULT; s++) {
                    if (strcmp(chorus_led_state_name((chorus_led_state_t)s), state_name) == 0) {
                        state = (chorus_led_state_t)s;
                    }
                }
                chorus_led_output_t o = chorus_led_render(&led, state, at);
                chorus_check(o.red == r && o.green == g && o.blue == b && o.brightness == br,
                             "at %llu ns (%s): shows %u %u %u @%u, the fixture says %u %u %u @%u",
                             at, state_name, o.red, o.green, o.blue, o.brightness, r, g, b, br);
                moments++;
            } else {
                chorus_check(0, "a .led line parses: %s", buf);
            }
        }
        line = eol ? eol + 1 : line + len;
    }
    chorus_check(moments == 10, "ten moments rendered (%d)", moments);

    /* The two-way's rear light never follows the visualizer (K68). */
    static chorus_led_t rear;
    chorus_led_init(&rear, chorus_controls_profile(CHORUS_CLASS_TWO_WAY));
    offer_sequence(&rear, &frames);
    chorus_led_output_t steady = chorus_led_render(&rear, CHORUS_LED_PLAYING, 999999999ull);
    int same = 1;
    for (uint64_t at = 1000000000ull; at <= 1080000000ull; at += 10000000ull) {
        chorus_led_output_t o = chorus_led_render(&rear, CHORUS_LED_PLAYING, at);
        same &= memcmp(&o, &steady, sizeof(o)) == 0;
    }
    chorus_check(same, "the two-way's rear status light stays steady through the sequence");

    chorus_led_inputs_t order[] = {{1, 1, 1, 1, 1, 1, 1, 1}, {1, 1, 1, 1, 1, 1, 0, 1},
                                   {1, 1, 1, 1, 1, 0, 0, 1}, {0, 0, 0, 0, 0, 0, 0, 1},
                                   {1, 0, 1, 1, 0, 0, 0, 1}, {1, 1, 1, 1, 0, 0, 0, 1},
                                   {1, 1, 1, 1, 0, 0, 0, 0}, {1, 1, 0, 1, 0, 0, 0, 0}};
    chorus_led_state_t want[] = {CHORUS_LED_FAULT,   CHORUS_LED_PAIRING,   CHORUS_LED_MUTED,
                                 CHORUS_LED_BOOT,    CHORUS_LED_LINK_DOWN, CHORUS_LED_LISTENING,
                                 CHORUS_LED_PLAYING, CHORUS_LED_IDLE};
    int ok = 1;
    for (size_t i = 0; i < sizeof(want) / sizeof(want[0]); i++) {
        ok &= chorus_led_decide(&order[i]) == want[i];
    }
    chorus_check(ok, "state priority: fault, pairing, muted, boot, link down, listening, playing, "
                     "idle (not adopted is idle)");
}

/* --- the voice path: mic audio through the gate, and the gate report --------- */

/* A fake capture source with the session seam's signature
 * (chorus_session_config_t.mic_capture): a deterministic signal, `chunk`
 * samples per ask, stamped on a fake monotonic clock that advances by the
 * samples handed out. Everything it ever produced is kept, so a test can hold
 * what left the endpoint to what was captured. */
#define MIC_KEPT 32768u
typedef struct {
    uint32_t seed;
    size_t chunk;
    uint64_t now_ns;
    int16_t kept[MIC_KEPT];
    size_t kept_count;
} fake_mic_t;

static size_t fake_mic_capture(void *ctx, int16_t *samples, size_t max, uint64_t *captured_ns)
{
    fake_mic_t *mic = ctx;
    size_t n = (mic->chunk < max) ? mic->chunk : max;
    if (mic->kept_count + n > MIC_KEPT) {
        return 0;
    }
    for (size_t i = 0; i < n; i++) {
        mic->seed = mic->seed * 1664525u + 1013904223u;
        samples[i] = (int16_t)(mic->seed >> 16);
        mic->kept[mic->kept_count++] = samples[i];
    }
    *captured_ns = mic->now_ns;
    mic->now_ns += (uint64_t)n * 1000000000ull / CHORUS_V2_MIC_SAMPLE_RATE_HZ;
    return n;
}

/* What the voice path produced: the frames, and each decoded message kind. */
#define VOICE_CAP 65536u
typedef struct {
    uint8_t bytes[VOICE_CAP];
    size_t len;
    size_t audio_frames;
    size_t state_frames;
    size_t other_frames;
} voice_sink_t;

static void voice_count(voice_sink_t *sink)
{
    sink->audio_frames = sink->state_frames = sink->other_frames = 0;
    size_t at = 0;
    while (at < sink->len) {
        chorus_v2_frame_t f = chorus_v2_decode_frame(sink->bytes + at, sink->len - at);
        if (f.consumed == 0) {
            sink->other_frames++;
            break;
        }
        at += f.consumed;
        if (f.outcome == CHORUS_FRAME_DECODED && f.message_type == CHORUS_V2_MIC_AUDIO) {
            sink->audio_frames++;
        } else if (f.outcome == CHORUS_FRAME_DECODED && f.message_type == CHORUS_V2_MIC_STATE) {
            sink->state_frames++;
        } else {
            sink->other_frames++;
        }
    }
}

/* One pass, in the session's order (firmware/src/session.c, pump_voice): the
 * gate report, then `asks` asks of the capture source. */
static void voice_pump(chorus_session_voice_t *voice, fake_mic_t *mic, int asks, int offset_known,
                       int64_t offset_ns, voice_sink_t *sink)
{
    sink->len += chorus_session_voice_report(voice, sink->bytes + sink->len, VOICE_CAP - sink->len);
    static int16_t captured[CHORUS_SESSION_VOICE_MAX_CAPTURE];
    for (int i = 0; i < asks; i++) {
        uint64_t captured_ns = 0;
        size_t got =
            fake_mic_capture(mic, captured, CHORUS_SESSION_VOICE_MAX_CAPTURE, &captured_ns);
        sink->len +=
            chorus_session_voice_capture(voice, captured, got, captured_ns, offset_known, offset_ns,
                                         sink->bytes + sink->len, VOICE_CAP - sink->len);
    }
    voice_count(sink);
}

/* Every mic_audio in the sink, decoded: the samples equal `want` in order,
 * sequences count from `first_sequence`, and each timestamp is the capture
 * instant of its first sample through the offset. */
static int voice_decodes_to(const voice_sink_t *sink, const int16_t *want, size_t want_count,
                            uint32_t first_sequence, uint64_t first_captured_ns, int64_t offset_ns)
{
    size_t at = 0;
    size_t samples = 0;
    uint32_t sequence = first_sequence;
    int ok = 1;
    while (at < sink->len) {
        chorus_v2_frame_t f = chorus_v2_decode_frame(sink->bytes + at, sink->len - at);
        if (f.consumed == 0) {
            return 0;
        }
        at += f.consumed;
        if (f.outcome != CHORUS_FRAME_DECODED || f.message_type != CHORUS_V2_MIC_AUDIO) {
            continue;
        }
        const chorus_v2_mic_audio_t *a = &f.message.as.mic_audio;
        size_t n = a->data.len / 2;
        uint64_t local_ns =
            first_captured_ns + (uint64_t)samples * 1000000000ull / CHORUS_V2_MIC_SAMPLE_RATE_HZ;
        ok &= a->format == CHORUS_V2_MIC_FORMAT_PCM_S16LE_16K_MONO && a->sequence == sequence &&
              a->timestamp_ns == (uint64_t)((int64_t)local_ns + offset_ns) &&
              a->data.len % 2 == 0 && n >= 1 && n <= CHORUS_V2_MIC_MAX_SAMPLES &&
              samples + n <= want_count;
        if (!ok) {
            return 0;
        }
        for (size_t i = 0; i < n; i++) {
            int16_t got =
                (int16_t)(uint16_t)(a->data.data[2 * i] | ((uint16_t)a->data.data[2 * i + 1] << 8));
            ok &= got == want[samples + i];
        }
        samples += n;
        sequence++;
    }
    return ok && samples == want_count;
}

static void set_switch(chorus_controls_t *c, int muted, uint64_t at, sink_t *sink)
{
    chorus_controls_level(c, CHORUS_INPUT_MIC_MUTE_SWITCH, muted, at);
    run(c, at, at + 30 * MS, sink);
}

static void the_voice_path_sends_mic_audio_only_through_the_gate(void)
{
    chorus_section("voice: mic audio leaves only with the gate live and the uplink requested");
    static chorus_controls_t c;
    static sink_t actions;
    static chorus_session_voice_t voice;
    static fake_mic_t mic;
    static voice_sink_t out;
    static uint8_t expected[64];
    memset(&actions, 0, sizeof(actions));
    memset(&mic, 0, sizeof(mic));
    memset(&out, 0, sizeof(out));
    mic.seed = 20261004u;
    mic.chunk = 320;
    mic.now_ns = 5000000000ull;
    const int64_t offset = -1234567890ll;
    const chorus_v2_voice_control_t on = {1, 0};
    const chorus_v2_voice_control_t off = {0, 0};
    const chorus_v2_voice_control_t listening_only = {0, 1};

    chorus_controls_init(&c, CHORUS_CLASS_COMPACT, "kitchen", "");
    chorus_session_voice_init(&voice, &c);
    chorus_session_voice_begin(&voice);
    chorus_check(chorus_session_voice_roles(&voice) == CHORUS_V2_ROLE_VOICE &&
                     chorus_session_voice_roles(NULL) == 0,
                 "a compact speaker declares the voice role; no voice path declares none");

    /* The gate closed (the switch not read yet), the uplink requested, the
     * offset known: only the gate stands in the way. */
    chorus_session_voice_control(&voice, &on);
    voice_pump(&voice, &mic, 1, 1, offset, &out);
    long n = load_hex("fixtures/protocol/v2/mic_state_muted.hex", expected, sizeof(expected));
    chorus_check(n > 0 && out.len == (size_t)n && memcmp(out.bytes, expected, out.len) == 0 &&
                     out.audio_frames == 0,
                 "a session's first report, the switch unread: exactly mic_state_muted.hex, and "
                 "no mic_audio (%zu bytes)",
                 out.len);
    out.len = 0;
    static int16_t shapes[CHORUS_SESSION_VOICE_MAX_CAPTURE];
    const size_t sizes[] = {1, 2, 319, 320, 1600, 1601, 4000, CHORUS_SESSION_VOICE_MAX_CAPTURE};
    size_t produced = 0;
    size_t tried = 0;
    for (int shape = 0; shape < 5; shape++) {
        for (size_t i = 0; i < CHORUS_SESSION_VOICE_MAX_CAPTURE; i++) {
            shapes[i] = shape == 0   ? 0
                        : shape == 1 ? INT16_MAX
                        : shape == 2 ? INT16_MIN
                        : shape == 3 ? (int16_t)(i * 37u)
                                     : (int16_t)((i & 1u) ? -12345 : 12345);
        }
        for (size_t k = 0; k < sizeof(sizes) / sizeof(sizes[0]); k++) {
            produced += chorus_session_voice_capture(&voice, shapes, sizes[k], 7 * MS, 1, offset,
                                                     out.bytes, VOICE_CAP);
            tried++;
        }
    }
    voice_pump(&voice, &mic, 8, 1, offset, &out);
    chorus_check(produced == 0 && out.len == 0 && voice.chunks_sent == 0 && voice.samples_sent == 0,
                 "the gate closed (switch unread): zero mic messages for %zu inputs of five "
                 "signals and eight lengths, and eight asks of the capture source",
                 tried);

    /* The switch read live: the gate report, then the captured samples. */
    set_switch(&c, 0, 0, &actions);
    size_t first = mic.kept_count;
    uint64_t first_ns = mic.now_ns;
    voice_pump(&voice, &mic, 3, 1, offset, &out);
    n = load_hex("fixtures/protocol/v2/mic_state_live.hex", expected, sizeof(expected));
    chorus_check(n > 0 && out.len > (size_t)n && memcmp(out.bytes, expected, (size_t)n) == 0 &&
                     out.state_frames == 1,
                 "the switch going live produces the gate report first: mic_state_live.hex");
    chorus_check(out.audio_frames == 3 && out.other_frames == 0 &&
                     voice_decodes_to(&out, mic.kept + first, 960, 0, first_ns, offset),
                 "gate live and uplink requested: three 20 ms asks leave as three mic_audio that "
                 "decode to the captured samples, sequences 0 to 2, capture instants through the "
                 "offset");

    /* A capture longer than one message is split, never trimmed. */
    out.len = 0;
    mic.chunk = 4000;
    first = mic.kept_count;
    first_ns = mic.now_ns;
    voice_pump(&voice, &mic, 1, 1, offset, &out);
    chorus_check(out.audio_frames == 3 && out.state_frames == 0 &&
                     voice_decodes_to(&out, mic.kept + first, 4000, 3, first_ns, offset),
                 "4000 samples in one capture leave as 1600 + 1600 + 800, decoding to the "
                 "captured samples, sequences 3 to 5, each stamped at its own first sample");
    mic.chunk = 320;

    /* The uplink not requested: nothing, whatever the gate says. */
    out.len = 0;
    chorus_session_voice_control(&voice, &off);
    uint64_t sent_before = voice.chunks_sent;
    voice_pump(&voice, &mic, 4, 1, offset, &out);
    chorus_check(out.len == 0 && voice.chunks_sent == sent_before && chorus_controls_mic_live(&c),
                 "gate live, uplink not requested: nothing is sent");
    chorus_session_voice_control(&voice, &listening_only);
    voice_pump(&voice, &mic, 4, 1, offset, &out);
    chorus_check(out.len == 0 && chorus_session_voice_listening(&voice),
                 "listening without uplink (another speaker carries the run): nothing is sent");
    chorus_session_voice_control(&voice, &on);
    first = mic.kept_count;
    first_ns = mic.now_ns;
    voice_pump(&voice, &mic, 2, 1, offset, &out);
    chorus_check(out.audio_frames == 2 &&
                     voice_decodes_to(&out, mic.kept + first, 640, 0, first_ns, offset),
                 "the uplink requested again: audio resumes at sequence 0");

    /* No offset: a timestamp is never guessed. */
    out.len = 0;
    voice_pump(&voice, &mic, 2, 0, 0, &out);
    chorus_check(out.len == 0, "no sync offset yet: nothing is sent");

    /* The mute switch: the gate report, after the last audio, and no more. */
    voice_pump(&voice, &mic, 1, 1, offset, &out);
    size_t audio_bytes = out.len;
    chorus_controls_level(&c, CHORUS_INPUT_MIC_MUTE_SWITCH, 1, 100 * MS);
    run(&c, 100 * MS, 110 * MS, &actions);
    voice_pump(&voice, &mic, 1, 1, offset, &out);
    chorus_check(out.state_frames == 0 && out.audio_frames == 2,
                 "a muting edge not yet debounced: no report, and the audio still flows");
    audio_bytes = out.len;
    run(&c, 111 * MS, 130 * MS, &actions);
    voice_pump(&voice, &mic, 4, 1, offset, &out);
    n = load_hex("fixtures/protocol/v2/mic_state_muted.hex", expected, sizeof(expected));
    chorus_check(n > 0 && out.len == audio_bytes + (size_t)n &&
                     memcmp(out.bytes + audio_bytes, expected, (size_t)n) == 0,
                 "the mute switch produces the gate report, mic_state_muted.hex, after the last "
                 "mic_audio, and four more asks produce nothing");
    out.len = 0;
    produced = 0;
    for (size_t k = 0; k < sizeof(sizes) / sizeof(sizes[0]); k++) {
        produced += chorus_session_voice_capture(&voice, shapes, sizes[k], 9 * MS, 1, offset,
                                                 out.bytes, VOICE_CAP);
    }
    voice_pump(&voice, &mic, 4, 1, offset, &out);
    chorus_check(produced == 0 && out.len == 0,
                 "muted with the uplink still requested: zero mic messages, and no second report "
                 "of the same state");

    /* The gate, not the report, is what holds samples back: closed after the
     * server was told `live` and before it is told `muted`, nothing leaves. */
    set_switch(&c, 0, 140 * MS, &actions);
    voice_pump(&voice, &mic, 1, 1, offset, &out);
    chorus_check(out.audio_frames == 1 && voice.reported == CHORUS_V2_MIC_GATE_LIVE,
                 "live and reported live: audio flows");
    set_switch(&c, 1, 170 * MS, &actions);
    produced = 0;
    for (size_t k = 0; k < sizeof(sizes) / sizeof(sizes[0]); k++) {
        produced += chorus_session_voice_capture(&voice, shapes, sizes[k], 9 * MS, 1, offset,
                                                 out.bytes, VOICE_CAP);
    }
    chorus_check(produced == 0 && voice.reported == CHORUS_V2_MIC_GATE_LIVE,
                 "the switch at mute before the report has gone: the gate alone passes nothing, "
                 "for eight lengths");
    out.len = 0;
    voice_pump(&voice, &mic, 1, 1, offset, &out);
    chorus_check(out.state_frames == 1 && out.audio_frames == 0, "and then the muted report");
    out.len = 0;

    /* Live again: the server is told before a sample leaves. */
    set_switch(&c, 0, 200 * MS, &actions);
    static int16_t one[320];
    uint8_t early[1024];
    chorus_check(chorus_session_voice_capture(&voice, one, 320, 250 * MS, 1, offset, early,
                                              sizeof(early)) == 0,
                 "live again but not yet reported: no mic_audio precedes the live report");
    first = mic.kept_count;
    first_ns = mic.now_ns;
    voice_pump(&voice, &mic, 1, 1, offset, &out);
    {
        voice_sink_t *o = &out;
        chorus_v2_frame_t f = chorus_v2_decode_frame(o->bytes, o->len);
        chorus_check(f.outcome == CHORUS_FRAME_DECODED && f.message_type == CHORUS_V2_MIC_STATE &&
                         f.message.as.mic_state.gate == CHORUS_V2_MIC_GATE_LIVE &&
                         out.state_frames == 1 && out.audio_frames == 1 &&
                         voice_decodes_to(&out, mic.kept + first, 320, 0, first_ns, offset),
                     "the switch back to live: the live report, then audio from sequence 0");
    }
    chorus_check(voice.reports_sent == 6,
                 "six gate reports in all, one per change: muted, live, muted, live, muted, "
                 "live (%u)",
                 voice.reports_sent);

    /* The voice role's own audio vector, produced by the voice path. */
    out.len = 0;
    chorus_session_voice_begin(&voice);
    chorus_session_voice_control(&voice, &on);
    uint8_t scratch[64];
    (void)chorus_session_voice_report(&voice, scratch, sizeof(scratch));
    static uint8_t burn[1024];
    for (int i = 0; i < 7; i++) {
        (void)chorus_session_voice_capture(&voice, one, 320, 0, 1, 0, burn, sizeof(burn));
    }
    const int16_t six[6] = {0, 1, -1, 12345, -32768, 32767};
    out.len = chorus_session_voice_capture(&voice, six, 6, 4234567890ull, 1, offset, out.bytes,
                                           VOICE_CAP);
    static uint8_t vector[64];
    n = load_hex("fixtures/protocol/v2/mic_audio.hex", vector, sizeof(vector));
    chorus_check(n > 0 && out.len == (size_t)n && memcmp(out.bytes, vector, out.len) == 0,
                 "the eighth chunk of a run, six samples captured at 3 s on the server "
                 "timeline: exactly fixtures/protocol/v2/mic_audio.hex (%zu bytes)",
                 out.len);

    /* A new session forgets the request and owes the server the gate. */
    out.len = 0;
    chorus_session_voice_begin(&voice);
    voice_pump(&voice, &mic, 2, 1, offset, &out);
    chorus_check(out.state_frames == 1 && out.audio_frames == 0 && !voice.uplink,
                 "a new session: the gate is reported again and nothing is sent until this "
                 "session's server requests the uplink");

    /* The LED. */
    chorus_led_inputs_t li = {1, 1, 1, 1, 0, 0, 0, 0};
    chorus_session_voice_control(&voice, &listening_only);
    li.listening = chorus_session_voice_listening(&voice);
    chorus_check(chorus_led_decide(&li) == CHORUS_LED_LISTENING &&
                     strcmp(chorus_led_state_name(CHORUS_LED_LISTENING), "listening") == 0,
                 "voice_control listening: the LED shows listening, over playing");
    static chorus_led_t led;
    chorus_led_init(&led, chorus_controls_profile(CHORUS_CLASS_COMPACT));
    chorus_led_output_t lit = chorus_led_render(&led, CHORUS_LED_LISTENING, 0);
    chorus_led_output_t playing = chorus_led_render(&led, CHORUS_LED_PLAYING, 0);
    chorus_led_output_t muted = chorus_led_render(&led, CHORUS_LED_MUTED, 0);
    chorus_check(memcmp(&lit, &playing, sizeof(lit)) != 0 &&
                     memcmp(&lit, &muted, sizeof(lit)) != 0 && lit.brightness > 0,
                 "listening has a colour of its own: %u %u %u @%u", lit.red, lit.green, lit.blue,
                 lit.brightness);
    li.mic_muted = 1;
    chorus_check(chorus_led_decide(&li) == CHORUS_LED_MUTED,
                 "the switch at mute while the room listens: the LED shows mute");
    li.mic_muted = 0;
    chorus_session_voice_begin(&voice);
    li.listening = chorus_session_voice_listening(&voice);
    chorus_check(chorus_led_decide(&li) == CHORUS_LED_PLAYING,
                 "the session ended: listening is over and the LED returns to playing");
}

static void a_class_without_a_microphone_never_sends_voice(void)
{
    chorus_section("voice: a class without a microphone never produces a voice message");
    const chorus_speaker_class_t classes[] = {CHORUS_CLASS_TWO_WAY, CHORUS_CLASS_SUBWOOFER,
                                              CHORUS_CLASS_STREAMING_AMP};
    const chorus_v2_voice_control_t on = {1, 1};
    for (size_t k = 0; k < sizeof(classes) / sizeof(classes[0]); k++) {
        static chorus_controls_t c;
        static sink_t actions;
        static chorus_session_voice_t voice;
        static fake_mic_t mic;
        static voice_sink_t out;
        memset(&actions, 0, sizeof(actions));
        memset(&mic, 0, sizeof(mic));
        memset(&out, 0, sizeof(out));
        mic.seed = 7u + (uint32_t)k;
        mic.chunk = 320;
        chorus_controls_init(&c, classes[k], "den", "");
        chorus_session_voice_init(&voice, &c);
        chorus_session_voice_begin(&voice);
        /* Everything a server and a binding could do to ask for audio. */
        chorus_session_voice_control(&voice, &on);
        int refused = chorus_controls_level(&c, CHORUS_INPUT_MIC_MUTE_SWITCH, 0, 0) == -1;
        run(&c, 0, 30 * MS, &actions);
        voice_pump(&voice, &mic, 8, 1, 0, &out);
        chorus_led_inputs_t li = {1, 1, 1, 1, 0, 0, 0, 0};
        li.listening = chorus_session_voice_listening(&voice);
        chorus_check(chorus_session_voice_roles(&voice) == 0 && refused && out.len == 0 &&
                         voice.chunks_sent == 0 && voice.reports_sent == 0 && !voice.uplink &&
                         chorus_led_decide(&li) == CHORUS_LED_PLAYING,
                     "%s: no voice role, no switch, and with the uplink asked for and eight asks "
                     "of a capture source: no mic_state, no mic_audio, no listening light",
                     chorus_speaker_class_name(classes[k]));
    }
}

int main(void)
{
    the_command_values_are_the_catalogs();
    the_profiles_are_the_owners_decisions();
    the_compact_class();
    the_mute_switch_cuts_the_microphone();
    the_two_way_class();
    the_subwoofer_class();
    the_streaming_amp_class();
    the_led_follows_the_visualizer_fixture();
    the_voice_path_sends_mic_audio_only_through_the_gate();
    a_class_without_a_microphone_never_sends_voice();
    return chorus_test_report("test_controls");
}
