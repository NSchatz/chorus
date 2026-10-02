#include "esp_discovery.h"

#include <errno.h>
#include <stdio.h>
#include <string.h>

#include "esp_log.h"
#include "lwip/inet.h"
#include "lwip/sockets.h"

#include "chorus/discovery.h"

static const char *TAG = "chorus-discovery";

/* One browse at a time, from app_main before the session exists and from the
 * session's task after: never both. The socket lives for one browse, so the
 * board holds no UDP control block while it plays. */
static int browse_fd = -1;
static uint8_t datagram[CHORUS_DISCOVERY_MAX_DATAGRAM];

static int open_socket(void)
{
    browse_fd = socket(AF_INET, SOCK_DGRAM, 0);
    if (browse_fd < 0) {
        ESP_LOGE(TAG, "a UDP socket to browse from could not be opened (errno %d)", errno);
        return -1;
    }
    /* RFC 6762 section 11: a multicast DNS packet is sent with an IP TTL of
     * 255. The query carries the unicast-response bit, so the answer comes
     * back to this socket's own port: nothing binds 5353 or joins the group. */
    unsigned char ttl = 255;
    (void)setsockopt(browse_fd, IPPROTO_IP, IP_MULTICAST_TTL, &ttl, sizeof(ttl));
    return 0;
}

static void close_socket(void)
{
    if (browse_fd >= 0) {
        close(browse_fd);
        browse_fd = -1;
    }
}

static int link_send(void *context, const uint8_t *query, size_t length)
{
    (void)context;
    if (browse_fd < 0) {
        return -1;
    }
    struct sockaddr_in group;
    memset(&group, 0, sizeof(group));
    group.sin_family = AF_INET;
    group.sin_port = htons(CHORUS_DISCOVERY_MDNS_PORT);
    group.sin_addr.s_addr = inet_addr(CHORUS_DISCOVERY_MDNS_GROUP_V4);
    int sent = sendto(browse_fd, query, length, 0, (const struct sockaddr *)&group, sizeof(group));
    return (sent == (int)length) ? 0 : -1;
}

static long link_receive(void *context, uint8_t *out, size_t capacity, uint32_t wait_ms)
{
    (void)context;
    if (browse_fd < 0) {
        return -1;
    }
    struct timeval wait;
    wait.tv_sec = (long)(wait_ms / 1000u);
    wait.tv_usec = (long)(wait_ms % 1000u) * 1000;
    if (setsockopt(browse_fd, SOL_SOCKET, SO_RCVTIMEO, &wait, sizeof(wait)) != 0) {
        return -1;
    }
    int got = recv(browse_fd, out, capacity, 0);
    if (got < 0) {
        return (errno == EAGAIN || errno == EWOULDBLOCK) ? 0 : -1;
    }
    return (long)got;
}

static const chorus_discovery_link_t browse_link = {
    .context = NULL,
    .send = link_send,
    .receive = link_receive,
};

static const chorus_relocator_t relocator = {
    .link = &browse_link,
    .service = CHORUS_DISCOVERY_AUDIO_SERVICE,
    .window_ms = CHORUS_DISCOVERY_DEFAULT_WINDOW_MS,
    .scratch = datagram,
    .scratch_capacity = sizeof(datagram),
};

/* The session's relocate seam: a browse of its own, on the session's task. */
static int relocate(void *context, char *address, size_t address_len)
{
    (void)context;
    if (open_socket() != 0) {
        return 0;
    }
    /* The cast drops const from a pointer the unit only reads through. */
    int found = chorus_discovery_relocate((void *)(uintptr_t)&relocator, address, address_len);
    close_socket();
    if (found) {
        ESP_LOGI(TAG, "server-located how=mdns address=%s (asked again by the session)", address);
    }
    return found;
}

void chorus_esp_discovery_locate(chorus_session_config_t *session)
{
    chorus_locate_t request;
    memset(&request, 0, sizeof(request));
    /* A socket that would not open leaves the link refusing its sends, which
     * the decision reports as discovery that could not run. */
    (void)open_socket();
    request.link = &browse_link;
    request.service = CHORUS_DISCOVERY_AUDIO_SERVICE;
    request.window_ms = CHORUS_DISCOVERY_DEFAULT_WINDOW_MS;
    request.store = session->store;
    request.configured = session->server;
    /* The committed server_address is loopback, and no board reaches a
     * server there (audit A-10). */
    request.loopback_is_usable = 0;

    static chorus_located_t located;
    static char line[640];
    int found = chorus_discovery_locate(&request, datagram, sizeof(datagram), &located);
    close_socket();
    chorus_located_line(&located, line, sizeof(line));
    if (found == 0) {
        snprintf(session->server, sizeof(session->server), "%s", located.address);
        ESP_LOGI(TAG, "%s", line);
    } else {
        ESP_LOGE(TAG, "%s", line);
        ESP_LOGE(TAG,
                 "no server address yet: the session keeps trying %s and browses again after "
                 "every few attempts that reach nothing",
                 session->server);
    }
    session->relocate = relocate;
    session->relocate_ctx = NULL;
}
