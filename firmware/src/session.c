#include "chorus/session.h"

#include "chorus/monotonic.h"
#include "chorus/protocol.h"

#include <errno.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>

/* One DMA-sized read at a time, and a buffer big enough that the largest frame
 * the protocol can carry (65538 bytes) always fits whole. Sized once here
 * rather than grown, because the endpoint has no heap on the audio path. */
#define CHORUS_SESSION_BUFFER (CHORUS_FRAME_HEADER_LEN + CHORUS_MAX_PAYLOAD_LEN + 4096)

const char *chorus_session_end_name(chorus_session_end_t end)
{
    switch (end) {
    case CHORUS_SESSION_RAN_ITS_TIME:
        return "ran-its-time";
    case CHORUS_SESSION_ADDRESS_UNUSABLE:
        return "address-unusable";
    case CHORUS_SESSION_STOPPED_ON_AMP_FAULT:
        return "stopped-on-amp-fault";
    case CHORUS_SESSION_LOG_UNWRITABLE:
        return "log-unwritable";
    }
    return "unknown";
}

typedef struct {
    FILE *log;
    chorus_telemetry_t telemetry;
} session_state_t;

static void publish(session_state_t *state, const char *event)
{
    char line[512];
    chorus_telemetry_line(&state->telemetry, line, sizeof(line));
    if (state->log != NULL) {
        fprintf(state->log, "%s event=%s\n", line, event);
        fflush(state->log);
    }
}

/* Split "host:port". IPv6 in brackets is not accepted here and says so rather
 * than being parsed wrong: an address the endpoint cannot use is a refusal at
 * start, not a reconnect loop against nothing. */
static int split_address(const char *address, char *host, size_t host_len, char *port,
                         size_t port_len)
{
    const char *colon = strrchr(address, ':');
    if (colon == NULL || colon == address || colon[1] == '\0') {
        return -1;
    }
    size_t host_bytes = (size_t)(colon - address);
    if (host_bytes + 1 > host_len) {
        return -1;
    }
    memcpy(host, address, host_bytes);
    host[host_bytes] = '\0';
    if (strlen(colon + 1) + 1 > port_len) {
        return -1;
    }
    snprintf(port, port_len, "%s", colon + 1);
    return 0;
}

static int connect_once(const char *host, const char *port)
{
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;

    struct addrinfo *results = NULL;
    if (getaddrinfo(host, port, &hints, &results) != 0) {
        return -1;
    }
    int fd = -1;
    for (struct addrinfo *entry = results; entry != NULL; entry = entry->ai_next) {
        fd = socket(entry->ai_family, entry->ai_socktype, entry->ai_protocol);
        if (fd < 0) {
            continue;
        }
        if (connect(fd, entry->ai_addr, entry->ai_addrlen) == 0) {
            break;
        }
        close(fd);
        fd = -1;
    }
    freeaddrinfo(results);
    if (fd >= 0) {
        int one = 1;
        (void)setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one));
        /* A read that blocks for ever is a supervisor that cannot service its
         * own exchange cadence or notice that its run is over. A timeout is
         * not a failure: it returns to the loop, which decides what to do. */
        struct timeval timeout;
        timeout.tv_sec = 0;
        timeout.tv_usec = 200000;
        (void)setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    }
    return fd;
}

/* Send one time-sync request with t0 stamped and the other three zero, which
 * is what docs/protocol.md says a client sends. */
static int send_time_sync_request(int fd, uint64_t t0_ns)
{
    chorus_time_sync_t request;
    memset(&request, 0, sizeof(request));
    request.t0_ns = t0_ns;
    uint8_t frame[CHORUS_FRAME_HEADER_LEN + CHORUS_TIME_SYNC_PAYLOAD_LEN];
    size_t written = 0;
    if (chorus_encode_time_sync(&request, frame, sizeof(frame), &written) != CHORUS_ENCODE_OK) {
        return -1;
    }
    size_t sent = 0;
    while (sent < written) {
        ssize_t n = send(fd, frame + sent, written - sent, 0);
        if (n <= 0) {
            if (n < 0 && errno == EINTR) {
                continue;
            }
            return -1;
        }
        sent += (size_t)n;
    }
    return 0;
}

