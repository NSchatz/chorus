# Research: the TV path (CEC, low-latency FEC and latency, TV capture)

Goal 13 (the TV path), research for the tracks `cec`, `lowlat-wire` and `capture`. Read
2026-10-01. Every number carries its source URL and the date read. Forum or blog material is
marked LEAD and is not a citation. ASSUMED marks values with no direct citation.

This file merges the three goal-13 research notes (CEC, FEC and latency, capture). Source keys in
brackets ([K-MODE], [D1], ...) are resolved in the per-section source lists at the end; all were
read 2026-10-01. "(snippet)" means a search-result summary only, which is a LEAD.

## 1. HDMI-CEC Audio System role on Linux (track `cec`)

### 1a. Bottom line

- Open `/dev/cecN` O_RDWR, read caps, set mode `CEC_MODE_INITIATOR |
  CEC_MODE_EXCL_FOLLOWER_PASSTHRU` (0x01 | 0x30 = 0x31) so chorus owns nearly every message,
  then `CEC_ADAP_S_LOG_ADDRS` with one logical address of type AUDIOSYSTEM (4), primary device
  type AUDIOSYSTEM (5), CEC version 1.4 (5) or 2.0 (6), OSD name up to 14 chars plus NUL. The
  kernel claims logical address 5 itself (polling) and emits a STATE_CHANGE event. [K-MODE] [K-LOG]
- Structs are fixed size and identical on x86_64 and aarch64 (generic ioctl encoding, naturally
  aligned fields): cec_caps 76 B, cec_log_addrs 92 B, cec_msg 56 B, cec_event 80 B. The first
  three were cross-checked by compiling the MIT `cec_linux` 0.2.2 crate (`size_of`) on x86_64.
- The Audio System core loop is small: answer <Give System Audio Mode Status>, <System Audio
  Mode Request> (broadcast <Set System Audio Mode>), <Give Audio Status> (mute bit 7, volume
  0..100 in bits 0..6), <User Control Pressed> 0x41/0x42/0x43 + <User Control Released>, ARC
  initiate/terminate, power status, standby, plus the core messages the kernel would otherwise
  answer (version, vendor id, phys addr, OSD name, features, abort). Android's CTS "audio" tests
  (Apache-2.0) are an executable checklist (1d).
- No CEC hardware or vivid in the dev container (no /sys/class/cec, no modules, uid 1000). Test
  design: an `Adapter` trait plus a scripted fake TV (1f). Real-kernel integration needs
  `modprobe vivid` on the owner's bench host.

### 1b. Kernel CEC userspace API

Open [K-OPEN]: `open("/dev/cecN", O_RDWR [| O_NONBLOCK])`, "Access mode must be O_RDWR"; with
O_NONBLOCK, ioctls that would block return EAGAIN. Nodes are per adapter, numbered in probe order.

CEC_ADAP_G_CAPS, `cec_caps` (76 B) [K-CAPS]: driver char[32] @0, name char[32] @32,
available_log_addrs u32 @64, capabilities u32 @68, version u32 @72 (KERNEL_VERSION() of the
framework API). Capability bits: PHYS_ADDR 0x1, LOG_ADDRS 0x2, TRANSMIT 0x4, PASSTHROUGH 0x8, RC
0x10, MONITOR_ALL 0x20, NEEDS_HPD 0x40, MONITOR_PIN 0x80, CONNECTOR_INFO 0x100, REPLY_VENDOR_ID
0x200. chorus needs LOG_ADDRS + TRANSMIT; PASSTHROUGH for the passthru follower mode (assumption
from the name and [K-MODE] wording: verify on the bench that vc4 sets it). PHYS_ADDR set means
userspace sets the physical address (Pulse-Eight style dongles); clear means the driver takes it
from the EDID (vc4 on the Pi). [K-PHYS]

Physical address, CEC_ADAP_G/S_PHYS_ADDR (u16) [K-PHYS]: a.b.c.d nibbles, MSB nibble = a; TV is
0.0.0.0, devices on TV inputs a.0.0.0; "The physical address a device shall use is stored in the
EDID of the sink." S_PHYS_ADDR needs CEC_CAP_PHYS_ADDR and initiator mode; with a valid address it
blocks until logical addresses are claimed (non-blocking: returns at once).
CEC_PHYS_ADDR_INVALID (0xffff, value from [CRATE]) unconfigures.

CEC_ADAP_G/S_LOG_ADDRS, `cec_log_addrs` (92 B) [K-LOG]:

| off | field | type | set by |
|---|---|---|---|
| 0 | log_addr | u8[4] | driver (0xff = CEC_LOG_ADDR_INVALID [CRATE]) |
| 4 | log_addr_mask | u16 | driver |
| 6 | cec_version | u8 | caller: 4 = 1.3a, 5 = 1.4(b), 6 = 2.0 |
| 7 | num_log_addrs | u8 | caller (<= available_log_addrs; 0 clears all) |
| 8 | vendor_id | u32 | caller (24-bit OUI; CEC_VENDOR_ID_NONE for none) |
| 12 | flags | u32 | caller |
| 16 | osd_name | char[15] | caller |
| 31 | primary_device_type | u8[4] | caller |
| 35 | log_addr_type | u8[4] | caller |
| 39 | all_device_types | u8[4] | caller (CEC 2.0) |
| 43 | features | u8[4][12] | caller (CEC 2.0: RC profile + device features) |
| 91 | 1 byte tail padding to 4-byte alignment | | |

CEC_MAX_LOG_ADDRS = 4. Flags: ALLOW_UNREG_FALLBACK 1, ALLOW_RC_PASSTHRU 2, CDC_ONLY 4,
CONFIG_FAILED 8 (driver-set). LOG_ADDR_TYPE: TV 0, RECORD 1, TUNER 2, PLAYBACK 3, AUDIOSYSTEM 4,
SPECIFIC 5, UNREGISTERED 6. PRIM_DEVTYPE: TV 0, RECORD 1, TUNER 3, PLAYBACK 4, AUDIOSYSTEM 5,
SWITCH 6, VIDEOPROC 7. ALL_DEVTYPE bits: TV 0x80, RECORD 0x40, TUNER 0x20, PLAYBACK 0x10,
AUDIOSYSTEM 0x08, SWITCH 0x04. [K-LOG] Behaviour: needs CEC_CAP_LOG_ADDRS; caller must be an
initiator (else EBUSY); with a valid phys addr it blocks until claimed; EBUSY if types are already
set (clear with num_log_addrs = 0 first); STATE_CHANGE fires on claim and clear; the driver may
claim fewer than asked; if none claimable log_addr[0] = 0xff, with UNREG_FALLBACK 0xf. [K-LOG]
[CRATE doc comments, MIT] The Audio System has exactly one logical address, 5 [AOSP-H]:
`num_log_addrs = 1`, `log_addr_type[0] = 4`, `primary_device_type[0] = 5`,
`all_device_types[0] = 0x08`. CEC 2.0 features[0]: byte 0 = RC profile, byte 1 = device features
(exact bits: verify in [K-LOG] before use; not recorded). [CRATE] declares `features: [[u8; 4];
12]`, transposed against the documented `[4][12]`: same 48 bytes, different indices; do not copy it.

CEC_G_MODE / CEC_S_MODE (u32) [K-MODE]: initiator (low nibble) NO_INITIATOR 0x0, INITIATOR 0x1
(default), EXCL_INITIATOR 0x2; follower (high nibble) NO_FOLLOWER 0x00 (default: replies to own
transmits only), FOLLOWER 0x10, EXCL_FOLLOWER 0x20, EXCL_FOLLOWER_PASSTHRU 0x30, MONITOR_PIN 0xd0,
MONITOR 0xe0, MONITOR_ALL 0xf0 (monitor modes need NO_INITIATOR and CAP_NET_ADMIN). EBUSY if
another exclusive holder exists. chorus uses **0x31**: in passthru "only this file descriptor will
receive CEC messages for processing ... allowing the exclusive follower to handle most core
messages". [K-MODE]

| message | normal mode | passthru |
|---|---|---|
| Get CEC Version 0x9f | core replies from cec_version | follower |
| Give Device Vendor ID 0x8c | core replies from vendor_id | follower |
| Abort 0xff | core replies Feature Abort "Refused" | follower |
| Give Physical Address 0x83 | core reports phys addr | follower |
| Give OSD Name 0x46 | core replies from osd_name | follower |
| Give Features 0xa5 | core replies (CEC 2.0) | follower |
| User Control Pressed/Released 0x44/0x45 | input key if CAP_RC + ALLOW_RC_PASSTHRU | always to followers |
| Report Physical Address 0x84 | core notes it, passes on | passed on |

Non-reply messages are processed by the core first; "If there is no follower, then the message is
just discarded and a feature abort is sent back to the initiator if the framework couldn't process
it" (S_MODE text quoted in [CRATE], from [K-MODE]). Replies to a transmit with `reply` set go to
the waiting filehandle. Logical address claiming (polling) is always the core's job. In passthru,
chorus must itself send <Feature Abort> [Unrecognized opcode] for directed messages it does not
handle.

CEC_TRANSMIT / CEC_RECEIVE, `cec_msg` (56 B) [K-RX]:

