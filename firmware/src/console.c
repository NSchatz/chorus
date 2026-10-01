#include "chorus/console.h"

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "chorus/codec.h"
#include "chorus/dsp_cost.h"

const char *const chorus_console_commands[] = {"power-save", "server",    "status", "decode-cost",
                                               "dsp-cost",   "resources", "help"};
const size_t chorus_console_command_count =
    sizeof(chorus_console_commands) / sizeof(chorus_console_commands[0]);

#define MAX_WORDS 4
#define WORD_LEN 160

static int reply(char *out, size_t out_len, int status, const char *fmt, ...)
{
    va_list args;
    va_start(args, fmt);
    vsnprintf(out, out_len, fmt, args);
    va_end(args);
    return status;
}

static int refuse(char *out, size_t out_len, const char *command, const char *reason,
                  const char *detail)
{
    return reply(out, out_len, 1, "error %s reason=%s detail=\"%s\"", command, reason, detail);
}

/* Split on spaces and tabs. Returns the word count, or -1 when there are more
 * than MAX_WORDS or one is longer than WORD_LEN. */
static int split(const char *line, char words[MAX_WORDS][WORD_LEN])
{
    int count = 0;
    const char *p = line;
    for (;;) {
        while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
            p++;
        }
        if (*p == '\0') {
            return count;
        }
        if (count == MAX_WORDS) {
            return -1;
        }
        size_t n = 0;
        while (p[n] != '\0' && p[n] != ' ' && p[n] != '\t' && p[n] != '\r' && p[n] != '\n') {
            n++;
        }
        if (n >= WORD_LEN) {
            return -1;
        }
        memcpy(words[count], p, n);
        words[count][n] = '\0';
        count++;
        p += n;
    }
}

/* `power-save <mode>`: set the mode, read it back, and say both. The reply's
 * `in-force` is the platform's own answer, never the word that was asked for:
 * the characterization run records it and refuses a run whose two disagree. */
static int power_save(chorus_console_t *c, int argc, char words[MAX_WORDS][WORD_LEN], char *out,
                      size_t out_len)
{
    int ok = 0;
    chorus_wifi_ps_t mode =
        (argc == 2) ? chorus_wifi_ps_from_name(words[1], &ok) : CHORUS_WIFI_PS_UNKNOWN;
    if (argc != 2 || !ok || mode == CHORUS_WIFI_PS_UNKNOWN) {
        return refuse(out, out_len, "power-save", "usage", "power-save none|min-modem|max-modem");
    }
    if (c->transport != CHORUS_TRANSPORT_WIRELESS || c->radio == NULL) {
        return refuse(out, out_len, "power-save", "not-wireless",
                      "this endpoint's link is wired; there is no radio to set");
    }
    if (c->radio->set_power_save(c->radio->ctx, mode) != 0) {
        return refuse(out, out_len, "power-save", "set-refused",
                      "the platform would not accept the mode");
    }
    chorus_wifi_ps_t in_force = CHORUS_WIFI_PS_UNKNOWN;
    if (c->radio->get_power_save(c->radio->ctx, &in_force) != 0) {
        return refuse(out, out_len, "power-save", "readback-refused",
                      "the platform would not say which mode is in force");
    }
    int agrees = (in_force == mode);
    return reply(out, out_len, agrees ? 0 : 1, "power-save set=%s in-force=%s agrees=%s",
                 chorus_wifi_ps_name(mode), chorus_wifi_ps_name(in_force), agrees ? "yes" : "no");
}

/* `server <host:port>`: checked here, applied by whatever connects next. */
static int server(chorus_console_t *c, int argc, char words[MAX_WORDS][WORD_LEN], char *out,
                  size_t out_len)
{
    if (argc != 2) {
        return refuse(out, out_len, "server", "usage", "server <host>:<port>");
    }
    const char *address = words[1];
    const char *colon = strrchr(address, ':');
    char *end = NULL;
    unsigned long port = (colon != NULL) ? strtoul(colon + 1, &end, 10) : 0;
    if (colon == NULL || colon == address || colon[1] == '\0' || end == NULL || *end != '\0' ||
        port == 0 || port > 65535) {
        return refuse(out, out_len, "server", "not-host-port",
                      "the address is not <host>:<port> with a port from 1 to 65535");
    }
    if (c->set_server == NULL) {
        return refuse(out, out_len, "server", "no-session",
                      "nothing in this image takes a server address");
    }
    if (c->set_server(c->server_ctx, address) != 0) {
        return refuse(out, out_len, "server", "not-accepted", "the session did not take it");
    }
    return reply(out, out_len, 0, "server set=%s applies=next-connection", address);
}

static int status(chorus_console_t *c, int argc, char *out, size_t out_len)
{
    (void)argc;
    chorus_telemetry_t t;
    if (c->telemetry == NULL || c->telemetry(c->telemetry_ctx, &t) != 0) {
        return refuse(out, out_len, "status", "no-telemetry",
                      "no session has published telemetry yet");
    }
    char line[CHORUS_CONSOLE_REPLY - 16];
    chorus_telemetry_line(&t, line, sizeof(line));
    return reply(out, out_len, 0, "status %s", line);
}

