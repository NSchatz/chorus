/* Wi-Fi provisioning, bound to the platform (esp_provision.h says what this is
 * and is not; chorus/provision.h is where the decisions are).
 *
 * # What runs where
 *
 * `chorus_esp_provision_or_join` runs on app_main's task and is the only
 * place the pure unit is driven from: it boots the unit, and while the unit
 * says the access point is up it waits on one small queue and hands each
 * thing that arrives (a posted form, a network from Espressif's client, a
 * reset, a tick) to the unit. The HTTP handlers and the event handlers run on
 * ESP-IDF's tasks and do nothing but copy what arrived and post to that
 * queue, so the unit is never entered from two tasks.
 *
 * # Two ways in, one unit
 *
 *   - THE SPEAKER'S OWN PAGE. `GET /` is the join form and `POST /join` takes
 *     it, on an HTTP server this file owns. That is the browser path: a
 *     phone's browser on the speaker's access point, protected by the access
 *     point's WPA2 passphrase (the setup secret). The join is this file's own
 *     (`own_join`), with the Wi-Fi driver's storage set to RAM, so the network
 *     lands in chorus's store and nowhere else.
 *   - ESPRESSIF'S CLIENTS. The same HTTP server is handed to the pinned
 *     espressif/network_provisioning manager (scheme SoftAP, protocomm
 *     security 1 with the setup secret as the proof of possession), so
 *     Espressif's phone app and `esp_prov` work as a cross-check. There the
 *     manager owns the join; this file takes the network from the manager's
 *     event, hands it to the unit, and reports the manager's verdict as the
 *     join's. The manager writes that network to the Wi-Fi driver's own NVS
 *     namespace as well (its manager.c sets WIFI_STORAGE_FLASH before
 *     esp_wifi_set_config); `radio_init` below empties that copy at the next
 *     boot, so the store is the one home again.
 *
 * Citations are to the pinned ESP-IDF v6.1 tree and to
 * espressif/network_provisioning 1.2.5 as the component registry serves it,
 * both read 2026-10-02.
 *
 * # What is printed
 *
 * The unit's lines (no network name, no passphrase, no setup secret: the host
 * test greps for all three), and two lines of this file's own: the setup
 * secret, once per boot that raises the access point, because the person at
 * the speaker has to read it; and the address of the page, read from the
 * access point's interface at run time. No network's name or passphrase is
 * ever handed to a log call here. */

#include "esp_provision.h"

#if defined(CHORUS_PROVISIONING)

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "esp_console.h"
#include "esp_err.h"
#include "esp_event.h"
#include "esp_http_server.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "network_provisioning/manager.h"
#include "network_provisioning/scheme_softap.h"

#include "esp_store.h"

#include "chorus/noise.h"
#include "chorus/provision.h"
#include "chorus/store.h"

static const char *TAG = "chorus-provision";

/* ASSUMED, all four, until the owner's bench session (docs/bench-packet.md S9)
 * says otherwise:
 *   - how long one join may take before it is called a failure;
 *   - how long the reply to a posted form is given to reach the phone before
 *     the radio leaves the access point's channel to join (the manager waits
 *     the same second for the same reason, its manager.c:1307-1309);
 *   - how many times the manager tries a network Espressif's client sent;
 *   - how long the manager's own join may take, attempts included. */
#define JOIN_TIMEOUT_MS 20000u
#define REPLY_GRACE_MS 1000u
#define MANAGER_JOIN_ATTEMPTS 2u
#define MANAGER_JOIN_TIMEOUT_MS 45000u

/* How often the loop wakes with nothing to do, to give the unit its tick. */
#define TICK_MS 1000u

typedef enum {
    /* A form body is waiting in `form`. */
    EVENT_FORM = 1,
    /* The manager received a network from Espressif's client; it is in
     * `app_network`, and the manager is already joining it. */
    EVENT_APP_NETWORK,
    EVENT_RESET
} event_kind_t;

typedef struct {
    int joined;
    /* A stable word for why not, or NULL. */
    const char *reason;
} join_result_t;

static struct {
    chorus_provision_t unit;
    chorus_radio_t inner;
    int radio_ready;
    int handlers_ready;

