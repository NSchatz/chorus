/* The endpoint's wireless bring-up.
 *
 * # What this unit is for
 *
 * One sentence of the roadmap phase: "WHEN a Wi-Fi endpoint starts THE SYSTEM
 * SHALL set its Wi-Fi power save mode explicitly rather than inherit the
 * platform default of minimum modem sleep, and SHALL report the mode it set."
 * Everything here exists to make that true and checkable on a host with no
 * radio.
 *
 * The reason the default is refused is in the carried ESP-IDF guide and is
 * quoted in full in docs/decisions/0021-the-wireless-tier.md: the default is
 * WIFI_PS_MIN_MODEM, and under modem sleep "the delay in receiving Wi-Fi data
 * may be the same as the DTIM cycle (minimum power-saving mode) or the
 * listening interval (maximum power-saving mode)". The DTIM cycle belongs to
 * the access point, so it is a delay this endpoint cannot correct and cannot
 * even see. An endpoint that never sets the mode has silently accepted it.
 *
 * # Why the radio is injectable
 *
 * The same reason the amplifier's I2C bus is (`chorus/amp.h`): the DECISIONS
 * are the interesting part, and the decisions are gradeable on a machine with
 * no ESP32-S3 if the platform is behind an interface. What mode to set, whether
 * the readback agrees, whether a coexistence declaration makes the setting
 * ineffective, and whether to join at all with a credential unknown are all
 * decided here and graded by firmware/tests/test_wifi.c against a simulated
 * radio that records what happened at the antenna. The real binding is in
 * firmware/main/esp_hal.c, is compiled only by ESP-IDF, and is claimed by
 * nobody: docs/verification-record.md says so beside the other bindings this
 * tree does not claim.
 *
 * # Setting the mode is not the same as the sleep being gone
 *
 * Two separate things can make the mode this endpoint set not the mode in
 * force, and they fail differently, so they are reported differently:
 *
 *   - the platform reports a different mode than the one that was set, which
 *     is a disagreement about what was configured;
 *   - the board is in a coexistence mode, where the same guide says "Wi-Fi will
 *     remain active only during Wi-Fi time slice, and sleep during non Wi-Fi
 *     time slice even if esp_wifi_set_ps(WIFI_PS_NONE) is called", which is a
 *     platform that agrees about the mode and sleeps anyway.
 *
 * In both cases the link still comes UP, because a house that cannot reach an
 * access point's preferred power save mode should still play music. What it
 * must not do is publish the wireless bound, and it does not: the report's
 * `bound_publishable` is zero and the telemetry line says so.
 *
 * # `publishable` is not `met`
 *
 * Nothing in this unit measures anything. `bound_publishable` says the
 * CONDITIONS under which the wireless bound may be published hold, which is a
 * statement about configuration and readback. Whether a wireless zone actually
 * holds the bound is a measured distribution over a real radio, it is operator
 * graded, and it is NOT passed in this repository.
 *
 * # No clock, and no credential
 *
 * Nothing here reads a clock of any kind, and in particular nothing here starts
 * a network time service after it joins. The three ordinary spellings of one
 * are on the settable-clock list in `firmware/check/endpoint_scan.c`, so a join
 * path that set a wall clock fails the safety scans rather than being caught by
 * a reviewer. The one thing this bring-up publishes is the mode it set. The
 * report never carries the network secret, in any field, so a published line
 * cannot leak one. */

#ifndef CHORUS_WIFI_H
#define CHORUS_WIFI_H

#include <stddef.h>

/* Which transport an endpoint's link is. A wired endpoint brings no radio up
 * and has no wireless claim made about it. */
typedef enum { CHORUS_TRANSPORT_WIRED = 0, CHORUS_TRANSPORT_WIRELESS } chorus_transport_t;

const char *chorus_transport_name(chorus_transport_t transport);
/* Parse `wired` or `wireless`. `*ok` is 0 for anything else. */
chorus_transport_t chorus_transport_from_name(const char *name, int *ok);

/* A Wi-Fi power save mode, in the platform's own terms.
 *
 * UNKNOWN is a real answer and not an error value: it is what a readback
 * carries when the platform named a mode this build does not have a word for,
 * and a mode nobody can name is not a mode anything may be claimed about. */
typedef enum {
    CHORUS_WIFI_PS_UNKNOWN = 0,
    /* WIFI_PS_NONE: modem sleep disabled entirely. */
    CHORUS_WIFI_PS_NONE,
    /* WIFI_PS_MIN_MODEM: the platform default, which wakes every DTIM. */
    CHORUS_WIFI_PS_MIN_MODEM,
    /* WIFI_PS_MAX_MODEM: wakes every listen interval. */
    CHORUS_WIFI_PS_MAX_MODEM
} chorus_wifi_ps_t;

const char *chorus_wifi_ps_name(chorus_wifi_ps_t mode);
/* Parse `none`, `min-modem` or `max-modem`. `*ok` is 0 for anything else,
 * including the word `unknown`: a power save mode is not a value this
 * repository declares unknown, because the whole point of the phase is that it
 * is SET rather than inherited. */
chorus_wifi_ps_t chorus_wifi_ps_from_name(const char *name, int *ok);

/* The mode the platform is in before anything sets one.
 *
 * "The default Modem-sleep mode is WIFI_PS_MIN_MODEM", verbatim from the
 * carried ESP-IDF guide. This constant is here so a test can assert that the
 * endpoint sets something OTHER than what it would have inherited, rather than
 * asserting a literal that agrees with itself. */
