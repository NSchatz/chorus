#!/usr/bin/env bash
# The eFuse guard, run over a BUILT image (audit A-15 and A-16).
#
#   tools/firmware-image-guard.sh <builddir>     after `idf.py -B <builddir> build`
#   tools/firmware-image-guard.sh --self-test    no build, no toolchain
#
# Guardrail 2: no Secure Boot, no Flash Encryption and no anti-rollback eFuses
# on development hardware, because an eFuse bit set to 1 "cannot be reverted
# back to 0". firmware/check/endpoint_scan.c holds the SOURCE side of that
# before anything is built. This holds the OUTPUT side, and answers the two
# ways a source scan is bypassable by construction:
#
# 1. The configuration the build actually used. Every sdkconfig the build
#    generated is graded against firmware/check/efuse-kconfig.list (the same
#    list the source scan reads): the one project_description.json names as
#    `config_file`, and the config/sdkconfig.json the app and the bootloader
#    were each compiled against. An option passed on the idf.py command line,
#    through an environment SDKCONFIG_DEFAULTS, or left in a stale sdkconfig
#    never appears in a committed file, and appears here.
#
# 2. What the linker actually pulled in. Both linked ELFs are read, the app
#    (chorus-endpoint.elf) AND the bootloader (bootloader/bootloader.elf),
#    because Secure Boot and Flash Encryption burn their eFuses from the
#    BOOTLOADER on first boot and never from the app.
#
#    THE RULE, and why it is sound: ESP-IDF compiles every function into its
#    own section (tools/cmake/build.cmake:100, -ffunction-sections) and links
#    with --gc-sections (CMakeLists.txt:256 of ESP-IDF itself), so a function
#    still present in the linked ELF is one the linker found reachable from
#    the image's roots. A burn function that is present is therefore
#    reachable, and one that is absent cannot be called. The rule is "no
#    symbol on the list below is in the ELF at all": it over-approximates a
#    call graph (a reference through a function-pointer table counts, a call
#    guarded by a runtime condition counts), so it can go red on an image
#    that would never burn, and it cannot go green on one that can. Without
#    --gc-sections it would only get stricter, never looser.
#
#    The ROM is the one exception to "present means reachable": the ROM's own
#    eFuse programmer lives at fixed addresses that the ROM linker script
#    (components/esp_rom/esp32s3/ld/esp32s3.rom.ld:570-572 in v6.1) defines as
#    absolute symbols in EVERY image, used or not. For those the rule is the
#    address itself: the Xtensa `call8` reaches +/-512 KiB and the ROM sits
#    megabytes below IRAM and flash, so a call into the ROM has to load the
#    target from a 32-bit literal (or a pointer in data). The ROM burn entry
#    points are refused when their address appears as an aligned word in any
#    loaded section of the image.
#
#    On the stock image nothing on the list is present, so there is no
#    allowlist. System init CAN burn (components/efuse/src/esp_efuse_startup.c
#    :76-163), but only under the options firmware/check/efuse-kconfig.list
#    refuses, and with those off the linker drops every writer.
#
#    What this cannot see, stated rather than hidden: a burn written as raw
#    register stores in first-party code, with no ESP-IDF or ROM function
#    between it and the eFuse controller. The source scan refuses the
#    spellings of that (EFUSE_PGM_, EFUSE_WRITE_OP_CODE, efuse_ll_set_pgm_cmd
#    and the EFUSE register struct).
#
# THE LIST. Read out of ESP-IDF v5.3.6 (Apache-2.0), local copy
# /cache/esp/esp-idf-v5.3.6, 2026-09-30, and re-read out of ESP-IDF v6.1
# (commit fff9895c), the version firmware/config/endpoint.conf declares, local
# copy /cache/esp/esp-idf-v6.1, 2026-09-30 (chorus goal 6): every name is still
# declared where it cites, the ROM addresses are unchanged, and v6.1 adds two
# writers, esp_efuse_set_recovery_bootloader_offset and
# esp_flash_encryption_use_efuse_key, listed below. Every writer and burner each header declares; the readers
# (esp_efuse_read_*, esp_efuse_get_*, esp_flash_encryption_enabled,
# esp_secure_boot_verify_*) and the pure checks are left out on purpose, and
# the stock image does link some of them. When IDF_PATH is set, a real run
# first checks that every name here is still declared where it cites, so an
# ESP-IDF upgrade that renames a writer fails the guard instead of shrinking it.

source "$(dirname "$0")/lib.sh"

