#include "chorus/wifi.h"

#include <stdio.h>
#include <string.h>

const char *chorus_transport_name(chorus_transport_t transport)
{
    switch (transport) {
    case CHORUS_TRANSPORT_WIRED:
        return "wired";
    case CHORUS_TRANSPORT_WIRELESS:
        return "wireless";
    }
    return "unknown";
}

chorus_transport_t chorus_transport_from_name(const char *name, int *ok)
{
    *ok = 1;
    if (strcmp(name, "wired") == 0) {
        return CHORUS_TRANSPORT_WIRED;
    }
    if (strcmp(name, "wireless") == 0) {
        return CHORUS_TRANSPORT_WIRELESS;
    }
    *ok = 0;
    return CHORUS_TRANSPORT_WIRED;
}

const char *chorus_wifi_ps_name(chorus_wifi_ps_t mode)
{
    switch (mode) {
    case CHORUS_WIFI_PS_UNKNOWN:
        return "unknown";
    case CHORUS_WIFI_PS_NONE:
        return "none";
    case CHORUS_WIFI_PS_MIN_MODEM:
        return "min-modem";
    case CHORUS_WIFI_PS_MAX_MODEM:
        return "max-modem";
    }
    return "unknown";
}

chorus_wifi_ps_t chorus_wifi_ps_from_name(const char *name, int *ok)
{
    *ok = 1;
    if (strcmp(name, "none") == 0) {
        return CHORUS_WIFI_PS_NONE;
    }
    if (strcmp(name, "min-modem") == 0) {
        return CHORUS_WIFI_PS_MIN_MODEM;
    }
    if (strcmp(name, "max-modem") == 0) {
        return CHORUS_WIFI_PS_MAX_MODEM;
    }
    *ok = 0;
    return CHORUS_WIFI_PS_UNKNOWN;
}

const char *chorus_wifi_status_name(chorus_wifi_status_t status)
{
    switch (status) {
    case CHORUS_WIFI_OK:
        return "ok";
    case CHORUS_WIFI_NOT_WIRELESS:
        return "link-is-wired";
    case CHORUS_WIFI_CREDENTIAL_UNKNOWN:
        return "wireless-credential-unknown";
    case CHORUS_WIFI_INIT_REFUSED:
        return "radio-init-refused";
    case CHORUS_WIFI_SET_REFUSED:
        return "power-save-mode-refused";
    case CHORUS_WIFI_READBACK_REFUSED:
        return "power-save-mode-unreadable";
    case CHORUS_WIFI_MODE_DISAGREES:
        return "power-save-mode-disagrees";
    case CHORUS_WIFI_SLEEPS_IN_COEXISTENCE:
        return "sleeps-in-coexistence";
    case CHORUS_WIFI_JOIN_REFUSED:
        return "join-refused";
    }
    return "unknown";
}

static chorus_wifi_status_t done(chorus_wifi_report_t *report, chorus_wifi_status_t status)
{
    report->status = status;
    return status;
}

/* Join the declared network, recording that a join was attempted at all.
 *
 * Called on the two paths where the mode this endpoint set is not the mode in
 * effect. The link still comes up there, deliberately: a house whose access
 * point or whose coexisting radio will not give it the mode it asked for should
 * still play music. What it must not do is publish the wireless bound, and
 * `bound_publishable` stays zero on both. */
static void attempt_join(const chorus_wifi_config_t *config, chorus_radio_t *radio,
                         chorus_wifi_report_t *report)
{
    if (radio->join == NULL) {
        return;
    }
    report->join_attempted = 1;
    if (radio->join(radio->ctx, config->ssid, config->secret) == 0) {
        report->link_up = 1;
    }
}

