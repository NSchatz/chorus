#include "chorus/monotonic.h"

#if defined(CHORUS_TARGET_ESP32S3)

#include "esp_timer.h"

uint64_t chorus_monotonic_now_ns(void)
{
    /* Microseconds since boot. Not settable, not steppable, and the same
     * counter the I2S driver's own timing is derived from. */
    return (uint64_t)esp_timer_get_time() * 1000ull;
}

void chorus_monotonic_sleep_ms(uint32_t ms)
{
    vTaskDelay(pdMS_TO_TICKS(ms));
}

#else

#include <errno.h>
#include <time.h>

uint64_t chorus_monotonic_now_ns(void)
{
    struct timespec ts;
    /* The one clock read in the endpoint tree, and it names the monotonic
     * source. `time_namespaces(7)` confirms this is the clock a container may
     * have offset and may never have stepped. */
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) {
        return 0;
    }
    return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

void chorus_monotonic_sleep_ms(uint32_t ms)
{
    struct timespec wanted;
    wanted.tv_sec = (time_t)(ms / 1000u);
    wanted.tv_nsec = (long)(ms % 1000u) * 1000000L;
    struct timespec remaining;
    while (nanosleep(&wanted, &remaining) != 0 && errno == EINTR) {
        wanted = remaining;
    }
}

#endif