| off | field | type | set by |
|---|---|---|---|
| 0 | tx_ts | u64 ns CLOCK_MONOTONIC | driver |
| 8 | rx_ts | u64 ns CLOCK_MONOTONIC | driver |
| 16 | len | u32 (1..16) | app on TX, driver on RX |
| 20 | timeout | u32 ms | app |
| 24 | sequence | u32 | driver (non-zero = result of an earlier non-blocking TX) |
| 28 | flags | u32 | app: REPLY_TO_FOLLOWERS 1, RAW 2 (needs privilege, EPERM), REPLY_VENDOR_ID 4 |
| 32 | msg | u8[16]: msg[0] = initiator<<4 \| destination, msg[1] = opcode, rest operands | app/driver |
| 48 | reply | u8: opcode to wait for (0 = none; Feature Abort also ends the wait) | app |
| 49 | rx_status | u8: OK 0x01, TIMEOUT 0x02, FEATURE_ABORT 0x04, ABORTED 0x08 | driver |
| 50 | tx_status | u8: OK 0x01, ARB_LOST 0x02, NACK 0x04, LOW_DRIVE 0x08, ERROR 0x10, MAX_RETRIES 0x20, ABORTED 0x40, TIMEOUT 0x80 | driver |
| 51..54 | tx_arb_lost_cnt, tx_nack_cnt, tx_low_drive_cnt, tx_error_cnt | u8 each | driver |
| 55 | tail padding to 8 | | |