/* `decode-cost [name]`: every carried fixture, or the one named. One reply
 * line, one `name:` group per fixture, so a script reads one line. */
static int decode_cost(chorus_console_t *c, int argc, char words[MAX_WORDS][WORD_LEN], char *out,
                       size_t out_len)
{
    if (argc > 2) {
        return refuse(out, out_len, "decode-cost", "usage", "decode-cost [fixture]");
    }
    if (c->fixture_count == 0 || c->now_ns == NULL) {
        return refuse(out, out_len, "decode-cost", "no-fixtures",
                      "this image carries no decode fixture");
    }
    size_t used = (size_t)snprintf(out, out_len, "decode-cost");
    int ran = 0;
    for (size_t i = 0; i < c->fixture_count; i++) {
        const chorus_decode_fixture_t *fx = &c->fixtures[i];
        if (argc == 2 && strcmp(words[1], fx->name) != 0) {
            continue;
        }
        chorus_decode_cost_t cost;
        char detail[256];
        detail[0] = '\0';
        if (chorus_decode_cost_run(fx, c->now_ns, &cost, detail, sizeof(detail)) != 0) {
            return refuse(out, out_len, "decode-cost", "decode-failed", detail);
        }
        if (used < out_len) {
            used += (size_t)snprintf(
                out + used, out_len - used,
                " %s:codec=%s,rate=%u,channels=%u,frames=%llu,elapsed_us=%llu,"
                "frames_per_s=%.0f,cpu_fraction=%.4f,decode_matches=%s",
                fx->name, cost.codec == CHORUS_CODEC_FLAC ? "flac" : "opus",
                (unsigned)cost.sample_rate_hz, (unsigned)cost.channels,
                (unsigned long long)cost.frames, (unsigned long long)(cost.elapsed_ns / 1000u),
                cost.frames_per_s, cost.cpu_fraction, cost.decode_matches ? "yes" : "no");
        }
        if (!cost.decode_matches) {
            return refuse(out, out_len, "decode-cost", "decode-differs",
                          "a decode did not hash to its fixture's decode_fnv1a64; no figure is "
                          "published for audio that is wrong");
        }
        ran++;
    }
    if (ran == 0) {
        return refuse(out, out_len, "decode-cost", "no-such-fixture",
                      "no carried fixture has that name");
    }
    if (c->stack_free_bytes != NULL && used < out_len) {
        snprintf(out + used, out_len - used, " stack_free_bytes=%u",
                 (unsigned)c->stack_free_bytes());
    }
    return 0;
}

/* `dsp-cost [config]`: every configuration of the chain, or the one named,
 * after decode-cost's shape: one reply line, one `name:` group each, and an
 * error rather than a figure for a chain whose output is not the host's. */
static int dsp_cost(chorus_console_t *c, int argc, char words[MAX_WORDS][WORD_LEN], char *out,
                    size_t out_len)
{
    if (argc > 2) {
        return refuse(out, out_len, "dsp-cost", "usage", "dsp-cost [flat|all-on|sub|two-way]");
    }
    if (c->now_ns == NULL) {
        return refuse(out, out_len, "dsp-cost", "no-clock", "this image binds no clock to time on");
    }
    size_t used = (size_t)snprintf(out, out_len, "dsp-cost");
    int ran = 0;
    for (size_t i = 0; i < CHORUS_DSP_COST_CONFIGS; i++) {
        if (argc == 2 && strcmp(words[1], chorus_dsp_cost_names[i]) != 0) {
            continue;
        }
        chorus_dsp_cost_t cost;
        char detail[256];
        detail[0] = '\0';
        if (chorus_dsp_cost_run(i, c->now_ns, c->dsp_alloc, c->dsp_free, &cost, detail,
                                sizeof(detail)) != 0) {
            return refuse(out, out_len, "dsp-cost", "chain-failed", detail);
        }
        if (used < out_len) {
            used += (size_t)snprintf(
                out + used, out_len - used,
                " %s:rate=%u,channels=2,outputs=%u,frames=%llu,elapsed_us=%llu,"
                "frames_per_s=%.0f,cpu_fraction=%.4f,deviation=%.1e,output_matches=%s,"
                "bit_exact=%s",
                cost.name, (unsigned)CHORUS_DSP_COST_RATE_HZ, (unsigned)cost.outputs,
                (unsigned long long)cost.frames, (unsigned long long)(cost.elapsed_ns / 1000u),
                cost.frames_per_s, cost.cpu_fraction, cost.deviation,
                cost.output_matches ? "yes" : "no", cost.bit_exact ? "yes" : "no");
        }
        if (!cost.output_matches) {
            return refuse(out, out_len, "dsp-cost", "output-differs",
                          "a chain's output is not within tolerance of the host's checksum; no "
                          "figure is published for audio that is wrong");
        }
        ran++;
    }
    if (ran == 0) {
        return refuse(out, out_len, "dsp-cost", "no-such-config",
                      "the configurations are flat, all-on, sub and two-way");
    }
    if (c->stack_free_bytes != NULL && used < out_len) {
        snprintf(out + used, out_len - used, " stack_free_bytes=%u",
                 (unsigned)c->stack_free_bytes());
    }
    return 0;
}