KCONFIG_LIST="$REPO_ROOT/firmware/check/efuse-kconfig.list"

# name|header, the header relative to $IDF_PATH/components.
BURN_SYMBOLS=(
    # components/efuse/include/esp_efuse.h: the public writers.
    "esp_efuse_write_field_blob|efuse/include/esp_efuse.h"
    "esp_efuse_write_field_cnt|efuse/include/esp_efuse.h"
    "esp_efuse_write_field_bit|efuse/include/esp_efuse.h"
    "esp_efuse_write_reg|efuse/include/esp_efuse.h"
    "esp_efuse_write_block|efuse/include/esp_efuse.h"
    "esp_efuse_write_key|efuse/include/esp_efuse.h"
    "esp_efuse_write_keys|efuse/include/esp_efuse.h"
    "esp_efuse_set_write_protect|efuse/include/esp_efuse.h"
    "esp_efuse_set_read_protect|efuse/include/esp_efuse.h"
    "esp_efuse_set_rom_log_scheme|efuse/include/esp_efuse.h"
    "esp_efuse_set_key_dis_read|efuse/include/esp_efuse.h"
    "esp_efuse_set_key_dis_write|efuse/include/esp_efuse.h"
    "esp_efuse_set_key_purpose|efuse/include/esp_efuse.h"
    "esp_efuse_set_keypurpose_dis_write|efuse/include/esp_efuse.h"
    "esp_efuse_set_digest_revoke|efuse/include/esp_efuse.h"
    "esp_efuse_set_write_protect_of_digest_revoke|efuse/include/esp_efuse.h"
    "esp_efuse_batch_write_begin|efuse/include/esp_efuse.h"
    "esp_efuse_batch_write_cancel|efuse/include/esp_efuse.h"
    "esp_efuse_batch_write_commit|efuse/include/esp_efuse.h"
    "esp_efuse_disable_rom_download_mode|efuse/include/esp_efuse.h"
    "esp_efuse_enable_rom_secure_download_mode|efuse/include/esp_efuse.h"
    "esp_efuse_disable_basic_rom_console|efuse/include/esp_efuse.h"
    "esp_efuse_enable_ecdsa_p192_curve_mode|efuse/include/esp_efuse.h"
    "esp_efuse_update_secure_version|efuse/include/esp_efuse.h"
    "esp_efuse_destroy_block|efuse/include/esp_efuse.h"
    "esp_efuse_set_recovery_bootloader_offset|efuse/include/esp_efuse.h"
    # components/efuse/private_include/esp_efuse_utility.h: where every public
    # writer ends up.
    "esp_efuse_utility_burn_efuses|efuse/private_include/esp_efuse_utility.h"
    "esp_efuse_utility_burn_chip|efuse/private_include/esp_efuse_utility.h"
    "esp_efuse_utility_burn_chip_opt|efuse/private_include/esp_efuse_utility.h"
    "esp_efuse_utility_write_blob|efuse/private_include/esp_efuse_utility.h"
    "esp_efuse_utility_write_cnt|efuse/private_include/esp_efuse_utility.h"
    "esp_efuse_utility_write_reg|efuse/private_include/esp_efuse_utility.h"
    "esp_efuse_utility_apply_new_coding_scheme|efuse/private_include/esp_efuse_utility.h"
    # components/hal/esp32s3/include/hal/efuse_hal.h: the programming command.
    "efuse_hal_program|hal/esp32s3/include/hal/efuse_hal.h"
    # components/bootloader_support/include/esp_flash_encrypt.h
    "esp_flash_encrypt_check_and_update|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_encrypt_init|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_encrypt_contents|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_encrypt_enable|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_encrypt_region|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_encryption_set_release_mode|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_encryption_enable_secure_features|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_write_protect_crypt_cnt|bootloader_support/include/esp_flash_encrypt.h"
    "esp_flash_encryption_use_efuse_key|bootloader_support/include/esp_flash_encrypt.h"
    # components/bootloader_support/include/esp_secure_boot.h
    "esp_secure_boot_permanently_enable|bootloader_support/include/esp_secure_boot.h"
    "esp_secure_boot_v2_permanently_enable|bootloader_support/include/esp_secure_boot.h"
    "esp_secure_boot_enable_secure_features|bootloader_support/include/esp_secure_boot.h"
)

