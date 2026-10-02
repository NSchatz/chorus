/* Wi-Fi provisioning for the compact speakers: the decisions.
 *
 * # What this unit is for
 *
 * A compact Wi-Fi speaker (K91) leaves the bench knowing no network: the
 * committed configuration declares `link_wifi_ssid` and `link_wifi_secret`
 * `unknown` and always will, because a network's name and passphrase are a
 * house's and never a repository's. This unit is how the speaker learns them
 * at run time: with nothing stored it raises its own access point, serves one
 * small join form, tries the network it was given, and keeps the credentials
 * only once the join worked. From then on it joins at boot with no access
 * point at all.
 *
 * Everything that DECIDES is here and is graded on a host by
 * firmware/tests/test_provision.c: the states, what a hostile form body is
 * refused for, where a credential may go, and what a reset erases. What talks
 * to the radio and to ESP-IDF's provisioning manager is
 * firmware/main/esp_provision.c, compiled only into the Wi-Fi profile's image
 * and claimed by nobody until the owner's bench session (docs/bench-packet.md
 * S9). docs/decisions/ADRNUM-wifi-provisioning-over-softap.md is the record.
 *
 * # The three seams
 *
 *   - the store (chorus/store.h): the setup secret under `prov_secret`, the
 *     network under `wifi_ssid` and `wifi_secret`. Nothing else is written.
 *   - the radio (chorus/wifi.h, `chorus_radio_t`): every join goes through
 *     `chorus_wifi_bring_up`, so the power save mode is SET and read back
 *     before any join here exactly as it is on a speaker that never
 *     provisioned (docs/decisions/0024-the-wireless-tier.md).
 *   - the platform (`chorus_provision_platform_t`, below): raising and
 *     dropping the access point, a random source, and a log sink.
 *
 * # Where a credential may go
 *
 * The network's passphrase goes to the radio (to join) and to the store (to
 * join again after a reboot). It goes nowhere else: no line this unit hands
 * the log sink carries it, the page it renders does not carry it, and the
 * network's NAME is kept out of both as well, because a log pasted back from
 * a bench would otherwise carry the house's network name (K27). A line says
 * how many bytes arrived and nothing about which.
 *
 * The SETUP secret is a different thing: twelve characters this speaker makes
 * for itself at first boot, which are the access point's WPA2 passphrase and
 * the proof of possession Espressif's own clients ask for. It is meant to be
 * read by the person standing at the speaker, so the binding prints it on the
 * serial console (`chorus_provision_setup_secret`); this unit's own lines
 * never carry it either, so a test can hold both rules to the same grep.
 *
 * # No clock of its own
 *
 * Time is passed in, as monotonic milliseconds, by whoever calls. Nothing
 * here reads a clock. */

#ifndef CHORUS_PROVISION_H
#define CHORUS_PROVISION_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/store.h"
#include "chorus/wifi.h"

/* The store keys this unit owns (the store's key rule: 1 to 15 of [a-z0-9_]). */
#define CHORUS_PROVISION_KEY_SETUP "prov_secret"
#define CHORUS_PROVISION_KEY_SSID "wifi_ssid"
#define CHORUS_PROVISION_KEY_SECRET "wifi_secret"

/* A network name is 1 to 32 bytes and a WPA2 passphrase 8 to 63 characters, or
 * the 64 hexadecimal digits of the key itself: the bounds of the fields the
 * radio takes (`uint8_t ssid[32]`, `uint8_t password[64]`, ESP-IDF v6.1
 * components/esp_wifi/include/esp_wifi_types_generic.h:564-565, read
 * 2026-10-02) and of IEEE 802.11's passphrase mapping. */
#define CHORUS_PROVISION_SSID_MAX 32
#define CHORUS_PROVISION_SECRET_MIN 8
#define CHORUS_PROVISION_SECRET_MAX 64

/* The longest form body read. Both fields fully percent-encoded are
 * 3 x (32 + 64) = 288 bytes, plus the two names; anything longer is not this
 * form. */
