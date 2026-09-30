#include "chorus/monotonic.h"

#if defined(CHORUS_TARGET_ESP32S3)

#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

uint64_t chorus_monotonic_now_ns(void)
{
    /* Microseconds since esp_timer's initialisation "shortly before the
     * app_main function is called"; no settable or steppable source, and safe
     * "in tasks as well as in ISR routines", which is why the playout path's
     * I2S interrupt stamps with it (the esp_timer guide in the pinned
     * ESP-IDF v6.1 tree, docs/en/api-reference/system/esp_timer.rst,
     * "Obtaining Current Time", read 2026-09-30).
     *
     * It is NOT the I2S clock's counter (audit A-18). On the ESP32-S3 the
     * esp_timer counts on the SYSTIMER, whose default source is the 40 MHz
     * XTAL (SYSTIMER_CLK_SRC_DEFAULT = SOC_MOD_CLK_XTAL), while I2S defaults
     * to PLL_F160M (I2S_CLK_SRC_DEFAULT = SOC_MOD_CLK_PLL_F160M) through a
     * fractional divider; both trace to the one external crystal, the root
     * the same header lists, but through different dividers and counters
     * (components/soc/esp32s3/include/soc/clk_tree_defs.h in the pinned
     * ESP-IDF v6.1, Apache-2.0, read 2026-09-30). So the endpoint never
     * assumes the DAC's rate from this clock: it measures it, by stamping the
     * frames the I2S DMA consumed on this clock (firmware/src/playout.c). */
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