    QueueHandle_t events;
    QueueHandle_t join_results;
    /* Guards `form`, `app_network`, `page` and `page_reason`: the things the
     * HTTP task and the event task share with app_main's. Never held across a
     * join. */
    SemaphoreHandle_t lock;

    char form[CHORUS_PROVISION_FORM_MAX];
    size_t form_length;
    chorus_provision_credentials_t app_network;
    char page_reason[CHORUS_PROVISION_REASON];
    char page[2048];

    esp_netif_t *ap_netif;
    httpd_handle_t server;
    int manager_up;
    int looping;

    /* The join in hand. `joining` is this file's own join; `manager_joining`
     * is one the manager owns. */
    volatile int joining;
    volatile int manager_joining;
    /* Whether the station holds an address, and on which network. Kept so the
     * bring-up that follows provisioning finds the link joined. */
    volatile int connected;
    char joined_ssid[CHORUS_PROVISION_SSID_MAX + 1];
    char joined_secret[CHORUS_PROVISION_SECRET_MAX + 1];
    const char *join_reason;
    char reason_text[32];
} g;

static uint64_t now_ms(void)
{
    /* Monotonic: microseconds since boot (esp_timer.h, "Returns time in
     * microseconds since boot"). Nothing here is on the audio path, and
     * nothing here reads a settable clock. */
    return (uint64_t)esp_timer_get_time() / 1000u;
}

/* --- the radio: a join that waits ------------------------------------------- */

/* The words for why a join failed. The first two are the manager's own two
 * (NETWORK_PROV_WIFI_STA_AUTH_ERROR, NETWORK_PROV_WIFI_STA_AP_NOT_FOUND), and
 * the reasons are sorted into them exactly as its manager.c:1812-1824 does. */
static const char *reason_word(uint8_t reason)
{
    switch (reason) {
    case WIFI_REASON_4WAY_HANDSHAKE_TIMEOUT:
    case WIFI_REASON_AUTH_FAIL:
    case WIFI_REASON_HANDSHAKE_TIMEOUT:
    case WIFI_REASON_MIC_FAILURE:
        return "auth-error";
    case WIFI_REASON_NO_AP_FOUND:
        return "network-not-found";
    default:
        snprintf(g.reason_text, sizeof(g.reason_text), "wifi-reason-%u", (unsigned)reason);
        return g.reason_text;
    }
}

static void post_join_result(int joined, const char *reason)
{
    join_result_t result = {.joined = joined, .reason = reason};
    (void)xQueueSend(g.join_results, &result, 0);
}

static void on_wifi_event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    if (base == IP_EVENT && id == IP_EVENT_STA_GOT_IP) {
        g.connected = 1;
        if (g.joining) {
            post_join_result(1, NULL);
        }
        return;
    }
    if (base == WIFI_EVENT && id == WIFI_EVENT_STA_DISCONNECTED) {
        const wifi_event_sta_disconnected_t *event = (const wifi_event_sta_disconnected_t *)data;
        int was_connected = g.connected;
        g.connected = 0;
        if (g.joining) {
            /* The disconnect this file asked for before a new join is not the
             * new join failing. */
            if (event->reason != WIFI_REASON_ASSOC_LEAVE) {
                post_join_result(0, reason_word(event->reason));
            }
            return;
        }
        if (was_connected && !g.manager_joining && !g.looping) {
            /* On its network and dropped (the router restarted): ask again,
             * for as long as it takes. The session supervisor does the same
             * for its socket (firmware/src/session.c). The reason number is
             * the platform's and names nobody. */
            ESP_LOGW(TAG, "provision: the link dropped (wifi-reason-%u); joining again",
                     (unsigned)event->reason);
            (void)esp_wifi_connect();
        } else if (!was_connected && !g.manager_joining && !g.looping && g.joined_ssid[0] != '\0') {
            (void)esp_wifi_connect();
        }
    }
}