# The ROM's eFuse programmer and key writer, and the Secure Boot key-digest
# revocation (which burns a revoke bit): components/esp_rom/esp32s3/ld/
# esp32s3.rom.ld:570,572 and :646 in v6.1 (573,575 in v5.3.6), same addresses.
ROM_BURN_SYMBOLS=(
    "ets_efuse_program|esp_rom/esp32s3/ld/esp32s3.rom.ld"
    "ets_efuse_write_key|esp_rom/esp32s3/ld/esp32s3.rom.ld"
    "ets_secure_boot_revoke_public_key_digest|esp_rom/esp32s3/ld/esp32s3.rom.ld"
)

# Families, on top of the exact names, so a sibling writer a later ESP-IDF adds
# is refused before anyone has read it.
BURN_FAMILIES='^(esp_efuse_(write|set|batch_write)_|esp_efuse_utility_burn_|esp_secure_boot_v?[0-9]*_?permanently_enable)'

# --- the configuration -----------------------------------------------------

# `refuse PREFIX` and `derived NAME` lines out of the committed list.
kconfig_rules() {
    if [ ! -r "$KCONFIG_LIST" ]; then
        note "FAIL efuse-kconfig-list-unreadable :: ${KCONFIG_LIST#"$REPO_ROOT"/} could not be read"
        return 2
    fi
    sed -n 's/^[[:space:]]*\(refuse\|derived\)[[:space:]]\{1,\}\(CONFIG_[A-Za-z0-9_]*\)[[:space:]]*=.*/\1 \2/p' \
        "$KCONFIG_LIST"
}

