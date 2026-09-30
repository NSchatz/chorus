/* The endpoint's link, P1 as approved at Checkpoint K.
 *
 * Graded here, on a machine with no SPI controller and no radio:
 *
 *   - the committed configuration's link is WIRED (the default for every
 *     wired class; audit A-14), and every board profile reads the way the
 *     image reads it, naming its board model and, while that model is
 *     ASSUMED, the Needs item it waits on;
 *   - a wired endpoint brings up the W5500 (init, start, an address) and never
 *     touches the radio; a wireless one runs the Wi-Fi tier unchanged and
 *     never touches the W5500;
 *   - a wired pin map that breaks a rule (no INT line, a reserved pin, a pin
 *     shared with the audio path, a clock past the W5500's or the GPIO
 *     matrix's limit) is refused by name before anything is touched;
 *   - each way the controller can fail is reported by its own name.
 *
 * The fake Ethernet controller keeps its own log of calls, and the checks read
 * that log and the fake radio's, never the bring-up's account of itself. */

#include "chorus/endpoint_config.h"
#include "chorus/link.h"
#include "fake_radio.h"
#include "harness.h"

#define NEEDS_ITEM "Your ESP32-S3 boards: module markings and a read-only chip report"

typedef struct {
    int inits;
    int starts;
    int waits;
    int init_refuses;
    int start_refuses;
    int address_never_arrives;
    chorus_eth_config_t given;
    uint32_t waited_ms;
} fake_ethernet_t;

static int fake_init(void *ctx, const chorus_eth_config_t *config)
{
    fake_ethernet_t *fake = ctx;
    fake->inits++;
    fake->given = *config;
    return fake->init_refuses ? -1 : 0;
}

static int fake_start(void *ctx)
{
    fake_ethernet_t *fake = ctx;
    fake->starts++;
    return fake->start_refuses ? -1 : 0;
}

static int fake_wait(void *ctx, uint32_t timeout_ms)
{
    fake_ethernet_t *fake = ctx;
    fake->waits++;
    fake->waited_ms = timeout_ms;
    return fake->address_never_arrives ? -1 : 0;
}

static chorus_ethernet_t fake_ethernet(fake_ethernet_t *fake)
{
    memset(fake, 0, sizeof(*fake));
    chorus_ethernet_t eth = {fake, fake_init, fake_start, fake_wait};
    return eth;
}

static chorus_endpoint_config_t committed;
static chorus_endpoint_config_t profiled;

static int load_committed(void)
{
    char detail[512];
    detail[0] = '\0';
    int ok = chorus_endpoint_config_load(&committed, chorus_endpoint_config_default_path(), detail,
                                         sizeof(detail)) == 0;
    chorus_check(ok, "the committed endpoint configuration loads: %s", detail);
    return ok;
}

static int load_profile(const char *name)
{
    char profile[1024];
    char relative[256];
    snprintf(relative, sizeof(relative), "firmware/boards/%s.conf", name);
    chorus_repo_path(profile, sizeof(profile), relative);
    char detail[512];
    detail[0] = '\0';
    int ok = chorus_endpoint_config_load_profile(&profiled, chorus_endpoint_config_default_path(),
                                                 profile, detail, sizeof(detail)) == 0;
    chorus_check(ok, "board profile %s loads over endpoint.conf: %s", name, detail);
    return ok;
}

static void the_committed_link_is_wired_and_the_board_is_named(void)
{
    chorus_section("the committed configuration: the wired default and the board");
    if (!load_committed()) {
        return;
    }
    chorus_check(committed.link.transport == CHORUS_TRANSPORT_WIRED,
                 "the committed link is `%s`, the default for every wired class",
                 chorus_transport_name(committed.link.transport));
    chorus_check(strcmp(committed.board.profile, "brick-s3-wired") == 0,
                 "endpoint.conf carries the default board profile: %s", committed.board.profile);
    chorus_check(committed.board.model_status == CHORUS_BOARD_ASSUMED &&
                     strcmp(committed.board.needs_item, NEEDS_ITEM) == 0,
                 "the board model \"%s\" is %s and names its Needs item \"%s\"",
                 committed.board.model, chorus_board_status_name(committed.board.model_status),
                 committed.board.needs_item);
    chorus_check(committed.eth.int_pin != CHORUS_PIN_NONE && committed.eth.int_pin == 6,
                 "the W5500's INT line is wired (GPIO%u)", committed.eth.int_pin);

    chorus_finding_t findings[32];
    size_t count = 0;
    chorus_endpoint_config_validate(&committed, findings, 32, &count);
    for (size_t i = 0; i < count; i++) {
        printf("  finding %s :: %s\n", findings[i].rule, findings[i].detail);
    }
    chorus_check(count == 0, "the committed configuration breaks no rule (%zu findings)", count);
}

