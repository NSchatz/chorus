/* The endpoint's one clock.
 *
 * Monotonic, always. BRIEF.md guardrail 4 and the chorus protocol both say
 * every timestamp on this path is nanoseconds from a monotonic source, and a
 * stepped clock is what breaks playback. This is the ONLY unit in the endpoint
 * tree that reads a clock at all, which is what makes
 * firmware/check/endpoint-scan.c's settable-clock rule cheap to state and hard
 * to evade: every other unit takes the time it needs as an argument.
 *
 * On the target this is `esp_timer_get_time()`, which counts microseconds since
 * boot and is not settable. On a host it is `clock_gettime(CLOCK_MONOTONIC)`.
 * Neither can be moved by a human or by an NTP daemon. */

#ifndef CHORUS_MONOTONIC_H
#define CHORUS_MONOTONIC_H

#include <stdint.h>

/* Nanoseconds from an unspecified origin that never moves backwards and is
 * never stepped. The origin is meaningless; only differences mean anything,
 * which is the whole reason the time-sync exchange exists. */
uint64_t chorus_monotonic_now_ns(void);

/* Sleep for at least `ms` milliseconds. Interruptions are absorbed, so a
 * caller's backoff is a backoff and not a busy loop with extra steps. */
void chorus_monotonic_sleep_ms(uint32_t ms);

#endif /* CHORUS_MONOTONIC_H */