# Grade one sdkconfig-format file (`CONFIG_X=y`) or one sdkconfig.json
# (`"X": true`). Prints one FAIL line per refused option that is on.
grade_configuration() {
    local file="$1"
    local label="$2"
    local rules
    rules="$(kconfig_rules)" || return 2
    awk -v label="$label" -v rules="$rules" '
        BEGIN {
            n = split(rules, lines, "\n")
            for (i = 1; i <= n; i++) {
                split(lines[i], f, " ")
                if (f[1] == "refuse") { refuse[++nr] = f[2] }
                if (f[1] == "derived") { derived[f[2]] = 1 }
            }
        }
        {
            name = ""
            if ($0 ~ /^[[:space:]]*CONFIG_[A-Za-z0-9_]+[[:space:]]*=[[:space:]]*y[[:space:]]*$/) {
                name = $0
                sub(/^[[:space:]]*/, "", name)
                sub(/[[:space:]]*=.*/, "", name)
            } else if ($0 ~ /^[[:space:]]*"[A-Za-z0-9_]+"[[:space:]]*:[[:space:]]*true[[:space:]]*,?[[:space:]]*$/) {
                name = $0
                sub(/^[[:space:]]*"/, "", name)
                sub(/".*/, "", name)
                name = "CONFIG_" name
            }
            if (name == "" || (name in derived)) { next }
            for (i = 1; i <= nr; i++) {
                if (index(name, refuse[i]) == 1) {
                    printf "FAIL efuse-burning-option-in-the-generated-configuration %s:%d %s is on (refused as %s by firmware/check/efuse-kconfig.list)\n", label, NR, name, refuse[i]
                    break
                }
            }
        }
    ' "$file"
}

# --- the linked image --------------------------------------------------------

# Grade one image from its `nm` listing and the aligned 32-bit words of its
# loaded sections (one lowercase, little-endian hex word per line, as
# `objdump -s` prints them). Prints one FAIL line per finding.
grade_symbols() {
    local nm_file="$1"
    local words_file="$2"
    local label="$3"
    local exact="" rom="" entry
    for entry in "${BURN_SYMBOLS[@]}"; do exact="$exact ${entry%%|*}"; done
    for entry in "${ROM_BURN_SYMBOLS[@]}"; do rom="$rom ${entry%%|*}"; done
    awk -v label="$label" -v exact="$exact" -v rom="$rom" -v families="$BURN_FAMILIES" \
        -v words_file="$words_file" '
        BEGIN {
            n = split(exact, e, " ")
            for (i = 1; i <= n; i++) { burn[e[i]] = 1 }
            n = split(rom, r, " ")
            for (i = 1; i <= n; i++) { romburn[r[i]] = 1 }
            while ((getline w < words_file) > 0) { word[w] = 1 }
        }
        {
            # `nm` prints "addr type name", or "type name" for an undefined
            # symbol, which in a linked image is still a reference.
            if (NF >= 3) { addr = $1; type = $2; name = $3 }
            else if (NF == 2) { addr = ""; type = $1; name = $2 }
            else { next }
            if (type == "A" || type == "a") {
                if (!(name in romburn)) { next }
                a = sprintf("%08s", tolower(addr))
                gsub(/ /, "0", a)
                a = substr(a, length(a) - 7)
                le = substr(a, 7, 2) substr(a, 5, 2) substr(a, 3, 2) substr(a, 1, 2)
                if (le in word) {
                    printf "FAIL efuse-burn-reachable-in-the-image %s: the ROM eFuse writer %s (0x%s) is loaded as a call target\n", label, name, a
                }
                next
            }
            if ((name in burn) || name ~ families) {
                printf "FAIL efuse-burn-linked-into-the-image %s: %s %s survives --gc-sections, so something in the image reaches it\n", label, type, name
            }
        }
    ' "$nm_file"
}

# --- self-test ----------------------------------------------------------------
#
# Runnable with no build and no toolchain: the same two graders a real run
# uses, fed fabricated inputs. Each demonstration either goes red naming what
# was smuggled in, or the self-test fails. A guard that has only ever been
# green is a guard nobody has seen work.

self_test() {
    local scratch
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/chorus-image-guard.XXXXXX")"
    local failures=0

    expect_red() {
        local what="$1" out="$2" needle="$3"
        if printf '%s\n' "$out" | grep -q -- "$needle"; then
            say "  pass  $what goes red: $(printf '%s\n' "$out" | grep -m1 -- "$needle")"
        else
            say "  FAIL  $what did NOT go red on $needle (output: ${out:-<none>})"
            failures=$((failures + 1))
        fi
    }
    expect_green() {
        local what="$1" out="$2"
        if [ -z "$out" ]; then
            say "  pass  $what is green"
        else
            say "  FAIL  $what is red, so every demonstration below would prove nothing: $out"
            failures=$((failures + 1))
        fi
    }

    # A clean configuration in both formats, carrying exactly what the stock
    # esp32s3 build carries under SECURE_: the derived flags, two numbers, and
    # the refused options written as not set.
    cat >"$scratch/sdkconfig" <<'EOF'
CONFIG_IDF_TARGET="esp32s3"
CONFIG_SOC_SECURE_BOOT_SUPPORTED=y
CONFIG_SECURE_BOOT_V2_RSA_SUPPORTED=y
CONFIG_SECURE_BOOT_V2_PREFERRED=y
# CONFIG_SECURE_SIGNED_APPS_NO_SECURE_BOOT is not set
CONFIG_SECURE_BOOT_IMAGE_DIGEST_LEN=32
CONFIG_SECURE_BOOT_ROM_FAST_WAKE_RESERVE_SIZE=0
# CONFIG_SECURE_BOOT is not set
# CONFIG_SECURE_FLASH_ENC_ENABLED is not set
CONFIG_SECURE_ROM_DL_MODE_ENABLED=y
CONFIG_BOOT_ROM_LOG_ALWAYS_ON=y
# CONFIG_BOOT_ROM_LOG_ALWAYS_OFF is not set
# CONFIG_EFUSE_VIRTUAL is not set
EOF
    cat >"$scratch/sdkconfig.json" <<'EOF'
{
    "SECURE_BOOT": false,
    "SECURE_BOOT_IMAGE_DIGEST_LEN": 32,
    "SECURE_BOOT_V2_RSA_SUPPORTED": true,
    "SECURE_FLASH_ENC_ENABLED": false,
    "SECURE_ROM_DL_MODE_ENABLED": true,
    "BOOT_ROM_LOG_ALWAYS_ON": true
}
EOF
    say "chorus: firmware-image-guard self-test, the configuration"
    expect_green "a stock-shaped sdkconfig" "$(grade_configuration "$scratch/sdkconfig" sdkconfig)"
    expect_green "a stock-shaped sdkconfig.json" \
        "$(grade_configuration "$scratch/sdkconfig.json" sdkconfig.json)"

    local option
    for option in CONFIG_SECURE_FLASH_ENC_ENABLED CONFIG_SECURE_BOOT \
        CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK CONFIG_SECURE_DISABLE_ROM_DL_MODE \
        CONFIG_BOOT_ROM_LOG_ALWAYS_OFF CONFIG_NVS_SEC_KEY_PROTECT_USING_HMAC \
        CONFIG_FLASH_ENCRYPTION_ENABLED; do
        { cat "$scratch/sdkconfig"; printf '%s=y\n' "$option"; } >"$scratch/smuggled"
        expect_red "$option=y in a generated sdkconfig" \
            "$(grade_configuration "$scratch/smuggled" sdkconfig)" " $option is on"
    done
    sed 's/"SECURE_FLASH_ENC_ENABLED": false/"SECURE_FLASH_ENC_ENABLED": true/' \
        "$scratch/sdkconfig.json" >"$scratch/smuggled.json"
    expect_red "SECURE_FLASH_ENC_ENABLED true in the sdkconfig.json the code was compiled against" \
        "$(grade_configuration "$scratch/smuggled.json" sdkconfig.json)" \
        " CONFIG_SECURE_FLASH_ENC_ENABLED is on"

    # A clean image: the reads the stock app links, the ROM writers present as
    # the absolute symbols every image carries, and one ROM function that IS
    # called (esp_rom_printf, whose address is in the words), so the address
    # rule is shown telling a used ROM entry from an unused one.
    cat >"$scratch/nm" <<'EOF'
4200a1b0 T esp_efuse_read_field_blob
4200a2c0 T esp_efuse_check_errors
4200a3d0 T esp_efuse_utility_process
4200a4e0 T esp_flash_encryption_enabled
4200a5f0 T spi_flash_encryption_hal_enable
400005d0 A esp_rom_printf
40001e9c A ets_efuse_program
40001eb4 A ets_efuse_write_key
40002160 A ets_secure_boot_revoke_public_key_digest
EOF
    printf '%s\n' 00000000 d0050040 741f0040 ffffffff >"$scratch/words"
    say "chorus: firmware-image-guard self-test, the linked image"
    expect_green "a stock-shaped image" "$(grade_symbols "$scratch/nm" "$scratch/words" app)"

    local symbol
    for symbol in "T esp_efuse_write_field_bit" "t esp_efuse_utility_burn_chip" \
        "T esp_flash_encrypt_check_and_update" "T esp_secure_boot_v2_permanently_enable" \
        "T efuse_hal_program" "T esp_efuse_set_rom_log_scheme" \
        "U esp_efuse_write_key" "T esp_efuse_write_some_future_field"; do
        { cat "$scratch/nm"; printf '4200b000 %s\n' "$symbol"; } >"$scratch/nm-smuggled"
        if [ "${symbol%% *}" = "U" ]; then
            { cat "$scratch/nm"; printf '         %s\n' "$symbol"; } >"$scratch/nm-smuggled"
        fi
        expect_red "an image that links ${symbol#* }" \
            "$(grade_symbols "$scratch/nm-smuggled" "$scratch/words" app)" " ${symbol#* } "
    done
    { cat "$scratch/words"; printf '9c1e0040\n'; } >"$scratch/words-smuggled"
    expect_red "an image that loads the ROM programmer's address" \
        "$(grade_symbols "$scratch/nm" "$scratch/words-smuggled" bootloader)" \
        "ROM eFuse writer ets_efuse_program"

    rm -rf "$scratch"
    if [ "$failures" -ne 0 ]; then
        say "FAIL firmware-image-guard-self-test: $failures demonstration(s) did not behave"
        return 1
    fi
    say "pass firmware-image-guard-self-test: a stock-shaped configuration and image are green, and every smuggled option, writer and ROM call goes red by name"
}

# --- a real build -------------------------------------------------------------

CRITERION="guardrail 2: an image built from this tree enables nothing that burns an eFuse (audit A-15, A-16)"

# Every name on the lists is still declared where it cites, in the ESP-IDF this
# image was built with.
check_list_is_current() {
    local entry name header stale=0
    for entry in "${BURN_SYMBOLS[@]}" "${ROM_BURN_SYMBOLS[@]}"; do
        name="${entry%%|*}"
        header="$IDF_PATH/components/${entry#*|}"
        if ! grep -qw -- "$name" "$header" 2>/dev/null; then
            note "FAIL efuse-burn-list-stale :: $name is not declared in ${entry#*|} under $IDF_PATH; re-derive the list in tools/firmware-image-guard.sh from this ESP-IDF's headers"
            stale=1
        fi
    done
    return "$stale"
}

# The aligned 32-bit words of every loaded section that has contents.
image_words() {
    local elf="$1"
    local sections=() section
    while IFS= read -r section; do
        sections+=(-j "$section")
    done < <("$READELF" -SW "$elf" | awk '
        {
            for (i = 1; i <= NF; i++) {
                if ($i == "PROGBITS") {
                    name = $(i - 1); size = $(i + 3); flags = $(i + 5)
                    if (flags ~ /A/ && size !~ /^0+$/) { print name }
                }
            }
        }')
    if [ "${#sections[@]}" -eq 0 ]; then
        note "FAIL image-has-no-loaded-sections :: $elf"
        return 2
    fi
    "$OBJDUMP" -s "${sections[@]}" "$elf" | awk '/^ [0-9a-f]+ / { for (i = 2; i <= 5; i++) if (length($i) == 8) print $i }'
}

guard_build() {
    local build_dir="$1"
    local findings="" out

    if [ -z "${IDF_PATH:-}" ]; then
        missing_prerequisite "$CRITERION" \
            "the exported ESP-IDF environment (IDF_PATH unset)" \
            ". \$IDF_PATH/export.sh, the same environment the image was built in"
    fi
    NM="xtensa-esp32s3-elf-nm"
    OBJDUMP="xtensa-esp32s3-elf-objdump"
    READELF="xtensa-esp32s3-elf-readelf"
    local tool
    for tool in "$NM" "$OBJDUMP" "$READELF"; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            missing_prerequisite "$CRITERION" "$tool, from the ESP-IDF esp32s3 toolchain" \
                ". \$IDF_PATH/export.sh"
        fi
    done

    local description="$build_dir/project_description.json"
    local config_file=""
    if [ -r "$description" ]; then
        config_file="$(sed -n 's/^[[:space:]]*"config_file":[[:space:]]*"\(.*\)",\{0,1\}[[:space:]]*$/\1/p' \
            "$description" | head -n 1)"
    fi
    local app_elf="$build_dir/chorus-endpoint.elf"
    local boot_elf="$build_dir/bootloader/bootloader.elf"
    local input
    for input in "$config_file" "$build_dir/config/sdkconfig.json" \
        "$build_dir/bootloader/config/sdkconfig.json" "$app_elf" "$boot_elf"; do
        if [ -z "$input" ] || [ ! -r "$input" ]; then
            note "FAIL image-guard-input-missing :: ${input:-the config_file named by $description} is not readable; build first with idf.py -B $build_dir build"
            return 2
        fi
    done

    say "chorus: firmware-image-guard over $build_dir"
    say "  configuration: $config_file, config/sdkconfig.json, bootloader/config/sdkconfig.json"
    say "  images:        chorus-endpoint.elf, bootloader/bootloader.elf"
    say "  lists:         firmware/check/efuse-kconfig.list; ${#BURN_SYMBOLS[@]} writers and ${#ROM_BURN_SYMBOLS[@]} ROM entry points (this file)"

    check_list_is_current || return 1

    out="$(grade_configuration "$config_file" "$config_file")" || return 2
    findings="$findings$out"
    out="$(grade_configuration "$build_dir/config/sdkconfig.json" config/sdkconfig.json)" || return 2
    findings="$findings${out:+$'\n'$out}"
    out="$(grade_configuration "$build_dir/bootloader/config/sdkconfig.json" \
        bootloader/config/sdkconfig.json)" || return 2
    findings="$findings${out:+$'\n'$out}"

    local scratch
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/chorus-image-guard.XXXXXX")"
    local elf label word_count
    for label in app bootloader; do
        if [ "$label" = app ]; then elf="$app_elf"; else elf="$boot_elf"; fi
        "$NM" "$elf" >"$scratch/$label.nm"
        image_words "$elf" >"$scratch/$label.words" || { rm -rf "$scratch"; return 2; }
        word_count="$(wc -l <"$scratch/$label.words" | tr -d ' ')"
        say "  $label: $(wc -l <"$scratch/$label.nm" | tr -d ' ') symbols, $word_count loaded words"
        out="$(grade_symbols "$scratch/$label.nm" "$scratch/$label.words" "$label")"
        findings="$findings${out:+$'\n'$out}"
    done
    rm -rf "$scratch"

    findings="$(printf '%s\n' "$findings" | sed '/^$/d')"
    if [ -n "$findings" ]; then
        printf '%s\n' "$findings" >&2
        return 1
    fi
    say "pass firmware-image-guard: the generated configuration enables no option that burns an eFuse, and neither the app nor the bootloader links an eFuse writer or loads a ROM eFuse writer's address"
}

case "${1:-}" in
    --self-test)
        self_test
        ;;
    "" | -h | --help)
        say "usage: tools/firmware-image-guard.sh <idf.py build dir> | --self-test"
        exit 2
        ;;
    *)
        guard_build "$1"
        ;;
esac