RECEIVE: timeout 0 waits forever; blocking + expiry gives ETIMEDOUT; non-blocking + empty gives
EAGAIN. TRANSMIT: timeout 0 becomes 1000 ms if reply is set. TX queue: 18 messages ("about 1
second worth of 2-byte messages"), EBUSY when full; the core also uses it. ENONET if the phys
addr is invalid. No field is named reserved; `flags` bits beyond 1/2/4 are undefined (set 0);
arrays in cec_log_addrs past num_log_addrs are ignored; no `reserved[]` arrays in any of the four
structs per the docs read. [K-RX] [K-LOG]

CEC_DQEVENT, `cec_event` (80 B) [K-EV]: ts u64 ns CLOCK_MONOTONIC @0; event u32 @8
(STATE_CHANGE 1, LOST_MSGS 2, PIN_CEC_LOW 3, PIN_CEC_HIGH 4, PIN_HPD_LOW 5, PIN_HPD_HIGH 6,
PIN_5V_LOW 7?, PIN_5V_HIGH 8?); flags u32 @12 (INITIAL_STATE 1, DROPPED_EVENTS 2); a 64-byte
union @16 (padded by u32 raw[16] per [CRATE]): state_change {u16 phys_addr, u16 log_addr_mask,
u16 have_conn_info}, lost_msgs {u32 lost_msgs}. The rendered docs list both PIN_HPD_HIGH and
PIN_5V_LOW as 6 (a rendering error); 7/8 for the 5V pair is an inference, LEAD, and chorus does
not need pin events. Event queues are per filehandle per type, the last event overwritten when
full. The RX queue "guarantees that all messages received in the last two seconds will be
stored"; overflow is reported via LOST_MSGS. The initial STATE_CHANGE arrives on open
(INITIAL_STATE). [K-EV] poll(): POLLIN/POLLRDNORM = messages queued, POLLPRI = events pending,
POLLOUT/POLLWRNORM = TX queue has room [K-POLL], so one fd fits a tokio/mio reactor or a poll loop.

ioctl numbers (x86_64 == aarch64, asm-generic encoding dir<<30 | size<<16 | 'a'(0x61)<<8 | nr,
_IOC_WRITE = 1, _IOC_READ = 2; nr/direction/struct from [CRATE] (MIT), sizes from the layouts):

| ioctl | define | value |
|---|---|---|
| CEC_ADAP_G_CAPS | _IOWR('a', 0, cec_caps) | 0xC04C6100 |
| CEC_ADAP_G_PHYS_ADDR | _IOR('a', 1, u16) | 0x80026101 |
| CEC_ADAP_S_PHYS_ADDR | _IOW('a', 2, u16) | 0x40026102 |
| CEC_ADAP_G_LOG_ADDRS | _IOR('a', 3, cec_log_addrs) | 0x805C6103 |
| CEC_ADAP_S_LOG_ADDRS | _IOWR('a', 4, cec_log_addrs) | 0xC05C6104 |
| CEC_TRANSMIT | _IOWR('a', 5, cec_msg) | 0xC0386105 |
| CEC_RECEIVE | _IOWR('a', 6, cec_msg) | 0xC0386106 |
| CEC_DQEVENT | _IOWR('a', 7, cec_event) | 0xC0506107 |
| CEC_G_MODE | _IOR('a', 8, u32) | 0x80046108 |
| CEC_S_MODE | _IOW('a', 9, u32) | 0x40046109 |

Computed with Python ctypes; `cec_linux` size_of on x86_64 gave CecMsg 56, CecLogAddrs 92,
CecCaps 76 (agrees). Proposal: keep these as `const`s with `const _: () =
assert!(size_of::<CecMsg>() == 56)` style checks, plus a bench check against `cec-ctl` behaviour
(using the binary is fine; its source is not to be opened).

### 1c. Messages for the Audio System role

Addressing: msg[0] = initiator high nibble, destination low nibble [CRATE] [K-RX]. Logical
addresses [AOSP-H]: 0 TV, 1 Recorder 1, 2 Recorder 2, 3 Tuner 1, 4 Playback 1, **5 Audio
System**, 6 Tuner 2, 7 Tuner 3, 8 Playback 2, 9 Recorder 3, 10 Tuner 4, 11 Playback 3, 12/13
reserved (Backup in 2.0), 14 free use (Specific), 15 Unregistered as initiator / **Broadcast** as
destination. Direct-only messages received as broadcast must be ignored (CTS test "12-2": Give
Audio Status, Give System Audio Mode Status, Request SAD, System Audio Mode Request, Request ARC
Initiation/Termination) [CTS-INV]. Android's validator table [AOSP-V] (Apache-2.0) encodes
per-opcode direct/broadcast rules and minimum operand lengths: a good model for chorus's validator.

Opcodes (values from the [CRATE] enum, cross-checked [AOSP-H]):

| opcode | name | dir | operands / Audio System action |
|---|---|---|---|
| 0x00 | Feature Abort | direct | [opcode][reason]; 0 Unrecognized opcode, 1 Not in correct mode, 2 Cannot provide source, 3 Invalid operand, 4 Refused, 5 Unable to determine |
| 0xff | Abort | direct | reply Feature Abort [0xff][4 Refused] (what the core does) |
| 0x04 | Image View On | direct to TV | sent by sources; sniffable as "TV turning on" only in monitor mode |
| 0x0d | Text View On | direct to TV | as above |
| 0x36 | Standby | both | go to standby; first, if SAM on, broadcast Set System Audio Mode [off] (CTS 11.2.15-6) |
| 0x44 | User Control Pressed | direct | Volume Up 0x41, Volume Down 0x42, Mute 0x43, Mute Function 0x65, Restore Volume Function 0x66, Power 0x40, Power Toggle 0x6b, Power Off 0x6c, Power On 0x6d |
| 0x45 | User Control Released | direct | ends press-and-hold (CTS 11.2.13-1..4 cover press/hold/no-release) |
| 0x70 | System Audio Mode Request | direct to 5 | [phys addr 2 B] = on (and the active source path); no operand = off. Reply: broadcast Set System Audio Mode [AOSP-AS] |
| 0x72 | Set System Audio Mode | both, initiator must be 5 [AOSP-V] | [0 off / 1 on] |
| 0x7d | Give System Audio Mode Status | direct | reply 0x7e |
| 0x7e | System Audio Mode Status | direct | [0 off / 1 on] |
| 0x71 | Give Audio Status | direct | reply 0x7a |
| 0x7a | Report Audio Status | direct | bit 7 mute, bits 0..6 volume 0..100 (0x64); 0x7f = unknown (crate comment: 0x65..0x7e reserved) [AOSP-B] [CRATE] |
| 0xa4 | Request Short Audio Descriptor | direct | 1..4 bytes, each [Audio Format ID (2 bits) + Audio Format Code (6 bits)] |
| 0xa3 | Report Short Audio Descriptor | direct | 1..4 SADs of 3 bytes (CEA-861 SAD: byte 0 bits 3..6 format, bits 0..2 channels-1); unsupported-only request: Feature Abort Invalid operand (CTS 11.2.15-13/14) |
| 0x8f | Give Device Power Status | direct | reply 0x90 |
| 0x90 | Report Power Status | direct/broadcast (2.0) | [0 On, 1 Standby, 2 Standby->On, 3 On->Standby] |
| 0x82 | Active Source | broadcast | [phys addr] |
| 0x85 | Request Active Source | broadcast | answer only if we are the active source |
| 0x80 | Routing Change | broadcast | [old PA][new PA] |
| 0x81 | Routing Information | broadcast | [PA] |
| 0x86 | Set Stream Path | broadcast | [PA] (TV selects a path) |
| 0x83 | Give Physical Address | direct | reply 0x84 broadcast |
| 0x84 | Report Physical Address | broadcast | [PA 2 B][primary device type 1 B] (5 for us) [AOSP-B] |
| 0x46 | Give OSD Name | direct | reply 0x47 |
| 0x47 | Set OSD Name | direct | 1..14 ASCII bytes [AOSP-V] |
| 0x8c | Give Device Vendor ID | direct | reply 0x87 broadcast |
| 0x87 | Device Vendor ID | broadcast | 3-byte OUI, MSB first [AOSP-B] |
| 0x9f | Get CEC Version | direct | reply 0x9e |
| 0x9e | CEC Version | direct | [4 = 1.3a, 5 = 1.4, 6 = 2.0] |
| 0xa5 | Give Features | direct | reply 0xa6 (2.0) |
| 0xa6 | Report Features | broadcast | [version][all device types][RC profile...][device features...] |
| 0xc0 | Initiate ARC | direct, AS -> TV | sent by us |
| 0xc1 | Report ARC Initiated | direct, TV -> AS | |
| 0xc2 | Report ARC Terminated | direct, TV -> AS | |
| 0xc3 | Request ARC Initiation | direct, TV -> AS | answer Initiate ARC, or Feature Abort (Unrecognized if no ARC; Not in correct mode if not directly on the TV) [AOSP-AS] |
| 0xc4 | Request ARC Termination | direct, TV -> AS | answer Terminate ARC |
| 0xc5 | Terminate ARC | direct, AS -> TV | sent by us |
| 0xa7/0xa8 | Request/Report Current Latency | broadcast | 2.0 dynamic AV sync; relevant to chorus latency later |
| 0x89/0xa0 | Vendor Command (With ID) | | Feature Abort unless needed |

Flows (Android AudioSystem behaviour, Apache-2.0 [AOSP-AS]; HDMI CTS IDs via [CTS-SAM]):

- Startup: broadcast Report Physical Address (the CTS harness expects it after boot), optionally
  Device Vendor ID. For ARC: directed Initiate ARC to the TV, expect Report ARC Initiated within
  1000 ms ([AOSP-ARC] TIMEOUT_MS = 1000).
- TV enables System Audio: TV -> 5 SAMR [PA]; we broadcast Set System Audio Mode [1] and unmute
  (11.2.15-16). SAMR with no operand: broadcast Set System Audio Mode [0] and mute (11.2.15-17).
  Any initiator (e.g. tuner 3) may send SAMR (11.2.15-1).
- We initiate SAM (11.2.15-2): after the TV asks Give System Audio Mode Status and ARC is up,
  send Request Active Source, then a directed Set System Audio Mode [1] to the TV; if the TV
  Feature-Aborts it (reason 4) within 1 s, do NOT broadcast Set System Audio Mode (11.2.15-3/-18).
- Volume: with SAM on the TV forwards keys as UCP 0x41/0x42/0x43 to 5 and shows the result by
  asking Give Audio Status; reply Report Audio Status (CTS expects 100 % -> 100, 50 % -> 46..54,
  mute -> bit 7 set). Sending Report Audio Status proactively after a change is common practice
  (LEAD; Android does it via its volume listener).
- Standby with SAM on: broadcast Set System Audio Mode [0] first.
- "TV turned on" signals visible without monitor mode: broadcast Active Source / Routing Change /
  Set Stream Path / Report Power Status (2.0 broadcasts it) / SAMR from the TV / Request ARC
  Initiation. Image View On / Text View On go to the TV, so chorus sees them only in monitor mode
  (CAP_NET_ADMIN; MONITOR_ALL also needs the cap bit). Polling the TV with Give Device Power
  Status is the reliable fallback.

### 1d. Executable checklist

Android CTS host tests (Apache-2.0) under `cts/hostsidetests/hdmicec/.../audio/`:
HdmiCecSystemAudioModeTest (HDMI CTS 11.2.15-1..19), HdmiCecAudioReturnChannelControlTest
(11.2.17-1..4), HdmiCecRemoteControlPassThroughTest (11.2.13-1..4), HdmiCecInvalidMessagesTest
(12-2), HdmiCecLogicalAddressTest. Proposal: port these one-to-one into the fake-TV suite.

### 1e. TV behaviour with an Audio System (Roku and general)

- Roku TV: "With CEC enabled, you can use your Roku TV remote to adjust the volume and mute the
  sound of your home theater, and display the volume level and mute status on the TV screen."
  Menu: Home > Settings > System > Control other devices (CEC) > System audio control / ARC /
  1-touch play. [ROKU-SS] [ROKU-HDMI] The on-screen level implies the TV reads Report Audio
  Status (inference).
- Roku ties this to the HDMI ARC/eARC port; the optical section offers no remote volume control
  [ROKU-SS]. Roku TV Ready soundbars are also controlled over IR/proprietary means ("don't block
  the IR receiver") [ROKU-SB].
- Optical + CEC: per spec System Audio Mode is independent of ARC (SAMR carries a PA, not a
  port), so a TV can in principle forward volume keys to 5 regardless; whether Roku does so when
  the audio device is not on the ARC port is unknown: LEAD, measure on the bench.
- Hardware implication (LEAD, design-level): a Raspberry Pi HDMI port is a source (TX) and cannot
  receive ARC audio. Audio from a Roku TV would come via optical or an ARC/eARC extractor, while
  CEC runs on the Pi's port plugged into a TV input (PA x.0.0.0 of that input, maybe not the ARC
  input). Request ARC Initiation may then need Feature Abort [Not in correct mode], as Android
  does when not directly connected. Bench must confirm what Roku does.

### 1f. Testing without hardware (proposal)

- vivid emulates CEC (CONFIG_VIDEO_VIVID_CEC, kernels 4.8+ [LKDDB]): HDMI inputs create one CEC
  adapter "the equivalent of e.g. a TV", and "each HDMI output will also create a CEC adapter that
  is hooked up to the corresponding input port" [K-VIVID]. `modprobe vivid` gives cec0 (receiver)
  and cec1 (transmitter) in the default config (kernel 4.8 announcement, search snippet: LEAD).
  [K-CEC] names vivid as the no-hardware option, and cec-follower (emulates a follower) and
  cec-compliance (tests a remote device).
- Not possible in the dev container (uid 1000, no /lib/modules, no /sys/class/cec, no vivid);
  loading vivid needs root on the host, then `--device /dev/cec0 --device /dev/cec1`. So vivid is
  a bench/host-CI tier, not a dev-container tier.
- Pure Rust fake: `trait CecAdapter { caps(); set_mode(u32); set_log_addrs(&LogAddrs) ->
  Result<LogAddrs>; phys_addr(); transmit(&Msg) -> TxResult; receive(timeout) -> Result<Msg,
  Timeout>; next_event() -> Option<Event>; }` with `LinuxCec` (ioctls) and `FakeBus`. `FakeBus`
  models the wire: claim by polling (NACK if unused), directed ACK/NACK, broadcast fan-out,
  tx_status, a monotonic fake clock, an 18-deep TX queue and a lost-messages counter.
  `ScriptedTv` (logical 0, PA 0.0.0.0) runs a timeline script ("at t send SAMR [1000]; expect
  within 1 s broadcast Set System Audio Mode [1]; send UCP 0x41 + UCR; expect Report Audio Status
  bit7=0 vol>prev"); variants Roku-like, Feature-Abort-everything, no-ARC, slow TV.
- Golden byte fixtures (header + opcode + operands) shared with any future C side.
- Bench tier: the same tests against vivid (cec0 as TV via `cec-ctl`/`cec-follower` binaries,
  chorus on cec1), then a real Roku TV with `cec-ctl --monitor`.

### 1g. Bench device names (owner's packet)

- Raspberry Pi 4 with `dtoverlay=vc4-kms-v3d`: both HDMI ports expose CEC as /dev/cec0 and
  /dev/cec1 (one per connector); with the old fkms overlay CEC is not on the kernel API. LEAD
  (Pi forums t=316138, t=327247). Pi 5 uses the same vc4 KMS family: expect the same two nodes
  (LEAD). HDMI0 vs HDMI1 mapping: read /sys/class/cec/cecN/device or CEC_ADAP_G_CONNECTOR_INFO
  (CAP_CONNECTOR_INFO).
- cec-gpio: bit-banged CEC on a GPIO, `compatible = "cec-gpio"; cec-gpios = <&gpio 6
  (GPIO_ACTIVE_HIGH|GPIO_OPEN_DRAIN)>;` (Pi 4B example on GPIO 6/7 with an HDMI passthrough
  breakout), used with `cec-ctl --monitor-pin` as a bus sniffer; adds a /dev/cecN. [K-CEC]
- Pulse-Eight USB-CEC: /dev/ttyACMX; `inputattach --pulse8-cec /dev/ttyACMX` (udev + systemd unit
  in the docs) creates /dev/cecX via pulse8-cec serio. No EDID: set the address with
  CEC_ADAP_S_PHYS_ADDR (or `cec-ctl -E <edid>`). RainShadow Tech dongle also supported. [K-CEC]
- A `cec-ctl -d /dev/cec0 --playback`-style smoke test is the owner's tool; chorus needs only
  read/write on /dev/cecN (group `video` on most distros: LEAD).

## 2. FEC and the latency budget (track `lowlat-wire`)

Inputs from the repo: BRIEF.md 2.2, 5.2, 5.7, 6; docs/decisions/0004-audio-chunk-reserved-bytes.md
(14 opaque bytes at offset 18 of the 32-byte chunk header, reserved for the TV path).

### 2a. XOR parity FEC schemes

- RFC 5109 (Generic FEC, "ULPFEC"; obsoletes RFC 2733 and 3009, not backward compatible): XOR
  over media packets selected by a mask; FEC header 10 octets incl. SN base (16 b), TS recovery
  (32 b), length recovery (16 b) (s7.3). Recovery = XOR of the received media packets and the FEC
  packet, shorter packets zero-padded (s9). "introduces little delay at the encoding side ...
  correctly received packets can be delivered immediately. Delay is only introduced ... when
  packet losses occur" (s15); the mask allows parity across up to 48 packets (s15).
  https://www.rfc-editor.org/rfc/rfc5109.html (read 2026-10-01)
- RFC 2733: same XOR and length recovery (s6.2); s4's interleaved pairs f(a,b) f(b,c) f(c,d)
  survive "some bursts of two consecutive packet losses"; scheme choice is "a tradeoff between
  overhead, delay, and recoverability". https://www.rfc-editor.org/rfc/rfc2733.html
  (read 2026-10-01)
- SMPTE ST 2022-1 (row/column, Pro-MPEG CoP3 lineage): L columns by D rows; column FEC XORs the D
  packets spaced L apart, row FEC XORs L adjacent packets. Overhead (L+D)/(L*D) for both streams;
  decoder latency "equal to the time to receive L x D packets"; a commercial decoder's limits
  L*D <= 100, 1 <= L <= 20, 4 <= D <= 20. The SMPTE text is paywalled; figures from a vendor
  manual: https://portal.vbrick.com/doc/VB9000/490/H264_AdminGuide/8_Advanced.11.3.html
  (read 2026-10-01). Column-only recovers any burst <= L, with rows L+1, also restated in a
  VideoFlow white paper: https://lafibre.info/images/tv/201508_limitations_cachees_du_fec.pdf
  (LEAD, not opened in depth).
- RFC 6363 (FECFRAME): a framework (source flows of ADUs, repair flows, source blocks); no code and
  no latency rule. https://www.rfc-editor.org/rfc/rfc6363.html (read 2026-10-01)
- RFC 6865: Simple Reed-Solomon for FECFRAME, GF(2^m), 2 <= m <= 16, n <= 2^m - 1 (255 at m=8);
  MDS, "a receiver can recover the k source symbols from any set of exactly k encoding symbols"
  (s1, s4.2). https://www.rfc-editor.org/rfc/rfc6865.html (read 2026-10-01)
- RFC 8681 is not Reed-Muller (correction to the task's label): "Sliding Window Random Linear Code
  (RLC) FEC Schemes for FECFRAME" over GF(2) and GF(2^8). s1.1: for block codes "the larger the
  block size, the higher the robustness ... but also the higher the maximum decoding latency";
  s1.2: "recovering an isolated lost source packet always requires waiting for the first repair
  packet to arrive after the end of the block", which sliding windows avoid.
  https://www.rfc-editor.org/rfc/rfc8681.html (read 2026-10-01)

### 2b. Formulas for chorus's simple scheme (derived from the above)

Chunk duration d, group of k data chunks + 1 parity, per-packet loss p.

- Recovery iff at most 1 of the k+1 packets is lost (parity's own loss is harmless); XOR of the
  k survivors equals the missing chunk (RFC 5109 s9).
- Overhead 1/k in packets plus header bytes; at 48 kHz/24-bit stereo k=4 adds 25% of 2.3 Mb/s,
  irrelevant on gigabit (BRIEF 6).
- Added latency: parity follows chunk k, so a lost chunk 1 is repaired (k-1)*d + d_parity after
  its own arrival; worst case ~k*d. Playout must cover the worst case: jitter-buffer floor
  B >= k*d + network jitter + scheduling margin (RFC 8681 s1.2). Lossless arrivals play
  immediately (RFC 5109 s15) but the target cannot drop below k*d without losing repair of early
  chunks.
- Residual loss (Bernoulli p): P_fail = 1 - (1-p)^(k+1) - (k+1)p(1-p)^k ~ C(k+1,2) p^2; residual
  per-chunk loss ~ k p^2. At p = 1e-4, k = 4: ~4e-8 per chunk.
- Unequal lengths: zero-pad and carry a "length recovery" XOR of the 16-bit lengths (RFC 5109
  s7.3, RFC 2733 s6.2); TV-mode chunks should be fixed-length, so it is cheap insurance. Protect
  timestamp and sequence the same way (RFC 5109 "TS recovery", "SN base").
- Interleave (column parity depth L): parity j covers j, j+L, j+2L, ... (D chunks); recovers any
  single burst <= L in the L*D block; overhead 1/D; latency ~ L*D*d (vbrick manual above). At
  2.5 ms chunks, L=4, D=4 costs 40 ms: over budget. Interleave pays only when bursts are real.

### 2c. Packet loss: wired vs Wi-Fi

- 1000BASE-T is specified for BER <= 1e-10 (IEEE 802.3ab tutorial, LEAD for the exact clause):
  https://grouper.ieee.org/groups/802/3/tutorial/march98/mick_170398.pdf (search hit, read
  2026-10-01). A 400-byte frame (3200 bits) then has frame error ratio <= 3.2e-7 from the PHY;
  real links are usually far better. Loss on a quiet switched LAN is dominated by buffer drops
  (egress congestion, microbursts), host socket overflow and link flaps. LEAD: no peer-reviewed
  home-LAN measurement found.
- Wi-Fi: long-distance 802.11 studies show bursts of 15-80% loss lasting from transient to 25-30
  minutes and residual loss 0-10% (MAC retries disabled): https://cs.nyu.edu/~lakshmi/wild_character.pdf
  (search summary, read 2026-10-01, LEAD). Out of scope (TV path is wired only, BRIEF 5.7).

### 2d. Lip-sync standards (positive = sound leads)

- ITU-R BT.1359-1 (1998): detectability about +45 to -125 ms, acceptability about +90 to -185 ms;
  "a positive value indicates that sound is advanced with respect to vision" (considering g,
  Note 1, Appendix fig. 2). Overall tolerance +90/-185 ms; production zone +25/-100 ms;
  transmitter input +22.5/-30 ms.
  https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.1359-1-199811-I!!PDF-E.pdf (read 2026-10-01)
- ATSC IS-191 (2003): sound "should never lead the video program by more than 15ms and should never
  lag the video program by more than 45ms" (+15/-45). Secondary:
  https://www.tvtechnology.com/opinions/managing-lip-sync-265013 (read 2026-10-01)
- EBU R37-2007: end to end, sound before picture <= 40 ms, after <= 60 ms (+40/-60); each stage
  within 5 ms early to 15 ms late (Table 1). https://tech.ebu.ch/docs/r/r037.pdf (read 2026-10-01)
- chorus target (BRIEF 2.2): +/-40 ms, never leading > 15 ms, i.e. [-40, +15] ms: stricter than
  ATSC on the lag side, equal on the lead side.

### 2e. Latency components

| Component | Typical value | Source (read 2026-10-01) |
|---|---|---|
| TV audio out on ARC/optical vs its picture | standard mode: video 115, ARC 116, S/PDIF 115 ms (~0-1 ms late); game mode: video 1, ARC/S/PDIF 67 ms (~66 ms LATE before chorus); 24 Hz: video 132, audio 159 (27 ms late). One TV (Sony XBR85X800H), LPCM 48k/16/2ch. LEAD | https://avlatency.com/measuring-latency/measurement-examples/ |
| TV optical vs ARC | optical PCM 2.0 ~70 ms, ARC PCM 4.5 ms. LEAD (secondary blog, not RTINGS) | https://www.soundbarmatch.com/blog/comparisons-face-offs/soundbar-hdmi-arc-vs-optical-real/ (search summary) |
| S/PDIF receiver (TI DIR9001) | 3/fS = 62.5 us at 48 kHz | https://www.ti.com/lit/ds/symlink/dir9001.pdf (table "tLATE", Fig. 14) |
| Capture (ALSA/USB) | set by buffer size; >= 2 periods per buffer; 1 ms periods (48 frames) x 2 gives ~1-2 ms; USB adds its 1 ms frame scheduling (LEAD) | https://www.alsa-project.org/wiki/FramesPeriods ; https://0pointer.net/blog/projects/all-about-periods.html (search summaries) |
| Packetization | N/48000 s: 120 frames = 2.5 ms, 240 = 5 ms, 48 = 1 ms | arithmetic (BRIEF 6: 20.83 us/frame) |
| Switched GbE per hop | 1500 B x 8 / 1e9 = 12 us plus a few us fabric; a 2.5 ms stereo S32 chunk (~1.06 kB) ~8.5 us | arithmetic; https://cache.industry.siemens.com/dl/files/587/94772587/att_113195/v1/94772587_ruggedcom_latency_switched_network_en.pdf (search hit, not opened, LEAD) |
| AES67 receive delay | 1 ms packet time (48 samples) required; suggested minimum playout delay 144 samples (3 ms) on one or two switches | https://ravenna-network.com/wp-content/uploads/2020/02/AES67-Practical-Guide-1.pdf (pp. 14, 21, fn. 29) |
| FEC wait (block XOR, k chunks) | worst case ~k x d (2b) | RFC 8681 s1.2; Roc fec.html (2f) |
| Endpoint output buffer (ALSA or I2S DMA) | 2-5 ms, chorus measurement pending (BRIEF 5.7: DSP+DAC 2-10 ms) | BRIEF 5.7 |
| DAC filter (TI PCM5102A) | normal 8x filter 20 tS = 417 us; low-latency 3.5 tS = 73 us at 48 kHz | https://www.ti.com/lit/ds/symlink/pcm5102a.pdf (spec table) |
| TAS5825M processing | not found; LEAD for goal 13 (datasheet) | |
| HDMI auto lip-sync | HDMI 1.3 EDID VSDB Video_Latency/Audio_Latency; HDMI 2.0 adds dynamic (CEC) reporting; TV support reported poor. LEAD (forums, a patent, no spec) | https://avlatency.com/terminology/ ; https://www.avsforum.com/threads/dynamic-auto-lip-sync-vs-lip-sync.2942676/ (search summaries) |

ALLM (HDMI 2.1 game mode) matters only indirectly: the picture gets fast but the audio output may
not (the 67 ms row), so chorus starts with audio already lagging. The signed A/V trim (BRIEF 5.7)
can only DELAY audio; it fixes "audio leads", never "audio lags".

### 2f. Production systems (docs only)

- Roc Toolkit (MPL-2.0, docs only): FECFRAME via OpenFEC, Reed-Solomon m=8 ("lower latency, lower
  rates") and LDPC-Staircase ("higher latency, higher rates"); "the latency (jitter buffer size)
  can not be less than fec block size", e.g. 10 source + 5 repair needs latency >= 10 packets and
  1.5x data rate. Example `--target-latency=200ms`; tuner profiles
  default/responsive/gradual/intact, backend niq. https://roc-streaming.org/toolkit/docs/internals/fec.html ;
  https://roc-streaming.org/toolkit/docs/about_project/features.html ;
  https://roc-streaming.org/toolkit/docs/manuals/roc_send.html ;
  https://roc-streaming.org/toolkit/docs/manuals/roc_recv.html (all read 2026-10-01)
- AES67: 1 ms packet time mandatory, 125 us optional (AVB Class A compatible), range 125 us to
  4 ms; playout delay must exceed packet time plus jitter, suggested start 3 ms (Practical Guide
  above); the 125 us/4 ms range is from search summaries of https://en.wikipedia.org/wiki/AES67
  (LEAD).
- Sonos: no millisecond figure; offers "TV Dialog Sync" to delay audio when audio leads.
  https://support.sonos.com/en-us/article/tv-audio-and-video-are-out-of-sync (read 2026-10-01). A
  "75 ms" figure appears only in community posts
  (https://en.community.sonos.com/home-theater-228993/lip-sync-problems-6765583, LEAD, not opened).

### 2g. Proposed budget and defaults (cheap to change)

Stereo, optical/ARC into server-side capture, one or two hops, wired endpoint, d = 2.5 ms, k = 4:

| Stage | ms |
|---|---|
| S/PDIF receiver | 0.06 |
| capture (1 ms period x 2) | 2.0 |
| packetize (120 frames) | 2.5 |
| server send + 2 hops + endpoint receive | 0.5 |
| FEC wait, worst case k x d | 10.0 |
| jitter margin | 2.0 |
| endpoint output buffer + DSP block | 4.0 |
| DAC filter | 0.4 |
| total, capture-in to air | ~21.5 |

Against [-40, +15] ms that leaves ~18 ms of lag headroom for the TV's own error; the standard-mode
TV (+1 ms late) fits; a game-mode TV (66 ms late) cannot be saved by any chorus setting. d = 5 ms,
k = 4 gives ~34 ms (~6 ms left: too tight); d = 2.5 ms, k = 2 gives ~16.5 ms at 50% overhead.

- Chunk d = 2.5 ms (120 frames at 48 kHz): 32 B header per ~960 B stereo S32 payload; 400 pps per
  endpoint is nothing for a wired ESP32-S3 or Linux client (verify on the W5500 bench, LEAD). 1 ms
  (AES67) makes the FEC wait 4 ms but costs 1000 pps and 4x header rate: the measured fallback.
- k = 4, one XOR parity sent right after the 4th chunk; 25% overhead; residual ~C(5,2) p^2 per
  group: at p = 1e-3 one unrecoverable group per ~1000 s, at p = 1e-4 one per ~28 h.
- No interleave by default. A single XOR group fails on any burst >= 2; if the bench shows bursts
  of 2-3, add a column mode (L = 2: two interleaved parity streams over 8 chunks), doubling the FEC
  wait to ~20 ms. Decide from data.
- Target playout delay (capture timestamp to DAC): 20 ms default, configurable 10-40 ms, floor
  d + k*d + net_p999 + margin, refused below it.
- Concealment: a failed group plays the lost 2.5 ms as a short crossfade to silence (or repeat),
  counted in telemetry; never a resync.
- Header (decision 0004's reserved bytes): group = seq div k, so a data chunk needs only a mode
  byte and k (2 bytes). Parity is a separate message type (decision 0003: unknown types skipped)
  with seq_base, k, XOR of timestamps, XOR of lengths and the XOR payload; length recovery is a
  2-byte formality kept for safety.
- Simulator loss model (crates/sync/src/house_report.rs lists "packet loss and reordering" as "Not
  modelled"): (a) Bernoulli p in {0, 1e-5, 1e-4, 1e-3, 1e-2}; (b) Gilbert-Elliott two-state
  Markov, p (good->bad), r (bad->good), loss h in good, k_bad in bad; stationary bad probability
  p/(p+r); mean burst 1/r; loss rate (p*k_bad + r*h)/(p+r). E. N. Gilbert, "Capacity of a
  burst-noise channel", BSTJ 39(5), 1960, pp. 1253-1265; E. O. Elliott, "Estimates of error rates
  for codes on burst-noise channels", BSTJ 42(5), 1963, pp. 1977-1997 (as cited in Hasslinger and
  Hohlfeld 2008,
  https://people.computing.clemson.edu/~jmarty/projects/lowLatencyNetworking/papers/APPFEC/GEModelForLossinTheRTInternet.pdf,
  read 2026-10-01; verify the Elliott issue number). Mean bursts 1, 2, 3 and a rare outage state
  (100 ms+, link flap) FEC cannot cover, to exercise concealment; seeded from
  crates/sync/src/rng.rs for determinism.
- Bench packet before tuning: a 24 h sequence-number loss and burst histogram on the owner's LAN at
  400 pps, to replace the 2c LEADs with a measurement (BRIEF 3.1 guardrail 3).

## 3. TV capture, rate matching, non-PCM refusal (track `capture`)

### 3a. The S/PDIF receivers

DIR9001 (TI SLES198A, rev. May 2015) [D1], full text:

- Input "28 kHz to 108 kHz", 24-bit max; accepts "32/44.1/48/88.2/96 kHz, +/-1500 ppm"; spec
  table: "IEC60958 sampling frequency accuracy Level II (+/-1000 ppm) Level III (+/-12.5%)"; "The
  capture ratio of the built-in PLL complies with level III ... (+/-12.5%)" [D1 p.1-2, s.8.3].
- Pin 1 AUDIO: "Channel-status data information of non-audio sample word, active-low"; Table 8:
  L = "Audio sample word represents linear PCM samples", H = "used for other purposes". It is
  channel status bit 1 of the PREVIOUS block, L channel only, updated at the block top. Only bits 1
  (AUDIO) and 3 (EMPH) come out on pins; the rest only serially on COUT [D1 s.8.3.8].
- Pin 27 ERROR: "Indication of internal PLL or data parity error"; Table 10: L = "Lock state of
  PLL and nondetection of parity error", H = "Unlock state of PLL or detection of parity error"
  [D1 s.8.3.9]. PLL lock-up "From biphase signal detection to error-out release (ERROR = L)":
  100 ms typ [D1 electrical characteristics].
- Parity errors: "For PCM data, interpolation processing by previous data is performed. For
  non-PCM data, interpolation is not performed ... (Non-PCM data is data with channel-status data
  bit 1 = 1.)" [D1 s.8.3.9.2]. "A rapid continuous change or a discontinuous change of the input
  sampling frequency causes the PLL to lose lock" [D1 s.8.3.9.3].
- FSOUT[1:0] MEASURES the rate against a 24.576 MHz XTI (channel status bits 24-27 "are not output
  through these pins"; without XTI always LL). Table 11: HL = out of range or unlocked; HH = 32 kHz
  (31.2-32.8); LL = 44.1 kHz (43-45.2); LH = 48 kHz (46.8-49.2) [D1 s.8.3.10]. Coarse (+/-2.5%): a
  rate CLASS, not a ppm estimate.
- Locked clocks (CKSEL = L): LRCKO = fS, BCKO = 64 fS, SCKO = 128/256/384/512 fS per PSCK; 48 kHz:
  BCKO 3.072 MHz, SCKO 12.288 MHz at 256 fS [D1 Table 5]. All recovered from the stream: the Pi as
  I2S consumer is clocked by the TV.
- Table 12, ERROR = H: PLL mode clocks from the "VCO free-running" clock (not constant), DOUT "MUTE
  (Low)", AUDIO LOW, FSOUT HL; AUTO mode (CKSEL tied to ERROR) uses XTI when unlocked, and "if an
  XTI clock source is not provided ... SCKO, BCKO, and LRCKO are not output during the ERROR
  period" [D1 s.8.4.1, s.8.3.4]. CLKST pulses on every lock change, "can be used for muting".
- Inference: with the TV off, a Pi on a DIR9001 board gets either zeros at a free-running wrong
  rate (PLL mode) or no clock, so the ALSA read stalls (AUTO, no XTI); which depends on a given
  Amazon module's CKSEL/XTI strapping (bench item). Wiring ERROR, AUDIO (ideally CLKST) to Pi GPIOs
  makes lock and non-PCM visible.

CS8416 (Cirrus DS578F5) [D2], why bit 1 alone is not enough: "certain non-audio sources, such as
AC-3 or MPEG encoders, may not adhere to this convention, and the bit may not be properly set." Its
autodetect looks for "sync codes in the proper format for IEC61937 or DTS"; AUDIO = OR of
AUTODETECT and bit 1; "If non-audio data is detected, the data is still processed exactly as if it
were normal audio ... It is up to the user to mute" [D2 s.10.2]. Register 0Bh flags PCM /
IEC61937 / DTS_LD / DTS_CD / DGTL_SIL ("at least 2047 consecutive constant samples of the same
24-bit audio data on both channels"); Pc/Pd in 23h-26h [D2 s.10.2.1, 0Bh]. So the DIR9001 (bit 1
only) misses a mislabelled IEC 61937 stream: chorus must also scan for Pa/Pb in software.

HiFiBerry Digi+ I/O (WM8804) [H1] [D3]: HiFiBerry's recording page (2020-08-05): "Sample rate and
bits/sample on the recording application have to be set to match exactly the source format";
"There is no detection of the sample rate of the source"; "If no source is connected, recording
will block. You won't just record silence, but the whole system might block"; recommended only
where "you know exactly what your source is delivering" [H1]. A search summary of the same page
adds "Only 2-channel PCM audio is supported. You can't use it to record Dolby Digital or
DTS-encoded data" (snippet, not in the fetched rendering). The chip has the flags (WM8804 Rev 4.5,
Table 45) [D3]: UNLOCK ("0 = Locked onto incoming S/PDIF stream"), TRANS_ERR, AUDIO_N ("Recovered
Channel Status bit 1"), PCM_N ("non-audio code (defined in IEC-61937) has been detected"),
NON_AUDIO = PCM_N OR AUDIO_N, ZEROFLAG ("1024 consecutive all zero frames"), REC_FREQ[1:0] (00 =
192, 01 = 96/88.2, 10 = 48/44.1, 11 = 32 kHz; 44.1 and 48 NOT distinguished), in registers or on
GPO pins. Whether HiFiBerry's (GPL, not opened) driver exposes them as ALSA controls is UNKNOWN.
LEADs: HiFiBerry community "Identify SPDIF/TOSLINK input sample rate via ALSA" (did not render),
moode "Digi+ I/O Can't Record 24/96", Volumio "Locking sample rate from Hifiberry Digi+ i/o
toslink input" (titles only). Bench: `amixer -c <card> contents`. Consequence: chorus may get only
PCM samples and a capture that BLOCKS with no source, so the software IEC 61937 scan is mandatory
and the capture thread must poll with a timeout.

### 3b. IEC 60958-3 channel status and IEC 61937

IEC 60958-3 [I1] (IS/IEC 60958-3:2003, BIS adoption, Public.Resource.Org; later editions not read):

- Byte 0 bit 0 "0" = consumer. Bit 1: "0" = "Audio sample word represents linear PCM samples";
  "1" = "Audio sample word used for other purposes" [I1 s.5].
- Byte 3 bits 24-27 (bit 24 first): 0000 = 44.1 kHz; 0100 = 48; 1100 = 32; 0010 = 22.05; 0001 =
  88.2; 0011 = 176.4; 0110 = 24; 0101 = 96; 0111 = 192; 1000 = "not indicated" (OCR'd table
  columns mapped in order; 48 kHz = 0100 also matches the S/PDIF Wikipedia summary, snippet). The
  DECLARED rate, not a measurement.
- Bits 28-29 clock accuracy: 00 = Level II, 10 = Level I, 01 = Level III, 11 = "Interface frame
  rate not matched to sampling frequency".
- s.7.2.1: Level I "+/-50 x 10^-6"; Level II "+/-1 000 x 10^-6"; Level III variable pitch, "A range
  of +/-12,5 % is envisaged". s.7.2.2: "By default, receivers should be able to lock to signals of
  level II accuracy". s.7.2.3: "the receiver shall support 32 kHz, 44,1 kHz and 48 kHz operation".
- 48 kHz from a TV is ASSUMED typical (Roku states nothing); Level II (+/-1000 ppm) is the worst
  case to accept, bounding the resampling ratio.

IEC 61937 [I2] (IEC 61937-1:2021 preview, iTeh):

- "Each data-burst consists of a 64-bit burst-preamble, followed by the burst-payload"; "The 16
  bits of a data-burst are placed in time-slots 12 to 27 of an IEC 60958 subframe. Both odd and
  even ... subframes ... are simultaneously used" [I2 s.4]. In a 24-bit sample (slots 4-27) the
  burst word is the TOP 16 bits; in S32_LE capture, bits 31..16.
- Table 3: Pa = F872h, Pb = 4E1Fh, Pc = burst-info, Pd = length-code; "The frame beginning the
  data-burst contains preamble word Pa in subframe 1, and Pb in subframe 2. The next frame contains
  Pc in subframe 1 and Pd in subframe 2" [I2 s.6.1.7]. Pc bits 0-6 data-type (IEC 61937-2, not
  read), bit 7 error flag, bits 13-15 bitstream number [I2 Table 5].
- Bit 1 "shall be set to '1'" for non-linear PCM (s.6.1.4); validity bit recommended '1' "to
  prevent accidental decoding of non-audio data to analogue" (s.6.1.3); bit 1 stays '1' in idle
  "when further non-linear PCM encoded audio is anticipated" (s.3.1.11).
- Burst repetition = "number of encoded audio samples of each channel" [I2 s.6.1.6]; AC-3: 1536
  frames = 32 ms at 48 kHz (A/52 frame size via P2 [P5], computed).
- WM8804 and CS8416 detect the "96 bit synchronization code ... 4*16bits of '0' +Pa (16bits)+Pb
  (16bits)" [D3 p.~50], [D2 s.10.2].

### 3c. Rate matching

- Adriaensen 2005 [A1]: map sample count to system time; wakeups carry delay, jitter, timer
  quantisation, and "the sample clock is not locked in any way to the one that drives the system
  timer" (s.1). Second-order loop, zero average error for zero acceleration: period rate F, loop
  bandwidth B, omega = 2 pi B / F, a = 0, b = sqrt(2) omega, c = omega^2, "b is set to give a
  critically damped loop" (s.3, eqs. 9-12). Per period: e = timer read minus predicted t1; t0 =
  t1; t1 += b e + e2; e2 += c e; e2 starts at the nominal period; Te = (t1 - t0) / period frames
  (s.2 eq. 5, s.4). USB jitter "from the original +/-2 ms to a range of about +/-10 us"; PCI
  "better than one microsecond"; "can easily reduce the system time jitter by a factor of 100"
  (s.5). No single recommended B.
- Adriaensen 2012 [A2]: control the ratio from the delay error E = W(t) - R(t) + d_res - Delta,
  NOT the raw buffer fill, which "doesn't change in a smooth way" (s.3.1, s.3.3). Without the
  resampler's fractional delay, at nominally equal rates the error is "a sawtooth function with a
  frequency equal to the difference between the two actual sample rates" the loop "may not
  completely remove" (s.3.2): exactly chorus's 48 to 48 kHz case. Error low-passed (second order,
  20x loop bandwidth), then a second-order loop; normal bandwidth "around 0.05 Hz"; a one-off
  integer skip/insert removes the initial error, then "at higher bandwidth for the first 4
  seconds" (s.3.3, s.3.4). Fast delay modulation acts like jitter; slow changes are like a
  listener moving ("one sample at 48 kHz corresponds to about 7 millimeters in air") (s.2). 48 to
  44.1 kHz settles in about 15 s, then phase variation "less than 0.5 degrees peak-to-peak" at
  1 kHz (about 1.4 us) (s.5).
- ALSA timestamps [K1]: snd_pcm_status gives trigger_tstamp, tstamp ("the current system timestamp
  updated during the last event or application query") and audio_tstamp; type CLOCK_REALTIME,
  CLOCK_MONOTONIC or CLOCK_MONOTONIC_RAW; "The link time can be used to track long-term drifts
  between audio and system time using the (tstamp-trigger_tstamp)/audio_tstamp ratio". Channel map
  via the "Capture Channel Map" control [K2].
- Clock tolerance: IEC Level II +/-1000 ppm, Level I +/-50 ppm [I1 s.7.2.1]; DIR9001 accepts
  "+/-1500 ppm" and its XTI "Frequency accuracy ... -100 100 ppm" [D1]. The hub's timeline is
  server-synced, so the observed rate is (TV error) minus (hub-to-server correction): roughly
  +/-1000 ppm plus the server-sync slew.

### 3d. Signal detection for TV autoplay

- Hardware lock: DIR9001 ERROR (L = locked, no parity error), lock-up 100 ms typ, CLKST pulse [D1];
  WM8804 UNLOCK [D3]; CS8416 UNLOCK [D2].
- When the TV stops (datasheet inference, bench to confirm): DIR9001 PLL mode keeps clocking from
  a free-running VCO with DOUT low, so capture returns ZEROS at a wrong rate; AUTO without XTI
  stops the bit clock, so capture STALLS; HiFiBerry says recording "will block" [H1]. Neither gives
  an ALSA error by design; the driver may time out with -EIO after a stall (ASSUMED, kernel source
  not opened). Treat each of (a) poll timeout > 2 periods, (b) DLL rate outside +/-1500 ppm of
  nominal, (c) lock GPIO high as "no signal".
- Digital silence: CS8416 "at least 2047 consecutive constant samples ..." [D2]; WM8804 ZEROFLAG
  "1024 consecutive all zero frames" [D3]. A TV on but silent likely sends zeros while locked
  (ASSUMED; bench).
- Proposal: "TV active" = locked, PCM, and any sample above -90 dBFS within the last 1 s; "TV idle"
  = unlocked or digital silence for an owner-set hold (default 10 min, ASSUMED). CEC power and
  Active Source (P2) are the primary trigger; S/PDIF is the fallback for optical.

### 3e. Channel orders, layout, stereo on 5.1

- HDMI/CTA-861 CA 0x0B = FL FR LFE FC RL RR (slots 0-5): STILL A LEAD. Open sources found were
  (a) Linux kernel source (codebrowser hdmi_chmap.c, lkml hdmi-codec patches, a 2026 ratatoskr
  "Consolidate CEA channel" RFC: NOT opened, GPL; a search summary quotes the kernel entry for 0x0b
  as "{RR, RL, FC, LFE, FR, FL}" channel 7 to 0, consistent with the LEAD), (b) copies of the
  paywalled CTA-861 or HDMI spec on third-party hosts (archive.org CTA-861-G, CEA-861-B; not
  opened, not licensed copies), or (c) unreachable: analog.com (AD9398 Table 33, ADV7513 PG Table
  64, ADV7511 PG) timed out and Mouser's mirror returned "Access denied". Read without the table:
  Xilinx PG235 (only a "/* Channel Allocation */" stub), i.MX6 HDMI chapter (CA[7:0] field only),
  TDA19988, Rockchip RK312x HDMI chapter. Next: ADI AD9398 Table 33 or ADV7513 PG from a network
  that reaches analog.com.
- ALSA surround51: "#0 - front left, #1 - front right, #2 - rear left, #3 - rear right, #4 -
  center, and #5 - LFE (subwoofer)" [M2] (U. Reddy, modified 2009-04-01, full page read);
  third-party doc; the LGPL alsa-lib config was not opened.
- WAVE_FORMAT_EXTENSIBLE: FL FR FC LFE BL BR (planning research [M1], Microsoft docs; not re-read).
- For goal 13's stereo scope none of this is on the path (optical and ARC deliver 2.0); it matters
  only with a 5.1 source (P2 Options B/C, not built).
- ITU-R BS.775-1 [T1]: front L/R "at the extremities of an arc subtending 60 degrees" (+/-30),
  centre at 0; surrounds "within the sectors from 100 degrees to 120 degrees from the centre front
  reference. Precise location is not necessary"; no closer than the fronts "unless compensating
  time delay is introduced" (+/-110 is the common midpoint, not the text). BS.775-3 (2012) [T2]
  keeps the 100-120 sentence and band-limits the LFE "up to 120 Hz".
- BS.775-1 Annex 5 "Upwards conversion" [T1]: stereo over three fronts goes "over the left and
  right loudspeakers only"; "When there is no surround signal in a programme, surround loudspeakers
  should not be activated"; mono to the centre only. Reference behaviour: 2.0 -> L, R; C and
  surrounds silent; sub by bass management. Table 2 downmix: L' = L + 0.7071 C + 0.7071 LS;
  R' = R + 0.7071 C + 0.7071 RS [T1].
- Sonos [S1]: Surrounds on/off, "TV Level" and "Music Level" sliders, "Music Playback" Ambient
  (default) or Full, which "does not apply to TV audio or Dolby Atmos music". Stereo TV rendering
  on surrounds is not stated; a search summary of community threads says it is upmixed with
  surrounds ambient-only and "no way" to get full stereo on them (snippet, LEAD).
- Optional passive upmix if the owner wants surrounds for TV: C = 0.7071 (L + R) (-3 dB, mirroring
  the downmix coefficient [T1]); ambient S = g (L - R), g at or below -6 dB, decorrelated between
  the two surrounds per Annex 5 s.2.2 ("decorrelation between each loudspeaker signal should be
  performed"). Coefficients are a chorus choice (DSP-8), not a standard; default OFF per s.2.1.

### 3f. Proposals

1. Capture: 2 ch S32_LE (24-bit in the top bits) at the declared nominal rate (48 kHz first;
   reopen at 44.1/32 on a rate-class change). Period 240 frames = 5 ms (BRIEF 5.7's 5 ms capture
   chunk), buffer 4 periods (20 ms), poll with a 2-period timeout; forward 5 ms chunks.
2. Timing: per period read snd_pcm_status tstamp on CLOCK_MONOTONIC (guardrail) [K1] into the
   [A1] DLL at F = 200 periods/s, B = 1 Hz (omega = 0.0314, b = 0.0444, c = 0.000987; computed),
   giving TV frames per hub ns; map to the server timeline with the existing sync offset/rate.
3. Ratio control per [A2]: from the delay error (in minus out plus fractional position minus
   target), 0.05 Hz normal, 0.5 Hz for the first 4 s, a one-off integer skip/insert. Clamp to
   1 +/- 1500 ppm (Level II 1000 ppm plus margin, the DIR9001 window); pinned at the clamp > 2 s
   means a wrong nominal rate: reopen. Simulator first: a fixture TV clock at +/-1000 ppm with
   wander, jittered period timestamps, assert bounded delay error.
4. Resampler: reuse CubicResampler (crates/sync/src/latency_grow.rs); its doc says Catmull-Rom is
   fine for "a stretch of under a tenth of a percent" (up to 1000 ppm, the clamp's edge). API gap:
   `render(&ChunkPlan)` takes a plan whose `segment` is private and built only by LatencyPlan, so
   capture needs a small public constructor (constant-rate ChunkPlan, or `render_positions` with
   source_start and a per-chunk rate). It already has with_capacity/room for a real-time thread;
   its fractional source position is [A2]'s d_res.
5. Non-PCM refusal (P2 Option C not built): never forward, room plays ramped silence, whenever ANY
   of (a) bit 1 = 1 (DIR9001 AUDIO high [D1], WM8804 AUDIO_N [D3], or COUT); (b) WM8804 PCM_N;
   (c) the always-on software scan sees top-16 bits L = 0xF872 and R = 0x4E1F in a frame [I2
   Table 3] (the Digi+ may expose neither flag; some encoders mislabel bit 1 [D2]); (d) validity
   bit set where visible. Mute on the chunk containing the sync word; unmute after 250 ms with bit
   1 = 0 and no sync word (about 8 AC-3 periods; other data-types not read, so 250 ms is ASSUMED).
   UI: "the TV is sending a compressed format (for example AC-3): set the TV's digital audio output
   to PCM" [P2's Roku menu]. Shared fixtures: PCM sine, synthetic IEC 61937 (Pa/Pb/Pc/Pd + zero
   payload at 1536-frame repetition), a mislabelled stream (bit 1 = 0 with bursts), a PCM signal
   hitting 0xF872 in L only.

Open items: CA 0x0B from a reachable ADI datasheet; IEC 61937-2 data-types and repetition periods
(paid; not needed for refusal); Digi+ I/O ALSA controls (bench `amixer`); DIR9001 module CKSEL/XTI
strapping (bench); whether a Roku TV sends zeros while on and silent, and stops the optical
carrier in standby (bench).

## 4. Where the notes disagree

- Capture period and chunk size: the FEC note budgets capture at a 1 ms period x 2 (2.0 ms) and a
  2.5 ms (120-frame) chunk; the capture note proposes a 240-frame (5 ms) period, a 4-period (20 ms)
  ALSA buffer, and 5 ms forwarded chunks per BRIEF 5.7. The FEC note itself computes that d = 5 ms,
  k = 4 totals ~34 ms ("too tight"). The two must be reconciled in the goal-13 decision record.

## What was read

All on 2026-10-01.

CEC: kernel documentation prose only (no header-source pages): [K-OPEN], [K-CAPS], [K-LOG],
[K-PHYS], [K-MODE], [K-RX], [K-EV], [K-POLL], [K-INTRO], [K-CEC], [K-VIVID]. Permissive source,
licence checked first: [CRATE] cec_linux 0.2.2, MIT (Cargo.toml `license = "MIT"`, LICENSE file
MIT, crates.io API), src/sys.rs and src/lib.rs (cloned to scratch; size test compiled); [AOSP-H],
[AOSP-AS], [AOSP-V], [AOSP-B], [AOSP-ARC], Constants.java, [CTS-SAM], [CTS-INV], all Apache-2.0.
Docs and support pages: Android HDMI-CEC and soundbar docs, [ROKU-SS], [ROKU-HDMI], [ROKU-SB],
[LKDDB], cec-o-matic (JS tool; static page gave no operand data). Search snippets only (LEAD):
Raspberry Pi forums t=316138, t=327247; dri-devel 2016-08 announcement (fetch returned 403).

CEC clean-room statement: no GPL source was opened: not libcec, not v4l-utils / cec-ctl /
cec-follower / cec-compliance source, not the Linux kernel's C source, not include/uapi/linux/cec.h
or cec-funcs.h, not the kernel "cec-header" doc page, and not vivid-cec.c (it appeared in search
results as a codebrowser link and was not opened). The ioctl defines appear as comments inside the
MIT `cec_linux` crate, permissive source allowed under docs/clean-room.md; the numeric codes were
recomputed independently from the documented struct layouts.

FEC and latency: read RFC 5109, RFC 2733, RFC 6363, RFC 6865, RFC 8681 (rfc-editor.org); ITU-R
BT.1359-1 PDF (itu.int); EBU R37-2007 PDF (tech.ebu.ch); tvtechnology.com "Managing lip sync"; TI
DIR9001 and PCM5102A datasheets (ti.com); vbrick SMPTE 2022-1 FEC page; Hasslinger/Hohlfeld
Gilbert-Elliott PDF (clemson.edu mirror); RAVENNA AES67 Practical Guide PDF; avlatency.com
measurement examples; Sonos support article; Roc Toolkit docs (fec.html, features.html,
roc_send.html, roc_recv.html). Search-result summaries only (not opened, LEAD): IEEE 802.3ab
tutorial, WiLD Wi-Fi loss paper, Siemens RUGGEDCOM latency FAQ, ALSA FramesPeriods wiki, 0pointer
periods blog, soundbarmatch, AVS Forum lip-sync threads, Sonos community, AES67 Wikipedia,
VideoFlow FEC paper. Correction: RFC 8681 is sliding-window RLC, not Reed-Muller.

FEC clean-room statement: no GPL source file was opened. No Snapcast, squeezelite, shairport-sync or
snapclient material of any kind was read; Roc Toolkit (MPL-2.0) was read through its documentation
pages only, never its source.

Capture: datasheets [D1], [D2], [D3] (PDF text extracted), [I1] (OCR quality fair), [I2], [H1],
[A1], [A2], [K1], [K2], [M2], [T1], [T2], [S1]; [P5] ATSC A/52:2018 via P2-theater-scope.md (not
re-read). Read but not cited for content: Xilinx PG235 v3.0 (amd.com), NXP i.MX6 HDMI chapter
(people.freebsd.org), NXP TDA19988 (datacapturecontrol.com), Rockchip RK312x HDMI TX chapter
(rockchip.fr). Unreachable: analog.com (AD9398, ADV7511/ADV7513 PGs: timeouts), Mouser ADI mirrors
(access denied), IT66121 PG (404), Intel AI bundle page (404 redirect), HiFiBerry community thread
(empty render). Searches (titles/summaries only, LEADs): HiFiBerry recording limits; CS8416 IEC
61937; HDMI CA tables (x4); IEC 60958-3 fields; ALSA surround51; Sonos surround; ITU BS.775.

Capture clean-room statement: no GPL or LGPL source file was opened: not alsa-lib, not the kernel
`sound/` tree (hdmi_chmap.c, hdmi-codec, the WM8804 or HiFiBerry drivers), not PulseAudio,
PipeWire, JACK, zita-ajbridge or alsa_in. Kernel source pages and lkml patches that appeared in
search results were not fetched; one search summary quoted a kernel table line, recorded above as a
LEAD only. Only datasheets, standards previews/public adoptions, papers, kernel documentation pages
and vendor docs were read. The repo file read was crates/sync/src/latency_grow.rs (chorus's own,
MIT OR Apache-2.0).

## Sources

### Section 1 (CEC), all read 2026-10-01

- [K-OPEN] https://docs.kernel.org/userspace-api/media/cec/cec-func-open.html (read 2026-10-01)
- [K-CAPS] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-caps.html (read 2026-10-01)
- [K-LOG] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-log-addrs.html (read 2026-10-01)
- [K-PHYS] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-phys-addr.html (read 2026-10-01)
- [K-MODE] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-g-mode.html (read 2026-10-01)
- [K-RX] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-receive.html (read 2026-10-01)
- [K-EV] https://docs.kernel.org/userspace-api/media/cec/cec-ioc-dqevent.html (read 2026-10-01)
- [K-POLL] https://docs.kernel.org/userspace-api/media/cec/cec-func-poll.html (read 2026-10-01)
- [K-INTRO] https://docs.kernel.org/userspace-api/media/cec/cec-intro.html (read 2026-10-01)
- [K-CEC] https://docs.kernel.org/admin-guide/media/cec.html (read 2026-10-01)
- [K-VIVID] https://docs.kernel.org/admin-guide/media/vivid.html (read 2026-10-01)
- [CRATE] cec_linux 0.2.2, MIT: https://github.com/User65k/cec_linux (read 2026-10-01)
- [AOSP-H] hdmi_cec.h, Apache-2.0:
  https://android.googlesource.com/platform/hardware/libhardware/+/lollipop-dev/include/hardware/hdmi_cec.h
  (read 2026-10-01)
- [AOSP-AS] HdmiCecLocalDeviceAudioSystem.java, [AOSP-V] HdmiCecMessageValidator.java, [AOSP-B]
  HdmiCecMessageBuilder.java, [AOSP-ARC] ArcInitiationActionFromAvr.java, Constants.java,
  Apache-2.0, under
  https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/hdmi/
  (read 2026-10-01)
- [CTS-SAM] [CTS-INV] Android CTS hdmicec audio tests, Apache-2.0:
  https://android.googlesource.com/platform/cts/+/refs/heads/main/hostsidetests/hdmicec/src/android/hdmicec/cts/audio/
  (read 2026-10-01)
- https://source.android.com/docs/devices/tv/hdmi-cec (read 2026-10-01)
- https://source.android.com/docs/core/audio/soundbar (read 2026-10-01)
- [ROKU-SS] https://support.roku.com/en-us/article/connect-surround-sound-to-your-roku-tv (read 2026-10-01)
- [ROKU-HDMI] https://support.roku.com/en-ca/article/configure-hdmi-settings-on-your-tv (read 2026-10-01)
- [ROKU-SB] https://support.roku.com/article/remote-not-controlling-soundbar (read 2026-10-01)
- [LKDDB] https://cateee.net/lkddb/web-lkddb/VIDEO_VIVID_CEC.html (read 2026-10-01)
- https://www.cec-o-matic.com/ (read 2026-10-01; JS tool, no operand data)

### Section 2 (FEC and latency), all read 2026-10-01

Every URL for section 2 is given inline in section 2, next to the figure it supports, with its
read date or LEAD status; they are not repeated here.

### Section 3 (capture), all read 2026-10-01

- [D1] TI DIR9001 datasheet SLES198A (Dec 2006, rev. May 2015),
  https://www.ti.com/lit/ds/symlink/dir9001.pdf (read 2026-10-01)
- [D2] Cirrus Logic CS8416 DS578F5,
  https://statics.cirrus.com/pubs/proDatasheet/CS8416_DS578F5.pdf (read 2026-10-01)
- [D3] Cirrus (Wolfson) WM8804 PD Rev 4.5 (March 2009),
  https://statics.cirrus.com/pubs/proDatasheet/WM8804_v4.5.pdf (read 2026-10-01)
- [I1] IS/IEC 60958-3:2003 (BIS adoption, Public.Resource.Org),
  https://law.resource.org/pub/in/bis/S04/is.iec.60958.3.2003.pdf (read 2026-10-01)
- [I2] IEC 61937-1:2021 preview (iTeh),
  https://cdn.standards.iteh.ai/samples/101993/f9c9621694de4bdcbc934f16d281bf1d/IEC-61937-1-2021.pdf
  (read 2026-10-01)
- [H1] HiFiBerry, "Comparison of HiFiBerry cards for audio recording" (2020-08-05),
  https://www.hifiberry.com/docs/hardware/comparison-of-hifiberry-cards-for-audio-recording/
  (read 2026-10-01)
- [A1] F. Adriaensen, "Using a DLL to filter time", LAC 2005,
  https://kokkinizita.linuxaudio.org/papers/usingdll.pdf (read 2026-10-01)
- [A2] F. Adriaensen, "Controlling adaptive resampling", LAC 2012,
  https://kokkinizita.linuxaudio.org/papers/adapt-resamp.pdf (read 2026-10-01)
- [K1] ALSA PCM timestamping, https://docs.kernel.org/sound/designs/timestamping.html
  (read 2026-10-01)
- [K2] ALSA PCM channel-mapping API,
  https://docs.kernel.org/sound/designs/channel-mapping-api.html (read 2026-10-01)
- [M2] U. Reddy, ALSA Multi-channel Audio mini-HOWTO,
  https://www.csa.iisc.ac.in/~udayb/alsamch.shtml (read 2026-10-01)
- [T1] ITU-R BS.775-1 (1994),
  https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.775-1-199407-S!!PDF-E.pdf (read 2026-10-01)
- [T2] ITU-R BS.775-3 (2012),
  https://www.itu.int/dms_pubrec/itu-r/rec/bs/R-REC-BS.775-3-201208-P!!PDF-E.pdf (read 2026-10-01)
- [S1] Sonos, "Change surround audio settings",
  https://support.sonos.com/en/article/change-surround-audio-settings (read 2026-10-01)
- [P5] ATSC A/52:2018 via P2-theater-scope.md (not re-read)
