/* The endpoint's wireless bring-up.
 *
 * Four criteria of `chorus#WIFI-7` are graded here, and all four are about a
 * DECISION rather than a measurement, which is why they are gradeable on a
 * machine with no radio:
 *
 *   AC-1.  the power save mode is SET explicitly rather than inherited, and the
 *          mode that was set is reported.
 *   AC-9.  a coexistence mode in which the platform sleeps outside its Wi-Fi
 *          time slice is reported as the mode NOT being in effect, and the
 *          wireless bound is not published as met.
 *   AC-10. a platform reporting a different mode than the one declared has both
 *          modes reported and the wireless bound is not claimed.
 *   AC-17. no credential is carried in the repository, and a run whose network
 *          name or secret is still unknown refuses to join, names which value
 *          is unknown and where to set it, and reports the link DOWN rather
 *          than retrying against a default.
 *
 * Everything is read off the SIMULATED RADIO's own event log and off the
 * published telemetry line, never off the bring-up's account of itself: the
 * fake appends the log, at the antenna, and the bring-up cannot reach it. The
 * fake powers up in the PLATFORM DEFAULT rather than in a neutral state, so a
 * bring-up that set nothing would leave it in minimum modem sleep and be seen.
 *
 * What none of this is: a measurement. Whether a wireless zone holds the 5 ms
 * multiroom bound is a distribution over a real radio, it is operator graded,
 * and docs/verification-record.md records it as NOT passed. */

#include "chorus/endpoint_config.h"
#include "chorus/telemetry.h"
#include "chorus/wifi.h"
#include "fake_radio.h"
#include "harness.h"

#define TEST_SSID "chorus-test-network"
#define TEST_SECRET "a-secret-that-is-not-in-this-repository"

static chorus_wifi_config_t a_wireless_link(chorus_wifi_ps_t mode)
{
    chorus_wifi_config_t config;
    memset(&config, 0, sizeof(config));
    config.transport = CHORUS_TRANSPORT_WIRELESS;
    config.power_save = mode;
    config.coexistence = 0;
    config.ssid_known = 1;
    snprintf(config.ssid, sizeof(config.ssid), "%s", TEST_SSID);
    config.secret_known = 1;
    snprintf(config.secret, sizeof(config.secret), "%s", TEST_SECRET);
    snprintf(config.source, sizeof(config.source), "firmware/config/endpoint.conf");
    return config;
}

/* AC-1. */
static void the_mode_is_set_rather_than_inherited(void)
{
    fake_radio_t fake;
    fake_radio_init(&fake);
    chorus_check(fake.power_save == CHORUS_WIFI_PLATFORM_DEFAULT &&
                     CHORUS_WIFI_PLATFORM_DEFAULT == CHORUS_WIFI_PS_MIN_MODEM,
                 "the simulated radio powers up in the platform default, which the carried "
                 "ESP-IDF guide states is WIFI_PS_MIN_MODEM: %s",
                 chorus_wifi_ps_name(fake.power_save));

    chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
    chorus_radio_t radio = fake_radio(&fake);
    chorus_wifi_report_t report;
    chorus_wifi_status_t status = chorus_wifi_bring_up(&config, &radio, &report);
    fake_radio_print(&fake);

    chorus_check(status == CHORUS_WIFI_OK, "a healthy bring-up succeeds: %s",
                 chorus_wifi_status_name(status));
    chorus_check(fake_radio_count(&fake, FAKE_RADIO_SET_POWER_SAVE) == 1,
                 "the mode was set exactly once, at the radio");
    chorus_check(fake.power_save == CHORUS_WIFI_PS_NONE,
                 "the radio is left in the mode the configuration declared: %s",
                 chorus_wifi_ps_name(fake.power_save));
    chorus_check(fake.power_save != CHORUS_WIFI_PLATFORM_DEFAULT,
                 "and it is NOT the platform default it would have inherited");
    chorus_check(report.declared == CHORUS_WIFI_PS_NONE && report.mode_read &&
                     report.in_force == CHORUS_WIFI_PS_NONE,
                 "the report names the mode it set and the mode the platform reports");
    chorus_check(report.mode_in_effect && report.bound_publishable && report.link_up,
                 "with the mode in force the link is up and the wireless bound is publishable");
    chorus_check(strstr(report.detail, "min-modem") != NULL,
                 "the report names the default it did not inherit: %s", report.detail);

    /* The ORDER, read off the log. The mode has to be set after the radio is
     * initialised (the carried guide: "after calling esp_wifi_init()") and
     * before the link is reported usable. */
    int at_init = fake_radio_first(&fake, FAKE_RADIO_INIT);
    int at_set = fake_radio_first(&fake, FAKE_RADIO_SET_POWER_SAVE);
    int at_get = fake_radio_first(&fake, FAKE_RADIO_GET_POWER_SAVE);
    int at_join = fake_radio_first(&fake, FAKE_RADIO_JOIN);
    chorus_check(at_init >= 0 && at_set > at_init,
                 "the mode is set AFTER the radio is initialised (%d then %d)", at_init, at_set);
    chorus_check(at_join > at_set && at_join > at_get,
                 "and it is set and read back BEFORE the join that makes the link usable (set %d, "
                 "read %d, join %d)",
                 at_set, at_get, at_join);

    /* And it is reported on the line one grep already reads. */
    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    chorus_telemetry_record_wifi(&telemetry, &report);
    telemetry.link = CHORUS_LINK_UP;
    char line[768];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    printf("     %s\n", line);
    chorus_check(strstr(line, "transport=wireless") != NULL &&
                     strstr(line, "wifi_ps_declared=none") != NULL &&
                     strstr(line, "wifi_ps_in_force=none") != NULL,
                 "the published line carries the mode it set and the mode in force");
    chorus_check(strstr(line, "wireless_bound=publishable") != NULL,
                 "and says the wireless bound may be published");
}