static void on_manager_event(void *arg, esp_event_base_t base, int32_t id, void *data)
{
    (void)arg;
    (void)base;
    switch (id) {
    case NETWORK_PROV_WIFI_CRED_RECV: {
        /* A network from Espressif's client. Also raised for a network this
         * file handed the manager, which it never does; so every one of
         * these came through protocomm. The fields are not terminated at
         * their full length (esp_wifi_types_generic.h:564-565). */
        const wifi_sta_config_t *station = (const wifi_sta_config_t *)data;
        chorus_provision_credentials_t network;
        memset(&network, 0, sizeof(network));
        network.ssid_length = strnlen((const char *)station->ssid, sizeof(station->ssid));
        memcpy(network.ssid, station->ssid, network.ssid_length);
        network.secret_length = strnlen((const char *)station->password, sizeof(station->password));
        memcpy(network.secret, station->password, network.secret_length);
        g.manager_joining = 1;
        xQueueReset(g.join_results);
        xSemaphoreTake(g.lock, portMAX_DELAY);
        g.app_network = network;
        xSemaphoreGive(g.lock);
        memset(&network, 0, sizeof(network));
        event_kind_t kind = EVENT_APP_NETWORK;
        (void)xQueueSend(g.events, &kind, 0);
        break;
    }
    case NETWORK_PROV_WIFI_CRED_SUCCESS:
        if (g.manager_joining) {
            post_join_result(1, NULL);
        }
        break;
    case NETWORK_PROV_WIFI_CRED_FAIL:
        if (g.manager_joining) {
            const network_prov_wifi_sta_fail_reason_t *reason =
                (const network_prov_wifi_sta_fail_reason_t *)data;
            post_join_result(0, (reason != NULL && *reason == NETWORK_PROV_WIFI_STA_AP_NOT_FOUND)
                                    ? "network-not-found"
                                    : "auth-error");
        }
        break;
    default:
        break;
    }
}

static int radio_init(void *ctx)
{
    (void)ctx;
    if (g.radio_ready) {
        /* The bring-up runs once per join; the radio is initialised once. */
        return 0;
    }
    if (g.inner.init(g.inner.ctx) != 0) {
        return -1;
    }
    if (!g.handlers_ready) {
        if (esp_event_handler_register(WIFI_EVENT, WIFI_EVENT_STA_DISCONNECTED, on_wifi_event,
                                       NULL) != ESP_OK ||
            esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP, on_wifi_event, NULL) !=
                ESP_OK ||
            esp_event_handler_register(NETWORK_PROV_EVENT, ESP_EVENT_ANY_ID, on_manager_event,
                                       NULL) != ESP_OK) {
            return -1;
        }
        g.handlers_ready = 1;
    }
    /* The store is where a network lives. If the Wi-Fi driver's own NVS
     * namespace holds one (Espressif's client was used last time, and the
     * manager wrote it there), empty it, then keep the driver's storage in
     * RAM so nothing this file joins is written there again. */
    wifi_config_t held;
    memset(&held, 0, sizeof(held));
    if (esp_wifi_get_config(WIFI_IF_STA, &held) == ESP_OK && held.sta.ssid[0] != '\0') {
        wifi_config_t empty;
        memset(&empty, 0, sizeof(empty));
        (void)esp_wifi_set_storage(WIFI_STORAGE_FLASH);
        (void)esp_wifi_set_config(WIFI_IF_STA, &empty);
        ESP_LOGI(TAG, "provision: the Wi-Fi driver's own copy of a network was emptied");
    }
    memset(&held, 0, sizeof(held));
    if (esp_wifi_set_storage(WIFI_STORAGE_RAM) != ESP_OK) {
        return -1;
    }
    g.radio_ready = 1;
    return 0;
}

static int radio_set_power_save(void *ctx, chorus_wifi_ps_t mode)
{
    (void)ctx;
    return g.inner.set_power_save(g.inner.ctx, mode);
}

static int radio_get_power_save(void *ctx, chorus_wifi_ps_t *mode)
{
    (void)ctx;
    return g.inner.get_power_save(g.inner.ctx, mode);
}

static int radio_coexistence_active(void *ctx, int *active)
{
    (void)ctx;
    return g.inner.coexistence_active(g.inner.ctx, active);
}

static int wait_for_join(uint32_t timeout_ms)
{
    join_result_t result = {.joined = 0, .reason = NULL};
    if (xQueueReceive(g.join_results, &result, pdMS_TO_TICKS(timeout_ms)) != pdTRUE) {
        g.join_reason = "join-timeout";
        return -1;
    }
    g.join_reason = result.reason;
    return result.joined ? 0 : -1;
}

