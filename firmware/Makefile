# The endpoint's host build and its host-runnable verification.
#
# Needs a C compiler and the SOURCE tree of the pinned ESP-IDF (CHORUS_IDF_V61_DIR),
# whose TF-PSA-Crypto the v2 session's crypto is compiled from, so the host runs
# the crypto the image runs (docs/decisions/0043-*). No ESP-IDF toolchain, no
# device, no privilege, no network beyond loopback. That is deliberate: the sync and
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

# --- the crypto library, compiled from the pinned ESP-IDF tree ----------------
#
# Protocol v2's key exchange (firmware/src/noise.c) reaches its primitives only
# through the PSA Crypto API. The device build gets that API from ESP-IDF's own
# mbedtls component; this host build compiles the same library, TF-PSA-Crypto
# 1.1.0, from the pinned ESP-IDF v6.1 checkout (commit
# fff9895c82d744c7237be8847347bdd1b07c6643), with firmware/crypto's minimal
# configuration. Nothing of it is copied into this repository: an absent tree is
# refused by name rather than worked around.
CHORUS_IDF_V61_DIR ?= /cache/esp/esp-idf-v6.1
CHORUS_IDF_V61_COMMIT := fff9895c82d744c7237be8847347bdd1b07c6643
PSA_DIR ?= $(CHORUS_IDF_V61_DIR)/components/mbedtls/mbedtls/tf-psa-crypto
PSA_BUILD := $(BUILD)/tf-psa-crypto
PSA_LIB := $(PSA_BUILD)/libchorus-tf-psa-crypto.a
PSA_INCLUDES := -I$(FW)/crypto -I$(FW)/crypto/include -I$(PSA_DIR)/include \
	-I$(PSA_DIR)/drivers/builtin/include
PSA_PRIVATE_INCLUDES := -I$(PSA_DIR)/core -I$(PSA_DIR)/drivers/builtin/src -I$(PSA_DIR)/dispatch \
	-I$(PSA_DIR)/platform -I$(PSA_DIR)/utilities -I$(PSA_DIR)/extras
