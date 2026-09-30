/* The endpoint's serial console (audit A-13).
 *
 * The bench scripts drive an endpoint over its serial line: they set the Wi-Fi
 * power save mode and read back the mode in force
 * (tools/wireless-characterization-run.sh), point it at a server
 * (tools/endpoint-rig-run.sh), read its telemetry, and time the decoders on
 * the chip (tools/decode-cost-run.sh). Every decision about a command line -
 * what it means, what it refuses, what it prints - is here and graded on a
 * host by firmware/tests/test_console.c; firmware/main/console_esp.c only
 * binds this to ESP-IDF's console REPL.
 *
 * Every value a command sets is RUNTIME ONLY. Nothing here writes flash or
 * NVS; a reboot returns the endpoint to its committed configuration
 * (provisioning into NVS is chorus goal 14's).
 *
 * One reply line per command, `<command> key=value ...`, or
 * `error <command> reason=<token> detail="..."` when the command is refused.
 * A script parses the key=value words and nothing else. */

#ifndef CHORUS_CONSOLE_H
#define CHORUS_CONSOLE_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/decode_cost.h"
#include "chorus/telemetry.h"
#include "chorus/wifi.h"

/* A reply fits in this. */
#define CHORUS_CONSOLE_REPLY 1024

typedef struct {
    /* This endpoint's link, from the committed configuration. A wired endpoint
     * refuses `power-save` by name: it has no radio to set. */
    chorus_transport_t transport;
    /* The radio, when the link is wireless; NULL otherwise. */
    chorus_radio_t *radio;

    /* The latest telemetry, for `status`. Returns 0 and fills `out`, or -1 when
     * no session has published yet. NULL: `status` says so. */
    int (*telemetry)(void *ctx, chorus_telemetry_t *out);
    void *telemetry_ctx;

    /* Hands a validated `host:port` to whatever connects next. Returns 0 when
     * accepted. NULL: `server` is refused by name. */
    int (*set_server)(void *ctx, const char *address);
    void *server_ctx;

    /* The decode-cost fixtures this image carries, and the clock they are
     * timed on (the monotonic one; a test hands in a fake). */
    const chorus_decode_fixture_t *fixtures;
    size_t fixture_count;
    chorus_clock_fn now_ns;
} chorus_console_t;

/* Run one command line and write its one reply line (no newline) into `out`.
 * Returns 0 when the command did what it says, nonzero when the reply is an
 * `error` line. */
int chorus_console_execute(chorus_console_t *console, const char *line, char *out,
                           size_t out_len);

/* The commands, for a help line and the binding's registration. */
extern const char *const chorus_console_commands[];
extern const size_t chorus_console_command_count;

#endif /* CHORUS_CONSOLE_H */
