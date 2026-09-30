/* The endpoint's serial console (audit A-13), on a host.
 *
 * What the bench scripts rely on, each graded here against the simulated radio
 * (tests/fake_radio.c) and the committed codec fixtures:
 *
 *   - `power-save <mode>` SETS the mode and replies with the platform's own
 *     readback, and a readback that disagrees is an error reply, never a
 *     success that repeats the requested word;
 *   - a wired endpoint refuses `power-save` by name, touching no radio;
 *   - `server <host:port>` hands a checked address to the session and refuses
 *     one that is not host:port;
 *   - `status` prints the telemetry line, or refuses when no session has
 *     published;
 *   - `decode-cost` decodes the fixtures through the endpoint's codec seam,
 *     times only the decode calls on the clock it is handed, and publishes a
 *     figure only for a decode that hashes to the fixture's decode_fnv1a64.
 *
 * Every figure the real-clock case prints is the HOST's cost and is not
 * recorded anywhere: the S3's is tools/decode-cost-run.sh's, on hardware. */

#include <arpa/inet.h>
#include <netinet/in.h>
#include <stdint.h>
#include <sys/socket.h>
#include <unistd.h>

#include "chorus/console.h"
#include "chorus/session.h"
#include "chorus/monotonic.h"
#include "chorus/telemetry.h"
#include "fake_radio.h"
#include "harness.h"

static uint8_t *slurp(const char *path, size_t *len)
{
    FILE *f = fopen(path, "rb");
    if (f == NULL) {
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t *data = malloc(size > 0 ? (size_t)size + 1 : 1);
    if (data == NULL || fread(data, 1, (size_t)size, f) != (size_t)size) {
        fclose(f);
        free(data);
        return NULL;
    }
    fclose(f);
    data[size] = 0;
    *len = (size_t)size;
    return data;
}

static int load_fixture(chorus_decode_fixture_t *fx, const char *name)
{
    char path[1024];
    fx->name = name;
    snprintf(path, sizeof(path), "%s/fixtures/codec/%s.fields", CHORUS_REPO_ROOT, name);
    fx->fields = (const char *)slurp(path, &fx->fields_len);
    snprintf(path, sizeof(path), "%s/fixtures/codec/%s.chunks", CHORUS_REPO_ROOT, name);
    fx->chunks = slurp(path, &fx->chunks_len);
    return (fx->fields != NULL && fx->chunks != NULL) ? 0 : -1;
}

/* A clock that advances exactly 1 ms per read, so the decode-cost arithmetic
 * is checked exactly: two reads per chunk, one before and one after. */
static uint64_t fake_now;
static uint64_t fake_clock(void)
{
    fake_now += 1000000ull;
    return fake_now;
}

static uint32_t fixed_stack_free(void)
{
    return 5120;
}

static char server_given[256];
static int server_accepts = 1;
static int take_server(void *ctx, const char *address)
{
    (void)ctx;
    snprintf(server_given, sizeof(server_given), "%s", address);
    return server_accepts ? 0 : -1;
}

static int have_telemetry;
static chorus_telemetry_t published;
static int read_telemetry(void *ctx, chorus_telemetry_t *out)
{
    (void)ctx;
    if (!have_telemetry) {
        return -1;
    }
    *out = published;
    return 0;
}

static chorus_console_t a_console(chorus_transport_t transport, chorus_radio_t *radio)
{
    chorus_console_t c;
    memset(&c, 0, sizeof(c));
    c.transport = transport;
    c.radio = radio;
    c.set_server = take_server;
    c.telemetry = read_telemetry;
    c.now_ns = fake_clock;
    return c;
}

static void power_save_sets_and_reads_back(void)
{
    chorus_section("power-save sets the mode and replies with the platform's readback");
    fake_radio_t fake;
    fake_radio_init(&fake);
    chorus_radio_t radio = fake_radio(&fake);
    chorus_console_t c = a_console(CHORUS_TRANSPORT_WIRELESS, &radio);
    char out[CHORUS_CONSOLE_REPLY];

    int rc = chorus_console_execute(&c, "power-save none\r\n", out, sizeof(out));
    chorus_check(rc == 0 && strcmp(out, "power-save set=none in-force=none agrees=yes") == 0,
                 "`power-save none` replies `%s` (rc %d)", out, rc);
    chorus_check(fake.power_save == CHORUS_WIFI_PS_NONE &&
                     fake_radio_count(&fake, FAKE_RADIO_SET_POWER_SAVE) == 1 &&
                     fake_radio_count(&fake, FAKE_RADIO_GET_POWER_SAVE) == 1,
                 "the simulated radio saw one set and one readback and is in `none`");

    rc = chorus_console_execute(&c, "power-save min-modem", out, sizeof(out));
    chorus_check(rc == 0 && strcmp(out, "power-save set=min-modem in-force=min-modem agrees=yes") ==
                                0,
                 "`power-save min-modem` replies `%s`", out);

    fake.readback_overridden = 1;
    fake.readback = CHORUS_WIFI_PS_MAX_MODEM;
    rc = chorus_console_execute(&c, "power-save none", out, sizeof(out));
    chorus_check(rc != 0 && strcmp(out, "power-save set=none in-force=max-modem agrees=no") == 0,
                 "a readback that disagrees is an error reply carrying both modes: `%s`", out);
    fake.readback_overridden = 0;

    fake.set_refuses = 1;
    rc = chorus_console_execute(&c, "power-save none", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "error power-save reason=set-refused") == out,
                 "a platform that refuses the mode is named: `%s`", out);
    fake.set_refuses = 0;

    fake.get_refuses = 1;
    rc = chorus_console_execute(&c, "power-save none", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "reason=readback-refused") != NULL,
                 "a platform that will not read back is named: `%s`", out);
    fake.get_refuses = 0;

    rc = chorus_console_execute(&c, "power-save doze", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "reason=usage") != NULL,
                 "a mode with no name is refused with the usage: `%s`", out);
}

