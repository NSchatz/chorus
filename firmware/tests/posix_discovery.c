#include "posix_discovery.h"

#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

int posix_discovery_open(posix_discovery_t *socket_state, char *detail, size_t detail_len)
{
    socket_state->fd = socket(AF_INET, SOCK_DGRAM, 0);
    if (socket_state->fd < 0) {
        snprintf(detail, detail_len, "a UDP socket to browse from could not be opened: %s",
                 strerror(errno));
        return -1;
    }
    struct sockaddr_in any;
    memset(&any, 0, sizeof(any));
    any.sin_family = AF_INET;
    any.sin_addr.s_addr = htonl(INADDR_ANY);
    any.sin_port = 0;
    if (bind(socket_state->fd, (const struct sockaddr *)&any, sizeof(any)) != 0) {
        snprintf(detail, detail_len, "a UDP socket to browse from could not be bound: %s",
                 strerror(errno));
        close(socket_state->fd);
        socket_state->fd = -1;
        return -1;
    }
    /* RFC 6762 section 11: a multicast DNS packet is sent with an IP TTL of
     * 255. Best effort, as the Rust browser's is. */
    unsigned char ttl = 255;
    (void)setsockopt(socket_state->fd, IPPROTO_IP, IP_MULTICAST_TTL, &ttl, sizeof(ttl));
    return 0;
}

void posix_discovery_close(posix_discovery_t *socket_state)
{
    if (socket_state->fd >= 0) {
        close(socket_state->fd);
        socket_state->fd = -1;
    }
}

static int posix_send(void *context, const uint8_t *query, size_t length)
{
    const posix_discovery_t *socket_state = (const posix_discovery_t *)context;
    if (socket_state->fd < 0) {
        return -1;
    }
    struct sockaddr_in group;
    memset(&group, 0, sizeof(group));
    group.sin_family = AF_INET;
    group.sin_port = htons(CHORUS_DISCOVERY_MDNS_PORT);
    if (inet_pton(AF_INET, CHORUS_DISCOVERY_MDNS_GROUP_V4, &group.sin_addr) != 1) {
        return -1;
    }
    ssize_t sent =
        sendto(socket_state->fd, query, length, 0, (const struct sockaddr *)&group, sizeof(group));
    return (sent == (ssize_t)length) ? 0 : -1;
}

static long posix_receive(void *context, uint8_t *out, size_t capacity, uint32_t wait_ms)
{
    const posix_discovery_t *socket_state = (const posix_discovery_t *)context;
    if (socket_state->fd < 0) {
        return -1;
    }
    struct pollfd waiting;
    waiting.fd = socket_state->fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    int ready = poll(&waiting, 1, (int)wait_ms);
    if (ready == 0 || (ready < 0 && errno == EINTR)) {
        return 0;
    }
    if (ready < 0) {
        return -1;
    }
    ssize_t got = recv(socket_state->fd, out, capacity, 0);
    if (got < 0) {
        /* A refusal an earlier send provoked (ICMP) is not a datagram and
         * not the socket failing: there is simply nothing yet. */
        return (errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR || errno == ECONNREFUSED)
                   ? 0
                   : -1;
    }
    return (long)got;
}

chorus_discovery_link_t posix_discovery_link(posix_discovery_t *socket_state)
{
    chorus_discovery_link_t link;
    link.context = socket_state;
    link.send = posix_send;
    link.receive = posix_receive;
    return link;
}
