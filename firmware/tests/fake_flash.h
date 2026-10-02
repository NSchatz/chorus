/* A simulated two-slot flash with ESP-IDF's bootloader, writing into one
 * event log.
 *
 * The same design as fake_amp and fake_radio (docs/decisions/0015-*): the log
 * is appended by the FAKE, and the update unit under test (chorus/ota.h) has
 * no way to reach it, so "the boot selection changed only after the image was
 * whole, digested and checked" is graded on what happened to the flash and
 * not on the unit's account of itself.
 *
 * WHAT IT MODELS, from ESP-IDF v6.1's source at the pinned commit
 * fff9895c82d744c7237be8847347bdd1b07c6643 (Apache-2.0, read 2026-10-02):
 *
 *   - otadata: two entries of {sequence, state, crc}, each in its own flash
 *     sector, rewritten by erase-then-write (components/app_update/
 *     esp_ota_ops.c:665-680, components/bootloader_support/src/
 *     bootloader_utility.c:307-318), so power lost between the two leaves
 *     the entry erased;
 *   - the bootloader's selection (bootloader_utility.c:379-474): a
 *     PENDING_VERIFY entry met at boot becomes ABORTED (:393-401); with no
 *     usable entry and no factory app, slot 0 (:404-420); else the valid
 *     entry with the higher sequence (bootloader_common_loader.c:78-97 and
 *     :151-175), whose slot is (sequence - 1) mod 2, and a NEW entry becomes
 *     PENDING_VERIFY (:443-449);
 *   - the load (bootloader_utility.c:576-627): the selected slot's image is
 *     verified, and when it does not verify the other slot is tried; the
 *     first boot of a blank otadata writes entry 0 as VALID (:488-499);
 *   - the app side (esp_ota_ops.c): esp_ota_begin refuses the running slot
 *     and a running image on trial, erases the slot and drops the inactive
 *     entry (:155-230, :1316-1362); esp_ota_end verifies the written image
 *     (:617-663); esp_ota_set_boot_partition verifies it again and writes the
 *     entry as NEW in the inactive sector with the next sequence (:778-860);
 *     esp_ota_mark_app_valid_cancel_rollback and
 *     esp_ota_mark_app_invalid_rollback rewrite the active entry
 *     (:1179-1236), the latter only when another image can be booted
 *     (:1077-1128); esp_ota_get_state_partition (:1282-1312);
 *   - the image: ESP-IDF's application image format (docs/en/api-reference/
 *     system/app_image_format.rst; components/bootloader_support/include/
 *     esp_app_format.h:77-121): the 0xE9 header, segments, a checksum byte on
 *     a sixteen byte boundary and an appended SHA-256, with the application
 *     description (components/esp_app_format/include/esp_app_desc.h:21-42)
 *     at the start of the first segment. fake_flash_make_image builds one and
 *     fake_flash_image_ok verifies one, so a real ESP-IDF image verifies too.
 *
 * WHAT IT DOES NOT MODEL: flash that only clears bits, wear, the factory and
 * test apps (the layout has neither), encryption, signatures, anti-rollback
 * (off, and it would need an eFuse). The otadata CRC is a CRC-32 of the
 * sequence as in the source, not held to the ROM's exact bytes.
 *
 * FAULTS. Every change to the medium (a slot erase, a data write, an otadata
 * erase, an otadata write, the bootloader's own writes) is one numbered step.
 * `power_loss_at` names the step at which power goes: that step lands half
 * done (a data write keeps the first half of its bytes; an otadata rewrite
 * keeps the erase and loses the write), every later call fails, and
 * fake_flash_boot powers the board up again. */

#ifndef CHORUS_FAKE_FLASH_H
#define CHORUS_FAKE_FLASH_H

#include "chorus/ota.h"

#include <stddef.h>
#include <stdint.h>

#define FAKE_FLASH_SLOTS 2
#define FAKE_FLASH_MAX_EVENTS 512
#define FAKE_FLASH_PATH_MAX 512
/* The host session binary's slot size when it makes a flash file. */
#define FAKE_FLASH_FILE_SLOT_BYTES (1024u * 1024u)
/* The smallest image fake_flash_make_image makes: header, one segment with
 * the application description, checksum padding and the digest. */
#define FAKE_FLASH_MIN_IMAGE_BYTES 336u

typedef enum {
    FAKE_FLASH_ERASE_SLOT,
    FAKE_FLASH_WRITE,
    FAKE_FLASH_FINISH,
    FAKE_FLASH_ABANDON,
    FAKE_FLASH_OTADATA_ERASE,
    FAKE_FLASH_OTADATA_WRITE,
    FAKE_FLASH_SET_BOOT,
    FAKE_FLASH_CONFIRM,
    FAKE_FLASH_INVALIDATE,
    FAKE_FLASH_REBOOT,
    FAKE_FLASH_REFUSED,
    FAKE_FLASH_POWER_LOST,
    FAKE_FLASH_BOOT_ABORTED,
    FAKE_FLASH_BOOT_PENDING,
    FAKE_FLASH_BOOT_FIRST,
    FAKE_FLASH_BOOT_FALLBACK,
    FAKE_FLASH_BOOTED
} fake_flash_event_kind_t;