#define CHORUS_PROVISION_FORM_MAX 384

/* The setup secret: 12 characters from 31 symbols with the look-alikes left
 * out (no 0, 1, i, l or o), about 59 bits, inside WPA2's 8 to 63. The access
 * point's name carries 6 more random characters of the same alphabet, NOT the
 * MAC and not the endpoint id: the name is broadcast, and a pasted log line
 * that holds it identifies nothing (K27). */
#define CHORUS_PROVISION_SETUP_CHARS 12
#define CHORUS_PROVISION_NAME_SUFFIX_CHARS 6
#define CHORUS_PROVISION_NAME_PREFIX "chorus-setup-"
#define CHORUS_PROVISION_NAME_MAX 32

/* ASSUMED, both, until the owner's bench session says otherwise (the envelope
 * of goal 14 names the retry count ASSUMED):
 *   - how many joins are tried at boot with the stored network before the
 *     access point comes back;
 *   - how long the access point then waits before it tries the stored network
 *     again by itself, so a speaker that booted faster than the house's router
 *     after a power cut does not sit in setup until somebody walks over. */
#define CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS 3u
#define CHORUS_PROVISION_REJOIN_SECONDS 120u

typedef enum {
    /* Nothing stored; the access point is about to be raised. */
    CHORUS_PROVISION_UNPROVISIONED = 0,
    /* The access point is up and the join form is being served. */
    CHORUS_PROVISION_AP_UP,
    /* A form (or Espressif's client) handed over a network that passed the
     * bounds. */
    CHORUS_PROVISION_CREDENTIALS_RECEIVED,
    /* A join is in progress. */
    CHORUS_PROVISION_JOINING,
    /* The speaker is on the network. Terminal for provisioning: the session
     * may start. */
    CHORUS_PROVISION_JOINED,
    /* The last join did not work; `chorus_provision_reason` says why. Passed
     * through on the way back to AP_UP (or to the next attempt at boot). */
    CHORUS_PROVISION_JOIN_FAILED,
    /* The store held a network at boot; no access point is raised for it. */
    CHORUS_PROVISION_PROVISIONED_AT_BOOT,
    /* The stored network was erased on request. */
    CHORUS_PROVISION_RESET,
    /* The link is wired: this unit touched nothing and never will. */
    CHORUS_PROVISION_WIRED,
    /* Provisioning cannot run (the store or the access point refused);
     * `chorus_provision_reason` names it. Terminal. */
    CHORUS_PROVISION_REFUSED
} chorus_provision_state_t;

const char *chorus_provision_state_name(chorus_provision_state_t state);

/* Why a form body, or a network handed over any other way, was refused. Every
 * one has a stable name; the form shows it to the person who typed. */
typedef enum {
    CHORUS_PROVISION_FORM_OK = 0,
    CHORUS_PROVISION_FORM_TOO_LONG,
    /* A pair with no `=`, or an empty name. */
    CHORUS_PROVISION_FORM_MALFORMED,
    /* `%` not followed by two hexadecimal digits. */
    CHORUS_PROVISION_FORM_BAD_ESCAPE,
    /* A zero byte, raw or as %00: both values are carried as C strings. */
    CHORUS_PROVISION_FORM_EMBEDDED_NUL,
    CHORUS_PROVISION_FORM_UNKNOWN_FIELD,
    CHORUS_PROVISION_FORM_DUPLICATE_FIELD,
    CHORUS_PROVISION_SSID_MISSING,
    CHORUS_PROVISION_SSID_EMPTY,
    CHORUS_PROVISION_SSID_TOO_LONG,
    CHORUS_PROVISION_SECRET_MISSING,
    CHORUS_PROVISION_SECRET_TOO_SHORT,
    CHORUS_PROVISION_SECRET_TOO_LONG,
    /* 64 characters that are not all hexadecimal digits. */
    CHORUS_PROVISION_SECRET_NOT_HEX,
    /* A passphrase character outside printable ASCII (32 to 126). */
    CHORUS_PROVISION_SECRET_NOT_PRINTABLE,
    /* No access point is up: nothing is being asked for a network. */
    CHORUS_PROVISION_NOT_ACCEPTING
} chorus_provision_form_status_t;

