# The one component. The shared units under firmware/src are compiled here
# exactly as the host build compiles them, from the same files, so the code
# that was graded on a host is the code that runs on the board.

# The decoders (goal 6): the codec seam and the vendored dr_flac and libopus
# (third_party/README.md). libopus's definitions and units come from the one
# list the host build and the Rust client read, third_party/opus/chorus-build.txt.
set(CHORUS_THIRD_PARTY "${CMAKE_CURRENT_LIST_DIR}/../../third_party")
file(STRINGS "${CHORUS_THIRD_PARTY}/opus/chorus-build.txt" chorus_opus_lines)
set(chorus_opus_srcs "")
set(chorus_opus_defs "")
foreach(line IN LISTS chorus_opus_lines)
    if(line MATCHES "^define (.+)$")
        list(APPEND chorus_opus_defs "${CMAKE_MATCH_1}")
    elseif(line MATCHES "^source (.+)$")
        list(APPEND chorus_opus_srcs "${CHORUS_THIRD_PARTY}/opus/${CMAKE_MATCH_1}")
    endif()
endforeach()
set(chorus_drflac_defs DR_FLAC_NO_STDIO DR_FLAC_NO_OGG DR_FLAC_NO_WCHAR DR_FLAC_NO_SIMD)

idf_component_register(
    SRCS
        "app_main.c"
        "esp_hal.c"
        "../src/conf.c"
        "../src/protocol.c"
        "../src/protocol_v2.c"
        "../src/noise.c"
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
        "../src/codec.c"
        "../src/codec_flac.c"
        "../src/codec_opus.c"
        "${CHORUS_THIRD_PARTY}/dr_flac/dr_flac.c"
        ${chorus_opus_srcs}
    INCLUDE_DIRS
        "."
        "../include"
        "${CHORUS_THIRD_PARTY}/dr_flac"
        "${CHORUS_THIRD_PARTY}/opus/include"
    PRIV_INCLUDE_DIRS
        "${CHORUS_THIRD_PARTY}/opus/celt"
        "${CHORUS_THIRD_PARTY}/opus/silk"
        "${CHORUS_THIRD_PARTY}/opus/src"
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
        mbedtls
    EMBED_TXTFILES
        "../config/endpoint.conf"
)

# CHORUS_TARGET_ESP32S3 selects the target clock reader in
# firmware/src/monotonic.c. The endpoint's committed configuration is embedded
# rather than read from a filesystem the board does not have.
target_compile_definitions(${COMPONENT_LIB} PRIVATE CHORUS_TARGET_ESP32S3=1)

# The vendored units are upstream's code, byte for byte: their own definitions,
# the floating-point flags every C unit here gets, and no warnings (they are
# upstream's to fix; chorus's codec units keep the component's flags). The
# dr_flac configuration applies wherever its header is included.
set_source_files_properties(${chorus_opus_srcs} PROPERTIES
    COMPILE_DEFINITIONS "${chorus_opus_defs}"
    COMPILE_OPTIONS "-w;-ffp-contract=off;-fno-fast-math")
set_source_files_properties("${CHORUS_THIRD_PARTY}/dr_flac/dr_flac.c" PROPERTIES
    COMPILE_OPTIONS "-w;-ffp-contract=off;-fno-fast-math")
target_compile_definitions(${COMPONENT_LIB} PRIVATE ${chorus_drflac_defs})
