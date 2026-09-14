# The endpoint's host build and its host-runnable verification.
#
# Needs a C compiler and nothing else of its own. No ESP-IDF, no device, no
# privilege, no network beyond loopback. That is deliberate: the sync and
# protocol cores are pure libraries held to committed fixtures, the amplifier
# sequencer is driven through an injectable I2C transport, and the session
# supervisor is graded against a real chorus-server on loopback. Everything
# that genuinely needs hardware lives behind an entry point under tools/ that
# refuses by name.
#
# Invoked from the repository root as `make firmware-check`.

REPO_ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)
FW := $(REPO_ROOT)/firmware
BUILD ?= $(FW)/build

CC ?= cc

# -ffp-contract=off is load-bearing and not a style choice: the sync core has
# to select the same sample as the Rust implementation on the same inputs, and
# a fused multiply-add would round differently from the two operations the Rust
# side compiles to. -fno-fast-math for the same reason.
CFLAGS ?= -std=c11 -O2 -g
CFLAGS += -Wall -Wextra -Werror -Wshadow -Wpointer-arith -Wstrict-prototypes
CFLAGS += -ffp-contract=off -fno-fast-math
CFLAGS += -D_POSIX_C_SOURCE=200809L
CFLAGS += -I$(FW)/include
CFLAGS += -DCHORUS_REPO_ROOT='"$(REPO_ROOT)"'
CFLAGS += -DCHORUS_ENDPOINT_CONFIG_PATH='"$(FW)/config/endpoint.conf"'
CFLAGS += -DCHORUS_CONFIG_CHECK_BIN='"$(BUILD)/chorus-endpoint-config-check"'
LDLIBS += -lm

LIB_SRC := \
	$(FW)/src/conf.c \
	$(FW)/src/protocol.c \
	$(FW)/src/sync_rng.c \
	$(FW)/src/sync_servo.c \
	$(FW)/src/sync_sim.c \
	$(FW)/src/sync_scenario.c \
	$(FW)/src/i2s.c \
	$(FW)/src/amp.c \
	$(FW)/src/telemetry.c \
	$(FW)/src/monotonic.c \
	$(FW)/src/session.c \
	$(FW)/src/wifi.c \
	$(FW)/src/endpoint_config.c

# The four units the configuration checker itself needs. It is built without
# the gate below, because it IS the gate. wifi.c is here because the reader
# turns the link section's words into the enums that section declares, and a
# checker that could not read the link could not refuse a bad one.
GATE_SRC := $(FW)/src/conf.c $(FW)/src/i2s.c $(FW)/src/wifi.c $(FW)/src/endpoint_config.c

SCAN_SRC := $(FW)/check/endpoint_scan.c

# The simulated part, output stage, I2S controller and radio. Test-only, and
# outside firmware/src on purpose: they are what the endpoint is graded AGAINST,
# and a fake inside the tree the scans cover would be endpoint source.
TEST_SUPPORT := $(FW)/tests/fake_amp.c $(FW)/tests/fake_radio.c

STAMP := $(BUILD)/config-ok.stamp

.PHONY: all check config-check golden-vectors sync-scenarios safety-scans amplifier clocking \
        telemetry wireless session-outage clean

# Every binary this makefile builds, enumerated. There is no wildcard over
# firmware/tests on purpose, and the cost of that is a test has to be named in
# TWO places to run at all: here, and in the run target `check` depends on. A
# test absent from either is a grading route pointing at something nothing
# builds.
all: $(BUILD)/chorus-endpoint-config-check $(BUILD)/chorus-endpoint-scan \
     $(BUILD)/chorus-endpoint-session $(BUILD)/test_protocol $(BUILD)/test_sync \
     $(BUILD)/test_amp $(BUILD)/test_i2s $(BUILD)/test_scan $(BUILD)/test_telemetry \
     $(BUILD)/test_wifi

$(BUILD):
	mkdir -p $(BUILD)

# --- the configuration gate --------------------------------------------------
#
# AC-11 asks the endpoint to "refuse rather than build" when a pin assignment
# claims a GPIO the ESP-IDF reference reserves, or when a 24-bit slot width is
# configured with an MCLK multiple not divisible by three. This is where that
# is true: nothing below compiles until the checker has passed over
# firmware/config/endpoint.conf, and the checker names the offending value and
# the rule it broke.
$(BUILD)/chorus-endpoint-config-check: $(GATE_SRC) $(FW)/check/main_config_check.c | $(BUILD)
	$(CC) $(CFLAGS) -o $@ $^ $(LDLIBS)

$(STAMP): $(BUILD)/chorus-endpoint-config-check $(FW)/config/endpoint.conf
	$(BUILD)/chorus-endpoint-config-check $(FW)/config/endpoint.conf
	touch $@

config-check: $(STAMP)

# --- binaries ----------------------------------------------------------------

$(BUILD)/chorus-endpoint-scan: $(SCAN_SRC) $(FW)/check/main_endpoint_scan.c | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/check -o $@ $^ $(LDLIBS)

$(BUILD)/chorus-endpoint-session: $(LIB_SRC) $(FW)/tests/main_session.c $(STAMP) | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/tests -o $@ $(LIB_SRC) $(FW)/tests/main_session.c $(LDLIBS)

# The scan's own tests link the scanner and not the endpoint library: the
# scanner is host tooling and deliberately outside the tree it scans.
$(BUILD)/test_scan: $(SCAN_SRC) $(FW)/tests/test_scan.c | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/check -I$(FW)/tests -o $@ $(SCAN_SRC) $(FW)/tests/test_scan.c $(LDLIBS)

$(BUILD)/test_protocol $(BUILD)/test_sync $(BUILD)/test_amp $(BUILD)/test_i2s \
$(BUILD)/test_telemetry $(BUILD)/test_wifi: $(BUILD)/test_%: $(LIB_SRC) $(TEST_SUPPORT) \
                         $(FW)/tests/test_%.c $(STAMP) | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/tests -o $@ $(LIB_SRC) $(TEST_SUPPORT) $(FW)/tests/test_$*.c $(LDLIBS)

# --- what CI names separately ------------------------------------------------

golden-vectors: $(BUILD)/test_protocol
	$(BUILD)/test_protocol

sync-scenarios: $(BUILD)/test_sync
	$(BUILD)/test_sync

safety-scans: $(BUILD)/chorus-endpoint-scan $(BUILD)/test_scan
	$(BUILD)/chorus-endpoint-scan $(REPO_ROOT)
	$(BUILD)/test_scan

amplifier: $(BUILD)/test_amp
	$(BUILD)/test_amp

clocking: $(BUILD)/test_i2s
	$(BUILD)/test_i2s

telemetry: $(BUILD)/test_telemetry
	$(BUILD)/test_telemetry

# chorus#WIFI-7: the power save mode the endpoint SETS, the readback, the two
# ways that mode can fail to be in effect, and the refusal to join on a
# credential this repository does not have. No radio, no ESP32-S3, no network.
wireless: $(BUILD)/test_wifi
	$(BUILD)/test_wifi

session-outage: $(BUILD)/chorus-endpoint-session
	bash $(FW)/tests/session-outage.sh

check: config-check golden-vectors sync-scenarios safety-scans clocking amplifier telemetry \
       wireless session-outage

clean:
	rm -rf $(BUILD)