static void a_wired_endpoint_has_no_radio_to_set(void)
{
    chorus_section("a wired endpoint refuses power-save and touches no radio");
    fake_radio_t fake;
    fake_radio_init(&fake);
    chorus_radio_t radio = fake_radio(&fake);
    chorus_console_t c = a_console(CHORUS_TRANSPORT_WIRED, &radio);
    char out[CHORUS_CONSOLE_REPLY];
    int rc = chorus_console_execute(&c, "power-save none", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "error power-save reason=not-wireless") == out,
                 "`%s`", out);
    chorus_check(fake.event_count == 0, "the simulated radio saw %zu events", fake.event_count);
}

static void server_takes_a_checked_address(void)
{
    chorus_section("server hands a checked host:port to the session");
    chorus_console_t c = a_console(CHORUS_TRANSPORT_WIRED, NULL);
    char out[CHORUS_CONSOLE_REPLY];
    int rc = chorus_console_execute(&c, "server bench-host:4010", out, sizeof(out));
    chorus_check(rc == 0 && strcmp(out, "server set=bench-host:4010 applies=next-connection") == 0 &&
                     strcmp(server_given, "bench-host:4010") == 0,
                 "`%s`, the session was given `%s`", out, server_given);
    const char *bad[] = {"server bench-host", "server :4010", "server bench-host:0",
                         "server bench-host:65536", "server bench-host:40x", "server"};
    for (size_t i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
        server_given[0] = '\0';
        rc = chorus_console_execute(&c, bad[i], out, sizeof(out));
        chorus_check(rc != 0 && strstr(out, "error server") == out && server_given[0] == '\0',
                     "`%s` is refused and nothing is handed on: `%s`", bad[i], out);
    }
    server_accepts = 0;
    rc = chorus_console_execute(&c, "server bench-host:4010", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "reason=not-accepted") != NULL, "`%s`", out);
    server_accepts = 1;
}

static void status_prints_the_telemetry_line(void)
{
    chorus_section("status prints the telemetry line, or says there is none");
    chorus_console_t c = a_console(CHORUS_TRANSPORT_WIRELESS, NULL);
    char out[CHORUS_CONSOLE_REPLY];
    have_telemetry = 0;
    int rc = chorus_console_execute(&c, "status", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "reason=no-telemetry") != NULL, "`%s`", out);

    chorus_telemetry_init(&published);
    published.link = CHORUS_LINK_UP;
    published.frames_received = 4800;
    have_telemetry = 1;
    char line[768];
    chorus_telemetry_line(&published, line, sizeof(line));
    rc = chorus_console_execute(&c, "status", out, sizeof(out));
    chorus_check(rc == 0 && strncmp(out, "status ", 7) == 0 && strcmp(out + 7, line) == 0,
                 "`status` is `status ` and the telemetry line: `%s`", out);
}