typedef struct {
    fake_flash_event_kind_t kind;
    /* The slot or the otadata entry the event is about; -1 when neither. */
    int index;
    uint32_t value;
} fake_flash_event_t;

typedef struct {
    uint32_t sequence;
    uint32_t state;
    uint32_t crc;
} fake_flash_otadata_t;

typedef enum {
    FAKE_BOOT_OK = 0,
    /* Power went during the bootloader's own writes; boot again. */
    FAKE_BOOT_POWER_LOST,
    /* No slot holds an image that verifies: the board is a brick. The
     * property test_ota.c asserts is that this never happens. */
    FAKE_BOOT_NOTHING_BOOTABLE
} fake_boot_t;

typedef struct fake_flash {
    uint8_t *slots[FAKE_FLASH_SLOTS];
    uint32_t slot_bytes;
    fake_flash_otadata_t otadata[FAKE_FLASH_SLOTS];

    /* The board: whether it has power, which slot its CPU runs (-1 none). */
    int powered;
    int running;
    int reboot_requested;
    uint32_t reboots;

    /* The write in progress (RAM: gone at a reboot). */
    int writing;
    int write_slot;
    uint32_t written;

    /* Faults. -1 is "never". */
    long steps;
    long power_loss_at;
    long data_writes;
    long write_error_at;
    long corrupt_write_at;
    int begin_refuses;
    int finish_refuses;
    int set_boot_refuses;
    int confirm_refuses;

    /* File-backed: every change is written through. -1 keeps it in memory. */
    int fd;

    fake_flash_event_t events[FAKE_FLASH_MAX_EVENTS];
    size_t event_count;
    /* Events past the log's end are counted, not kept. */
    size_t events_dropped;
} fake_flash_t;

/* A blank flash in memory: both slots erased, otadata erased, powered off.
 * Returns 0, or -1 when the memory is not there. */
int fake_flash_init(fake_flash_t *fake, uint32_t slot_bytes);
void fake_flash_free(fake_flash_t *fake);

/* The same over a file that outlives the process. An existing file is read
 * (its own slot size wins); a missing one is made blank with `slot_bytes`.
 * `*created` says which. Returns 0, or -1 with `detail` saying why. */
int fake_flash_open(fake_flash_t *fake, const char *path, uint32_t slot_bytes, int *created,
                    char *detail, size_t detail_len);

/* The first flash over USB: write `image` to `slot` directly. The otadata is
 * left as it is (blank on a new board, so the bootloader boots slot 0). */
int fake_flash_install(fake_flash_t *fake, int slot, const uint8_t *image, size_t length);

/* Power up and run the bootloader. On FAKE_BOOT_OK `running` is the slot. */
fake_boot_t fake_flash_boot(fake_flash_t *fake);

/* The medium, as the update unit sees it. */
chorus_ota_flash_t fake_flash_ops(fake_flash_t *fake);

/* What the bootloader's record says about `slot` (esp_ota_get_state_partition). */
chorus_ota_image_state_t fake_flash_state(const fake_flash_t *fake, int slot);
/* 1 when `slot` holds an image that verifies. */
int fake_flash_slot_bootable(const fake_flash_t *fake, int slot);
/* The version in `slot`'s application description: 0 with `out` set, or -1. */
int fake_flash_slot_version(const fake_flash_t *fake, int slot, char *out, size_t out_len);

/* Build an application image: `version` in its description, `body_bytes` of
 * filler from `seed` after it. Returns its length, or 0 when `capacity` is
 * too small. */
size_t fake_flash_make_image(uint8_t *out, size_t capacity, const char *version, size_t body_bytes,
                             uint32_t seed);
/* Verify the image at `bytes`: 1 with `*length` set when the header, the
 * segments, the checksum and the appended SHA-256 all hold, else 0. */
int fake_flash_image_ok(const uint8_t *bytes, size_t capacity, size_t *length);

size_t fake_flash_count(const fake_flash_t *fake, fake_flash_event_kind_t kind);
/* Index of the first and last event of `kind`, or -1. */
int fake_flash_first(const fake_flash_t *fake, fake_flash_event_kind_t kind);
int fake_flash_last(const fake_flash_t *fake, fake_flash_event_kind_t kind);
const char *fake_flash_event_kind_name(fake_flash_event_kind_t kind);
void fake_flash_print(const fake_flash_t *fake);

/* The note's store: memory, or a file when `path` is not empty. `powered`,
 * when set, points at the board's power (fake_flash_t.powered): a store on a
 * board with no power does nothing, as the unit calling it would not be
 * running either. */
typedef struct {
    const int *powered;
    uint8_t note[CHORUS_OTA_NOTE_MAX];
    size_t length;
    int present;
    int save_refuses;
    uint32_t saves;
    uint32_t clears;
    char path[FAKE_FLASH_PATH_MAX];
} fake_notes_t;

void fake_notes_init(fake_notes_t *fake, const char *path);
chorus_ota_notes_t fake_notes_ops(fake_notes_t *fake);

#endif /* CHORUS_FAKE_FLASH_H */