const char *chorus_provision_form_status_name(chorus_provision_form_status_t status);

/* A network, as the form parser hands it over. Both are C strings: a value
 * with a zero byte in it was refused. */
typedef struct {
    char ssid[CHORUS_PROVISION_SSID_MAX + 1];
    size_t ssid_length;
    char secret[CHORUS_PROVISION_SECRET_MAX + 1];
    size_t secret_length;
} chorus_provision_credentials_t;

/* The bounds alone, for a network that arrived some other way than the form
 * (Espressif's client through the manager, or the store at boot). */
chorus_provision_form_status_t chorus_provision_check_credentials(const char *ssid,
                                                                  size_t ssid_length,
                                                                  const char *secret,
                                                                  size_t secret_length);

/* Parse the join form's body: `application/x-www-form-urlencoded`, the two
 * fields `ssid` and `secret` in either order, `+` for a space and `%XX`
 * escapes. `body` need not be terminated. On anything but OK, `*out` is
 * zeroed: a refused body leaves no part of a passphrase behind. */
chorus_provision_form_status_t chorus_provision_parse_form(const char *body, size_t length,
                                                           chorus_provision_credentials_t *out);

/* The platform, injectable for the same reason the radio is. Each call
 * returns 0 on success. */
typedef struct {
    void *ctx;
    /* Raise the access point under `name`, WPA2 with passphrase `key`, and
     * serve the join form on it. */
    int (*ap_start)(void *ctx, const char *name, const char *key);
    /* Drop the access point and stop serving. */
    int (*ap_stop)(void *ctx);
    /* Why the platform's last join failed, in a stable word of its own
     * (`auth-error`, `network-not-found`), or NULL when it has none. Optional:
     * NULL leaves the bring-up's own status name as the reason. */
    const char *(*join_reason)(void *ctx);
    /* Random bytes for the setup secret (the board: the hardware generator
     * behind chorus_noise_system_random). */
    int (*random)(void *ctx, uint8_t *out, size_t length);
    /* One line of what happened, with no credential in it. Optional. */
    void (*log)(void *ctx, const char *line);
} chorus_provision_platform_t;

#define CHORUS_PROVISION_TRACE 64
#define CHORUS_PROVISION_REASON 64

typedef struct {
    /* The link as the committed configuration declares it (transport, power
     * save, coexistence), with the network filled in for the join in hand.
     * Holds a passphrase in RAM; nothing prints it. */
    chorus_wifi_config_t wifi;
    const chorus_store_t *store;
    chorus_radio_t *radio;
    chorus_provision_platform_t platform;

    chorus_provision_state_t state;
    /* Every state entered, in order, so a test reads the path and not only
     * where it ended. Stops recording when full; `trace_dropped` counts. */
    chorus_provision_state_t trace[CHORUS_PROVISION_TRACE];
    size_t trace_count;
    size_t trace_dropped;

    int ap_up;
    /* Whether the store holds a usable network. */
    int stored;
    char ap_name[CHORUS_PROVISION_NAME_MAX + 1];
    char setup_secret[CHORUS_PROVISION_SETUP_CHARS + 1];
    /* Why the last join or form was refused; empty when nothing was. */
    char reason[CHORUS_PROVISION_REASON];
    /* When the access point next tries the stored network by itself. */
    uint64_t rejoin_at_ms;
    /* Joins asked of the radio by this unit since `chorus_provision_init`. */
    unsigned joins;
    /* The last bring-up's report: the power save mode it set, and the rest. */
    chorus_wifi_report_t report;
} chorus_provision_t;

/* Bind the unit. `link` is copied; its network, if it names one, is ignored:
 * the store is the only source. Nothing is touched yet. */