static void decode_cost_times_only_the_decode(void)
{
    chorus_section("decode-cost decodes the fixtures and times the decode calls");
    chorus_decode_fixture_t fx[2];
    int loaded = load_fixture(&fx[0], "flac-s16-stereo-44k1") == 0 &&
                 load_fixture(&fx[1], "opus-tv10-celt-stereo") == 0;
    chorus_check(loaded, "fixtures/codec flac-s16-stereo-44k1 and opus-tv10-celt-stereo read");
    if (!loaded) {
        return;
    }

    /* Exact arithmetic on the 1 ms clock: 11 FLAC chunks, 2 reads each. */
    chorus_decode_cost_t cost;
    char detail[256] = "";
    fake_now = 0;
    int rc = chorus_decode_cost_run(&fx[0], fake_clock, &cost, detail, sizeof(detail));
    double audio_s = (double)cost.frames / 44100.0;
    chorus_check(rc == 0 && cost.chunks == 11 && cost.frames == 44877 &&
                     cost.elapsed_ns == 11ull * 1000000ull && cost.decode_matches,
                 "flac: %u chunks, %llu frames, %llu ns timed (one fake ms per decode call), "
                 "decode hashes to decode_fnv1a64: %s",
                 cost.chunks, (unsigned long long)cost.frames,
                 (unsigned long long)cost.elapsed_ns, cost.decode_matches ? "yes" : "no");
    chorus_check(rc == 0 && cost.cpu_fraction > 0.0107 / audio_s * 1.0 &&
                     cost.cpu_fraction < 0.0111 / audio_s &&
                     cost.frames_per_s > 44877.0 / 0.0111 && cost.frames_per_s < 44877.0 / 0.0109,
                 "flac: cpu_fraction %.6f = 0.011 s over %.4f s of audio; %.0f frames/s",
                 cost.cpu_fraction, audio_s, cost.frames_per_s);

    fake_now = 0;
    rc = chorus_decode_cost_run(&fx[1], fake_clock, &cost, detail, sizeof(detail));
    chorus_check(rc == 0 && cost.decode_matches && cost.sample_rate_hz == 48000 &&
                     cost.elapsed_ns == (uint64_t)cost.chunks * 1000000ull,
                 "opus: %u chunks, %llu frames, decode hashes to decode_fnv1a64: %s (%s)",
                 cost.chunks, (unsigned long long)cost.frames, cost.decode_matches ? "yes" : "no",
                 detail);

    /* A corrupted chunk is not a figure. */
    uint8_t *broken = malloc(fx[0].chunks_len);
    memcpy(broken, fx[0].chunks, fx[0].chunks_len);
    for (size_t i = 40; i < 200; i++) {
        broken[i] ^= 0x5a;
    }
    chorus_decode_fixture_t bad = fx[0];
    bad.chunks = broken;
    rc = chorus_decode_cost_run(&bad, fake_clock, &cost, detail, sizeof(detail));
    chorus_check(rc != 0 || !cost.decode_matches,
                 "a corrupted chunk yields no matching decode (rc %d, detail `%s`)", rc, detail);

    chorus_console_t c = a_console(CHORUS_TRANSPORT_WIRED, NULL);
    c.fixtures = fx;
    c.fixture_count = 2;
    char out[CHORUS_CONSOLE_REPLY];
    fake_now = 0;
    rc = chorus_console_execute(&c, "decode-cost", out, sizeof(out));
    chorus_check(rc == 0 && strstr(out, "decode-cost flac-s16-stereo-44k1:codec=flac,") == out &&
                     strstr(out, " opus-tv10-celt-stereo:codec=opus,") != NULL &&
                     strstr(out, "decode_matches=no") == NULL,
                 "`decode-cost` is one line with one group per fixture: `%s`", out);
    c.stack_free_bytes = fixed_stack_free;
    rc = chorus_console_execute(&c, "decode-cost opus-tv10-celt-stereo", out, sizeof(out));
    chorus_check(rc == 0 && strstr(out, " stack_free_bytes=5120") != NULL,
                 "the reply ends with the console task's least free stack: `%s`", out);
    chorus_check(rc == 0 && strstr(out, "flac") == NULL, "`decode-cost <name>` runs that one");
    rc = chorus_console_execute(&c, "decode-cost mp3", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "reason=no-such-fixture") != NULL, "`%s`", out);
    chorus_decode_fixture_t with_bad[2] = {bad, fx[1]};
    c.fixtures = with_bad;
    rc = chorus_console_execute(&c, "decode-cost flac-s16-stereo-44k1", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "error decode-cost") == out,
                 "a wrong decode is an error reply, not a figure: `%s`", out);
    free(broken);

    /* On the real monotonic clock: the HOST's figure, printed and not kept. */
    c.fixtures = fx;
    c.now_ns = chorus_monotonic_now_ns;
    rc = chorus_console_execute(&c, "decode-cost", out, sizeof(out));
    chorus_check(rc == 0, "on the host's monotonic clock (host figure, not a measurement): %s",
                 out);

    free((void *)fx[0].fields);
    free((void *)fx[0].chunks);
    free((void *)fx[1].fields);
    free((void *)fx[1].chunks);
}