static void every_profile_reads_as_the_image_reads_it(void)
{
    chorus_section("board profiles");
    if (load_profile("brick-s3-wired")) {
        chorus_check(profiled.link.transport == CHORUS_TRANSPORT_WIRED &&
                         strcmp(profiled.board.profile, "brick-s3-wired") == 0,
                     "brick-s3-wired: link %s", chorus_transport_name(profiled.link.transport));
        chorus_check(profiled.board.model_status == CHORUS_BOARD_ASSUMED &&
                         strcmp(profiled.board.needs_item, NEEDS_ITEM) == 0,
                     "brick-s3-wired: model \"%s\" %s, waiting on \"%s\"", profiled.board.model,
                     chorus_board_status_name(profiled.board.model_status),
                     profiled.board.needs_item);
    }
    if (load_profile("compact-s3-wifi")) {
        chorus_check(profiled.link.transport == CHORUS_TRANSPORT_WIRELESS &&
                         strcmp(profiled.board.profile, "compact-s3-wifi") == 0,
                     "compact-s3-wifi: link %s, the compact speakers' Wi-Fi tier (K91)",
                     chorus_transport_name(profiled.link.transport));
        chorus_check(profiled.link.power_save == CHORUS_WIFI_PS_NONE,
                     "compact-s3-wifi: the power save mode it sets is `%s`, from endpoint.conf",
                     chorus_wifi_ps_name(profiled.link.power_save));
        chorus_check(profiled.board.model_status == CHORUS_BOARD_ASSUMED &&
                         strcmp(profiled.board.needs_item, NEEDS_ITEM) == 0,
                     "compact-s3-wifi: model \"%s\" %s, waiting on \"%s\"", profiled.board.model,
                     chorus_board_status_name(profiled.board.model_status),
                     profiled.board.needs_item);
        chorus_finding_t findings[32];
        size_t count = 0;
        chorus_endpoint_config_validate(&profiled, findings, 32, &count);
        chorus_check(count == 0, "compact-s3-wifi breaks no rule (%zu findings)", count);
    }

    /* A profile may set only board keys, and only keys the base carries. */
    char detail[512];
    static char base[16384];
    FILE *file = fopen(chorus_endpoint_config_default_path(), "rb");
    size_t read = file ? fread(base, 1, sizeof(base) - 1, file) : 0;
    if (file) {
        fclose(file);
    }
    base[read] = '\0';
    chorus_check(chorus_endpoint_config_parse_profile(&profiled, "endpoint.conf", base, "smuggled",
                                                      "board_profile = x\nespidf_version = v9\n",
                                                      detail, sizeof(detail)) != 0 &&
                     strstr(detail, "espidf_version") != NULL,
                 "a profile that sets a platform key is refused by name: %s", detail);
    chorus_check(chorus_endpoint_config_parse_profile(&profiled, "endpoint.conf", base, "smuggled",
                                                      "board_profile = x\nlink_wifi_secret = s\n",
                                                      detail, sizeof(detail)) != 0,
                 "a profile that carries a credential is refused: %s", detail);
    chorus_check(chorus_endpoint_config_parse_profile(&profiled, "endpoint.conf", base, "smuggled",
                                                      "board_profile = x\npin_eth_nope = 4\n",
                                                      detail, sizeof(detail)) != 0,
                 "a profile that sets a key the base does not carry is refused: %s", detail);
}

static void a_wired_endpoint_never_touches_the_radio(void)
{
    chorus_section("wired: the W5500, never the radio");
    fake_ethernet_t eth_log;
    chorus_ethernet_t eth = fake_ethernet(&eth_log);
    fake_radio_t radio_log;
    fake_radio_init(&radio_log);
    chorus_radio_t radio = fake_radio(&radio_log);
    chorus_link_report_t report;
    chorus_bring_up_status_t status = chorus_link_bring_up(&committed.link, &committed.eth,
                                                           &committed.pins, &eth, &radio, &report);
    chorus_check(status == CHORUS_BRING_UP_OK && report.link_up, "the wired link comes up: %s",
                 report.detail);
    chorus_check(eth_log.inits == 1 && eth_log.starts == 1 && eth_log.waits == 1,
                 "the W5500 was initialised, started and waited on once each (%d, %d, %d)",
                 eth_log.inits, eth_log.starts, eth_log.waits);
    chorus_check(eth_log.given.int_pin == committed.eth.int_pin &&
                     eth_log.given.cs == committed.eth.cs &&
                     eth_log.waited_ms == committed.eth.address_timeout_ms,
                 "the controller was given the committed wiring (INT GPIO%u, CS GPIO%u) and the "
                 "committed address timeout (%u ms)",
                 eth_log.given.int_pin, eth_log.given.cs, eth_log.waited_ms);
    chorus_check(radio_log.event_count == 0 && radio_log.joins == 0 && !report.radio_touched,
                 "the radio saw nothing: %zu events, %zu joins", radio_log.event_count,
                 radio_log.joins);
    chorus_check(report.wifi.status == CHORUS_WIFI_NOT_WIRELESS && !report.wifi.bound_publishable,
                 "and no wireless claim is made about a wired endpoint (%s)",
                 chorus_wifi_status_name(report.wifi.status));
}