/* This file's own join: configure the station, connect, and wait for an
 * address or a refusal. The driver's storage is RAM, so nothing is written to
 * flash here. */
static int own_join(const char *ssid, const char *secret)
{
    wifi_config_t station;
    memset(&station, 0, sizeof(station));
    /* Both fields may be full with no terminator: a 32-byte name, a 64-digit
     * key. */
    memcpy(station.sta.ssid, ssid, strnlen(ssid, sizeof(station.sta.ssid)));
    memcpy(station.sta.password, secret, strnlen(secret, sizeof(station.sta.password)));

    g.connected = 0;
    xQueueReset(g.join_results);
    g.joining = 1;
    (void)esp_wifi_disconnect();
    int rc = -1;
    if (esp_wifi_set_storage(WIFI_STORAGE_RAM) == ESP_OK &&
        esp_wifi_set_config(WIFI_IF_STA, &station) == ESP_OK && esp_wifi_connect() == ESP_OK) {
        rc = wait_for_join(JOIN_TIMEOUT_MS);
    } else {
        g.join_reason = "radio-refused";
    }
    g.joining = 0;
    if (rc != 0) {
        /* Stop the driver's own retries: the unit decides whether to try
         * again. */
        (void)esp_wifi_disconnect();
    }
    memset(&station, 0, sizeof(station));
    return rc;
}

static int radio_join(void *ctx, const char *ssid, const char *secret)
{
    (void)ctx;
    g.join_reason = NULL;
    /* The bring-up that follows provisioning asks for the network this file
     * just joined. It is joined. */
    if (g.connected && strcmp(ssid, g.joined_ssid) == 0 && strcmp(secret, g.joined_secret) == 0) {
        return 0;
    }
    int rc;
    if (g.manager_joining) {
        /* Espressif's client: the manager is already joining this network. */
        rc = wait_for_join(MANAGER_JOIN_TIMEOUT_MS);
        g.manager_joining = 0;
        if (rc != 0) {
            /* Back to accepting, whichever state the manager stopped in. */
            (void)network_prov_mgr_reset_wifi_sm_state_for_reprovision();
        }
    } else {
        rc = own_join(ssid, secret);
    }
    memset(g.joined_ssid, 0, sizeof(g.joined_ssid));
    memset(g.joined_secret, 0, sizeof(g.joined_secret));
    if (rc == 0) {
        g.connected = 1;
        snprintf(g.joined_ssid, sizeof(g.joined_ssid), "%s", ssid);
        snprintf(g.joined_secret, sizeof(g.joined_secret), "%s", secret);
    }
    return rc;
}

/* --- the pages --------------------------------------------------------------- */

static esp_err_t send_page(httpd_req_t *request, const char *status, size_t length)
{
    httpd_resp_set_status(request, status);
    httpd_resp_set_type(request, "text/html; charset=utf-8");
    /* A page with a form on it is never one a phone should show from memory. */
    httpd_resp_set_hdr(request, "Cache-Control", "no-store");
    return httpd_resp_send(request, g.page, (ssize_t)length);
}

static esp_err_t get_page(httpd_req_t *request)
{
    xSemaphoreTake(g.lock, portMAX_DELAY);
    /* The page needs the last refusal's reason and nothing else of the unit,
     * which app_main's task may be inside for the length of a join. A view
     * holding only that reason is rendered instead. */
    static chorus_provision_t view;
    memset(&view, 0, sizeof(view));
    snprintf(view.reason, sizeof(view.reason), "%s", g.page_reason);
    size_t length = chorus_provision_page(&view, g.page, sizeof(g.page));
    esp_err_t err = send_page(request, "200 OK", length);
    xSemaphoreGive(g.lock);
    return err;
}