void chorus_provision_init(chorus_provision_t *p, const chorus_wifi_config_t *link,
                           const chorus_store_t *store, chorus_radio_t *radio,
                           const chorus_provision_platform_t *platform);

/* What a boot does, in order:
 *
 *   1. a wired link ends here, in WIRED. No store read, no radio, no access
 *      point: a wired profile never provisions.
 *   2. load the setup secret, or make and store one. A secret that cannot be
 *      stored is REFUSED rather than used once and forgotten: the person at
 *      the speaker would be handed a passphrase that changes at every boot.
 *   3. with a usable network in the store: PROVISIONED_AT_BOOT, then up to
 *      CHORUS_PROVISION_BOOT_JOIN_ATTEMPTS joins. The first that works ends
 *      in JOINED and no access point was ever raised.
 *   4. with none, or after the last failed join: the access point is raised
 *      (AP_UP) and the failed join's reason is kept for the form. The stored
 *      network is NOT erased by a failed join: a router that is off is not a
 *      wrong passphrase.
 *
 * Returns the state it ended in: WIRED, JOINED, AP_UP or REFUSED. */
chorus_provision_state_t chorus_provision_boot(chorus_provision_t *p, uint64_t now_ms);

/* A network arrived while the access point is up. Checked against the bounds,
 * then joined ONCE; only a join that worked is stored (`wifi_ssid`,
 * `wifi_secret`), and then the access point is dropped and the state is
 * JOINED. A join that failed, or a store that refused, leaves the access
 * point up with the reason kept, and whatever the store held before is still
 * there. `*refusal` (optional) says why a network was not even tried. */
chorus_provision_state_t chorus_provision_submit(chorus_provision_t *p,
                                                 const chorus_provision_credentials_t *network,
                                                 uint64_t now_ms,
                                                 chorus_provision_form_status_t *refusal);

/* The same, from the form's body. */
chorus_provision_state_t chorus_provision_submit_form(chorus_provision_t *p, const char *body,
                                                      size_t length, uint64_t now_ms,
                                                      chorus_provision_form_status_t *refusal);

/* Called now and then while the access point is up. When a network is stored
 * and CHORUS_PROVISION_REJOIN_SECONDS have passed since the access point came
 * up or last tried, the stored network is tried once more. */
chorus_provision_state_t chorus_provision_tick(chorus_provision_t *p, uint64_t now_ms);

/* The reset (the console command; the pairing button held): erase `wifi_ssid`
 * and `wifi_secret` and nothing else. The setup secret stays, so the access
 * point returns under the same name and passphrase. With the access point up
 * the state ends in AP_UP; otherwise in UNPROVISIONED, and the caller boots
 * again (the board restarts). Returns 0 when both keys are gone. */
int chorus_provision_reset(chorus_provision_t *p);

/* Copy the network in hand into `link` (the fields `chorus_wifi_bring_up`
 * reads), for the bring-up that follows a JOINED. Returns 0 when JOINED. */
int chorus_provision_export(const chorus_provision_t *p, chorus_wifi_config_t *link);

const char *chorus_provision_ap_name(const chorus_provision_t *p);
/* For the binding's one console line and for nothing else. */
const char *chorus_provision_setup_secret(const chorus_provision_t *p);
/* Why the last join or form was refused, or "" . */
const char *chorus_provision_reason(const chorus_provision_t *p);

/* The join page (`GET /`): the form, and the last refusal's reason when there
 * is one. It never carries a network's name or any secret. Returns the number
 * of bytes written, or 0 when `capacity` is too small. */
size_t chorus_provision_page(const chorus_provision_t *p, char *out, size_t capacity);
/* The reply to a form (`POST /join`): accepted and being tried, or refused
 * and why. */
size_t chorus_provision_reply_page(chorus_provision_form_status_t status, char *out,
                                   size_t capacity);

#endif /* CHORUS_PROVISION_H */
