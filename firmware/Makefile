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
	$(FW)/src/lowlat.c \
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
	$(FW)/src/dsp_cost.c \
	$(FW)/src/sync_conf.c \
	$(FW)/src/playout.c \
	$(FW)/src/volume.c \
	$(FW)/src/controls.c \
	$(FW)/src/dsp.c \
	$(FW)/src/endpoint_dsp.c \
	$(FW)/src/ota.c \
	$(FW)/src/store.c \
	$(FW)/src/provision.c \
	$(FW)/src/identity.c \
	$(FW)/src/discovery.c

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
TEST_SUPPORT := $(FW)/tests/fake_amp.c $(FW)/tests/fake_radio.c $(FW)/tests/fake_store.c \
	$(FW)/tests/fake_flash.c

# The host session binary's own glue (goal 14), outside firmware/src for the
# same reason: the board has NVS and lwIP behind the same two seams
# (firmware/main/esp_identity.c, esp_discovery.c), the host a directory of
# files and POSIX sockets.
SESSION_SUPPORT := $(FW)/tests/file_store.c $(FW)/tests/posix_discovery.c

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

# --- objects and header dependencies -------------------------------------------
#
# Every chorus unit compiles once, to its own object under $(OBJ), and every
# binary links the objects of the sources it names, in the same order. gcc
# compiled each source of a one-command build as its own translation unit anyway
# (no -flto, no -fwhole-program), so the code linked is the code it was; what
# changes is that a unit twenty binaries share compiles once, and that each
# object records every header it read (-MD -MP, the vendored headers seen
# through -isystem included), so a changed header rebuilds exactly the objects
# that include it. Before, the binaries named only their .c files, and a change
# to a header alone left every binary built from it stale.
#
# The include paths per directory are the ones each unit was compiled with: a
# firmware/src unit includes nothing from firmware/tests or firmware/check, so the
# -I$(FW)/tests the test binaries added changed nothing in it (its preprocessed
# text is the same with and without, checked 2026-10-03 for every unit), and
# test_scan.c's -I$(FW)/check is the only one a test unit needs.
OBJ := $(BUILD)/obj
DEPFLAGS := -MD -MP
fwobj = $(patsubst $(FW)/%.c,$(OBJ)/%.o,$(1))
LIB_OBJ := $(call fwobj,$(LIB_SRC))
GATE_OBJ := $(call fwobj,$(GATE_SRC) $(FW)/check/main_config_check.c)
SCAN_OBJ := $(call fwobj,$(SCAN_SRC))
CODEC_OBJ := $(call fwobj,$(CODEC_SRC))
TEST_SUPPORT_OBJ := $(call fwobj,$(TEST_SUPPORT))
SESSION_SUPPORT_OBJ := $(call fwobj,$(SESSION_SUPPORT))

.PHONY: all check config-check golden-vectors sync-scenarios safety-scans amplifier clocking \
        telemetry wireless link console provisioning session-outage noise-vectors psa-tree codec codec-fixtures playout volume controls dsp lowlat \
        identity discovery ota clean

# Every binary this makefile builds, enumerated. There is no wildcard over
# firmware/tests on purpose, and the cost of that is a test has to be named in
# TWO places to run at all: here, and in the run target `check` depends on. A
# test absent from either is a grading route pointing at something nothing
# builds.
all: $(BUILD)/chorus-endpoint-config-check $(BUILD)/chorus-endpoint-scan \
     $(BUILD)/chorus-endpoint-session $(BUILD)/test_protocol $(BUILD)/test_protocol_v2 \
     $(BUILD)/test_noise $(BUILD)/test_lowlat $(BUILD)/test_sync $(BUILD)/test_amp $(BUILD)/test_i2s $(BUILD)/test_scan $(BUILD)/test_telemetry \
     $(BUILD)/test_wifi $(BUILD)/test_link $(BUILD)/test_console $(BUILD)/test_codec $(BUILD)/test_playout $(BUILD)/test_volume $(BUILD)/test_controls $(BUILD)/test_dsp $(BUILD)/test_endpoint_dsp \
     $(BUILD)/test_identity $(BUILD)/test_discovery $(BUILD)/test_provision $(BUILD)/test_ota \
     $(BUILD)/chorus-endpoint-dsp-session $(BUILD)/opus_compare

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
	$(CC) $(PSA_CFLAGS) $(DEPFLAGS) $(PSA_CONFIG) $(PSA_INCLUDES) $(PSA_PRIVATE_INCLUDES) -c $< -o $@

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
$(BUILD)/chorus-endpoint-config-check: $(GATE_OBJ) | $(BUILD)
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