static esp_err_t post_join(httpd_req_t *request)
{
    /* One byte more than the bound is read, so a body that is too long is
     * seen to be too long rather than cut to fit. */
    char body[CHORUS_PROVISION_FORM_MAX + 1];
    size_t length = 0;
    chorus_provision_form_status_t status = CHORUS_PROVISION_FORM_OK;
    if (request->content_len > CHORUS_PROVISION_FORM_MAX) {
        status = CHORUS_PROVISION_FORM_TOO_LONG;
    }
    while (status == CHORUS_PROVISION_FORM_OK && length < request->content_len) {
        int got = httpd_req_recv(request, body + length, request->content_len - length);
        if (got == HTTPD_SOCK_ERR_TIMEOUT) {
            continue;
        }
        if (got <= 0) {
            memset(body, 0, sizeof(body));
            return ESP_FAIL;
        }
        length += (size_t)got;
    }
    if (status == CHORUS_PROVISION_FORM_OK) {
        /* Parsed here only to choose the reply. The unit parses the same
         * bytes again on app_main's task and is the one that acts on them. */
        chorus_provision_credentials_t network;
        status = chorus_provision_parse_form(body, length, &network);
        memset(&network, 0, sizeof(network));
    }
    xSemaphoreTake(g.lock, portMAX_DELAY);
    if (status != CHORUS_PROVISION_FORM_TOO_LONG) {
        memcpy(g.form, body, length);
        g.form_length = length;
        event_kind_t kind = EVENT_FORM;
        (void)xQueueSend(g.events, &kind, 0);
    }
    memset(body, 0, sizeof(body));
    size_t page_length = chorus_provision_reply_page(status, g.page, sizeof(g.page));
    esp_err_t err = send_page(
        request, status == CHORUS_PROVISION_FORM_OK ? "200 OK" : "400 Bad Request", page_length);
    xSemaphoreGive(g.lock);
    return err;
}

/* --- the platform: the access point ------------------------------------------ */

static void stop_manager_and_server(void)
{
    if (g.manager_up) {
        network_prov_mgr_stop_provisioning();
        /* Until the service has stopped (manager.h, network_prov_mgr_wait),
         * which also returns the radio to station mode (manager.c:667). */
        network_prov_mgr_wait();
        network_prov_mgr_deinit();
        g.manager_up = 0;
    }
    if (g.server != NULL) {
        network_prov_scheme_softap_set_httpd_handle(NULL);
        (void)httpd_stop(g.server);
        g.server = NULL;
    }
}

static int platform_ap_start(void *ctx, const char *name, const char *key)
{
    (void)ctx;
    /* The radio first: the unit raises the access point before any join on a
     * speaker with nothing stored, so nothing has initialised it yet. */
    if (radio_init(NULL) != 0) {
        ESP_LOGE(TAG, "provision: the radio could not be initialised");
        return -1;
    }
    if (g.ap_netif == NULL) {
        g.ap_netif = esp_netif_create_default_wifi_ap();
        if (g.ap_netif == NULL) {
            return -1;
        }
    }

    /* The HTTP server is this file's, handed to the manager
     * (scheme_softap.h, network_prov_scheme_softap_set_httpd_handle), so the
     * join form and protocomm's endpoints are one server on one port.
     * protocomm registers one handler per endpoint (five: prov-session,
     * proto-ver, prov-config, prov-scan, prov-ctrl; manager.c:478-581) and this
     * file two. A phone that keeps sockets open must not shut the next
     * request out, so the least recently used one may be purged. */
    httpd_config_t http = HTTPD_DEFAULT_CONFIG();
    http.max_uri_handlers = 12;
    http.lru_purge_enable = true;
    if (httpd_start(&g.server, &http) != ESP_OK) {
        g.server = NULL;
        ESP_LOGE(TAG, "provision: the HTTP server could not be started");
        return -1;
    }
    static const httpd_uri_t page = {.uri = "/", .method = HTTP_GET, .handler = get_page};
    static const httpd_uri_t join = {.uri = "/join", .method = HTTP_POST, .handler = post_join};
    if (httpd_register_uri_handler(g.server, &page) != ESP_OK ||
        httpd_register_uri_handler(g.server, &join) != ESP_OK) {
        stop_manager_and_server();
        return -1;
    }
    network_prov_scheme_softap_set_httpd_handle(g.server);

    network_prov_mgr_config_t manager = {
        .scheme = network_prov_scheme_softap,
        .scheme_event_handler = NETWORK_PROV_EVENT_HANDLER_NONE,
        .app_event_handler = NETWORK_PROV_EVENT_HANDLER_NONE,
        .network_prov_wifi_conn_cfg = {.wifi_conn_attempts = MANAGER_JOIN_ATTEMPTS},
    };
    if (network_prov_mgr_init(manager) != ESP_OK) {
        stop_manager_and_server();
        return -1;
    }
    g.manager_up = 1;
    /* The unit says when the access point drops, not a timer of the
     * manager's: without this the manager stops itself 30 s after a join
     * (CONFIG_NETWORK_PROV_AUTOSTOP_TIMEOUT). */
    (void)network_prov_mgr_disable_auto_stop(1000);
    /* Security 1, the setup secret as the proof of possession; the same
     * secret is the access point's WPA2 passphrase (`key`). ESP-IDF's own
     * provisioning guide accepts exactly this: "Unique per-device passphrase
     * can also act as a proof-of-possession"
     * (docs/en/api-reference/provisioning/provisioning.rst:100). */
    if (network_prov_mgr_start_provisioning(NETWORK_PROV_SECURITY_1, key, name, key) != ESP_OK) {
        stop_manager_and_server();
        return -1;
    }
    /* The manager leaves the driver's storage on flash when it returns
     * (manager.c:2300); this file's joins keep it in RAM. */
    (void)esp_wifi_set_storage(WIFI_STORAGE_RAM);

    /* The two lines the person at the speaker needs. The address is read from
     * the access point's interface, not written here. */
    esp_netif_ip_info_t ip;
    memset(&ip, 0, sizeof(ip));
    (void)esp_netif_get_ip_info(g.ap_netif, &ip);
    ESP_LOGI(TAG,
             "provision: setup secret %s (the access point's passphrase and the proof of "
             "possession)",
             key);
    char address[16];
    ESP_LOGI(TAG, "provision: page http://%s/",
             esp_ip4addr_ntoa(&ip.ip, address, (int)sizeof(address)));
    return 0;
}