/* Every mode the configuration may declare reaches the radio as itself. A
 * bring-up that always set WIFI_PS_NONE would pass the test above. */
static void every_declared_mode_is_the_mode_that_is_set(void)
{
    const chorus_wifi_ps_t modes[] = {CHORUS_WIFI_PS_NONE, CHORUS_WIFI_PS_MIN_MODEM,
                                      CHORUS_WIFI_PS_MAX_MODEM};
    for (size_t i = 0; i < sizeof(modes) / sizeof(modes[0]); i++) {
        fake_radio_t fake;
        fake_radio_init(&fake);
        chorus_wifi_config_t config = a_wireless_link(modes[i]);
        chorus_radio_t radio = fake_radio(&fake);
        chorus_wifi_report_t report;
        chorus_wifi_bring_up(&config, &radio, &report);
        chorus_check(fake.power_save == modes[i] && report.in_force == modes[i],
                     "a declared `%s` is the mode the radio is left in",
                     chorus_wifi_ps_name(modes[i]));
    }

    /* The names round trip, so a configuration file and a published line cannot
     * mean different things by the same word. */
    for (size_t i = 0; i < sizeof(modes) / sizeof(modes[0]); i++) {
        int ok = 0;
        chorus_wifi_ps_t parsed = chorus_wifi_ps_from_name(chorus_wifi_ps_name(modes[i]), &ok);
        chorus_check(ok && parsed == modes[i], "`%s` round trips through the name",
                     chorus_wifi_ps_name(modes[i]));
    }
    int ok = 1;
    (void)chorus_wifi_ps_from_name("unknown", &ok);
    chorus_check(!ok,
                 "`unknown` is NOT a power save mode: this phase declares the mode rather than "
                 "leaving it unknown");
    ok = 1;
    (void)chorus_wifi_ps_from_name("off", &ok);
    chorus_check(!ok, "and neither is a word the platform does not have");
}

