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

# The board profile (goal 8, docs/decisions/0057-*): firmware/boards/<profile>.conf,
# chosen at build time with -D CHORUS_BOARD_PROFILE=<profile> (tools/firmware-image.sh
# passes it) and otherwise the default endpoint.conf names. It is copied under
# one fixed name so its embedded symbol does not depend on which profile it is,
# and embedded beside endpoint.conf; app_main reads both through one reader.
if(NOT CHORUS_BOARD_PROFILE)
    file(STRINGS "${CMAKE_CURRENT_LIST_DIR}/../config/endpoint.conf" chorus_default_profile
        REGEX "^board_profile *= *")
    string(REGEX REPLACE "^board_profile *= *" "" CHORUS_BOARD_PROFILE "${chorus_default_profile}")
endif()
set(chorus_board_file "${CMAKE_CURRENT_LIST_DIR}/../boards/${CHORUS_BOARD_PROFILE}.conf")
if(NOT EXISTS "${chorus_board_file}")
    message(FATAL_ERROR "chorus: no board profile ${chorus_board_file}")
endif()
# Not during ESP-IDF's early expansion, which runs this file in script mode to
# learn its requirements, with the build directory's root as its binary
# directory and without the -D values (docs/en/api-guides/build-system.rst:1684
# in the pinned v6.1): there the copy landed at <build>/board_profile.conf and
# named the DEFAULT profile whatever was built, and that is the file
# tools/firmware-flash.sh and tools/firmware-stage.sh read as "the profile the
# build recorded" (found by the emulator run, goal 14). tools/firmware-image.sh
# now puts the real copy there after the build.
if(NOT CMAKE_BUILD_EARLY_EXPANSION)
    configure_file("${chorus_board_file}" "${CMAKE_CURRENT_BINARY_DIR}/board_profile.conf" COPYONLY)
    message(STATUS "chorus: board profile ${CHORUS_BOARD_PROFILE}")
endif()

# Wi-Fi provisioning (goal 14, docs/decisions/0103-wifi-provisioning-over-softap.md):
# the binding to the access point, the HTTP server and the pinned
# espressif/network_provisioning manager is compiled only into an image whose
# board profile is wireless. A wired speaker never provisions, and its image
# carries none of it. The decision is read from the profile itself, so a
# second wireless profile gets the binding without being named here.
file(STRINGS "${chorus_board_file}" chorus_wireless_profile REGEX "^link_transport *= *wireless")
set(chorus_profile_srcs "")
if(chorus_wireless_profile)
    list(APPEND chorus_profile_srcs "esp_provision.c")
    message(STATUS "chorus: Wi-Fi provisioning is in this image")
endif()

# The subwoofer's knobs (ADR 0248): esp_knobs.c is compiled only into an image
# whose board profile wires a knob. Its ADC calls link esp_adc's adc_common.c,
# whose constructor (adc_hw_calibration, components/esp_adc/adc_common.c:59 in
# the pinned v6.1) calibrates the SAR ADC before app_main on every boot and
# waits for each conversion with no timeout (esp_hal_ana_conv/adc_hal_common.c
# :126). The emulator models no SAR ADC, so its boot stopped there (the nightly
# qemu-boot and ota-qemu of 2026-10-07). A board without knobs carries none of
# it, and the call in app_main is then nothing (esp_knobs.h).
file(STRINGS "${chorus_board_file}" chorus_knobs_profile REGEX "^pin_knob_(level|phase) *= *[0-9]")
if(chorus_knobs_profile)
    list(APPEND chorus_profile_srcs "esp_knobs.c")
    message(STATUS "chorus: the subwoofer's knobs are in this image")
endif()