PSA_CONFIG := -DTF_PSA_CRYPTO_CONFIG_FILE='"chorus_psa_config.h"'
# Every library source; the configuration decides what each one compiles to,
# and the linker takes only what noise.c reaches. Not the endpoint's code, so
# not held to its -Werror, but compiled with its floating-point flags anyway.
PSA_SRC := $(sort $(wildcard $(PSA_DIR)/core/*.c $(PSA_DIR)/drivers/builtin/src/*.c \
	$(PSA_DIR)/platform/*.c $(PSA_DIR)/utilities/*.c $(PSA_DIR)/extras/*.c))
PSA_OBJ := $(patsubst $(PSA_DIR)/%.c,$(PSA_BUILD)/%.o,$(PSA_SRC))
PSA_CFLAGS := -std=c11 -O2 -ffp-contract=off -fno-fast-math -D_POSIX_C_SOURCE=200809L

CFLAGS += $(PSA_INCLUDES) $(PSA_CONFIG)

LIB_SRC := \
	$(FW)/src/conf.c \
	$(FW)/src/protocol.c \
	$(FW)/src/protocol_v2.c \
	$(FW)/src/noise.c \
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
	$(FW)/src/link.c \
	$(FW)/src/endpoint_config.c \
	$(FW)/src/console.c \
	$(FW)/src/decode_cost.c \
	$(FW)/src/sync_conf.c \
	$(FW)/src/playout.c \
	$(FW)/src/volume.c \
	$(FW)/src/controls.c \
	$(FW)/src/dsp.c

# The four units the configuration checker itself needs. It is built without
# the gate below, because it IS the gate. wifi.c is here because the reader
# turns the link section's words into the enums that section declares, and a
# checker that could not read the link could not refuse a bad one.
GATE_SRC := $(FW)/src/conf.c $(FW)/src/i2s.c $(FW)/src/wifi.c $(FW)/src/link.c \
	$(FW)/src/endpoint_config.c $(FW)/src/amp.c $(FW)/src/volume.c

# The board profiles (firmware/boards/, docs/decisions/0057-*): each is laid over
# endpoint.conf and checked the way the image reads it, and the default one,
# named by endpoint.conf's board_profile, has to agree with endpoint.conf.
BOARDS := $(sort $(wildcard $(FW)/boards/*.conf))
DEFAULT_BOARD := $(FW)/boards/$(shell sed -n 's/^board_profile *= *//p' $(FW)/config/endpoint.conf).conf

SCAN_SRC := $(FW)/check/endpoint_scan.c

# The simulated part, output stage, I2S controller and radio. Test-only, and
# outside firmware/src on purpose: they are what the endpoint is graded AGAINST,
# and a fake inside the tree the scans cover would be endpoint source.
TEST_SUPPORT := $(FW)/tests/fake_amp.c $(FW)/tests/fake_radio.c

STAMP := $(BUILD)/config-ok.stamp

# --- the decoders (goal 6, proposal P9) ----------------------------------------
#
# FLAC and Opus for coded_chunk, over two vendored libraries whose pins are in
# third_party/README.md. The vendored code is compiled with the floating-point
# flags above (no contraction, no fast math) but without the warning set: it is
# upstream's code, kept byte for byte, and its warnings are upstream's to fix.
# The chorus units that call it (firmware/src/codec*.c) take the full CFLAGS,
# and see the vendored headers through -isystem so that a warning in a header
# chorus does not own cannot fail chorus's build. libopus's definitions and
# unit list are read from third_party/opus/chorus-build.txt, the same file the
# Rust client's sys crate reads. DR_FLAC_NO_SIMD keeps the host on the portable
# C path the ESP32-S3 runs.
THIRD_PARTY := $(REPO_ROOT)/third_party
OPUS_DIR := $(THIRD_PARTY)/opus
OPUS_LIST := $(OPUS_DIR)/chorus-build.txt
OPUS_DEFS := $(addprefix -D,$(shell sed -n 's/^define //p' $(OPUS_LIST)))
OPUS_SRC := $(addprefix $(OPUS_DIR)/,$(shell sed -n 's/^source //p' $(OPUS_LIST)))
DRFLAC_DEFS := -DDR_FLAC_NO_STDIO -DDR_FLAC_NO_OGG -DDR_FLAC_NO_WCHAR -DDR_FLAC_NO_SIMD
VENDOR_CFLAGS := -std=c11 -O2 -g -ffp-contract=off -fno-fast-math
CODEC_INC := -isystem $(THIRD_PARTY)/dr_flac -isystem $(OPUS_DIR)/include
CODEC_SRC := $(FW)/src/codec.c $(FW)/src/codec_flac.c $(FW)/src/codec_opus.c
VENDOR_OBJ := $(BUILD)/vendor/dr_flac.o \
	$(patsubst $(OPUS_DIR)/%.c,$(BUILD)/vendor/opus/%.o,$(OPUS_SRC))
# The session decodes coded_chunk through the seam, so the endpoint library
# carries the codec units and every binary that links it the vendored objects.
LIB_SRC += $(CODEC_SRC)
CFLAGS += $(DRFLAC_DEFS) $(CODEC_INC)

.PHONY: all check config-check golden-vectors sync-scenarios safety-scans amplifier clocking \
        telemetry wireless link console session-outage noise-vectors psa-tree codec codec-fixtures playout volume controls dsp clean

# Every binary this makefile builds, enumerated. There is no wildcard over
# firmware/tests on purpose, and the cost of that is a test has to be named in
# TWO places to run at all: here, and in the run target `check` depends on. A
# test absent from either is a grading route pointing at something nothing
# builds.
all: $(BUILD)/chorus-endpoint-config-check $(BUILD)/chorus-endpoint-scan \
     $(BUILD)/chorus-endpoint-session $(BUILD)/test_protocol $(BUILD)/test_protocol_v2 \
     $(BUILD)/test_noise $(BUILD)/test_sync $(BUILD)/test_amp $(BUILD)/test_i2s $(BUILD)/test_scan $(BUILD)/test_telemetry \
     $(BUILD)/test_wifi $(BUILD)/test_link $(BUILD)/test_console $(BUILD)/test_codec $(BUILD)/test_playout $(BUILD)/test_volume $(BUILD)/test_controls $(BUILD)/test_dsp $(BUILD)/opus_compare

$(BUILD):
	mkdir -p $(BUILD)

# The tree is refused by name when it is absent, and when it is a checkout of
# anything but the pinned commit: a different crypto library is one nobody
# chose.
psa-tree:
	@if [ ! -f "$(PSA_DIR)/include/psa/crypto.h" ]; then \
		echo "firmware: the crypto library is compiled from the pinned ESP-IDF v6.1 tree, and" \
		     "$(PSA_DIR) holds no TF-PSA-Crypto (no include/psa/crypto.h). Install ESP-IDF v6.1" \
		     "(commit $(CHORUS_IDF_V61_COMMIT)) and point CHORUS_IDF_V61_DIR at it." >&2; \
		exit 2; \
	fi
	@found="$$(git -C "$(CHORUS_IDF_V61_DIR)" rev-parse HEAD 2>/dev/null)"; \
	if [ "$$found" != "$(CHORUS_IDF_V61_COMMIT)" ]; then \
		echo "firmware: $(CHORUS_IDF_V61_DIR) is at '$${found:-no git commit}', not the pinned" \
		     "ESP-IDF v6.1 commit $(CHORUS_IDF_V61_COMMIT); the crypto library is compiled only" \
		     "from the pinned tree." >&2; \
		exit 2; \
	fi

$(PSA_BUILD)/%.o: $(PSA_DIR)/%.c $(FW)/crypto/chorus_psa_config.h | psa-tree
	@mkdir -p $(dir $@)
	$(CC) $(PSA_CFLAGS) $(PSA_CONFIG) $(PSA_INCLUDES) $(PSA_PRIVATE_INCLUDES) -c $< -o $@

$(PSA_LIB): $(PSA_OBJ) $(FW)/crypto/chorus_psa_config.h | psa-tree
	rm -f $@
	$(AR) rcs $@ $(PSA_OBJ)

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

$(STAMP): $(BUILD)/chorus-endpoint-config-check $(FW)/config/endpoint.conf $(BOARDS)
	$(BUILD)/chorus-endpoint-config-check $(FW)/config/endpoint.conf
	@test -f $(DEFAULT_BOARD) || { echo "firmware: endpoint.conf names board_profile" \
		"$(notdir $(DEFAULT_BOARD:.conf=)) and $(DEFAULT_BOARD) does not exist" >&2; exit 1; }
	$(BUILD)/chorus-endpoint-config-check --agrees $(FW)/config/endpoint.conf $(DEFAULT_BOARD)
	set -e; for b in $(BOARDS); do \
		$(BUILD)/chorus-endpoint-config-check $(FW)/config/endpoint.conf $$b; done
	touch $@

config-check: $(STAMP)

# --- binaries ----------------------------------------------------------------

$(BUILD)/chorus-endpoint-scan: $(SCAN_SRC) $(FW)/check/main_endpoint_scan.c | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/check -o $@ $^ $(LDLIBS)

$(BUILD)/chorus-endpoint-session: $(LIB_SRC) $(FW)/tests/main_session.c $(STAMP) $(PSA_LIB) \
                                  $(VENDOR_OBJ) | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/tests -o $@ $(LIB_SRC) $(FW)/tests/main_session.c $(PSA_LIB) \
		$(VENDOR_OBJ) $(LDLIBS)

# The scan's own tests link the scanner and not the endpoint library: the
# scanner is host tooling and deliberately outside the tree it scans.
$(BUILD)/test_scan: $(SCAN_SRC) $(FW)/tests/test_scan.c | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/check -I$(FW)/tests -o $@ $(SCAN_SRC) $(FW)/tests/test_scan.c $(LDLIBS)

$(BUILD)/test_protocol $(BUILD)/test_protocol_v2 $(BUILD)/test_noise $(BUILD)/test_sync \
$(BUILD)/test_amp $(BUILD)/test_i2s $(BUILD)/test_telemetry \
$(BUILD)/test_wifi $(BUILD)/test_link $(BUILD)/test_console $(BUILD)/test_playout \
$(BUILD)/test_volume $(BUILD)/test_controls $(BUILD)/test_dsp: $(BUILD)/test_%: $(LIB_SRC) $(TEST_SUPPORT) \
                         $(FW)/tests/test_%.c $(STAMP) $(PSA_LIB) $(VENDOR_OBJ) | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/tests -o $@ $(LIB_SRC) $(TEST_SUPPORT) $(FW)/tests/test_$*.c \
		$(PSA_LIB) $(VENDOR_OBJ) $(LDLIBS)

# The decoders' test links the codec units and the vendored objects and nothing
# else of the endpoint: the seam has no dependency on the session.
$(BUILD)/vendor/dr_flac.o: $(THIRD_PARTY)/dr_flac/dr_flac.c $(THIRD_PARTY)/dr_flac/dr_flac.h | $(BUILD)
	mkdir -p $(dir $@)
	$(CC) $(VENDOR_CFLAGS) $(DRFLAC_DEFS) -c -o $@ $<

$(BUILD)/vendor/opus/%.o: $(OPUS_DIR)/%.c $(OPUS_LIST) | $(BUILD)
	mkdir -p $(dir $@)
	$(CC) $(VENDOR_CFLAGS) $(OPUS_DEFS) -I$(OPUS_DIR)/include -I$(OPUS_DIR)/celt \
		-I$(OPUS_DIR)/silk -I$(OPUS_DIR)/src -c -o $@ $<

$(BUILD)/test_codec: $(CODEC_SRC) $(VENDOR_OBJ) $(FW)/tests/test_codec.c $(STAMP) | $(BUILD)
	$(CC) $(CFLAGS) -I$(FW)/tests -o $@ $(CODEC_SRC) \
		$(FW)/tests/test_codec.c $(VENDOR_OBJ) $(LDLIBS)

# libopus's conformance tool (RFC 6716 section 6, RFC 8251), vendored unchanged:
# it judges a decode against the official reference decode.
$(BUILD)/opus_compare: $(OPUS_DIR)/src/opus_compare.c | $(BUILD)
	$(CC) $(VENDOR_CFLAGS) -o $@ $< -lm

# --- what CI names separately ------------------------------------------------

# v1's three vectors, then every v2 vector (protocol v2 keeps v1's frame and
# v1's three messages byte for byte), then the key exchange's.
golden-vectors: $(BUILD)/test_protocol $(BUILD)/test_protocol_v2 $(BUILD)/test_noise
	$(BUILD)/test_protocol
	$(BUILD)/test_protocol_v2
	$(BUILD)/test_noise

noise-vectors: $(BUILD)/test_noise
	$(BUILD)/test_noise

sync-scenarios: $(BUILD)/test_sync
	$(BUILD)/test_sync

# The source scan and its demonstrations, then the post-build eFuse guard's
# self-test (tools/firmware-image-guard.sh): the guard itself needs a built
# image and runs after the ESP-IDF build, but the demonstration that it goes
# red on a smuggled option, writer or ROM call needs nothing, so it runs here.
safety-scans: $(BUILD)/chorus-endpoint-scan $(BUILD)/test_scan
	$(BUILD)/chorus-endpoint-scan $(REPO_ROOT)
	$(BUILD)/test_scan
	bash $(REPO_ROOT)/tools/firmware-image-guard.sh --self-test

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

# P1's link: a wired endpoint brings up the W5500 and never the radio, a
# wireless one the radio and never the W5500, a wired pin map that breaks a
# rule is refused by name, and every board profile reads as the image reads it.
# No SPI controller, no radio, no network.
link: $(BUILD)/test_link
	$(BUILD)/test_link

# Audit A-13: the endpoint's serial console, the commands the bench scripts send
# (power-save with its readback, server, status, decode-cost), against the
# simulated radio and the committed codec fixtures.
console: $(BUILD)/test_console
	$(BUILD)/test_console

# The playout path (audit A-9, goal 8): the jitter buffer, the DMA-consumed
# frames the I2S interrupt stamps on the monotonic clock, and the servo they
# feed, driven by a fake DMA on a fake clock; and config/sync.conf read by the
# endpoint and held to the Linux client's constants (A-12).
playout: $(BUILD)/test_playout
	$(BUILD)/test_playout

# Goal 11: the room's volume on the endpoint (K81, I10; docs/decisions/0074-*):
# min(ramped gain, the limit received, max_volume) at every frame, integer,
# the ramp's endpoints and monotonicity, mute, and frames in == frames out,
# alone and through the playout path and a decoded room_volume.
volume: $(BUILD)/test_volume
	$(BUILD)/test_volume

# Goal 9: the controls per class (K67-K70) as the controller role, held to
# fixtures/controls/<class>.hex, the microphone gate, and the status LED held
# to the visualizer fixture (docs/decisions/0063-*).
controls: $(BUILD)/test_controls
	$(BUILD)/test_controls

# Goal 12: the DSP library's C mirror (firmware/src/dsp.c) against every
# shared fixture in fixtures/dsp, the files crates/dsp/tests/shared_fixtures.rs
# runs the Rust crate against (docs/dsp.md).
dsp: $(BUILD)/test_dsp
	$(BUILD)/test_dsp

session-outage: $(BUILD)/chorus-endpoint-session
	bash $(FW)/tests/session-outage.sh

# FLAC and Opus against the shared fixtures in fixtures/codec: bit-exact FLAC,
# Opus final ranges and exact decodes, then opus_compare over each Opus decode
# the test wrote (the test prints the counts line; opus_compare prints its
# verdict per vector).
codec: $(BUILD)/test_codec $(BUILD)/opus_compare
	CHORUS_CODEC_OUT=$(BUILD)/codec-out $(BUILD)/test_codec $(BUILD)/opus_compare

# Regenerates fixtures/codec from the reference tools (never run by the gate):
# FLAC=<path to flac 1.5.0> OPUS_VECTORS=<the unpacked RFC 8251 vectors>.
codec-fixtures:
	python3 $(REPO_ROOT)/tools/codec-fixtures/generate.py --flac "$(FLAC)" \
		--vectors "$(OPUS_VECTORS)" --out $(REPO_ROOT)/fixtures/codec

check: config-check golden-vectors sync-scenarios safety-scans clocking amplifier telemetry \
       wireless link console playout volume controls dsp session-outage codec

clean:
	rm -rf $(BUILD)
