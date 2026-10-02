/* The endpoint's firmware update: an A/B state machine with rollback.
 *
 * Two app slots, one of which runs. A new image is written to the other one,
 * checked, selected for the next boot, booted once on trial, and kept only
 * when it proves it still reaches its server; otherwise the bootloader goes
 * back to the image that ran before. This unit is the decisions. It reads no
 * clock, holds no heap and names no ESP-IDF call: the flash, the bootloader's
 * record and the reboot are the `chorus_ota_flash_t` it is handed
 * (firmware/main/esp_ota.c on the board, firmware/tests/fake_flash.c under
 * test), and the time is passed in as monotonic nanoseconds.
 *
 * The contract it mirrors is ESP-IDF v6.1's (read 2026-10-02 at the pinned
 * tag, commit fff9895c82d744c7237be8847347bdd1b07c6643):
 *
 *   - the image states NEW, PENDING_VERIFY, VALID, INVALID, ABORTED and
 *     UNDEFINED are components/bootloader_support/include/esp_flash_partitions.h:67-74;
 *   - with CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE the bootloader turns a
 *     PENDING_VERIFY entry it meets at boot into ABORTED and a NEW one it
 *     selects into PENDING_VERIFY (components/bootloader_support/src/
 *     bootloader_utility.c:393-401 and :443-449), and an INVALID or ABORTED
 *     entry is never selected (bootloader_common_loader.c:78-86);
 *   - the app confirms the running image, or marks it invalid and reboots
 *     (components/app_update/esp_ota_ops.c:1179-1236), and the call that
 *     starts a write refuses while the running image is still PENDING_VERIFY
 *     (:178-185). The calls themselves are named in one unit only,
 *     firmware/main/esp_ota.c; the source scan refuses them here.
 *
 * The rules (docs/decisions/0107-*, docs/protocol.md "Firmware update"):
 *
 *   1. Nothing is written without an offer. An offer carries the size, the
 *      SHA-256 and the version; one that does not fit the slot, is for
 *      another board, arrives while another transfer is being received or
 *      while the running image is still on trial is refused by name.
 *   2. The boot slot changes only after the whole image is written AND its
 *      SHA-256 equals the offer's AND the medium's own check of the written
 *      image passes.
 *   3. After the reboot the new image is on trial (pending-verify). It
 *      confirms only when its session reached its server within
 *      `confirm_ns` of boot; otherwise it marks itself invalid and reboots,
 *      and the bootloader falls back.
 *   4. A rollback is reported: the image that was tried is kept in a small
 *      note across the reboot, and the first session afterwards says
 *      `rolled_back` naming it.
 *   5. Whatever fails, and wherever power is lost, a bootable VALID slot
 *      stays selected (firmware/tests/test_ota.c asserts it after every
 *      injected fault). */

#ifndef CHORUS_OTA_H
#define CHORUS_OTA_H

#include <stddef.h>
#include <stdint.h>

#define CHORUS_OTA_SHA256_LEN 32u
/* A version or a board name on the wire is a short text; this unit keeps at
 * most this many bytes of one, NUL-terminated (ESP-IDF's own app version
 * field is 32 bytes, components/esp_app_format/include/esp_app_desc.h:30). */
#define CHORUS_OTA_TEXT_MAX 48u
/* The most bytes one firmware_chunk carries (docs/protocol.md). */
#define CHORUS_OTA_MAX_CHUNK_BYTES 4096u
/* One firmware_status after this many chunks. ASSUMED: nothing measured it;
 * with 4096-byte chunks it is one acknowledgement per 64 KiB. */
#define CHORUS_OTA_ACK_EVERY 16u
/* Statuses waiting to be sent. One chunk can raise two (the last
 * acknowledgement and `verified`), and a refusal can arrive between them. */
#define CHORUS_OTA_STATUS_QUEUE 4u
/* The note kept across the reboot is at most this many bytes. */
#define CHORUS_OTA_NOTE_MAX 128u

/* The bootloader's record of an image, as ESP-IDF names them
 * (esp_flash_partitions.h:67-74). UNDEFINED is also what a slot with no
 * record reads as. */
typedef enum {
    CHORUS_OTA_IMAGE_NEW = 0,
    CHORUS_OTA_IMAGE_PENDING_VERIFY = 1,
    CHORUS_OTA_IMAGE_VALID = 2,
    CHORUS_OTA_IMAGE_INVALID = 3,
    CHORUS_OTA_IMAGE_ABORTED = 4,
    CHORUS_OTA_IMAGE_UNDEFINED = 5
} chorus_ota_image_state_t;

const char *chorus_ota_image_state_name(chorus_ota_image_state_t state);

/* The medium. Every function returns 0 when it did what it says and nonzero
 * when it did not; a nonzero return leaves the boot selection as it was. */
