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
    FAKE_RADIO_JOIN
} fake_radio_event_kind_t;

typedef struct {
    fake_radio_event_kind_t kind;
    chorus_wifi_ps_t mode;
} fake_radio_event_t;

#define FAKE_RADIO_MAX_EVENTS 32
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
} fake_radio_t;

void fake_radio_init(fake_radio_t *fake);

/* The radio interface, with every call bound. */
chorus_radio_t fake_radio(fake_radio_t *fake);

/* The same, with `coexistence_active` left NULL, which is what a platform with
 * no opinion looks like: the committed declaration then stands alone. */
chorus_radio_t fake_radio_without_coexistence_query(fake_radio_t *fake);

/* Index of the first event of `kind`, or -1. */
int fake_radio_first(const fake_radio_t *fake, fake_radio_event_kind_t kind);
/* How many events of `kind` happened. */
size_t fake_radio_count(const fake_radio_t *fake, fake_radio_event_kind_t kind);

const char *fake_radio_event_kind_name(fake_radio_event_kind_t kind);
void fake_radio_print(const fake_radio_t *fake);

#endif /* CHORUS_FAKE_RADIO_H */