/* AC-10. */
static void a_platform_reporting_another_mode_reports_both_and_claims_nothing(void)
{
    fake_radio_t fake;
    fake_radio_init(&fake);
    /* The platform accepts the mode and reports another one. */
    fake.readback_overridden = 1;
    fake.readback = CHORUS_WIFI_PS_MIN_MODEM;

    chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
    chorus_radio_t radio = fake_radio(&fake);
    chorus_wifi_report_t report;
    chorus_wifi_status_t status = chorus_wifi_bring_up(&config, &radio, &report);
    fake_radio_print(&fake);

    chorus_check(status == CHORUS_WIFI_MODE_DISAGREES,
                 "the disagreement is reported by name: %s", chorus_wifi_status_name(status));
    chorus_check(report.declared == CHORUS_WIFI_PS_NONE &&
                     report.in_force == CHORUS_WIFI_PS_MIN_MODEM && report.mode_read,
                 "BOTH modes are reported: declared %s, in force %s",
                 chorus_wifi_ps_name(report.declared), chorus_wifi_ps_name(report.in_force));
    chorus_check(strstr(report.detail, "none") != NULL &&
                     strstr(report.detail, "min-modem") != NULL,
                 "and both are named in the detail a person reads: %s", report.detail);
    chorus_check(!report.bound_publishable && !report.mode_in_effect,
                 "the wireless bound is NOT claimed");

    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    chorus_telemetry_record_wifi(&telemetry, &report);
    char line[768];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    printf("     %s\n", line);
    chorus_check(strstr(line, "wifi=power-save-mode-disagrees") != NULL &&
                     strstr(line, "wifi_ps_declared=none") != NULL &&
                     strstr(line, "wifi_ps_in_force=min-modem") != NULL,
                 "the published line carries both modes and the condition by name");
    chorus_check(strstr(line, "wireless_bound=withheld") != NULL &&
                     strstr(line, "wireless_bound=publishable") == NULL,
                 "and the line does not publish the wireless bound");
}

/* AC-9, in both spellings: the board declares a coexistence, and the platform
 * reports one the board did not declare. */
static void a_coexistence_mode_means_the_mode_set_is_not_in_effect(void)
{
    /* Declared in the committed configuration. */
    fake_radio_t fake;
    fake_radio_init(&fake);
    chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
    config.coexistence = 1;
    chorus_radio_t radio = fake_radio_without_coexistence_query(&fake);
    chorus_wifi_report_t report;
    chorus_wifi_status_t status = chorus_wifi_bring_up(&config, &radio, &report);

    chorus_check(status == CHORUS_WIFI_SLEEPS_IN_COEXISTENCE,
                 "a declared coexistence is reported by name: %s",
                 chorus_wifi_status_name(status));
    chorus_check(report.mode_read && report.in_force == CHORUS_WIFI_PS_NONE,
                 "the mode WAS set and the platform DOES report it: %s",
                 chorus_wifi_ps_name(report.in_force));
    chorus_check(!report.mode_in_effect && !report.bound_publishable,
                 "and the mode is still not in effect, so the bound is not published as met");
    chorus_check(strstr(report.detail, "coexistence") != NULL &&
                     strstr(report.detail, "NOT in effect") != NULL,
                 "the report says the mode it set is not in effect: %s", report.detail);

    /* Reported by the platform, with the board declaring none. */
    fake_radio_t second;
    fake_radio_init(&second);
    second.coexistence_active = 1;
    chorus_wifi_config_t quiet = a_wireless_link(CHORUS_WIFI_PS_NONE);
    chorus_check(quiet.coexistence == 0, "the committed declaration says no coexistence");
    chorus_radio_t asking = fake_radio(&second);
    chorus_wifi_report_t second_report;
    chorus_wifi_status_t second_status = chorus_wifi_bring_up(&quiet, &asking, &second_report);
    chorus_check(second_status == CHORUS_WIFI_SLEEPS_IN_COEXISTENCE,
                 "a platform that reports a coexistence the board did not declare is believed: %s",
                 chorus_wifi_status_name(second_status));
    chorus_check(!second_report.bound_publishable, "and the bound is withheld there too");
    chorus_check(fake_radio_count(&second, FAKE_RADIO_COEXISTENCE_QUERY) == 1,
                 "the platform was asked exactly once");

    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    chorus_telemetry_record_wifi(&telemetry, &second_report);
    char line[768];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    printf("     %s\n", line);
    chorus_check(strstr(line, "wifi=sleeps-in-coexistence") != NULL &&
                     strstr(line, "wireless_bound=withheld") != NULL,
                 "the published line names the condition and withholds the bound");

    /* A coexisting endpoint still plays: the link comes up, and only the CLAIM
     * is withheld. A bring-up that refused to join here would take a house off
     * the air over a power-save mode. */
    chorus_check(second_report.link_up && second_report.join_attempted,
                 "the link still comes up: only the wireless bound is withheld");
}