typedef struct chorus_ota_flash {
    void *context;
    /* The slot this image runs from, 0 or 1; -1 when it cannot be told. */
    int (*running_slot)(void *context);
    chorus_ota_image_state_t (*slot_state)(void *context, int slot);
    /* How many bytes an image in `slot` may be. */
    uint32_t (*slot_capacity)(void *context, int slot);
    /* Prepare `slot` for an image of `size` bytes (the erase). Refused for
     * the running slot. */
    int (*begin)(void *context, int slot, uint32_t size);
    /* Write the next bytes; `offset` is where they go in the image. */
    int (*write)(void *context, uint32_t offset, const uint8_t *data, size_t length);
    /* The medium's own check of the image just written. */
    int (*finish)(void *context);
    /* Give up the write in progress, if any. */
    void (*abandon)(void *context);
    /* Select `slot` for the next boot, on trial. */
    int (*set_boot)(void *context, int slot);
    /* The running image is good: end its trial. */
    int (*confirm_running)(void *context);
    /* The running image is bad: mark it and reboot into the one before.
     * On the board this does not return when it works. */
    int (*invalidate_running_and_reboot)(void *context);
    /* Reboot. On the board this does not return. */
    void (*reboot)(void *context);
} chorus_ota_flash_t;

/* Where the note lives across a reboot (NVS key `ota_note` on the board, a
 * file beside the fake flash on the host). `load` returns 0 with *length set,
 * 1 when there is no note, and -1 when the store failed. */
typedef struct chorus_ota_notes {
    void *context;
    int (*load)(void *context, uint8_t *out, size_t capacity, size_t *length);
    int (*save)(void *context, const uint8_t *note, size_t length);
    int (*clear)(void *context);
} chorus_ota_notes_t;

/* The unit's states (program research section 3.2, design envelope section
 * 2). Names are chorus_ota_state_name's. */
typedef enum {
    /* A confirmed image runs and nothing is in progress. */
    CHORUS_OTA_RUNNING_VALID = 0,
    /* An offer was accepted and its bytes are being written. */
    CHORUS_OTA_RECEIVING,
    /* Every byte is written; the digest and the medium's check are next.
     * Passed through inside one call, never rested in. */
    CHORUS_OTA_WRITTEN_UNVERIFIED,
    /* The new image is selected and the reboot is due. */
    CHORUS_OTA_PENDING_REBOOT,
    /* The running image is on trial. */
    CHORUS_OTA_PENDING_VERIFY,
    /* The running image passed its trial in this boot. */
    CHORUS_OTA_VALID,
    /* The image that was tried is not the one running. */
    CHORUS_OTA_ROLLED_BACK,
    /* The last offer was refused; `reason` says why. Nothing is in progress. */
    CHORUS_OTA_REFUSED
} chorus_ota_state_t;

const char *chorus_ota_state_name(chorus_ota_state_t state);

/* firmware_status.state on the wire (docs/protocol.md, "0x1A firmware
 * status"). */
typedef enum {
    CHORUS_OTA_WIRE_IDLE = 0,
    CHORUS_OTA_WIRE_RECEIVING = 1,
    CHORUS_OTA_WIRE_VERIFIED = 2,
    CHORUS_OTA_WIRE_PENDING_VERIFY = 3,
    CHORUS_OTA_WIRE_CONFIRMED = 4,
    CHORUS_OTA_WIRE_ROLLED_BACK = 5,
    CHORUS_OTA_WIRE_REFUSED = 6
} chorus_ota_wire_state_t;

/* firmware_status.reason on the wire. */
typedef enum {
    CHORUS_OTA_REASON_NONE = 0,
    CHORUS_OTA_REASON_TOO_LARGE = 1,
    CHORUS_OTA_REASON_BAD_DIGEST = 2,
    CHORUS_OTA_REASON_WRITE_FAILED = 3,
    CHORUS_OTA_REASON_BUSY = 4,
    CHORUS_OTA_REASON_WRONG_BOARD = 5,
    CHORUS_OTA_REASON_NOT_CONFIRMED = 6,
    CHORUS_OTA_REASON_BAD_OFFSET = 7,
    CHORUS_OTA_REASON_MEDIUM_REFUSED = 8
} chorus_ota_reason_t;

const char *chorus_ota_reason_name(chorus_ota_reason_t reason);

typedef struct {
    /* Nonzero. Zero cancels the transfer in progress (chorus_ota_cancel). */
    uint32_t transfer;
    uint32_t size;
    uint8_t sha256[CHORUS_OTA_SHA256_LEN];
    /* 1 to CHORUS_OTA_MAX_CHUNK_BYTES. */
    uint16_t chunk_bytes;
    char version[CHORUS_OTA_TEXT_MAX];
    char board[CHORUS_OTA_TEXT_MAX];
} chorus_ota_offer_t;

/* One firmware_status, ready for the wire. */
typedef struct {
    uint32_t transfer;
    uint8_t state;  /* chorus_ota_wire_state_t */
    uint8_t reason; /* chorus_ota_reason_t */
    uint32_t received;
    /* The RUNNING image's version and board, and the slot it runs from (255
     * unknown). */
    char version[CHORUS_OTA_TEXT_MAX];
    char board[CHORUS_OTA_TEXT_MAX];
    uint8_t slot;
    /* The version of the image the status is about: the offered one while
     * receiving, verified, refused or rolled back; empty when none. */
    char image_version[CHORUS_OTA_TEXT_MAX];
} chorus_ota_status_t;