# --- objects ------------------------------------------------------------------

$(OBJ)/src/%.o: $(FW)/src/%.c | $(BUILD)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) $(DEPFLAGS) -c -o $@ $<

$(OBJ)/tests/%.o: $(FW)/tests/%.c | $(BUILD)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) $(DEPFLAGS) -I$(FW)/check -I$(FW)/tests -c -o $@ $<

$(OBJ)/check/%.o: $(FW)/check/%.c | $(BUILD)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) $(DEPFLAGS) -I$(FW)/check -c -o $@ $<

# Nothing of the endpoint compiles until the configuration gate has passed, as
# before: every object but the checker's own and the scanner's (host tooling,
# outside the tree it scans) waits for the stamp. Order-only, so a renewed stamp
# relinks the binaries (they name it) without recompiling every unit.
$(filter-out $(GATE_OBJ) $(OBJ)/tests/test_scan.o,$(LIB_OBJ) \
	$(patsubst $(FW)/tests/%.c,$(OBJ)/tests/%.o,$(wildcard $(FW)/tests/*.c))): | $(STAMP)

# --- binaries ----------------------------------------------------------------

$(BUILD)/chorus-endpoint-scan: $(SCAN_OBJ) $(OBJ)/check/main_endpoint_scan.o | $(BUILD)
	$(CC) $(CFLAGS) -o $@ $^ $(LDLIBS)

# fake_flash.c is linked for --ota-flash: the host session's two-slot flash in
# a file, so a test can "reboot" the endpoint by starting the binary again.
$(BUILD)/chorus-endpoint-session: $(LIB_OBJ) $(SESSION_SUPPORT_OBJ) $(OBJ)/tests/main_session.o \
                                  $(OBJ)/tests/fake_flash.o $(STAMP) $(PSA_LIB) $(VENDOR_OBJ) | $(BUILD)
	$(CC) $(CFLAGS) -o $@ $(LIB_OBJ) $(SESSION_SUPPORT_OBJ) $(OBJ)/tests/main_session.o \
		$(OBJ)/tests/fake_flash.o $(PSA_LIB) $(VENDOR_OBJ) $(LDLIBS)

# Goal 12's end to end on fakes (firmware/tests/dsp-session.sh): the real
# session, the real playout path and its sound chain, and a fake I2S DMA that
# keeps every frame written. Links pthreads for the writer thread.
$(BUILD)/chorus-endpoint-dsp-session: $(LIB_OBJ) $(OBJ)/tests/main_dsp_session.o $(STAMP) \
                                      $(PSA_LIB) $(VENDOR_OBJ) | $(BUILD)
	$(CC) $(CFLAGS) -o $@ $(LIB_OBJ) $(OBJ)/tests/main_dsp_session.o $(PSA_LIB) \
		$(VENDOR_OBJ) $(LDLIBS) -lpthread

# The scan's own tests link the scanner and not the endpoint library: the
# scanner is host tooling and deliberately outside the tree it scans.
$(BUILD)/test_scan: $(SCAN_OBJ) $(OBJ)/tests/test_scan.o | $(BUILD)
	$(CC) $(CFLAGS) -o $@ $^ $(LDLIBS)

$(BUILD)/test_protocol $(BUILD)/test_protocol_v2 $(BUILD)/test_noise $(BUILD)/test_lowlat \
$(BUILD)/test_sync $(BUILD)/test_amp $(BUILD)/test_i2s $(BUILD)/test_telemetry \
$(BUILD)/test_wifi $(BUILD)/test_link $(BUILD)/test_console $(BUILD)/test_playout \
$(BUILD)/test_volume $(BUILD)/test_controls $(BUILD)/test_dsp \
$(BUILD)/test_endpoint_dsp $(BUILD)/test_identity $(BUILD)/test_provision $(BUILD)/test_ota \
$(BUILD)/test_discovery: $(BUILD)/test_%: $(LIB_OBJ) $(TEST_SUPPORT_OBJ) \
                         $(OBJ)/tests/test_%.o $(STAMP) $(PSA_LIB) $(VENDOR_OBJ) | $(BUILD)
	$(CC) $(CFLAGS) -o $@ $(LIB_OBJ) $(TEST_SUPPORT_OBJ) $(OBJ)/tests/test_$*.o \
		$(PSA_LIB) $(VENDOR_OBJ) $(LDLIBS)

# The decoders' test links the codec units and the vendored objects and nothing
# else of the endpoint: the seam has no dependency on the session.
$(BUILD)/vendor/dr_flac.o: $(THIRD_PARTY)/dr_flac/dr_flac.c $(THIRD_PARTY)/dr_flac/dr_flac.h | $(BUILD)
	mkdir -p $(dir $@)
	$(CC) $(VENDOR_CFLAGS) $(DEPFLAGS) $(DRFLAC_DEFS) -c -o $@ $<

$(BUILD)/vendor/opus/%.o: $(OPUS_DIR)/%.c $(OPUS_LIST) | $(BUILD)
	mkdir -p $(dir $@)
	$(CC) $(VENDOR_CFLAGS) $(DEPFLAGS) $(OPUS_DEFS) -I$(OPUS_DIR)/include -I$(OPUS_DIR)/celt \
		-I$(OPUS_DIR)/silk -I$(OPUS_DIR)/src -c -o $@ $<

$(BUILD)/test_codec: $(CODEC_OBJ) $(VENDOR_OBJ) $(OBJ)/tests/test_codec.o $(STAMP) | $(BUILD)
	$(CC) $(CFLAGS) -o $@ $(CODEC_OBJ) \
		$(OBJ)/tests/test_codec.o $(VENDOR_OBJ) $(LDLIBS)

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

# Goal 13: the low-latency datagram layer (chorus/lowlat.h) against the shared
# vectors in fixtures/protocol/lowlat, which crates/protocol/tests/lowlat.rs
# holds the Rust implementation to: the same datagrams byte for byte, the same
# rebuilds and the same counts, sealed and opened through PSA.
lowlat: $(BUILD)/test_lowlat
	$(BUILD)/test_lowlat

sync-scenarios: $(BUILD)/test_sync
	$(BUILD)/test_sync

# The source scan and its demonstrations, then the post-build eFuse guard's
# self-test (tools/firmware-image-guard.sh): the guard itself needs a built
# image and runs after the ESP-IDF build, but the demonstration that it goes
# red on a smuggled option, writer or ROM call needs nothing, so it runs here.
# Since goal 14 the scan allows ESP-IDF's OTA calls in ONE unit
# (firmware/main/esp_ota.c) and nowhere else, and the guard also asserts the
# bootloader's rollback on and anti-rollback off.
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

# Goal 14: Wi-Fi provisioning for the compact speakers and the store it keeps a
# network in (chorus/provision.h, chorus/store.h). Every state and transition,
# the join form's parser on good and hostile bodies, the setup secret, a full
# run from first boot to a reboot that joins with no access point, and the
# rule that no network name, passphrase or setup secret reaches a log line or
# the store's event log. Against the simulated radio and a simulated store; no
# radio, no flash, no ESP-IDF. The manager's own handshake is not host-run
# (ESP-IDF's protocomm does not build for the host: its CMakeLists.txt returns
# early on the linux target), and docs/bench-packet.md S9 is where it runs.
provisioning: $(BUILD)/test_provision
	$(BUILD)/test_provision

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
# to the visualizer fixture (docs/decisions/0063-*). And the session's voice
# path (chorus/session.h) over a fake capture source: no mic_audio with the
# gate closed or the uplink not requested, the captured samples when both
# allow, the gate report at every switch change, held to the voice role's
# vectors in fixtures/protocol/v2, and no voice message from a class without a
# microphone.
controls: $(BUILD)/test_controls
	$(BUILD)/test_controls

# Goal 12: the DSP library's C mirror (firmware/src/dsp.c) against every
# shared fixture in fixtures/dsp, the files crates/dsp/tests/shared_fixtures.rs
# runs the Rust crate against (docs/dsp.md).
dsp: $(BUILD)/test_dsp
	$(BUILD)/test_dsp

# Goal 12: the endpoint's sound chain in the playout path
# (firmware/include/chorus/endpoint_dsp.h): the wire's sound and the sub knobs
# as settings, when the chain is in the path, the gain inside it, the latency
# in the device delay, bass management and the two-way at the LR4 levels, and
# the limiter at the room's limit.
endpoint-dsp: $(BUILD)/test_endpoint_dsp
	$(BUILD)/test_endpoint_dsp

# Goal 12, done-when line C on the C endpoint: the real chorus-server, a bonded
# 2.1 set and a two-way of real C sessions on loopback, their written frames
# captured from a fake I2S DMA and analysed (firmware/tests/dsp-session.sh).
# Goal 14: who the board is across boots (chorus/identity.h) over the fake
# store: the id (random, never the MAC) and the Noise key made once and the
# same after a reboot, a failed store write refused rather than run unsaved,
# the server's pin kept and a changed server key still refused, and the
# session taking all of it from the store. Then the same end to end
# (firmware/tests/identity-session.sh): the real chorus-server on loopback and
# the session binary over a directory store, a reboot being a new process.
identity: $(BUILD)/test_identity $(BUILD)/chorus-endpoint-session
	$(BUILD)/test_identity
	bash $(FW)/tests/identity-session.sh

# Goal 14: the endpoint's DNS-SD browse (chorus/discovery.h) against the
# packets in fixtures/discovery, the ones crates/discovery/tests/dnssd_vectors.rs
# holds the Rust resolver to, then the browse and the fallback order over a
# fake link and the fake store. No socket: the live exchange over real
# multicast is `make verify-endpoint-mdns`, outside the gate.
discovery: $(BUILD)/test_discovery
	$(BUILD)/test_discovery

dsp-session: $(BUILD)/chorus-endpoint-dsp-session
	bash $(FW)/tests/dsp-session.sh

# Goal 14's foundation line: the firmware update's state machine
# (firmware/src/ota.c) over a fake two-slot flash with ESP-IDF v6.1's bootloader
# rule (firmware/tests/fake_flash.c): power lost at every write index, a seeded
# campaign of random faults and each named one, a VALID slot bootable after
# every one of them, and a bad image rolled back.
ota: $(BUILD)/test_ota
	$(BUILD)/test_ota

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

check: config-check golden-vectors lowlat sync-scenarios safety-scans clocking amplifier telemetry \
       wireless link console playout volume controls dsp endpoint-dsp identity discovery ota \
       session-outage dsp-session codec provisioning

clean:
	rm -rf $(BUILD)

# The headers each object read, recorded by its last compile (-MD -MP above).
TEST_MAIN_OBJ := $(patsubst $(FW)/tests/%.c,$(OBJ)/tests/%.o,$(wildcard $(FW)/tests/*.c))
-include $(patsubst %.o,%.d,$(sort $(LIB_OBJ) $(GATE_OBJ) $(SCAN_OBJ) $(OBJ)/check/main_endpoint_scan.o \
	$(TEST_MAIN_OBJ) $(PSA_OBJ) $(VENDOR_OBJ)))
