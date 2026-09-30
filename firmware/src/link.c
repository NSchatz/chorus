#include "chorus/link.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>

const char *chorus_spi_host_name(chorus_spi_host_t host)
{
    switch (host) {
    case CHORUS_SPI_HOST_SPI2:
        return "spi2";
    case CHORUS_SPI_HOST_SPI3:
        return "spi3";
    case CHORUS_SPI_HOST_UNKNOWN:
        break;
    }
    return "unknown";
}

chorus_spi_host_t chorus_spi_host_from_name(const char *name)
{
    if (strcmp(name, "spi2") == 0) {
        return CHORUS_SPI_HOST_SPI2;
    }
    if (strcmp(name, "spi3") == 0) {
        return CHORUS_SPI_HOST_SPI3;
    }
    return CHORUS_SPI_HOST_UNKNOWN;
}

const char *chorus_bring_up_status_name(chorus_bring_up_status_t status)
{
    switch (status) {
    case CHORUS_BRING_UP_OK:
        return "up";
    case CHORUS_BRING_UP_CONFIG_REFUSED:
        return "config-refused";
    case CHORUS_BRING_UP_ETH_INIT_REFUSED:
        return "ethernet-init-refused";
    case CHORUS_BRING_UP_ETH_START_REFUSED:
        return "ethernet-start-refused";
    case CHORUS_BRING_UP_NO_ADDRESS:
        return "no-address";
    case CHORUS_BRING_UP_WIRELESS_DOWN:
        return "wireless-down";
    }
    return "unnamed";
}

static void add_finding(chorus_finding_t *findings, size_t capacity, size_t *count,
                        const char *rule, const char *fmt, ...)
{
    if (*count >= capacity) {
        return;
    }
    chorus_finding_t *finding = &findings[*count];
    snprintf(finding->rule, sizeof(finding->rule), "%s", rule);
    va_list args;
    va_start(args, fmt);
    vsnprintf(finding->detail, sizeof(finding->detail), fmt, args);
    va_end(args);
    (*count)++;
}

typedef struct {
    const char *name;
    uint32_t pin;
} named_pin_t;

/* The SPI2 host's IO_MUX pins on an ESP32-S3 (quad mode), from the pinned
 * ESP-IDF v6.1 tree, components/esp_hal_gpspi/esp32s3/include/soc/spi_pins.h:
 * SPI2_IOMUX_PIN_NUM_CS 10, _MOSI 11, _CLK 12, _MISO 13 (read 2026-09-30). Only
 * these reach 80 MHz; every other routing goes through the GPIO matrix. */
static int on_spi2_iomux(const chorus_eth_config_t *eth)
{
    return eth->spi_host == CHORUS_SPI_HOST_SPI2 && eth->cs == 10 && eth->mosi == 11 &&
           eth->sclk == 12 && eth->miso == 13;
}

size_t chorus_link_validate(const chorus_wifi_config_t *link, const chorus_eth_config_t *eth,
                            const chorus_pin_map_t *audio, chorus_finding_t *findings,
                            size_t capacity, size_t *count)
{
    size_t before = *count;
    if (link->transport != CHORUS_TRANSPORT_WIRED) {
        return 0;
    }

    const named_pin_t spi[] = {
        {"pin_eth_sclk", eth->sclk}, {"pin_eth_mosi", eth->mosi}, {"pin_eth_miso", eth->miso},
        {"pin_eth_cs", eth->cs},     {"pin_eth_int", eth->int_pin}, {"pin_eth_rst", eth->rst},
    };
    const size_t spi_count = sizeof(spi) / sizeof(spi[0]);

    for (size_t i = 0; i < spi_count; i++) {
        chorus_gpio_validate(spi[i].name, spi[i].pin, audio->octal_psram, findings, capacity,
                             count);
    }
    for (size_t i = 0; i < 4; i++) {
        if (spi[i].pin == CHORUS_PIN_NONE) {
            add_finding(findings, capacity, count, "w5500-spi-pin-not-routed",
                        "%s = none: a wired link talks to the W5500 over SPI, and SPI needs its "
                        "clock, both data lines and a chip select",
                        spi[i].name);
        }
    }
    if (eth->int_pin == CHORUS_PIN_NONE) {
        add_finding(findings, capacity, count, "w5500-int-not-wired",
                    "pin_eth_int = none: without its interrupt line the W5500 driver polls, and "
                    "P1 (docs/proposals/P1-embedded-platform.md, approved at Checkpoint K) takes "
                    "the W5500 \"wired with INT\"; the polling mode carries a LEAD of 1.32 s "
                    "responses. Route the INT line to a GPIO and name it here");
    }

    /* One signal per pin, across the Ethernet pins and against the audio pins
     * the same board profile names. */
    const named_pin_t audio_pins[] = {
        {"pin_i2s_mclk", audio->mclk}, {"pin_i2s_bclk", audio->bclk},
        {"pin_i2s_ws", audio->ws},     {"pin_i2s_dout", audio->dout},
        {"pin_i2c_sda", audio->sda},   {"pin_i2c_scl", audio->scl},
        {"pin_amp_power_down", audio->amp_power_down},
    };
    const size_t audio_count = sizeof(audio_pins) / sizeof(audio_pins[0]);
    for (size_t i = 0; i < spi_count; i++) {
        if (spi[i].pin == CHORUS_PIN_NONE) {
            continue;
        }
        for (size_t j = i + 1; j < spi_count; j++) {
            if (spi[i].pin == spi[j].pin) {
                add_finding(findings, capacity, count, "gpio-assigned-twice",
                            "%s and %s are both GPIO%u", spi[i].name, spi[j].name, spi[i].pin);
            }
        }
        for (size_t j = 0; j < audio_count; j++) {
            if (spi[i].pin == audio_pins[j].pin) {
                add_finding(findings, capacity, count, "gpio-assigned-twice",
                            "%s and %s are both GPIO%u", spi[i].name, audio_pins[j].name,
                            spi[i].pin);
            }
        }
    }

    if (eth->spi_host == CHORUS_SPI_HOST_UNKNOWN) {
        add_finding(findings, capacity, count, "w5500-spi-host-unknown",
                    "eth_spi_host names no SPI host; it is `spi2` or `spi3` (SPI0 and SPI1 carry "
                    "the flash and PSRAM)");
    }
    if (eth->spi_clock_mhz == 0 || eth->spi_clock_mhz > CHORUS_W5500_MAX_SPI_MHZ) {
        add_finding(findings, capacity, count, "w5500-spi-clock-out-of-range",
                    "eth_spi_clock_mhz = %u is outside 1 to %u MHz. WIZnet: \"SPI (Serial "
                    "Peripheral Interface) up to 80MHz\" (docs.wiznet.io, read 2026-09-30)",
                    eth->spi_clock_mhz, CHORUS_W5500_MAX_SPI_MHZ);
    } else if (eth->spi_clock_mhz > CHORUS_SPI_GPIO_MATRIX_MAX_MHZ && !on_spi2_iomux(eth)) {
        add_finding(findings, capacity, count, "spi-clock-above-gpio-matrix-limit",
                    "eth_spi_clock_mhz = %u on pins routed through the GPIO matrix. ESP-IDF (spi "
                    "master reference): the GPIO matrix \"Allows signals with clock frequencies "
                    "only up to 40 MHz, as opposed to 80 MHz if IO_MUX pins are used\"",
                    eth->spi_clock_mhz);
    }
    if (eth->address_timeout_ms == 0) {
        add_finding(findings, capacity, count, "link-address-timeout-is-zero",
                    "link_address_timeout_ms = 0 gives the network no time to hand out an "
                    "address");
    }
    return *count - before;
}