/* AC-17. */
static void an_unknown_credential_refuses_to_join_and_reports_the_link_down(void)
{
    const char *keys[] = {"link_wifi_ssid", "link_wifi_secret"};
    for (size_t i = 0; i < 2; i++) {
        fake_radio_t fake;
        fake_radio_init(&fake);
        chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
        if (i == 0) {
            config.ssid_known = 0;
            config.ssid[0] = '\0';
        } else {
            config.secret_known = 0;
            config.secret[0] = '\0';
        }
        chorus_radio_t radio = fake_radio(&fake);
        chorus_wifi_report_t report;
        chorus_wifi_status_t status = chorus_wifi_bring_up(&config, &radio, &report);

        chorus_check(status == CHORUS_WIFI_CREDENTIAL_UNKNOWN,
                     "an unknown %s refuses by name: %s", keys[i],
                     chorus_wifi_status_name(status));
        chorus_check(strstr(report.detail, keys[i]) != NULL,
                     "the refusal names WHICH value is unknown: %s", report.detail);
        chorus_check(strstr(report.detail, "firmware/config/endpoint.conf") != NULL,
                     "and where to set it");
        chorus_check(!report.link_up, "the link is reported down");
        chorus_check(!report.join_attempted && fake.joins == 0 &&
                         fake_radio_count(&fake, FAKE_RADIO_JOIN) == 0,
                     "and NO join was attempted at all, so nothing was retried against a default");
        chorus_check(fake.event_count == 0,
                     "the radio was not touched at all: %zu events", fake.event_count);

        chorus_telemetry_t telemetry;
        chorus_telemetry_init(&telemetry);
        /* Start from a link that says it is up, so the assertion below is about
         * the record putting it down rather than about it never having been up. */
        telemetry.link = CHORUS_LINK_UP;
        chorus_telemetry_record_wifi(&telemetry, &report);
        char line[768];
        chorus_telemetry_line(&telemetry, line, sizeof(line));
        printf("     %s\n", line);
        chorus_check(strstr(line, "link=down") != NULL && strstr(line, "link=up") == NULL,
                     "the published line reports the link DOWN: %s", line);
        chorus_check(strstr(line, "wifi=wireless-credential-unknown") != NULL,
                     "and names the condition");
        chorus_check(strstr(line, "wireless_bound=withheld") != NULL,
                     "and claims no wireless bound");
    }
}

/* AC-17's other half: nothing published carries a secret, even when there is
 * one to carry. */
static void the_secret_reaches_the_radio_and_nothing_that_is_published(void)
{
    fake_radio_t fake;
    fake_radio_init(&fake);
    chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
    chorus_radio_t radio = fake_radio(&fake);
    chorus_wifi_report_t report;
    chorus_wifi_bring_up(&config, &radio, &report);

    chorus_check(strcmp(fake.joined_secret, TEST_SECRET) == 0 &&
                     strcmp(fake.joined_ssid, TEST_SSID) == 0,
                 "the secret reaches the radio, which is the only thing that needs it");
    chorus_check(strstr(report.detail, TEST_SECRET) == NULL,
                 "and it is nowhere in the report: %s", report.detail);

    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    chorus_telemetry_record_wifi(&telemetry, &report);
    char line[768];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    chorus_check(strstr(line, TEST_SECRET) == NULL,
                 "and nowhere in the published line either");
}

/* Every other way the platform can refuse, each by its own name, each leaving
 * the link down and claiming nothing. */
