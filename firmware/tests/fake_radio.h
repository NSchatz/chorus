/* A simulated Wi-Fi radio, writing into one event log.
 *
 * The log is appended by the FAKE and not by the bring-up. That is the same
 * design firmware/tests/fake_amp.c uses and it is here for the same reason: the
 * assertion is about what happened at the antenna and in what order, and a log
 * the bring-up wrote would be the bring-up's account of itself. The bring-up
 * has no way to reach this log at all.
 *
 * The simulated radio POWERS UP IN THE PLATFORM DEFAULT, which the carried
 * ESP-IDF guide states is WIFI_PS_MIN_MODEM. That is the whole point of the
 * fake: a bring-up that never set the mode would leave the fake in minimum
 * modem sleep and the test would see it, so "sets its mode explicitly rather
 * than inherit the default" is graded against something that can be false. */

#ifndef CHORUS_FAKE_RADIO_H
#define CHORUS_FAKE_RADIO_H

#include "chorus/wifi.h"

#include <stddef.h>

typedef enum {
    FAKE_RADIO_INIT,
    FAKE_RADIO_SET_POWER_SAVE,
    FAKE_RADIO_GET_POWER_SAVE,
    FAKE_RADIO_COEXISTENCE_QUERY,
    FAKE_RADIO_JOIN,
    /* The speaker's own access point (goal 14, chorus/provision.h). */
    FAKE_RADIO_AP_START,
    FAKE_RADIO_AP_STOP
} fake_radio_event_kind_t;

typedef struct {
    fake_radio_event_kind_t kind;
    chorus_wifi_ps_t mode;
} fake_radio_event_t;

/* Room for a provisioning run: several joins, each an init, a set, a readback,
 * a coexistence query and the join itself. */
#define FAKE_RADIO_MAX_EVENTS 256
#define FAKE_RADIO_TEXT 128

typedef struct {
    fake_radio_event_t events[FAKE_RADIO_MAX_EVENTS];
    size_t event_count;

    /* The mode the radio is actually in. Starts at the platform default. */
    chorus_wifi_ps_t power_save;

    /* Set to make a call refuse. */
    int init_refuses;
    int set_refuses;
    int get_refuses;
    int join_refuses;

    /* What the readback answers with, when it is not to answer with the mode
     * the radio is in. A platform that accepts a mode and reports another is
     * exactly the disagreement AC-10 is about. */
    int readback_overridden;
    chorus_wifi_ps_t readback;

    /* What the coexistence query answers. */
    int coexistence_active;

    /* What the join was given. Recorded so a test can assert the secret DID
     * reach the radio and did NOT reach anything published. */
    char joined_ssid[FAKE_RADIO_TEXT];
    char joined_secret[FAKE_RADIO_TEXT];
    size_t joins;

    /* The network in range (goal 14). When `network_known` is set, a join
     * succeeds only for this name with this passphrase, and a join that fails
     * leaves the platform's word for why in `join_reason`: the two words
     * ESP-IDF's provisioning manager has, an unknown network and a refused
     * passphrase. Unset, every join succeeds unless `join_refuses`, as before. */
    int network_known;
    char network_ssid[FAKE_RADIO_TEXT];
    char network_secret[FAKE_RADIO_TEXT];
    char join_reason[FAKE_RADIO_TEXT];

    /* The speaker's own access point: whether it is up, under what name and
     * key, and how often it was raised and dropped. */
    int ap_up;
    int ap_start_refuses;
    char ap_name[FAKE_RADIO_TEXT];
    char ap_key[FAKE_RADIO_TEXT];
    size_t ap_starts;
    size_t ap_stops;
} fake_radio_t;

void fake_radio_init(fake_radio_t *fake);

/* The radio interface, with every call bound. */
chorus_radio_t fake_radio(fake_radio_t *fake);

/* The same, with `coexistence_active` left NULL, which is what a platform with
 * no opinion looks like: the committed declaration then stands alone. */
chorus_radio_t fake_radio_without_coexistence_query(fake_radio_t *fake);

/* The network in range from here on (see `network_known`). */
void fake_radio_set_network(fake_radio_t *fake, const char *ssid, const char *secret);

/* The access point and the join's reason, in the shape
 * `chorus_provision_platform_t` takes them; `ctx` is the `fake_radio_t`. They
 * are plain functions and not a struct so this header needs nothing of the
 * provisioning unit's. */
int fake_radio_ap_start(void *ctx, const char *name, const char *key);
int fake_radio_ap_stop(void *ctx);
const char *fake_radio_join_reason(void *ctx);

/* Index of the first event of `kind`, or -1. */
int fake_radio_first(const fake_radio_t *fake, fake_radio_event_kind_t kind);
/* How many events of `kind` happened. */
size_t fake_radio_count(const fake_radio_t *fake, fake_radio_event_kind_t kind);

const char *fake_radio_event_kind_name(fake_radio_event_kind_t kind);
void fake_radio_print(const fake_radio_t *fake);

#endif /* CHORUS_FAKE_RADIO_H */