/* The session side of `server` and `status`: a run that starts against an
 * address nothing listens on, and is handed a loopback listener by the console
 * hook on its second attempt, connects THERE, logs the change, and hands its
 * telemetry on at every event. The listener speaks no protocol, so the
 * handshake times out; what is graded is where the run connected. */
static int updates_asked;
static char update_to[64];
static int update_server(void *ctx, char *address, size_t len)
{
    (void)ctx;
    updates_asked++;
    if (updates_asked != 2) {
        return 0;
    }
    snprintf(address, len, "%s", update_to);
    return 1;
}

static int telemetry_handed_on;
static int telemetry_saw_attempts;
static void take_telemetry(void *ctx, const chorus_telemetry_t *t)
{
    (void)ctx;
    telemetry_handed_on++;
    if (t->connect_attempts > 0) {
        telemetry_saw_attempts = 1;
    }
}

static void the_session_connects_where_the_console_says(void)
{
    chorus_section("the session connects to the address the console set, from the next attempt");
    int listener = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = 0;
    socklen_t addr_len = sizeof(addr);
    int ready = listener >= 0 && bind(listener, (struct sockaddr *)&addr, sizeof(addr)) == 0 &&
                listen(listener, 4) == 0 &&
                getsockname(listener, (struct sockaddr *)&addr, &addr_len) == 0;
    chorus_check(ready, "a loopback listener on port %u", (unsigned)ntohs(addr.sin_port));
    if (!ready) {
        return;
    }
    snprintf(update_to, sizeof(update_to), "127.0.0.1:%u", (unsigned)ntohs(addr.sin_port));

    char log_path[] = "/tmp/chorus-test-console-XXXXXX";
    int log_fd = mkstemp(log_path);
    close(log_fd);

    chorus_session_config_t config;
    memset(&config, 0, sizeof(config));
    /* Port 1 on loopback: nothing listens there, so the first attempt fails. */
    snprintf(config.server, sizeof(config.server), "127.0.0.1:1");
    config.first_backoff_ms = 50;
    config.max_backoff_ms = 100;
    config.run_seconds = 1;
    config.sync_interval_ms = 500;
    config.filter_window = 8;
    config.smoothing_alpha = 0.25;
    config.event_log_path = log_path;
    config.handshake_timeout_ms = 300;
    config.server_update = update_server;
    config.on_telemetry = take_telemetry;
    chorus_session_result_t result;
    (void)chorus_session_run(&config, &result);

    int accepted = accept(listener, NULL, NULL);
    chorus_check(accepted >= 0, "the listener the console named was connected to (%d updates asked)",
                 updates_asked);
    if (accepted >= 0) {
        close(accepted);
    }
    close(listener);

    char log[16384];
    size_t got = 0;
    FILE *f = fopen(log_path, "r");
    if (f != NULL) {
        got = fread(log, 1, sizeof(log) - 1, f);
        fclose(f);
    }
    log[got] = '\0';
    unlink(log_path);
    char want[128];
    snprintf(want, sizeof(want), "event=server-changed detail=\"server=%s\"", update_to);
    const char *failed = strstr(log, "event=connect-failed");
    const char *changed = strstr(log, want);
    chorus_check(failed != NULL && changed != NULL && failed < changed,
                 "the event log has a failed attempt against the old address, then `%s`", want);
    chorus_check(telemetry_handed_on > 0 && telemetry_saw_attempts,
                 "the run handed its telemetry on %d times, with its connect attempts in it",
                 telemetry_handed_on);
}

static void everything_else_is_refused_by_name(void)
{
    chorus_section("an unknown or empty command is refused by name");
    chorus_console_t c = a_console(CHORUS_TRANSPORT_WIRED, NULL);
    char out[CHORUS_CONSOLE_REPLY];
    int rc = chorus_console_execute(&c, "reboot", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "reason=unknown-command") != NULL, "`%s`", out);
    rc = chorus_console_execute(&c, "   ", out, sizeof(out));
    chorus_check(rc != 0 && strstr(out, "reason=empty") != NULL, "`%s`", out);
    rc = chorus_console_execute(&c, "help", out, sizeof(out));
    chorus_check(rc == 0 && strstr(out, "values=runtime-only") != NULL, "`%s`", out);
}

int main(void)
{
    power_save_sets_and_reads_back();
    a_wired_endpoint_has_no_radio_to_set();
    server_takes_a_checked_address();
    status_prints_the_telemetry_line();
    decode_cost_times_only_the_decode();
    the_session_connects_where_the_console_says();
    everything_else_is_refused_by_name();
    return chorus_test_report("test_console");
}