typedef struct {
    const chorus_ota_flash_t *flash;
    /* Optional. NULL keeps no note: a rollback is then not reported. */
    const chorus_ota_notes_t *notes;
    /* What the running image is. */
    char version[CHORUS_OTA_TEXT_MAX];
    char board[CHORUS_OTA_TEXT_MAX];
    /* How long after boot an image on trial has to reach its server
     * (endpoint.conf `ota_confirm_seconds`, as nanoseconds). */
    uint64_t confirm_ns;
    /* The bad image's switch: 1 never confirms, so the trial always ends in
     * a rollback. For the rollback tests (the host session binary's
     * --ota-never-confirm, the emulator's bad image); 0 in a real image. */
    int never_confirm;
    /* Called at every change of state, on the caller's task. Optional. */
    void (*on_change)(void *context, const chorus_ota_status_t *status, chorus_ota_state_t state);
    void *change_context;
} chorus_ota_config_t;

typedef struct chorus_ota {
    chorus_ota_config_t config;
    chorus_ota_state_t state;
    chorus_ota_reason_t reason;
    int running;
    /* The transfer in progress, or the one the state is about. */
    chorus_ota_offer_t offer;
    int target;
    uint32_t received;
    uint32_t chunks_since_ack;
    int gap_reported;
    /* The running hash of the bytes written so far, in order: a
     * psa_hash_operation_t, kept opaque so this header needs no PSA include.
     * The storage is sized and aligned in ota.c against the real type. */
    union {
        uint64_t align;
        uint8_t bytes[512];
    } hash;
    int hashing;
    /* The trial's deadline, on the caller's monotonic clock. */
    uint64_t confirm_deadline_ns;
    int rollback_impossible;
    int reboot_due;
    int rebooting;
    /* What the note said at boot (the image that was tried). */
    int have_note;
    uint32_t note_transfer;
    int note_slot;
    char note_version[CHORUS_OTA_TEXT_MAX];
    chorus_ota_status_t queue[CHORUS_OTA_STATUS_QUEUE];
    size_t queued;
    /* Counters a test and the console read. */
    uint32_t chunks_written;
    uint32_t chunks_ignored;
    uint32_t offers_refused;
} chorus_ota_t;

/* Start the unit after a boot: read the running slot, its state and the
 * note, and decide what this boot is (running-valid, pending-verify with its
 * deadline at now + confirm_ns, or rolled-back). Returns 0, or -1 when the
 * configuration has no flash. */
int chorus_ota_boot(chorus_ota_t *ota, const chorus_ota_config_t *config, uint64_t now_ns);

/* An offer from the server. Accepted, it prepares the other slot and the unit
 * is `receiving`; refused, a status names the reason. The same offer again
 * (same transfer, size and digest) while receiving resumes it: the status
 * says how many bytes are already written. */
void chorus_ota_offer(chorus_ota_t *ota, const chorus_ota_offer_t *offer, uint64_t now_ns);

/* The cancel (an offer whose transfer is 0): give up the transfer in
 * progress. The boot selection is untouched. */
void chorus_ota_cancel(chorus_ota_t *ota);

/* One chunk. Only the next bytes in order are written; a duplicate is
 * ignored, a gap is ignored and reported once (the status's `received` is
 * where to resume). The last byte runs rule 2 and, when it holds, selects
 * the new slot and asks for the reboot. */
void chorus_ota_chunk(chorus_ota_t *ota, uint32_t transfer, uint32_t offset, const uint8_t *data,
                      size_t length, uint64_t now_ns);

/* The self-test passed: this boot's session reached its server (the first
 * record from the server opened after the greeting). Confirms an image on
 * trial when the deadline has not passed. */
void chorus_ota_session_healthy(chorus_ota_t *ota, uint64_t now_ns);

/* Called regularly with the time. An image on trial past its deadline is
 * marked invalid and the reboot asked for. */
void chorus_ota_tick(chorus_ota_t *ota, uint64_t now_ns);

/* A session began: queue the status every session opens with. */
void chorus_ota_session_started(chorus_ota_t *ota);

/* The opening status of a session was sent. A rollback that has now been
 * reported is forgotten (the note is cleared), so the next boot is quiet. */
void chorus_ota_reported(chorus_ota_t *ota);

/* The next status to send: 1 with *out set, or 0 when none waits. */
int chorus_ota_take_status(chorus_ota_t *ota, chorus_ota_status_t *out);

/* The status as it is now, without queueing anything. */
void chorus_ota_status(const chorus_ota_t *ota, chorus_ota_status_t *out);

/* 1 when the unit wants the reboot (after its statuses are sent). */
int chorus_ota_reboot_due(const chorus_ota_t *ota);

/* Reboot now. On the board this does not return; on a fake it does, and
 * chorus_ota_rebooting is then 1 for the caller to end its run. */
void chorus_ota_reboot(chorus_ota_t *ota);
int chorus_ota_rebooting(const chorus_ota_t *ota);

#endif /* CHORUS_OTA_H */