static void every_platform_refusal_has_a_name_and_claims_nothing(void)
{
    struct {
        const char *what;
        int init_refuses;
        int set_refuses;
        int get_refuses;
        int join_refuses;
        chorus_wifi_status_t expected;
    } cases[] = {
        {"the radio will not initialise", 1, 0, 0, 0, CHORUS_WIFI_INIT_REFUSED},
        {"the platform will not take the mode", 0, 1, 0, 0, CHORUS_WIFI_SET_REFUSED},
        {"the platform will not say what mode it is in", 0, 0, 1, 0,
         CHORUS_WIFI_READBACK_REFUSED},
        {"the network will not be joined", 0, 0, 0, 1, CHORUS_WIFI_JOIN_REFUSED},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        fake_radio_t fake;
        fake_radio_init(&fake);
        fake.init_refuses = cases[i].init_refuses;
        fake.set_refuses = cases[i].set_refuses;
        fake.get_refuses = cases[i].get_refuses;
        fake.join_refuses = cases[i].join_refuses;
        chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
        chorus_radio_t radio = fake_radio(&fake);
        chorus_wifi_report_t report;
        chorus_wifi_status_t status = chorus_wifi_bring_up(&config, &radio, &report);
        chorus_check(status == cases[i].expected, "%s is reported as `%s`", cases[i].what,
                     chorus_wifi_status_name(status));
        chorus_check(!report.link_up && !report.bound_publishable,
                     "the link is down and no wireless bound is claimed");
        chorus_check(report.detail[0] != '\0', "and it says why: %s", report.detail);
    }

    /* A radio that binds no call at all is the same class of failure and is not
     * a crash: the bring-up asks for nothing it was not given. */
    fake_radio_t bare;
    fake_radio_init(&bare);
    chorus_radio_t nothing;
    memset(&nothing, 0, sizeof(nothing));
    nothing.ctx = &bare;
    chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
    chorus_wifi_report_t report;
    chorus_wifi_status_t status = chorus_wifi_bring_up(&config, &nothing, &report);
    chorus_check(status == CHORUS_WIFI_SET_REFUSED && !report.bound_publishable,
                 "a radio that binds nothing refuses rather than claiming a mode: %s",
                 chorus_wifi_status_name(status));
}

/* A wired endpoint brings no radio up and has no wireless claim made about it. */
static void a_wired_endpoint_touches_no_radio(void)
{
    fake_radio_t fake;
    fake_radio_init(&fake);
    chorus_wifi_config_t config = a_wireless_link(CHORUS_WIFI_PS_NONE);
    config.transport = CHORUS_TRANSPORT_WIRED;
    chorus_radio_t radio = fake_radio(&fake);
    chorus_wifi_report_t report;
    chorus_wifi_status_t status = chorus_wifi_bring_up(&config, &radio, &report);

    chorus_check(status == CHORUS_WIFI_NOT_WIRELESS && fake.event_count == 0,
                 "a wired link touches the radio not at all: %zu events", fake.event_count);
    chorus_check(fake.power_save == CHORUS_WIFI_PLATFORM_DEFAULT,
                 "and leaves it exactly as it found it");
    chorus_check(report.link_up && !report.bound_publishable,
                 "the link is usable and no wireless bound is published about it");

    chorus_telemetry_t telemetry;
    chorus_telemetry_init(&telemetry);
    chorus_telemetry_record_wifi(&telemetry, &report);
    telemetry.link = CHORUS_LINK_UP;
    char line[768];
    chorus_telemetry_line(&telemetry, line, sizeof(line));
    printf("     %s\n", line);
    chorus_check(strstr(line, "transport=wired") != NULL &&
                     strstr(line, "wifi=not-applicable") != NULL &&
                     strstr(line, "wifi_ps_declared=not-applicable") != NULL &&
                     strstr(line, "wifi_ps_in_force=not-applicable") != NULL &&
                     strstr(line, "wireless_bound=not-applicable") != NULL,
                 "and the published line says `not-applicable` rather than omitting the fields");

    /* A telemetry surface that has never seen a bring-up says the same thing,
     * because an endpoint nobody has told which link it is on is not a wireless
     * one. */
    chorus_telemetry_t fresh;
    chorus_telemetry_init(&fresh);
    chorus_telemetry_line(&fresh, line, sizeof(line));
    chorus_check(strstr(line, "transport=wired") != NULL &&
                     strstr(line, "wireless_bound=not-applicable") != NULL,
                 "an endpoint that has published nothing about a radio claims nothing about one");
}

/* The COMMITTED configuration, read by the reader the board uses. AC-17's first
 * clause is about this repository and not about a struct a test built. */