static void a_wireless_endpoint_never_touches_the_w5500(void)
{
    chorus_section("wireless: the Wi-Fi tier, never the W5500");
    if (!load_profile("compact-s3-wifi")) {
        return;
    }
    chorus_wifi_config_t link = profiled.link;
    link.ssid_known = 1;
    snprintf(link.ssid, sizeof(link.ssid), "chorus-test-network");
    link.secret_known = 1;
    snprintf(link.secret, sizeof(link.secret), "a-secret-that-is-not-in-this-repository");

    fake_ethernet_t eth_log;
    chorus_ethernet_t eth = fake_ethernet(&eth_log);
    fake_radio_t radio_log;
    fake_radio_init(&radio_log);
    chorus_radio_t radio = fake_radio(&radio_log);
    chorus_link_report_t report;
    chorus_bring_up_status_t status =
        chorus_link_bring_up(&link, &profiled.eth, &profiled.pins, &eth, &radio, &report);
    chorus_check(status == CHORUS_BRING_UP_OK && radio_log.joins == 1 &&
                     radio_log.power_save == CHORUS_WIFI_PS_NONE,
                 "the Wi-Fi tier comes up as before, modem sleep off: %s", report.detail);
    chorus_check(eth_log.inits == 0 && eth_log.starts == 0 && eth_log.waits == 0 &&
                     !report.ethernet_touched,
                 "the W5500 saw nothing (%d inits, %d starts, %d waits)", eth_log.inits,
                 eth_log.starts, eth_log.waits);

    /* As shipped, with the credentials declared unknown: refused by name,
     * still without the W5500. */
    fake_radio_init(&radio_log);
    status =
        chorus_link_bring_up(&profiled.link, &profiled.eth, &profiled.pins, &eth, &radio, &report);
    chorus_check(status == CHORUS_BRING_UP_WIRELESS_DOWN &&
                     report.wifi.status == CHORUS_WIFI_CREDENTIAL_UNKNOWN && eth_log.inits == 0,
                 "the shipped Wi-Fi profile refuses to join on unknown credentials: %s",
                 report.wifi.detail);
}

static void refused(const char *what, chorus_eth_config_t eth, const char *rule)
{
    fake_ethernet_t eth_log;
    chorus_ethernet_t ethernet = fake_ethernet(&eth_log);
    fake_radio_t radio_log;
    fake_radio_init(&radio_log);
    chorus_radio_t radio = fake_radio(&radio_log);
    chorus_link_report_t report;
    chorus_bring_up_status_t status =
        chorus_link_bring_up(&committed.link, &eth, &committed.pins, &ethernet, &radio, &report);
    chorus_check(status == CHORUS_BRING_UP_CONFIG_REFUSED && strstr(report.detail, rule) != NULL &&
                     eth_log.inits == 0 && radio_log.event_count == 0,
                 "%s is refused as %s, nothing touched: %s", what, rule, report.detail);
}