static int platform_ap_stop(void *ctx)
{
    (void)ctx;
    /* The reply to the form, or the manager's last status, gets its second
     * to reach the phone before the access point goes. */
    vTaskDelay(pdMS_TO_TICKS(REPLY_GRACE_MS));
    stop_manager_and_server();
    (void)esp_wifi_set_storage(WIFI_STORAGE_RAM);
    return 0;
}

static const char *platform_join_reason(void *ctx)
{
    (void)ctx;
    return g.join_reason;
}

static void platform_log(void *ctx, const char *line)
{
    (void)ctx;
    ESP_LOGI(TAG, "%s", line);
}

/* --- the reset ---------------------------------------------------------------- */

void chorus_esp_provision_request_reset(void)
{
    if (g.events == NULL) {
        ESP_LOGW(TAG, "provision: this boot has not reached provisioning yet");
        return;
    }
    if (g.looping) {
        /* The access point is up: the loop on app_main's task takes it. */
        event_kind_t kind = EVENT_RESET;
        (void)xQueueSend(g.events, &kind, 0);
        return;
    }
    /* Joined, and the session may be playing. The stored network is erased
     * now; the speaker stays on its network until it is restarted, and the
     * boot after that is unprovisioned. Nothing here restarts it: a restart
     * with the output stage live is the amplifier sequencer's to order, not
     * this file's. */
    if (chorus_provision_reset(&g.unit) == 0) {
        ESP_LOGI(TAG, "provision: restart the speaker to set it up again");
    }
}

static int console_reset(int argc, char **argv)
{
    (void)argc;
    (void)argv;
    chorus_esp_provision_request_reset();
    return 0;
}

static void register_console_reset(void)
{
    const esp_console_cmd_t command = {
        .command = "wifi-reset",
        .help = "Erase the stored Wi-Fi network (the setup secret stays); goal 14",
        .hint = NULL,
        .func = console_reset,
    };
    if (esp_console_cmd_register(&command) != ESP_OK) {
        ESP_LOGW(TAG, "provision: the console command wifi-reset could not be registered");
    }
}

/* --- the loop ----------------------------------------------------------------- */

static void publish_reason(void)
{
    xSemaphoreTake(g.lock, portMAX_DELAY);
    snprintf(g.page_reason, sizeof(g.page_reason), "%s", chorus_provision_reason(&g.unit));
    xSemaphoreGive(g.lock);
}

