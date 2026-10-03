/* The endpoint's network link: which one, whether its pins are allowed, and
 * the order it is brought up in.
 *
 * P1 as approved at Checkpoint K (docs/proposals/P1-embedded-platform.md):
 * every speaker is an ESP32-S3; the wired classes reach the network through a
 * WIZnet W5500 on SPI with its interrupt line wired, and that is the DEFAULT
 * link; the compact speakers may instead run the native Wi-Fi tier (K91). The
 * ESP32-S3 has no Ethernet MAC of its own, so SPI is the only wired path.
 *
 * A third link exists for one board that is not a speaker (goal 14): the
 * emulator's. QEMU's esp32s3 machine has no SPI peripheral and no radio, and
 * offers the OpenCores Ethernet controller instead, so the emulated board's
 * profile declares `link_transport = emulated` and the same three calls
 * (init, start, wait for an address) run on that controller. Nothing about a
 * wired or a wireless endpoint changes.
 *
 * Everything here is a DECISION and is graded on a host by
 * firmware/tests/test_link.c against a fake Ethernet controller and the fake
 * radio: a wired endpoint never brings the radio up, a wireless one never
 * touches the SPI Ethernet controller, and a wired pin map that breaks a rule
 * is refused by name before anything is touched. The binding to ESP-IDF's
 * esp_eth and the pinned espressif/w5500 driver is firmware/main/esp_link.c,
 * which is compiled only by ESP-IDF and is not claimed by this repository. */

#ifndef CHORUS_LINK_H
#define CHORUS_LINK_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/i2s.h"
#include "chorus/wifi.h"

/* The two SPI hosts an ESP32-S3 offers a peripheral (SPI0 and SPI1 belong to
 * the flash and PSRAM). Named rather than numbered in the configuration,
 * because ESP-IDF's own enum starts at SPI1_HOST = 0 and a bare number is a
 * place to be off by one. */
typedef enum {
    CHORUS_SPI_HOST_UNKNOWN = 0,
    CHORUS_SPI_HOST_SPI2,
    CHORUS_SPI_HOST_SPI3
} chorus_spi_host_t;

const char *chorus_spi_host_name(chorus_spi_host_t host);
/* CHORUS_SPI_HOST_UNKNOWN for a name the format does not define. */
chorus_spi_host_t chorus_spi_host_from_name(const char *name);

/* The W5500's wiring. A pin the board does not route is CHORUS_PIN_NONE. */
typedef struct {
    chorus_spi_host_t spi_host;
    uint32_t spi_clock_mhz;
    uint32_t sclk;
    uint32_t mosi;
    uint32_t miso;
    uint32_t cs;
    /* The interrupt line. Required: without it the driver polls, and P1 records
     * a LEAD of 1.32 s responses in that mode. */
    uint32_t int_pin;
    /* The hardware reset line; CHORUS_PIN_NONE when the board ties it. */
    uint32_t rst;
    /* How long the wired bring-up waits for the network to hand it an address
     * before it reports the link down. */
    uint32_t address_timeout_ms;
} chorus_eth_config_t;

/* The highest SPI clock the W5500 accepts: WIZnet, "SPI (Serial Peripheral
 * Interface) up to 80MHz", https://docs.wiznet.io/Product/Chip/Ethernet/W5500,
 * read 2026-09-30. */
#define CHORUS_W5500_MAX_SPI_MHZ 80u

/* The highest SPI clock ESP-IDF allows through the GPIO matrix: "Allows
 * signals with clock frequencies only up to 40 MHz, as opposed to 80 MHz if
 * IO_MUX pins are used" (ESP-IDF v6.1 docs/en/api-reference/peripherals/
 * spi_master.rst, "GPIO Matrix and IO_MUX", read 2026-09-30 from the pinned
 * tree). */
#define CHORUS_SPI_GPIO_MATRIX_MAX_MHZ 40u

/* Append a finding for every rule the link breaks. For a wired link: every
 * W5500 pin passes the GPIO rules of chorus/i2s.h, SCLK, MOSI, MISO and CS are
 * routed, the INT line is routed, no pin is shared with another W5500 signal
 * or with the audio pins, the SPI host is named, and the clock is inside the
 * W5500's limit and the GPIO matrix's. A wireless link carries no Ethernet
 * rule: its W5500 pins are never driven. An emulated link carries one, the
 * address timeout: its controller has no pins. */
