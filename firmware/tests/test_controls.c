/* The endpoint's controls per class, its status LED and its microphone gate
 * (brief section 13 item 2; K65, K67-K70; docs/decisions/0063-*).
 *
 * Each class is driven by a timed input script on a fake monotonic clock, and
 * the controller_command frames its controls produce are held byte for byte
 * to the committed fixtures/controls/<class>.hex, which the server's
 * crates/server/tests/controller_role.rs decodes and applies to a room: that
 * pair is the controller role end to end. The LED is fed
 * fixtures/controls/visualizer-sequence.hex and held, moment by moment, to
 * visualizer-sequence.led. No pin, no clock and no network. */

#include <stdint.h>
#include <string.h>

#include "chorus/controls.h"
#include "chorus/protocol_v2.h"
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

    chorus_led_inputs_t li = {1, 1, 1, 1, 1, 0, 0};
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

    chorus_led_inputs_t order[] = {
        {1, 1, 1, 1, 1, 1, 1}, {1, 1, 1, 1, 1, 1, 0}, {1, 1, 1, 1, 1, 0, 0}, {0, 0, 0, 0, 0, 0, 0},
        {1, 0, 1, 1, 0, 0, 0}, {1, 1, 1, 1, 0, 0, 0}, {1, 1, 0, 1, 0, 0, 0}};
    chorus_led_state_t want[] = {CHORUS_LED_FAULT, CHORUS_LED_PAIRING,   CHORUS_LED_MUTED,
                                 CHORUS_LED_BOOT,  CHORUS_LED_LINK_DOWN, CHORUS_LED_PLAYING,
                                 CHORUS_LED_IDLE};
    int ok = 1;
    for (size_t i = 0; i < sizeof(want) / sizeof(want[0]); i++) {
        ok &= chorus_led_decide(&order[i]) == want[i];
    }
    chorus_check(ok, "state priority: fault, pairing, muted, boot, link down, playing, idle "
                     "(not adopted is idle)");
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
    return chorus_test_report("test_controls");
}
