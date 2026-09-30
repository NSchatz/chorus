#include "chorus/console.h"

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "chorus/codec.h"

const char *const chorus_console_commands[] = {"power-save", "server", "status", "decode-cost",
                                               "help"};
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
    chorus_wifi_ps_t mode = (argc == 2) ? chorus_wifi_ps_from_name(words[1], &ok)
                                        : CHORUS_WIFI_PS_UNKNOWN;
    if (argc != 2 || !ok || mode == CHORUS_WIFI_PS_UNKNOWN) {
        return refuse(out, out_len, "power-save", "usage",
                      "power-save none|min-modem|max-modem");
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

int chorus_console_execute(chorus_console_t *console, const char *line, char *out,
                           size_t out_len)
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
    if (strcmp(words[0], "help") == 0) {
        return reply(out, out_len, 0,
                     "help commands=power-save,server,status,decode-cost values=runtime-only");
    }
    return refuse(out, out_len, "console", "unknown-command",
                  "the commands are power-save, server, status, decode-cost and help");
}