static void a_wired_pin_map_that_breaks_a_rule_is_refused_by_name(void)
{
    chorus_section("wired pin rules");
    chorus_eth_config_t eth = committed.eth;
    eth.int_pin = CHORUS_PIN_NONE;
    refused("a W5500 without its INT line", eth, "w5500-int-not-wired");

    eth = committed.eth;
    eth.cs = 30;
    refused("a chip select on a flash and PSRAM pin", eth, "gpio-reserved-for-flash-and-psram");

    eth = committed.eth;
    eth.int_pin = 35;
    refused("an INT line on an octal PSRAM pin", eth, "gpio-reserved-for-octal-flash-or-psram");

    eth = committed.eth;
    eth.cs = committed.pins.bclk;
    refused("a chip select shared with the I2S bit clock", eth, "gpio-assigned-twice");

    eth = committed.eth;
    eth.miso = eth.mosi;
    refused("MISO and MOSI on one pin", eth, "gpio-assigned-twice");

    eth = committed.eth;
    eth.sclk = CHORUS_PIN_NONE;
    refused("an unrouted SPI clock", eth, "w5500-spi-pin-not-routed");

    eth = committed.eth;
    eth.spi_clock_mhz = 81;
    refused("an SPI clock past the W5500's 80 MHz", eth, "w5500-spi-clock-out-of-range");

    eth = committed.eth;
    eth.spi_clock_mhz = 60;
    eth.cs = 7;
    refused("60 MHz through the GPIO matrix", eth, "spi-clock-above-gpio-matrix-limit");

    eth = committed.eth;
    eth.spi_clock_mhz = 80;
    chorus_finding_t findings[16];
    size_t count = 0;
    chorus_link_validate(&committed.link, &eth, &committed.pins, findings, 16, &count);
    chorus_check(count == 0, "80 MHz on the SPI2 IO_MUX pins (10 to 13) is allowed (%zu findings)",
                 count);

    eth = committed.eth;
    eth.rst = CHORUS_PIN_NONE;
    count = 0;
    chorus_link_validate(&committed.link, &eth, &committed.pins, findings, 16, &count);
    chorus_check(count == 0, "a reset line the board ties (none) is allowed (%zu findings)", count);

    /* A wireless link drives no W5500 pin, so none of those rules apply. */
    chorus_wifi_config_t wireless = committed.link;
    wireless.transport = CHORUS_TRANSPORT_WIRELESS;
    eth = committed.eth;
    eth.int_pin = CHORUS_PIN_NONE;
    count = 0;
    chorus_link_validate(&wireless, &eth, &committed.pins, findings, 16, &count);
    chorus_check(count == 0, "a wireless link is not held to the W5500's wiring (%zu findings)",
                 count);
}

static void each_controller_failure_has_its_own_name(void)
{
    chorus_section("controller failures");
    const struct {
        int init, start, address;
        chorus_bring_up_status_t want;
    } cases[] = {
        {1, 0, 0, CHORUS_BRING_UP_ETH_INIT_REFUSED},
        {0, 1, 0, CHORUS_BRING_UP_ETH_START_REFUSED},
        {0, 0, 1, CHORUS_BRING_UP_NO_ADDRESS},
    };
    for (size_t i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        fake_ethernet_t eth_log;
        chorus_ethernet_t eth = fake_ethernet(&eth_log);
        eth_log.init_refuses = cases[i].init;
        eth_log.start_refuses = cases[i].start;
        eth_log.address_never_arrives = cases[i].address;
        fake_radio_t radio_log;
        fake_radio_init(&radio_log);
        chorus_radio_t radio = fake_radio(&radio_log);
        chorus_link_report_t report;
        chorus_bring_up_status_t status = chorus_link_bring_up(
            &committed.link, &committed.eth, &committed.pins, &eth, &radio, &report);
        chorus_check(status == cases[i].want && !report.link_up && radio_log.event_count == 0,
                     "%s is reported as itself, link down, radio untouched: %s",
                     chorus_bring_up_status_name(cases[i].want), report.detail);
    }
    const chorus_bring_up_status_t all[] = {
        CHORUS_BRING_UP_OK,
        CHORUS_BRING_UP_CONFIG_REFUSED,
        CHORUS_BRING_UP_ETH_INIT_REFUSED,
        CHORUS_BRING_UP_ETH_START_REFUSED,
        CHORUS_BRING_UP_NO_ADDRESS,
        CHORUS_BRING_UP_WIRELESS_DOWN,
    };
    int distinct = 1;
    for (size_t i = 0; i < 6; i++) {
        for (size_t j = i + 1; j < 6; j++) {
            if (strcmp(chorus_bring_up_status_name(all[i]), chorus_bring_up_status_name(all[j])) ==
                0) {
                distinct = 0;
            }
        }
    }
    chorus_check(distinct, "every link status has its own name");
}

int main(void)
{
    if (load_committed()) {
        the_committed_link_is_wired_and_the_board_is_named();
        every_profile_reads_as_the_image_reads_it();
        a_wired_endpoint_never_touches_the_radio();
        a_wireless_endpoint_never_touches_the_w5500();
        a_wired_pin_map_that_breaks_a_rule_is_refused_by_name();
        each_controller_failure_has_its_own_name();
    }
    return chorus_test_report("test_link");
}