chorus_bring_up_status_t chorus_link_bring_up(const chorus_wifi_config_t *link,
                                          const chorus_eth_config_t *eth,
                                          const chorus_pin_map_t *audio,
                                          chorus_ethernet_t *ethernet, chorus_radio_t *radio,
                                          chorus_link_report_t *report)
{
    memset(report, 0, sizeof(*report));
    report->transport = link->transport;

    if (link->transport == CHORUS_TRANSPORT_WIRELESS) {
        /* The Wi-Fi tier, exactly as it was before a wired link existed. The
         * Ethernet controller is not so much as initialised. */
        chorus_wifi_status_t wifi = chorus_wifi_bring_up(link, radio, &report->wifi);
        report->radio_touched = (wifi != CHORUS_WIFI_NOT_WIRELESS &&
                                 wifi != CHORUS_WIFI_CREDENTIAL_UNKNOWN);
        report->link_up = report->wifi.link_up;
        report->status = report->link_up ? CHORUS_BRING_UP_OK : CHORUS_BRING_UP_WIRELESS_DOWN;
        snprintf(report->detail, sizeof(report->detail), "link=wireless status=%s wifi=%s",
                 chorus_bring_up_status_name(report->status), chorus_wifi_status_name(wifi));
        return report->status;
    }

    /* Wired. The wireless bring-up is asked too, because it is what says, for
     * the telemetry line, that a wired endpoint publishes no wireless claim;
     * it returns before touching the radio for any transport but wireless. */
    (void)chorus_wifi_bring_up(link, radio, &report->wifi);

    chorus_finding_t findings[16];
    size_t count = 0;
    chorus_link_validate(link, eth, audio, findings, 16, &count);
    if (count > 0) {
        report->status = CHORUS_BRING_UP_CONFIG_REFUSED;
        snprintf(report->detail, sizeof(report->detail),
                 "link=wired status=%s rule=%s :: %s (%zu finding%s; nothing was touched)",
                 chorus_bring_up_status_name(report->status), findings[0].rule, findings[0].detail,
                 count, count == 1 ? "" : "s");
        return report->status;
    }

    report->ethernet_touched = 1;
    if (ethernet->init == NULL || ethernet->init(ethernet->ctx, eth) != 0) {
        report->status = CHORUS_BRING_UP_ETH_INIT_REFUSED;
    } else if (ethernet->start == NULL || ethernet->start(ethernet->ctx) != 0) {
        report->status = CHORUS_BRING_UP_ETH_START_REFUSED;
    } else if (ethernet->wait_for_address == NULL ||
               ethernet->wait_for_address(ethernet->ctx, eth->address_timeout_ms) != 0) {
        report->status = CHORUS_BRING_UP_NO_ADDRESS;
    } else {
        report->status = CHORUS_BRING_UP_OK;
        report->link_up = 1;
    }
    snprintf(report->detail, sizeof(report->detail),
             "link=wired phy=w5500 status=%s spi=%s clock_mhz=%u sclk=%u mosi=%u miso=%u cs=%u "
             "int=%u address_timeout_ms=%u",
             chorus_bring_up_status_name(report->status), chorus_spi_host_name(eth->spi_host),
             (unsigned)eth->spi_clock_mhz, (unsigned)eth->sclk, (unsigned)eth->mosi,
             (unsigned)eth->miso, (unsigned)eth->cs, (unsigned)eth->int_pin,
             (unsigned)eth->address_timeout_ms);
    return report->status;
}