int chorus_session_run(const chorus_session_config_t *config, chorus_session_result_t *out)
{
    memset(out, 0, sizeof(*out));
    chorus_telemetry_init(&out->telemetry);

    char host[CHORUS_SESSION_ADDRESS_MAX];
    char port[16];
    if (split_address(config->server, host, sizeof(host), port, sizeof(port)) != 0) {
        out->end = CHORUS_SESSION_ADDRESS_UNUSABLE;
        snprintf(out->detail, sizeof(out->detail),
                 "the server address '%s' is not host:port, so there is nothing to join",
                 config->server);
        return -1;
    }

    session_state_t state;
    memset(&state, 0, sizeof(state));
    chorus_telemetry_init(&state.telemetry);
    state.log = NULL;
    if (config->event_log_path != NULL) {
        state.log = fopen(config->event_log_path, "w");
        if (state.log == NULL) {
            out->end = CHORUS_SESSION_LOG_UNWRITABLE;
            snprintf(out->detail, sizeof(out->detail), "%s could not be opened for writing",
                     config->event_log_path);
            return -1;
        }
    }

    chorus_offset_filter_t filter;
    chorus_offset_filter_init(&filter, config->filter_window, config->smoothing_alpha);

    static uint8_t buffer[CHORUS_SESSION_BUFFER];
    size_t held = 0;

    uint64_t started_ns = chorus_monotonic_now_ns();
    uint64_t run_ns = (uint64_t)config->run_seconds * 1000000000ull;
    uint32_t backoff_ms = config->first_backoff_ms;
    uint64_t went_down_ns = 0;
    int have_been_up = 0;

    publish(&state, "start");

    while (run_ns == 0 || chorus_monotonic_now_ns() - started_ns < run_ns) {
        state.telemetry.link = CHORUS_LINK_CONNECTING;
        state.telemetry.connect_attempts++;
        int fd = connect_once(host, port);
        if (fd < 0) {
            state.telemetry.link = CHORUS_LINK_DOWN;
            state.telemetry.audio = (state.telemetry.audio == CHORUS_AUDIO_STOPPED_ON_AMP_FAULT)
                                        ? state.telemetry.audio
                                        : CHORUS_AUDIO_IDLE;
            publish(&state, "connect-failed");
            chorus_monotonic_sleep_ms(backoff_ms);
            /* Doubling, capped. Never zero, never unbounded, and there is no
             * attempt counter that can run out. */
            backoff_ms = (backoff_ms * 2 > config->max_backoff_ms) ? config->max_backoff_ms
                                                                   : backoff_ms * 2;
            continue;
        }

        backoff_ms = config->first_backoff_ms;
        state.telemetry.link = CHORUS_LINK_UP;
        if (have_been_up) {
            state.telemetry.rejoins++;
            uint64_t outage = chorus_monotonic_now_ns() - went_down_ns;
            if (outage > out->longest_outage_ns) {
                out->longest_outage_ns = outage;
            }
        }
        have_been_up = 1;
        held = 0;
        publish(&state, "link-up");

        uint64_t chunks_at_connect = state.telemetry.chunks_played;
        uint64_t next_exchange_ns = chorus_monotonic_now_ns();
        uint64_t pending_t0_ns = 0;
        int exchange_outstanding = 0;
        int skipped_last = 0;

        for (;;) {
            uint64_t now_ns = chorus_monotonic_now_ns();
            if (run_ns != 0 && now_ns - started_ns >= run_ns) {
                break;
            }
            if (!exchange_outstanding && now_ns >= next_exchange_ns) {
                pending_t0_ns = chorus_monotonic_now_ns();
                if (send_time_sync_request(fd, pending_t0_ns) != 0) {
                    break;
                }
                exchange_outstanding = 1;
            }

            if (held == sizeof(buffer)) {
                /* A whole buffer that decodes nothing means the peer is not
                 * speaking this protocol. Report it and reconnect rather than
                 * scanning forward for something that looks like a header. */
                publish(&state, "framing-error");
                break;
            }
            ssize_t got = recv(fd, buffer + held, sizeof(buffer) - held, 0);
            if (got == 0) {
                publish(&state, "peer-closed");
                break;
            }
            if (got < 0) {
                if (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK) {
                    /* Nothing arrived inside the read timeout. The link is not
                     * down; go round and let the loop decide. */
                    continue;
                }
                publish(&state, "read-failed");
                break;
            }
            held += (size_t)got;

            size_t consumed_total = 0;
            int lost = 0;
            for (;;) {
                chorus_frame_t frame =
                    chorus_decode_frame(buffer + consumed_total, held - consumed_total);
                if (frame.consumed == 0) {
                    /* Truncated header, or a declared length longer than what
                     * is in hand. Wait for more; consume nothing. */
                    break;
                }
                consumed_total += frame.consumed;

                switch (frame.outcome) {
                case CHORUS_FRAME_DECODED:
                    skipped_last = 0;
                    if (frame.message_type == CHORUS_MSG_AUDIO_CHUNK) {
                        const chorus_audio_chunk_t *chunk = &frame.message.audio_chunk;
                        size_t sample_bytes = chorus_sample_format_bytes(chunk->sample_format);
                        size_t frame_bytes = (size_t)chunk->channels * sample_bytes;
                        state.telemetry.chunks_played++;
                        state.telemetry.frames_played +=
                            (frame_bytes == 0) ? 0 : chunk->audio_data_len / frame_bytes;
                        state.telemetry.last_sequence = chunk->sequence;
                        state.telemetry.have_sequence = 1;
                        if (state.telemetry.audio == CHORUS_AUDIO_IDLE) {
                            state.telemetry.audio = CHORUS_AUDIO_RUNNING;
                        }
                    } else if (frame.message_type == CHORUS_MSG_TIME_SYNC &&
                               exchange_outstanding) {
                        chorus_time_sync_t exchange = frame.message.time_sync;
                        /* t3 is the endpoint's own receive stamp on the
                         * endpoint's clock. The server cannot know it and one
                         * that invented it would be handing us a round trip it
                         * made up. */
                        exchange.t3_ns = chorus_monotonic_now_ns();
                        if (exchange.t0_ns == pending_t0_ns) {
                            uint64_t rtt = chorus_time_sync_rtt_ns(&exchange);
                            int64_t offset = chorus_time_sync_offset_ns(&exchange);
                            double filtered = chorus_offset_filter_push(
                                &filter, (double)exchange.t3_ns, (double)rtt, (double)offset);
                            const chorus_sample_t *selected =
                                chorus_offset_filter_selected(&filter);
                            state.telemetry.offset_known = 1;
                            state.telemetry.offset_ns = (int64_t)filtered;
                            state.telemetry.round_trip_ns =
                                (selected == NULL) ? rtt : (uint64_t)selected->rtt_ns;
                            /* Half the round trip of the sample the offset came
                             * FROM, which RFC 5905 section 4 makes the bound on
                             * that offset. */
                            state.telemetry.bound_ns = state.telemetry.round_trip_ns / 2;
                            state.telemetry.exchanges++;
                            exchange_outstanding = 0;
                            next_exchange_ns =
                                chorus_monotonic_now_ns() +
                                (uint64_t)config->sync_interval_ms * 1000000ull;
                        }
                    }
                    break;

                case CHORUS_FRAME_SKIPPED_UNKNOWN_TYPE:
                    /* AC-7: skip the frame and keep the session open. This is
                     * rule 3 of docs/protocol.md's decoder behaviour, and it is
                     * the reading the endpoint takes deliberately - see
                     * docs/decisions/0015. */
                    state.telemetry.skipped_frames++;
                    skipped_last = 1;
                    break;

                case CHORUS_FRAME_PAYLOAD_TOO_SHORT_FOR_TYPE:
                case CHORUS_FRAME_INVALID_FIELD:
                    /* One frame, not the session - unless it follows a skip.
                     * docs/protocol.md: "What is not conformant is stepping
                     * over an uncorroborated length and then playing what
                     * follows." A rejected frame right after a skipped one is
                     * what lost alignment looks like from here, so the link is
                     * dropped instead. */
                    if (skipped_last) {
                        lost = 1;
                    }
                    break;

                default:
                    break;
                }
                if (lost) {
                    break;
                }
            }

            if (consumed_total > 0) {
                memmove(buffer, buffer + consumed_total, held - consumed_total);
                held -= consumed_total;
            }
            if (lost) {
                publish(&state, "alignment-lost-after-skip");
                break;
            }
        }

        close(fd);
        if (state.telemetry.chunks_played > chunks_at_connect) {
            out->connections_that_played++;
        }
        state.telemetry.link = CHORUS_LINK_DOWN;
        if (state.telemetry.audio != CHORUS_AUDIO_STOPPED_ON_AMP_FAULT) {
            state.telemetry.audio = CHORUS_AUDIO_IDLE;
        }
        went_down_ns = chorus_monotonic_now_ns();
        publish(&state, "link-down");

        if (run_ns != 0 && chorus_monotonic_now_ns() - started_ns >= run_ns) {
            break;
        }
        chorus_monotonic_sleep_ms(backoff_ms);
        backoff_ms = (backoff_ms * 2 > config->max_backoff_ms) ? config->max_backoff_ms
                                                               : backoff_ms * 2;
    }

    out->end = CHORUS_SESSION_RAN_ITS_TIME;
    out->telemetry = state.telemetry;
    publish(&state, "stop");
    if (state.log != NULL) {
        fclose(state.log);
    }
    return 0;
}