size_t chorus_link_validate(const chorus_wifi_config_t *link, const chorus_eth_config_t *eth,
                            const chorus_pin_map_t *audio, chorus_finding_t *findings,
                            size_t capacity, size_t *count);

/* The SPI Ethernet controller, as the wired bring-up takes it. Each call
 * returns 0 on success. */
typedef struct {
    void *ctx;
    /* Create the SPI bus, the W5500 MAC and PHY, and attach it to the network
     * stack, from `config`. */
    int (*init)(void *ctx, const chorus_eth_config_t *config);
    /* Start the controller; the link is negotiated after this. */
    int (*start)(void *ctx);
    /* Block until the network stack has an address on this interface, or
     * `timeout_ms` passes. 0 when an address arrived. */
    int (*wait_for_address)(void *ctx, uint32_t timeout_ms);
} chorus_ethernet_t;

typedef enum {
    CHORUS_BRING_UP_OK = 0,
    /* The configuration breaks a link rule; nothing was touched. */
    CHORUS_BRING_UP_CONFIG_REFUSED,
    /* The controller could not be created or attached. */
    CHORUS_BRING_UP_ETH_INIT_REFUSED,
    /* The controller would not start. */
    CHORUS_BRING_UP_ETH_START_REFUSED,
    /* The link came up and no address arrived in time. */
    CHORUS_BRING_UP_NO_ADDRESS,
    /* The wireless tier's bring-up left the link down; its own status says
     * why (chorus/wifi.h). */
    CHORUS_BRING_UP_WIRELESS_DOWN
} chorus_bring_up_status_t;

const char *chorus_bring_up_status_name(chorus_bring_up_status_t status);

#define CHORUS_LINK_DETAIL 512

typedef struct {
    chorus_bring_up_status_t status;
    chorus_transport_t transport;
    int link_up;
    /* Whether each side was touched at all, so a test reads the decision off
     * the report as well as off the fakes. */
    int ethernet_touched;
    int radio_touched;
    /* The wireless bring-up's own report. For a wired link it is the report
     * chorus_wifi_bring_up gives a wired configuration (`not-applicable`,
     * nothing touched), so the telemetry line reads the same either way. */
    chorus_wifi_report_t wifi;
    char detail[CHORUS_LINK_DETAIL];
} chorus_link_report_t;

/* Bring the link up in the order its transport needs.
 *
 * Wired: the link rules are checked first and a refusal touches nothing;
 * then init, start and the wait for an address, each refused by name. The
 * radio is never touched: no init, no mode, no join.
 *
 * Wireless: chorus_wifi_bring_up, unchanged (the Wi-Fi tier, K91). The
 * Ethernet controller is never touched.
 *
 * Emulated: init, start and the wait for an address on the controller the
 * caller bound (the emulator's), each refused by the same names. The radio is
 * never touched and no W5500 pin rule applies. */
chorus_bring_up_status_t chorus_link_bring_up(const chorus_wifi_config_t *link,
                                              const chorus_eth_config_t *eth,
                                              const chorus_pin_map_t *audio,
                                              chorus_ethernet_t *ethernet, chorus_radio_t *radio,
                                              chorus_link_report_t *report);

/* Where an emulated board's server is.
 *
 * The emulator's user network carries no multicast, so the DNS-SD browse
 * (chorus/discovery.h) finds nothing there, and the committed server_address is
 * loopback, which is the guest itself. What that network does give is a
 * gateway that IS the host: QEMU's documentation of user networking draws the
 * guest behind a "Firewall/DHCP server" at the address the lease names as the
 * router (https://www.qemu.org/docs/master/system/devices/net.html, read
 * 2026-10-02), and a TCP connection to that address reaches a listener on the
 * host's loopback (tried in the pinned emulator, docs/decisions/0109-*).
 * So the emulated board's server is the gateway its address lease named, at
 * the port the committed server_address names, and no address is written in
 * any file.
 *
 * Writes `<gateway>:<port of configured>` into `out` and returns 0. Returns -1,
 * with `out` empty, when `gateway` is not a dotted IPv4 address or is the
 * unset address, when `configured` has no numeric port, or when the result
 * does not fit. */
int chorus_link_emulated_server(const char *gateway, const char *configured, char *out,
                                size_t out_len);

#endif /* CHORUS_LINK_H */
