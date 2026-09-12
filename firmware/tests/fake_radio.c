#include "fake_radio.h"

#include <stdio.h>
#include <string.h>

static void record(fake_radio_t *fake, fake_radio_event_kind_t kind, chorus_wifi_ps_t mode)
{
    if (fake->event_count >= FAKE_RADIO_MAX_EVENTS) {
        return;
    }
    fake->events[fake->event_count].kind = kind;
    fake->events[fake->event_count].mode = mode;
    fake->event_count++;
}

void fake_radio_init(fake_radio_t *fake)
{
    memset(fake, 0, sizeof(*fake));
    /* The platform default, and not a neutral value. A bring-up that set
     * nothing would leave the radio here, which is what makes the first
     * assertion of this phase gradeable. */
    fake->power_save = CHORUS_WIFI_PLATFORM_DEFAULT;
    fake->readback = CHORUS_WIFI_PS_UNKNOWN;
}

static int fake_init(void *ctx)
{
    fake_radio_t *fake = (fake_radio_t *)ctx;
    record(fake, FAKE_RADIO_INIT, fake->power_save);
    return fake->init_refuses ? -1 : 0;
}

static int fake_set_power_save(void *ctx, chorus_wifi_ps_t mode)
{
    fake_radio_t *fake = (fake_radio_t *)ctx;
    record(fake, FAKE_RADIO_SET_POWER_SAVE, mode);
    if (fake->set_refuses) {
        return -1;
    }
    fake->power_save = mode;
    return 0;
}

static int fake_get_power_save(void *ctx, chorus_wifi_ps_t *mode)
{
    fake_radio_t *fake = (fake_radio_t *)ctx;
    chorus_wifi_ps_t answer = fake->readback_overridden ? fake->readback : fake->power_save;
    record(fake, FAKE_RADIO_GET_POWER_SAVE, answer);
    if (fake->get_refuses) {
        return -1;
    }
    *mode = answer;
    return 0;
}

static int fake_coexistence_active(void *ctx, int *active)
{
    fake_radio_t *fake = (fake_radio_t *)ctx;
    record(fake, FAKE_RADIO_COEXISTENCE_QUERY, fake->power_save);
    *active = fake->coexistence_active;
    return 0;
}

static int fake_join(void *ctx, const char *ssid, const char *secret)
{
    fake_radio_t *fake = (fake_radio_t *)ctx;
    record(fake, FAKE_RADIO_JOIN, fake->power_save);
    fake->joins++;
    snprintf(fake->joined_ssid, sizeof(fake->joined_ssid), "%s", ssid);
    snprintf(fake->joined_secret, sizeof(fake->joined_secret), "%s", secret);
    return fake->join_refuses ? -1 : 0;
}

chorus_radio_t fake_radio(fake_radio_t *fake)
{
    chorus_radio_t radio;
    radio.ctx = fake;
    radio.init = fake_init;
    radio.set_power_save = fake_set_power_save;
    radio.get_power_save = fake_get_power_save;
    radio.coexistence_active = fake_coexistence_active;
    radio.join = fake_join;
    return radio;
}

chorus_radio_t fake_radio_without_coexistence_query(fake_radio_t *fake)
{
    chorus_radio_t radio = fake_radio(fake);
    radio.coexistence_active = NULL;
    return radio;
}

int fake_radio_first(const fake_radio_t *fake, fake_radio_event_kind_t kind)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == kind) {
            return (int)i;
        }
    }
    return -1;
}

size_t fake_radio_count(const fake_radio_t *fake, fake_radio_event_kind_t kind)
{
    size_t count = 0;
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == kind) {
            count++;
        }
    }
    return count;
}

const char *fake_radio_event_kind_name(fake_radio_event_kind_t kind)
{
    switch (kind) {
    case FAKE_RADIO_INIT:
        return "init";
    case FAKE_RADIO_SET_POWER_SAVE:
        return "set-power-save";
    case FAKE_RADIO_GET_POWER_SAVE:
        return "get-power-save";
    case FAKE_RADIO_COEXISTENCE_QUERY:
        return "coexistence-query";
    case FAKE_RADIO_JOIN:
        return "join";
    }
    return "unknown";
}

void fake_radio_print(const fake_radio_t *fake)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        printf("     %zu %s %s\n", i, fake_radio_event_kind_name(fake->events[i].kind),
               chorus_wifi_ps_name(fake->events[i].mode));
    }
}