chorus_wifi_status_t chorus_wifi_bring_up(const chorus_wifi_config_t *config,
                                          chorus_radio_t *radio, chorus_wifi_report_t *report)
{
    memset(report, 0, sizeof(*report));
    report->transport = config->transport;
    report->declared = config->power_save;

    /* 1. A wired link is not brought up. Nothing is touched, and in particular
     *    no wireless claim is made about a wired endpoint: bound_publishable
     *    stays zero and the telemetry line reads `not-applicable`. */
    if (config->transport != CHORUS_TRANSPORT_WIRELESS) {
        report->link_up = 1;
        snprintf(report->detail, sizeof(report->detail),
                 "the link is declared %s in %s, so no radio is brought up and no wireless bound "
                 "is published about this endpoint",
                 chorus_transport_name(config->transport), config->source);
        return done(report, CHORUS_WIFI_NOT_WIRELESS);
    }

    /* 2. A credential this repository does not have. Before the radio is
     *    touched, so the refusal is provably before any join, and there is no
     *    default network to fall back to. */
    if (!config->ssid_known || !config->secret_known) {
        const char *which = !config->ssid_known ? "link_wifi_ssid" : "link_wifi_secret";
        const char *and_also =
            (!config->ssid_known && !config->secret_known) ? " (and link_wifi_secret too)" : "";
        snprintf(report->detail, sizeof(report->detail),
                 "%s reads `unknown` in %s%s, so this endpoint will not join: set it there, or "
                 "provision it out of band. No default network is tried and no join is attempted; "
                 "the link is reported down",
                 which, config->source, and_also);
        report->link_up = 0;
        return done(report, CHORUS_WIFI_CREDENTIAL_UNKNOWN);
    }

    /* 3. The radio itself. */
    if (radio->init != NULL && radio->init(radio->ctx) != 0) {
        snprintf(report->detail, sizeof(report->detail),
                 "the radio could not be initialised; no power save mode was set and the link is "
                 "down");
        return done(report, CHORUS_WIFI_INIT_REFUSED);
    }

    /* 4. SET the mode, explicitly, before anything is told the link is usable.
     *    The platform default is CHORUS_WIFI_PLATFORM_DEFAULT and inheriting it
     *    is what this phase exists to stop. */
    if (radio->set_power_save == NULL ||
        radio->set_power_save(radio->ctx, config->power_save) != 0) {
        snprintf(report->detail, sizeof(report->detail),
                 "the platform refused the power save mode `%s` declared in %s; the platform "
                 "default is `%s` and this endpoint does not inherit it silently, so the link is "
                 "down",
                 chorus_wifi_ps_name(config->power_save), config->source,
                 chorus_wifi_ps_name(CHORUS_WIFI_PLATFORM_DEFAULT));
        return done(report, CHORUS_WIFI_SET_REFUSED);
    }

    /* 5. READ it back. Setting a mode and being in it are different claims. */
    chorus_wifi_ps_t in_force = CHORUS_WIFI_PS_UNKNOWN;
    if (radio->get_power_save == NULL || radio->get_power_save(radio->ctx, &in_force) != 0) {
        snprintf(report->detail, sizeof(report->detail),
                 "the power save mode `%s` was set from %s and the platform would not say what "
                 "mode it is in; the mode in force is unknown, so the wireless bound is not "
                 "published",
                 chorus_wifi_ps_name(config->power_save), config->source);
        return done(report, CHORUS_WIFI_READBACK_REFUSED);
    }
    report->mode_read = 1;
    report->in_force = in_force;

    if (in_force != config->power_save) {
        /* AC-10. Both modes are reported, the link still comes up, and the
         * bound is not claimed. A house that cannot get the mode it asked for
         * should still play music; what it must not do is publish a bound it
         * has no grounds for. */
        snprintf(report->detail, sizeof(report->detail),
                 "the power save mode declared in %s is `%s` and the platform reports `%s`; both "
                 "are reported and the wireless bound is NOT claimed",
                 config->source, chorus_wifi_ps_name(config->power_save),
                 chorus_wifi_ps_name(in_force));
        attempt_join(config, radio, report);
        return done(report, CHORUS_WIFI_MODE_DISAGREES);
    }

    /* 6. Coexistence. The platform agrees about the mode and sleeps anyway. */
    int coexisting = config->coexistence;
    if (radio->coexistence_active != NULL) {
        int reported = 0;
        if (radio->coexistence_active(radio->ctx, &reported) == 0 && reported) {
            coexisting = 1;
        }
    }
    if (coexisting) {
        /* AC-9. The mode IS set and IS in force, and the sleep is still there:
         * "Wi-Fi will remain active only during Wi-Fi time slice, and sleep
         * during non Wi-Fi time slice even if esp_wifi_set_ps(WIFI_PS_NONE) is
         * called". */
        snprintf(report->detail, sizeof(report->detail),
                 "the power save mode `%s` was set and the platform reports `%s`, and the radio is "
                 "in a coexistence mode in which it sleeps outside its Wi-Fi time slice anyway, so "
                 "the mode that was set is NOT in effect and the wireless bound is NOT published "
                 "as met",
                 chorus_wifi_ps_name(config->power_save), chorus_wifi_ps_name(in_force));
        attempt_join(config, radio, report);
        return done(report, CHORUS_WIFI_SLEEPS_IN_COEXISTENCE);
    }

    report->mode_in_effect = 1;

    /* 7. Join, and only now is the link usable. */
    if (radio->join == NULL || radio->join(radio->ctx, config->ssid, config->secret) != 0) {
        report->join_attempted = radio->join != NULL;
        snprintf(report->detail, sizeof(report->detail),
                 "the power save mode `%s` was set and read back, and the network declared in %s "
                 "could not be joined; the link is down",
                 chorus_wifi_ps_name(config->power_save), config->source);
        return done(report, CHORUS_WIFI_JOIN_REFUSED);
    }
    report->join_attempted = 1;
    report->link_up = 1;
    report->bound_publishable = 1;
    snprintf(report->detail, sizeof(report->detail),
             "the power save mode `%s` was set from %s before the link was reported usable, the "
             "platform reports `%s`, and the platform default `%s` was not inherited",
             chorus_wifi_ps_name(config->power_save), config->source,
             chorus_wifi_ps_name(in_force),
             chorus_wifi_ps_name(CHORUS_WIFI_PLATFORM_DEFAULT));
    return done(report, CHORUS_WIFI_OK);
}