static void the_committed_configuration_carries_no_credential(void)
{
    chorus_endpoint_config_t config;
    char detail[512];
    detail[0] = '\0';
    int loaded =
        chorus_endpoint_config_load(&config, chorus_endpoint_config_default_path(), detail,
                                    sizeof(detail));
    chorus_check(loaded == 0, "the committed endpoint configuration loads: %s", detail);
    if (loaded != 0) {
        return;
    }

    chorus_check(config.link.transport == CHORUS_TRANSPORT_WIRELESS,
                 "the committed link is declared `%s`",
                 chorus_transport_name(config.link.transport));
    chorus_check(config.link.power_save == CHORUS_WIFI_PS_NONE,
                 "the committed power save mode is `%s`",
                 chorus_wifi_ps_name(config.link.power_save));
    chorus_check(config.link.power_save != CHORUS_WIFI_PLATFORM_DEFAULT,
                 "and it is not the platform default, which is the whole of the first assertion");
    chorus_check(!config.link.ssid_known && config.link.ssid[0] == '\0',
                 "the committed network name is DECLARED UNKNOWN and carries no value");
    chorus_check(!config.link.secret_known && config.link.secret[0] == '\0',
                 "the committed network secret is DECLARED UNKNOWN and carries no value");

    /* So the shipped endpoint refuses to join, by name, as it stands. */
    fake_radio_t fake;
    fake_radio_init(&fake);
    chorus_radio_t radio = fake_radio(&fake);
    chorus_wifi_report_t report;
    chorus_wifi_status_t status = chorus_wifi_bring_up(&config.link, &radio, &report);
    chorus_check(status == CHORUS_WIFI_CREDENTIAL_UNKNOWN && fake.joins == 0,
                 "so the endpoint this repository ships refuses to join: %s", report.detail);
}

/* Every status the enum has is named, and no two share a name. A condition with
 * no name is a condition nobody can act on. */
static void every_status_has_its_own_name(void)
{
    const chorus_wifi_status_t all[] = {
        CHORUS_WIFI_OK,
        CHORUS_WIFI_NOT_WIRELESS,
        CHORUS_WIFI_CREDENTIAL_UNKNOWN,
        CHORUS_WIFI_INIT_REFUSED,
        CHORUS_WIFI_SET_REFUSED,
        CHORUS_WIFI_READBACK_REFUSED,
        CHORUS_WIFI_MODE_DISAGREES,
        CHORUS_WIFI_SLEEPS_IN_COEXISTENCE,
        CHORUS_WIFI_JOIN_REFUSED,
    };
    size_t count = sizeof(all) / sizeof(all[0]);
    int collisions = 0;
    for (size_t i = 0; i < count; i++) {
        for (size_t j = i + 1; j < count; j++) {
            if (strcmp(chorus_wifi_status_name(all[i]), chorus_wifi_status_name(all[j])) == 0) {
                collisions++;
            }
        }
    }
    chorus_check(collisions == 0, "all %zu statuses have distinct names", count);
    for (size_t i = 0; i < count; i++) {
        chorus_check(strcmp(chorus_wifi_status_name(all[i]), "unknown") != 0,
                     "status %zu is named", i);
    }
}

int main(void)
{
    chorus_section("the mode is set, not inherited");
    the_mode_is_set_rather_than_inherited();

    chorus_section("every declared mode");
    every_declared_mode_is_the_mode_that_is_set();

    chorus_section("the platform reports another mode");
    a_platform_reporting_another_mode_reports_both_and_claims_nothing();

    chorus_section("a coexistence mode");
    a_coexistence_mode_means_the_mode_set_is_not_in_effect();

    chorus_section("a credential this repository does not have");
    an_unknown_credential_refuses_to_join_and_reports_the_link_down();

    chorus_section("the secret goes to the radio and nowhere else");
    the_secret_reaches_the_radio_and_nothing_that_is_published();

    chorus_section("every platform refusal");
    every_platform_refusal_has_a_name_and_claims_nothing();

    chorus_section("a wired endpoint");
    a_wired_endpoint_touches_no_radio();

    chorus_section("the committed configuration");
    the_committed_configuration_carries_no_credential();

    chorus_section("the conditions are all named");
    every_status_has_its_own_name();

    return chorus_test_report("endpoint wireless bring-up");
}
