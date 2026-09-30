#include "esp_link.h"

#include <string.h>

#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_eth.h"
#include "esp_eth_mac_w5500.h"
#include "esp_eth_phy_w5500.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"

static const char *TAG = "chorus-link";

#define GOT_ADDRESS_BIT BIT0

/* One wired interface per board. */
typedef struct {
    esp_eth_handle_t handle;
    esp_netif_t *netif;
    EventGroupHandle_t events;
} esp_link_t;

static esp_link_t link;

static int to_gpio(uint32_t pin)
{
    return (pin == CHORUS_PIN_NONE) ? -1 : (int)pin;
}

static spi_host_device_t to_host(chorus_spi_host_t host)
{
    return (host == CHORUS_SPI_HOST_SPI3) ? SPI3_HOST : SPI2_HOST;
}

static void on_got_address(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    (void)base;
    (void)id;
    (void)data;
    xEventGroupSetBits(link.events, GOT_ADDRESS_BIT);
}

static int esp_link_init(void *ctx, const chorus_eth_config_t *config)
{
    (void)ctx;
    memset(&link, 0, sizeof(link));
    link.events = xEventGroupCreate();
    if (link.events == NULL) {
        return -1;
    }
    if (esp_netif_init() != ESP_OK) {
        return -1;
    }
    esp_err_t loop = esp_event_loop_create_default();
    if (loop != ESP_OK && loop != ESP_ERR_INVALID_STATE) {
        return -1;
    }
    /* The W5500 driver takes its INT line through the GPIO ISR service. */
    esp_err_t isr = gpio_install_isr_service(0);
    if (isr != ESP_OK && isr != ESP_ERR_INVALID_STATE) {
        ESP_LOGE(TAG, "the GPIO interrupt service the W5500's INT line needs could not start");
        return -1;
    }

    spi_host_device_t host = to_host(config->spi_host);
    spi_bus_config_t bus = {
        .miso_io_num = to_gpio(config->miso),
        .mosi_io_num = to_gpio(config->mosi),
        .sclk_io_num = to_gpio(config->sclk),
        .quadwp_io_num = -1,
        .quadhd_io_num = -1,
    };
    if (spi_bus_initialize(host, &bus, SPI_DMA_CH_AUTO) != ESP_OK) {
        ESP_LOGE(TAG, "the SPI bus for the W5500 could not be created");
        return -1;
    }
    spi_device_interface_config_t device = {
        .mode = 0,
        .clock_speed_hz = (int)(config->spi_clock_mhz * 1000u * 1000u),
        .queue_size = 20,
        .spics_io_num = to_gpio(config->cs),
    };
    eth_w5500_config_t w5500 = ETH_W5500_DEFAULT_CONFIG(host, &device);
    /* The INT line, never polling: chorus_link_validate refuses a wired link
     * without one, so poll_period_ms stays 0. */
    w5500.base.int_gpio_num = to_gpio(config->int_pin);
    w5500.base.poll_period_ms = 0;

    eth_mac_config_t mac_config = ETH_MAC_DEFAULT_CONFIG();
    eth_phy_config_t phy_config = ETH_PHY_DEFAULT_CONFIG();
    phy_config.reset_gpio_num = to_gpio(config->rst);
    esp_eth_mac_t *mac = esp_eth_mac_new_w5500(&w5500, &mac_config);
    esp_eth_phy_t *phy = esp_eth_phy_new_w5500(&phy_config);
    if (mac == NULL || phy == NULL) {
        ESP_LOGE(TAG, "the W5500 MAC or PHY could not be created");
        return -1;
    }
    esp_eth_config_t eth_config = ETH_DEFAULT_CONFIG(mac, phy);
    if (esp_eth_driver_install(&eth_config, &link.handle) != ESP_OK) {
        ESP_LOGE(TAG, "the W5500 driver could not be installed");
        return -1;
    }

    /* The W5500 carries no burned-in address of its own; the S3's own
     * Ethernet address is read (never written) and given to it. */
    uint8_t address[6];
    if (esp_read_mac(address, ESP_MAC_ETH) != ESP_OK ||
        esp_eth_ioctl(link.handle, ETH_CMD_S_MAC_ADDR, address) != ESP_OK) {
        ESP_LOGE(TAG, "the W5500 could not be given this board's Ethernet address");
        return -1;
    }

    esp_netif_config_t netif_config = ESP_NETIF_DEFAULT_ETH();
    link.netif = esp_netif_new(&netif_config);
    if (link.netif == NULL ||
        esp_netif_attach(link.netif, esp_eth_new_netif_glue(link.handle)) != ESP_OK) {
        ESP_LOGE(TAG, "the W5500 could not be attached to the network stack");
        return -1;
    }
    if (esp_event_handler_register(IP_EVENT, IP_EVENT_ETH_GOT_IP, on_got_address, NULL) != ESP_OK) {
        return -1;
    }
    /* Nothing here starts a network time service: the endpoint's only clock
     * is firmware/src/monotonic.c, and the safety scans fail this file if a
     * settable one ever appears. */
    return 0;
}

static int esp_link_start(void *ctx)
{
    (void)ctx;
    return (esp_eth_start(link.handle) == ESP_OK) ? 0 : -1;
}

static int esp_link_wait_for_address(void *ctx, uint32_t timeout_ms)
{
    (void)ctx;
    EventBits_t bits = xEventGroupWaitBits(link.events, GOT_ADDRESS_BIT, pdFALSE, pdTRUE,
                                           pdMS_TO_TICKS(timeout_ms));
    return (bits & GOT_ADDRESS_BIT) ? 0 : -1;
}

void chorus_esp_link_ethernet(chorus_ethernet_t *ethernet)
{
    ethernet->ctx = &link;
    ethernet->init = esp_link_init;
    ethernet->start = esp_link_start;
    ethernet->wait_for_address = esp_link_wait_for_address;
}