idf_component_register(
    SRCS
        "app_main.c"
        "esp_hal.c"
        "esp_link.c"
        "esp_playout.c"
        "esp_marker.c"
        "console_esp.c"
        "esp_ota.c"
        "esp_store.c"
        ${chorus_profile_srcs}
        "esp_identity.c"
        "esp_discovery.c"
        "../src/conf.c"
        "../src/protocol.c"
        "../src/protocol_v2.c"
        "../src/noise.c"
        "../src/lowlat.c"
        "../src/sync_rng.c"
        "../src/sync_servo.c"
        "../src/sync_sim.c"
        "../src/sync_scenario.c"
        "../src/i2s.c"
        "../src/amp.c"
        "../src/line_dac.c"
        "../src/telemetry.c"
        "../src/monotonic.c"
        "../src/session.c"
        "../src/wifi.c"
        "../src/link.c"
        "../src/endpoint_config.c"
        "../src/sync_conf.c"
        "../src/playout.c"
        "../src/volume.c"
        "../src/controls.c"
        "../src/dsp.c"
        "../src/endpoint_dsp.c"
        "../src/codec.c"
        "../src/codec_flac.c"
        "../src/codec_opus.c"
        "../src/console.c"
        "../src/decode_cost.c"
        "../src/dsp_cost.c"
        "../src/ota.c"
        "../src/store.c"
        "../src/provision.c"
        "../src/identity.c"
        "../src/discovery.c"
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
    # and driver/i2s_std.h, and driver/gptimer.h for the GPIO marker (goal 9);
    # esp_adc for the subwoofer's knobs (esp_adc/adc_oneshot.h), named for
    # every profile like esp_http_server below; only a profile with knobs
    # compiles esp_knobs.c, so only its image links the ADC.
    REQUIRES
        esp_adc
        esp_driver_gpio
        esp_driver_gptimer
        esp_driver_i2c
        esp_driver_i2s
        esp_driver_spi
        esp_eth
        esp_timer
        esp_netif
        esp_wifi
        esp_event
        nvs_flash
        lwip
        mbedtls
        console
        # The firmware update (goal 14): esp_ota.c is the one unit that names
        # app_update's calls (firmware/check/endpoint_scan.c, OTA_GLUE_UNIT).
        app_update
        esp_app_format
        esp_partition
        # The join form's server (goal 14). Named for every profile because a
        # component's requirements are fixed before the profile is known to
        # CMake; only the Wi-Fi profile's binding calls it, and the linker
        # leaves it out of the wired image.
        esp_http_server
    EMBED_TXTFILES
        "../config/endpoint.conf"
        "${CMAKE_CURRENT_BINARY_DIR}/board_profile.conf"
        # audit A-12: the endpoint reads the loop's constants from the file
        # the Linux client is held to, not from literals.
        "../../config/sync.conf"
    # The endpoint console's decode-cost fixtures (audit A-13, goal 7's S3
    # decode-cost measurement): one FLAC and one Opus stream from fixtures/codec,
    # the files the host test and the Linux client read.
    EMBED_FILES
        "../../fixtures/codec/flac-s16-stereo-44k1.fields"
        "../../fixtures/codec/flac-s16-stereo-44k1.chunks"
        "../../fixtures/codec/opus-tv10-celt-stereo.fields"
        "../../fixtures/codec/opus-tv10-celt-stereo.chunks"
)

# CHORUS_TARGET_ESP32S3 selects the target clock reader in
# firmware/src/monotonic.c. The endpoint's committed configuration is embedded
# rather than read from a filesystem the board does not have.
target_compile_definitions(${COMPONENT_LIB} PRIVATE CHORUS_TARGET_ESP32S3=1)
if(chorus_wireless_profile)
    target_compile_definitions(${COMPONENT_LIB} PRIVATE CHORUS_PROVISIONING=1)
endif()
if(chorus_knobs_profile)
    target_compile_definitions(${COMPONENT_LIB} PRIVATE CHORUS_KNOBS=1)
endif()
# The emulator run's bad image (goal 14, docs/decisions/0111-*): the update
# unit built never to confirm its trial (firmware/main/esp_ota.c), so the
# bootloader's rollback is exercised. tools/firmware-image.sh passes
# CHORUS_OTA_NEVER_CONFIRM=0 or 1; only the emulator's profile may be 1.
if(CHORUS_OTA_NEVER_CONFIRM)
    file(STRINGS "${chorus_board_file}" chorus_emulated_profile REGEX "^link_transport *= *emulated")
    if(NOT chorus_emulated_profile)
        message(FATAL_ERROR "chorus: CHORUS_OTA_NEVER_CONFIRM is for the emulator's board only, "
            "and ${CHORUS_BOARD_PROFILE} is not it")
    endif()
    target_compile_definitions(${COMPONENT_LIB} PRIVATE CHORUS_OTA_NEVER_CONFIRM=1)
    message(STATUS "chorus: this image never confirms its trial (the emulator run's bad image)")
endif()

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