/* `resources`: heap, the named tasks' least free stack, the FIFO after the
 * writer and the marker's counts, on one line. A value the image cannot give
 * reads `none` (no PSRAM, no marker pin) or `absent` (no such task running)
 * or `unknown` (no playout path yet), never a zero that looks measured. */
static int resources(chorus_console_t *c, int argc, char *out, size_t out_len)
{
    if (argc != 1) {
        return refuse(out, out_len, "resources", "usage", "resources");
    }
    if (c->resources == NULL) {
        return refuse(out, out_len, "resources", "not-bound",
                      "this image does not report its resources");
    }
    chorus_console_resources_t r;
    memset(&r, 0, sizeof(r));
    r.marker_pin = -1;
    c->resources(c->resources_ctx, &r);
    size_t used =
        (size_t)snprintf(out, out_len, "resources heap_internal_free=%u heap_internal_min_free=%u",
                         (unsigned)r.internal_free, (unsigned)r.internal_min_free);
    if (used < out_len) {
        if (r.psram_present) {
            used += (size_t)snprintf(out + used, out_len - used,
                                     " heap_psram_free=%u heap_psram_min_free=%u",
                                     (unsigned)r.psram_free, (unsigned)r.psram_min_free);
        } else {
            used += (size_t)snprintf(out + used, out_len - used,
                                     " heap_psram_free=none heap_psram_min_free=none");
        }
    }
    size_t n =
        (r.stack_count > CHORUS_CONSOLE_MAX_TASKS) ? CHORUS_CONSOLE_MAX_TASKS : r.stack_count;
    for (size_t i = 0; i < n && used < out_len; i++) {
        const chorus_console_stack_t *t = &r.stacks[i];
        if (t->present) {
            used += (size_t)snprintf(out + used, out_len - used, " stack_free.%s=%u", t->name,
                                     (unsigned)t->free_bytes);
        } else {
            used += (size_t)snprintf(out + used, out_len - used, " stack_free.%s=absent", t->name);
        }
    }
    if (used < out_len) {
        if (r.fifo_known) {
            used += (size_t)snprintf(out + used, out_len - used, " fifo_frames=%.1f fifo_us=%.1f",
                                     r.fifo_frames, r.fifo_ns / 1000.0);
        } else {
            used += (size_t)snprintf(out + used, out_len - used,
                                     " fifo_frames=unknown fifo_us=unknown");
        }
    }
    if (used < out_len) {
        if (r.marker_pin >= 0) {
            used += (size_t)snprintf(
                out + used, out_len - used,
                " marker_pin=%d marker_period_ms=%u marker_armed=%llu "
                "marker_edges=%llu marker_missed=%llu "
                "marker_last_boundary_ns=%llu",
                (int)r.marker_pin, (unsigned)r.marker_period_ms, (unsigned long long)r.marker_armed,
                (unsigned long long)r.marker_edges, (unsigned long long)r.marker_missed,
                (unsigned long long)r.marker_last_boundary_ns);
        } else {
            used += (size_t)snprintf(out + used, out_len - used, " marker_pin=none");
        }
    }
    return 0;
}

int chorus_console_execute(chorus_console_t *console, const char *line, char *out, size_t out_len)
{
    char words[MAX_WORDS][WORD_LEN];
    int argc = split(line, words);
    if (argc < 0) {
        return refuse(out, out_len, "console", "too-long", "too many or too long words");
    }
    if (argc == 0) {
        return refuse(out, out_len, "console", "empty", "no command");
    }
    if (strcmp(words[0], "power-save") == 0) {
        return power_save(console, argc, words, out, out_len);
    }
    if (strcmp(words[0], "server") == 0) {
        return server(console, argc, words, out, out_len);
    }
    if (strcmp(words[0], "status") == 0) {
        return status(console, argc, out, out_len);
    }
    if (strcmp(words[0], "decode-cost") == 0) {
        return decode_cost(console, argc, words, out, out_len);
    }
    if (strcmp(words[0], "dsp-cost") == 0) {
        return dsp_cost(console, argc, words, out, out_len);
    }
    if (strcmp(words[0], "resources") == 0) {
        return resources(console, argc, out, out_len);
    }
    if (strcmp(words[0], "help") == 0) {
        return reply(out, out_len, 0,
                     "help commands=power-save,server,status,decode-cost,dsp-cost,resources "
                     "values=runtime-only");
    }
    return refuse(out, out_len, "console", "unknown-command",
                  "the commands are power-save, server, status, decode-cost, dsp-cost, resources "
                  "and help");
}
