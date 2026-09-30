# The one component. The shared units under firmware/src are compiled here
# exactly as the host build compiles them, from the same files, so the code
# that was graded on a host is the code that runs on the board.

idf_component_register(
    SRCS
        "app_main.c"
        "esp_hal.c"
        "../src/conf.c"
        "../src/protocol.c"
        "../src/sync_rng.c"
        "../src/sync_servo.c"
        "../src/sync_sim.c"
        "../src/sync_scenario.c"
        "../src/i2s.c"
        "../src/amp.c"
        "../src/telemetry.c"
        "../src/monotonic.c"
        "../src/session.c"
        "../src/wifi.c"
        "../src/endpoint_config.c"
    INCLUDE_DIRS
        "."
        "../include"
    # ESP-IDF v6 split the old `driver` component into one component per
    # peripheral (docs.espressif.com/projects/esp-idf/en/v6.1/esp32s3/
    # migration-guides/release-6.x/6.0/peripherals.html, read 2026-09-30); the
    # endpoint names the three it includes: driver/gpio.h, driver/i2c_master.h
    # and driver/i2s_std.h.
    REQUIRES
        esp_driver_gpio
        esp_driver_i2c
        esp_driver_i2s
        esp_timer
        esp_netif
        esp_wifi
        esp_event
        nvs_flash
        lwip
    EMBED_TXTFILES
        "../config/endpoint.conf"
)

# CHORUS_TARGET_ESP32S3 selects the target clock reader in
# firmware/src/monotonic.c. The endpoint's committed configuration is embedded
# rather than read from a filesystem the board does not have.
target_compile_definitions(${COMPONENT_LIB} PRIVATE CHORUS_TARGET_ESP32S3=1)
