/* The build gate.
 *
 * `make firmware-check` runs this over firmware/config/endpoint.conf before it
 * compiles anything else, so a pin assignment that claims a reserved GPIO, or a
 * 24-bit slot width with an MCLK multiple not divisible by three, REFUSES
 * RATHER THAN BUILDS. Every finding names the offending value and the rule it
 * broke, with the rule's source quoted, because "the build refused" is only
 * useful if it says what to change.
 *
 *   chorus-endpoint-config-check [path/to/endpoint.conf] */

#include "chorus/endpoint_config.h"

#include <stdio.h>

#define MAX_FINDINGS 32

int main(int argc, char **argv)
{
    const char *path = (argc > 1) ? argv[1] : chorus_endpoint_config_default_path();

    chorus_endpoint_config_t config;
    char detail[512];
    detail[0] = '\0';
    if (chorus_endpoint_config_load(&config, path, detail, sizeof(detail)) != 0) {
        fprintf(stderr, "FAIL endpoint-configuration-unreadable :: %s\n", detail);
        return 2;
    }

    chorus_finding_t findings[MAX_FINDINGS];
    size_t count = 0;
    chorus_endpoint_config_validate(&config, findings, MAX_FINDINGS, &count);

    for (size_t i = 0; i < count; i++) {
        fprintf(stderr, "FAIL %s :: %s\n", findings[i].rule, findings[i].detail);
    }
    if (count > 0) {
        fprintf(stderr,
                "chorus-endpoint-config-check: %s is refused, so nothing that depends on it is "
                "built. Fix the value the rule names, not the rule.\n",
                path);
        return 1;
    }

    printf("pass endpoint-configuration: %s\n", path);
    printf("  i2s            %u Hz, %u-bit slots, MCLK x%u (%llu Hz), BCLK %llu Hz, integral "
           "division %s\n",
           config.clock.sample_rate_hz, config.clock.slot_bit_width, config.clock.mclk_multiple,
           (unsigned long long)chorus_i2s_mclk_hz(&config.clock),
           (unsigned long long)chorus_i2s_bclk_hz(&config.clock),
           chorus_i2s_bclk_division_is_integral(&config.clock) ? "yes" : "no");
    printf("  dma            %u frames x %u descriptors, placed in %s memory\n",
           config.clock.dma_frame_num, config.clock.dma_desc_num,
           chorus_mem_placement_name(config.dma_placement));
    printf("  pins           mclk=%u bclk=%u ws=%u dout=%u sda=%u scl=%u amp_pdn=%u "
           "octal_psram=%s\n",
           config.pins.mclk, config.pins.bclk, config.pins.ws, config.pins.dout, config.pins.sda,
           config.pins.scl, config.pins.amp_power_down, config.pins.octal_psram ? "yes" : "no");
    printf("  analog gain    %.3f dB requested against a ceiling of %.3f dB\n", config.gain.db,
           config.amp.analog_gain_ceiling_db);
    printf("  amplifier      i2c address %s, gain code %s, registers %s\n",
           config.amp.address_known ? "configured" : "DECLARED UNKNOWN",
           config.gain.code_known ? "configured" : "DECLARED UNKNOWN",
           (config.amp.reg_device_id_known && config.amp.reg_fault_known &&
            config.amp.reg_analog_gain_known && config.amp.reg_state_control_known)
               ? "configured"
               : "DECLARED UNKNOWN");
    printf("  toolchain      esp-idf %s\n", config.espidf_version);
    return 0;
}