#define CHORUS_WIFI_PLATFORM_DEFAULT CHORUS_WIFI_PS_MIN_MODEM

#define CHORUS_WIFI_TEXT 128

/* The link, as the committed configuration declares it.
 *
 * `ssid_known` and `secret_known` are zero when the configuration declares that
 * value the literal `unknown`, which is what firmware/config/endpoint.conf
 * carries and will carry for as long as this is a repository rather than a
 * house. */
typedef struct {
    chorus_transport_t transport;
    chorus_wifi_ps_t power_save;
    /* Whether a non-Wi-Fi radio shares this board's time slice. Declared, not
     * probed: it is a fact about a board, like the octal-PSRAM flag. */
    int coexistence;
    int ssid_known;
    char ssid[CHORUS_WIFI_TEXT];
    int secret_known;
    char secret[CHORUS_WIFI_TEXT];
    /* The file these were declared in, carried so a refusal can name where to
     * set the value it is refusing on. */
    char source[CHORUS_WIFI_TEXT];
} chorus_wifi_config_t;

/* The radio, injectable so that every decision above is observable on a host
 * with no ESP32-S3. Each call returns 0 on success.
 *
 * `coexistence_active` lets a platform that knows better than the committed
 * declaration say so. A platform that has no opinion leaves it NULL and the
 * declaration stands. */
typedef struct {
    void *ctx;
    int (*init)(void *ctx);
    int (*set_power_save)(void *ctx, chorus_wifi_ps_t mode);
    int (*get_power_save)(void *ctx, chorus_wifi_ps_t *mode);
    int (*coexistence_active)(void *ctx, int *active);
    int (*join)(void *ctx, const char *ssid, const char *secret);
} chorus_radio_t;

/* How bring-up ended. Every one has a stable name, because a condition a reader
 * cannot act on is not a report. */
typedef enum {
    CHORUS_WIFI_OK = 0,
    /* The endpoint's link is wired; no radio was touched. */
    CHORUS_WIFI_NOT_WIRELESS,
    /* The network name or the secret is still `unknown`. */
    CHORUS_WIFI_CREDENTIAL_UNKNOWN,
    /* The platform would not initialise the radio. */
    CHORUS_WIFI_INIT_REFUSED,
    /* The platform would not accept the mode. */
    CHORUS_WIFI_SET_REFUSED,
    /* The platform would not say what mode it is in. */
    CHORUS_WIFI_READBACK_REFUSED,
    /* The platform reports a mode other than the one that was set. */
    CHORUS_WIFI_MODE_DISAGREES,
    /* The platform agrees about the mode and sleeps anyway. */
    CHORUS_WIFI_SLEEPS_IN_COEXISTENCE,
    /* The platform would not join the network. */
    CHORUS_WIFI_JOIN_REFUSED
} chorus_wifi_status_t;

const char *chorus_wifi_status_name(chorus_wifi_status_t status);

#define CHORUS_WIFI_DETAIL 512

typedef struct {
    chorus_wifi_status_t status;
    chorus_transport_t transport;
    /* The mode the committed configuration asked for. */
    chorus_wifi_ps_t declared;
    /* Whether a readback happened at all, and what it said. A report that has
     * not read the mode back says `not-read` rather than naming a mode, for the
     * same reason the telemetry line says `none` rather than `0` for a bound
     * nobody has measured. */
    int mode_read;
    chorus_wifi_ps_t in_force;
    /* Whether the mode this endpoint set is the mode in force AND nothing else
     * makes the platform sleep anyway. */
    int mode_in_effect;
    /* Whether the wireless bound may be PUBLISHED. Never whether it is met. */
    int bound_publishable;
    /* Whether the link is usable. A session is not told the link is usable
     * until this is 1. */
    int link_up;
    /* Whether a join was attempted at all. Zero on every refusal that happens
     * before the radio is asked to join, which is what makes "reports the link
     * down rather than retrying against a default" provable rather than
     * promised. */
    int join_attempted;
    char detail[CHORUS_WIFI_DETAIL];
} chorus_wifi_report_t;

/* Bring the wireless link up, or refuse and say why.
 *
 * The order, and the reason for each step:
 *
 *   1. a wired link is not brought up at all. Nothing is touched and no
 *      wireless claim is made.
 *   2. refuse while the network name or the secret reads `unknown`, naming
 *      which one and the file to set it in. Nothing has touched the radio, so
 *      the refusal is provably before any join, and no default network exists
 *      anywhere in this tree to fall back to.
 *   3. initialise the radio.
 *   4. SET the power save mode from the committed configuration. After the
 *      initialise and before the join, which is the order the carried guide
 *      fixes: the mode is set "after calling esp_wifi_init()", and modem sleep
 *      starts "when station connects to AP".
 *   5. READ the mode back. A platform that reports a different mode gets both
 *      modes reported and the bound withheld.
 *   6. ask about coexistence. A platform that sleeps outside its Wi-Fi time
 *      slice gets that reported and the bound withheld, even though it agreed
 *      about the mode.
 *   7. join, and only then is the link usable.
 *
 * Steps 4 and 5 are both before step 7, which is the whole of "set its Wi-Fi
 * power save mode explicitly before it reports the link usable". */
chorus_wifi_status_t chorus_wifi_bring_up(const chorus_wifi_config_t *config,
                                          chorus_radio_t *radio, chorus_wifi_report_t *report);

#endif /* CHORUS_WIFI_H */