void chorus_esp_provision_or_join(chorus_endpoint_config_t *config, chorus_radio_t *radio)
{
    if (g.events == NULL) {
        g.events = xQueueCreate(4, sizeof(event_kind_t));
        g.join_results = xQueueCreate(4, sizeof(join_result_t));
        g.lock = xSemaphoreCreateMutex();
    }
    if (g.events == NULL || g.join_results == NULL || g.lock == NULL) {
        ESP_LOGE(TAG, "provision: no memory for provisioning; the link stays down");
        return;
    }

    /* Wrap the radio: the same platform calls, with an initialise that
     * happens once and a join that waits for an address. The wrapped one is
     * what the bring-up after this is handed too. */
    g.inner = *radio;
    radio->ctx = NULL;
    radio->init = radio_init;
    radio->set_power_save = radio_set_power_save;
    radio->get_power_save = radio_get_power_save;
    radio->coexistence_active =
        (g.inner.coexistence_active != NULL) ? radio_coexistence_active : NULL;
    radio->join = radio_join;

    const chorus_provision_platform_t platform = {
        .ctx = NULL,
        .ap_start = platform_ap_start,
        .ap_stop = platform_ap_stop,
        .join_reason = platform_join_reason,
        /* The hardware generator, through the PSA call the session's keys
         * already use (firmware/src/noise.c). */
        .random = chorus_noise_system_random,
        .log = platform_log,
    };
    chorus_provision_init(&g.unit, &config->link, chorus_esp_store(), radio, &platform);
    register_console_reset();

    g.looping = 1;
    chorus_provision_state_t state = chorus_provision_boot(&g.unit, now_ms());
    publish_reason();
    while (state == CHORUS_PROVISION_AP_UP) {
        event_kind_t kind = 0;
        if (xQueueReceive(g.events, &kind, pdMS_TO_TICKS(TICK_MS)) != pdTRUE) {
            state = chorus_provision_tick(&g.unit, now_ms());
            publish_reason();
            continue;
        }
        chorus_provision_form_status_t refusal = CHORUS_PROVISION_FORM_OK;
        if (kind == EVENT_FORM) {
            char body[CHORUS_PROVISION_FORM_MAX];
            xSemaphoreTake(g.lock, portMAX_DELAY);
            size_t length = g.form_length;
            memcpy(body, g.form, length);
            memset(g.form, 0, sizeof(g.form));
            g.form_length = 0;
            xSemaphoreGive(g.lock);
            /* The reply is on its way to the phone; the join moves the radio
             * off the access point's channel. */
            vTaskDelay(pdMS_TO_TICKS(REPLY_GRACE_MS));
            state = chorus_provision_submit_form(&g.unit, body, length, now_ms(), &refusal);
            memset(body, 0, sizeof(body));
        } else if (kind == EVENT_APP_NETWORK) {
            chorus_provision_credentials_t network;
            xSemaphoreTake(g.lock, portMAX_DELAY);
            network = g.app_network;
            memset(&g.app_network, 0, sizeof(g.app_network));
            xSemaphoreGive(g.lock);
            state = chorus_provision_submit(&g.unit, &network, now_ms(), &refusal);
            memset(&network, 0, sizeof(network));
            if (g.manager_joining) {
                /* The unit refused the network before any join (an open
                 * network, say), and the manager is joining it regardless.
                 * Stop that, and put the manager back to accepting. */
                g.manager_joining = 0;
                (void)network_prov_mgr_reset_wifi_sm_state_for_reprovision();
            }
        } else if (kind == EVENT_RESET) {
            (void)chorus_provision_reset(&g.unit);
            state = g.unit.state;
        }
        publish_reason();
    }
    g.looping = 0;

    if (state != CHORUS_PROVISION_JOINED) {
        /* WIRED cannot happen in this image (it is built for a wireless
         * profile) but would end here harmlessly; REFUSED has said why. The
         * bring-up that follows refuses on the unknown credential and the
         * link stays down. */
        ESP_LOGE(TAG, "provision: ended %s (%s); the link stays down",
                 chorus_provision_state_name(state), chorus_provision_reason(&g.unit));
        return;
    }
    (void)chorus_provision_export(&g.unit, &config->link);
}

#endif /* CHORUS_PROVISIONING */
