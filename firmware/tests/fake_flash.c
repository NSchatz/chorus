#include "fake_flash.h"

#include <fcntl.h>
#include <psa/crypto.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/* The image format's numbers (esp_app_format.h:77-121, esp_app_desc.h:21-42,
 * esp_image_format.c:59 for the checksum's seed). */
#define IMAGE_MAGIC 0xE9u
#define IMAGE_HEADER_LEN 24u
#define SEGMENT_HEADER_LEN 8u
#define IMAGE_MAX_SEGMENTS 16u
#define CHECKSUM_SEED 0xEFu
#define HASH_LEN 32u
#define APP_DESC_LEN 256u
#define APP_DESC_MAGIC 0xABCD5432u
#define APP_DESC_VERSION_AT 16u
#define APP_DESC_VERSION_LEN 32u
#define ERASED 0xFFFFFFFFu
#define SECTOR 4096u

/* The file: a header, then the two slots. */
static const char FILE_MAGIC[8] = {'c', 'h', 'f', 'l', 'a', 's', 'h', '1'};
#define FILE_HEADER_LEN 64u

/* --- small things ------------------------------------------------------------ */

static void put_le32(uint8_t *out, uint32_t v)
{
    out[0] = (uint8_t)v;
    out[1] = (uint8_t)(v >> 8);
    out[2] = (uint8_t)(v >> 16);
    out[3] = (uint8_t)(v >> 24);
}

static uint32_t get_le32(const uint8_t *in)
{
    return (uint32_t)in[0] | ((uint32_t)in[1] << 8) | ((uint32_t)in[2] << 16) |
           ((uint32_t)in[3] << 24);
}

/* CRC-32 (reflected, 0xEDB88320) of the sequence, seeded all-ones: the shape
 * of bootloader_common_ota_select_crc (bootloader_common_loader.c:73-76). */
static uint32_t sequence_crc(uint32_t sequence)
{
    uint8_t bytes[4];
    put_le32(bytes, sequence);
    uint32_t crc = 0xFFFFFFFFu;
    for (size_t i = 0; i < 4; i++) {
        crc ^= bytes[i];
        for (int bit = 0; bit < 8; bit++) {
            crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
        }
    }
    return crc;
}

static void record(fake_flash_t *fake, fake_flash_event_kind_t kind, int index, uint32_t value)
{
    if (fake->event_count >= FAKE_FLASH_MAX_EVENTS) {
        fake->events_dropped++;
        return;
    }
    fake_flash_event_t *e = &fake->events[fake->event_count++];
    e->kind = kind;
    e->index = index;
    e->value = value;
}

/* --- the file ----------------------------------------------------------------- */

static void write_through(const fake_flash_t *fake, off_t at, const uint8_t *bytes, size_t length)
{
    if (fake->fd < 0) {
        return;
    }
    size_t done = 0;
    while (done < length) {
        ssize_t n = pwrite(fake->fd, bytes + done, length - done, at + (off_t)done);
        if (n <= 0) {
            /* A host file that cannot be written is the test rig failing,
             * not the flash under test: say so and stop. */
            perror("fake_flash: flash file");
            exit(70);
        }
        done += (size_t)n;
    }
}

static void sync_otadata(const fake_flash_t *fake)
{
    uint8_t raw[FAKE_FLASH_SLOTS * 12];
    for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
        put_le32(raw + i * 12, fake->otadata[i].sequence);
        put_le32(raw + i * 12 + 4, fake->otadata[i].state);
        put_le32(raw + i * 12 + 8, fake->otadata[i].crc);
    }
    write_through(fake, 12, raw, sizeof(raw));
}

static void sync_slot(const fake_flash_t *fake, int slot, uint32_t offset, size_t length)
{
    write_through(fake,
                  (off_t)FILE_HEADER_LEN + (off_t)slot * (off_t)fake->slot_bytes + (off_t)offset,
                  fake->slots[slot] + offset, length);
}

/* --- the image ---------------------------------------------------------------- */

int fake_flash_image_ok(const uint8_t *bytes, size_t capacity, size_t *length)
{
    if (capacity < IMAGE_HEADER_LEN || bytes[0] != IMAGE_MAGIC || bytes[1] == 0 ||
        bytes[1] > IMAGE_MAX_SEGMENTS) {
        return 0;
    }
    size_t at = IMAGE_HEADER_LEN;
    uint8_t checksum = CHECKSUM_SEED;
    for (uint8_t s = 0; s < bytes[1]; s++) {
        if (capacity - at < SEGMENT_HEADER_LEN) {
            return 0;
        }
        uint32_t data_len = get_le32(bytes + at + 4);
        at += SEGMENT_HEADER_LEN;
        if (data_len > capacity - at) {
            return 0;
        }
        for (uint32_t i = 0; i < data_len; i++) {
            checksum ^= bytes[at + i];
        }
        at += data_len;
    }
    /* The checksum is the last byte of the sixteen it pads to. */
    size_t padded = (at + 1 + 15) & ~(size_t)15;
    if (padded > capacity || bytes[padded - 1] != checksum) {
        return 0;
    }
    at = padded;
    if (bytes[IMAGE_HEADER_LEN - 1] == 1) {
        if (capacity - at < HASH_LEN) {
            return 0;
        }
        uint8_t digest[HASH_LEN];
        size_t got = 0;
        if (psa_crypto_init() != PSA_SUCCESS ||
            psa_hash_compute(PSA_ALG_SHA_256, bytes, at, digest, sizeof(digest), &got) !=
                PSA_SUCCESS ||
            got != HASH_LEN || memcmp(digest, bytes + at, HASH_LEN) != 0) {
            return 0;
        }
        at += HASH_LEN;
    }
    if (length != NULL) {
        *length = at;
    }
    return 1;
}

size_t fake_flash_make_image(uint8_t *out, size_t capacity, const char *version, size_t body_bytes,
                             uint32_t seed)
{
    /* Segments are whole words, as a linker makes them. */
    size_t data_len = (APP_DESC_LEN + body_bytes + 3) & ~(size_t)3;
    size_t unpadded = IMAGE_HEADER_LEN + SEGMENT_HEADER_LEN + data_len;
    size_t padded = (unpadded + 1 + 15) & ~(size_t)15;
    size_t total = padded + HASH_LEN;
    if (total > capacity) {
        return 0;
    }
    memset(out, 0, total);
    out[0] = IMAGE_MAGIC;
    out[1] = 1;    /* one segment */
    out[2] = 0x02; /* DIO */
    out[3] = 0x3F; /* 80 MHz, 8 MB */
    put_le32(out + 4, 0x40370000u);
    out[8] = 0xEE;                 /* WP pin: disabled */
    out[12] = 0x09;                /* chip id: ESP32-S3 */
    out[IMAGE_HEADER_LEN - 1] = 1; /* a SHA-256 is appended */
    uint8_t *segment = out + IMAGE_HEADER_LEN;
    put_le32(segment, 0x3C000020u);
    put_le32(segment + 4, (uint32_t)data_len);
    uint8_t *data = segment + SEGMENT_HEADER_LEN;
    put_le32(data, APP_DESC_MAGIC);
    snprintf((char *)data + APP_DESC_VERSION_AT, APP_DESC_VERSION_LEN, "%s", version);
    snprintf((char *)data + APP_DESC_VERSION_AT + APP_DESC_VERSION_LEN, 32, "chorus-endpoint");
    /* xorshift32 filler: different seeds are different images. */
    uint32_t x = (seed == 0) ? 0x9E3779B9u : seed;
    for (size_t i = APP_DESC_LEN; i < data_len; i++) {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        data[i] = (uint8_t)x;
    }
    uint8_t checksum = CHECKSUM_SEED;
    for (size_t i = 0; i < data_len; i++) {
        checksum ^= data[i];
    }
    out[padded - 1] = checksum;
    size_t got = 0;
    if (psa_crypto_init() != PSA_SUCCESS ||
        psa_hash_compute(PSA_ALG_SHA_256, out, padded, out + padded, HASH_LEN, &got) !=
            PSA_SUCCESS) {
        return 0;
    }
    return total;
}

int fake_flash_slot_bootable(const fake_flash_t *fake, int slot)
{
    if (slot < 0 || slot >= FAKE_FLASH_SLOTS) {
        return 0;
    }
    return fake_flash_image_ok(fake->slots[slot], fake->slot_bytes, NULL);
}

int fake_flash_slot_version(const fake_flash_t *fake, int slot, char *out, size_t out_len)
{
    if (slot < 0 || slot >= FAKE_FLASH_SLOTS || out_len == 0 ||
        fake->slot_bytes < IMAGE_HEADER_LEN + SEGMENT_HEADER_LEN + APP_DESC_LEN) {
        return -1;
    }
    const uint8_t *desc = fake->slots[slot] + IMAGE_HEADER_LEN + SEGMENT_HEADER_LEN;
    if (fake->slots[slot][0] != IMAGE_MAGIC || get_le32(desc) != APP_DESC_MAGIC) {
        return -1;
    }
    char version[APP_DESC_VERSION_LEN + 1];
    memcpy(version, desc + APP_DESC_VERSION_AT, APP_DESC_VERSION_LEN);
    version[APP_DESC_VERSION_LEN] = '\0';
    snprintf(out, out_len, "%s", version);
    return 0;
}

/* --- making one ---------------------------------------------------------------- */

static void blank(fake_flash_t *fake)
{
    for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
        fake->otadata[i].sequence = ERASED;
        fake->otadata[i].state = ERASED;
        fake->otadata[i].crc = ERASED;
    }
    fake->powered = 0;
    fake->running = -1;
    fake->write_slot = -1;
    fake->power_loss_at = -1;
    fake->write_error_at = -1;
    fake->corrupt_write_at = -1;
    fake->fd = -1;
}

int fake_flash_init(fake_flash_t *fake, uint32_t slot_bytes)
{
    memset(fake, 0, sizeof(*fake));
    blank(fake);
    fake->slot_bytes = slot_bytes;
    for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
        fake->slots[i] = malloc(slot_bytes);
        if (fake->slots[i] == NULL) {
            fake_flash_free(fake);
            return -1;
        }
        memset(fake->slots[i], 0xFF, slot_bytes);
    }
    return 0;
}

void fake_flash_free(fake_flash_t *fake)
{
    for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
        free(fake->slots[i]);
        fake->slots[i] = NULL;
    }
    if (fake->fd >= 0) {
        close(fake->fd);
        fake->fd = -1;
    }
}

static int read_whole(int fd, off_t at, uint8_t *out, size_t length)
{
    size_t done = 0;
    while (done < length) {
        ssize_t n = pread(fd, out + done, length - done, at + (off_t)done);
        if (n <= 0) {
            return -1;
        }
        done += (size_t)n;
    }
    return 0;
}

int fake_flash_open(fake_flash_t *fake, const char *path, uint32_t slot_bytes, int *created,
                    char *detail, size_t detail_len)
{
    *created = 0;
    int fd = open(path, O_RDWR);
    if (fd < 0) {
        fd = open(path, O_RDWR | O_CREAT | O_EXCL, 0600);
        if (fd < 0) {
            snprintf(detail, detail_len, "%s could not be opened or made", path);
            return -1;
        }
        *created = 1;
    }
    uint8_t header[FILE_HEADER_LEN];
    if (!*created) {
        if (read_whole(fd, 0, header, sizeof(header)) != 0 ||
            memcmp(header, FILE_MAGIC, sizeof(FILE_MAGIC)) != 0) {
            snprintf(detail, detail_len, "%s is not a chorus fake flash file", path);
            close(fd);
            return -1;
        }
        slot_bytes = get_le32(header + 8);
    }
    if (slot_bytes < FAKE_FLASH_MIN_IMAGE_BYTES || slot_bytes > 16u * 1024u * 1024u ||
        fake_flash_init(fake, slot_bytes) != 0) {
        snprintf(detail, detail_len, "%s: a slot of %u bytes is not usable", path,
                 (unsigned)slot_bytes);
        close(fd);
        return -1;
    }
    fake->fd = fd;
    if (*created) {
        memset(header, 0, sizeof(header));
        memcpy(header, FILE_MAGIC, sizeof(FILE_MAGIC));
        put_le32(header + 8, slot_bytes);
        write_through(fake, 0, header, sizeof(header));
        sync_otadata(fake);
        for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
            sync_slot(fake, i, 0, slot_bytes);
        }
        return 0;
    }
    for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
        fake->otadata[i].sequence = get_le32(header + 12 + i * 12);
        fake->otadata[i].state = get_le32(header + 12 + i * 12 + 4);
        fake->otadata[i].crc = get_le32(header + 12 + i * 12 + 8);
        if (read_whole(fd, (off_t)FILE_HEADER_LEN + (off_t)i * (off_t)slot_bytes, fake->slots[i],
                       slot_bytes) != 0) {
            snprintf(detail, detail_len, "%s is shorter than its two slots", path);
            fake_flash_free(fake);
            return -1;
        }
    }
    return 0;
}

int fake_flash_install(fake_flash_t *fake, int slot, const uint8_t *image, size_t length)
{
    if (slot < 0 || slot >= FAKE_FLASH_SLOTS || length > fake->slot_bytes) {
        return -1;
    }
    memset(fake->slots[slot], 0xFF, fake->slot_bytes);
    memcpy(fake->slots[slot], image, length);
    sync_slot(fake, slot, 0, fake->slot_bytes);
    return 0;
}

/* --- the medium's steps -------------------------------------------------------- */

/* One change to the medium is about to happen. Returns 1 when power goes at
 * this step: the caller applies its half-done form and stops. */
static int step_loses_power(fake_flash_t *fake)
{
    long this_step = fake->steps++;
    if (fake->power_loss_at >= 0 && this_step == fake->power_loss_at) {
        fake->powered = 0;
        fake->power_loss_at = -1;
        record(fake, FAKE_FLASH_POWER_LOST, -1, (uint32_t)this_step);
        return 1;
    }
    return 0;
}

static int entry_invalid(const fake_flash_otadata_t *e)
{
    return e->sequence == ERASED || e->state == CHORUS_OTA_IMAGE_INVALID ||
           e->state == CHORUS_OTA_IMAGE_ABORTED;
}

static int entry_valid(const fake_flash_otadata_t *e)
{
    return !entry_invalid(e) && e->crc == sequence_crc(e->sequence);
}

/* bootloader_common_get_active_otadata: the valid entry, the higher sequence
 * when both are (entry 0 on a tie), -1 when neither. */
static int active_entry(const fake_flash_t *fake)
{
    int v0 = entry_valid(&fake->otadata[0]);
    int v1 = entry_valid(&fake->otadata[1]);
    if (v0 && v1) {
        return (fake->otadata[0].sequence >= fake->otadata[1].sequence) ? 0 : 1;
    }
    if (v0) {
        return 0;
    }
    return v1 ? 1 : -1;
}

static int entry_slot(const fake_flash_otadata_t *e)
{
    return (int)((e->sequence - 1u) % FAKE_FLASH_SLOTS);
}

/* Rewrite one otadata entry: erase its sector, then write it. Returns -1
 * when power went (after the erase, or before anything). */
static int rewrite_entry(fake_flash_t *fake, int index, uint32_t sequence, uint32_t state)
{
    if (step_loses_power(fake)) {
        return -1;
    }
    fake->otadata[index].sequence = ERASED;
    fake->otadata[index].state = ERASED;
    fake->otadata[index].crc = ERASED;
    sync_otadata(fake);
    record(fake, FAKE_FLASH_OTADATA_ERASE, index, 0);
    if (step_loses_power(fake)) {
        return -1;
    }
    fake->otadata[index].sequence = sequence;
    fake->otadata[index].state = state;
    fake->otadata[index].crc = sequence_crc(sequence);
    sync_otadata(fake);
    record(fake, FAKE_FLASH_OTADATA_WRITE, index, state);
    return 0;
}

chorus_ota_image_state_t fake_flash_state(const fake_flash_t *fake, int slot)
{
    for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
        const fake_flash_otadata_t *e = &fake->otadata[i];
        if (entry_slot(e) == slot && e->crc == sequence_crc(e->sequence)) {
            return (e->state <= CHORUS_OTA_IMAGE_ABORTED) ? (chorus_ota_image_state_t)e->state
                                                          : CHORUS_OTA_IMAGE_UNDEFINED;
        }
    }
    return CHORUS_OTA_IMAGE_UNDEFINED;
}

/* --- the bootloader ------------------------------------------------------------ */

fake_boot_t fake_flash_boot(fake_flash_t *fake)
{
    fake->powered = 1;
    fake->running = -1;
    fake->writing = 0;
    fake->write_slot = -1;
    fake->written = 0;
    fake->reboot_requested = 0;
    fake->reboots++;

    /* An image that was on trial and did not confirm before this boot. */
    for (int i = 0; i < FAKE_FLASH_SLOTS; i++) {
        if (fake->otadata[i].state == CHORUS_OTA_IMAGE_PENDING_VERIFY) {
            uint32_t sequence = fake->otadata[i].sequence;
            if (rewrite_entry(fake, i, sequence, CHORUS_OTA_IMAGE_ABORTED) != 0) {
                return FAKE_BOOT_POWER_LOST;
            }
            record(fake, FAKE_FLASH_BOOT_ABORTED, i, sequence);
        }
    }

    int start;
    int first_contents = 0;
    if (entry_invalid(&fake->otadata[0]) && entry_invalid(&fake->otadata[1])) {
        /* No factory app in this layout: "trying OTA 0". */
        start = 0;
        const fake_flash_otadata_t *a = &fake->otadata[0];
        const fake_flash_otadata_t *b = &fake->otadata[1];
        first_contents = (a->sequence == ERASED || a->crc != sequence_crc(a->sequence)) &&
                         (b->sequence == ERASED || b->crc != sequence_crc(b->sequence));
    } else {
        int active = active_entry(fake);
        if (active < 0) {
            /* "ota data partition invalid and no factory, will try all
             * partitions": from the factory index, which this layout lacks. */
            start = -1;
        } else {
            start = entry_slot(&fake->otadata[active]);
            if (fake->otadata[active].state == CHORUS_OTA_IMAGE_NEW) {
                uint32_t sequence = fake->otadata[active].sequence;
                if (rewrite_entry(fake, active, sequence, CHORUS_OTA_IMAGE_PENDING_VERIFY) != 0) {
                    return FAKE_BOOT_POWER_LOST;
                }
                record(fake, FAKE_FLASH_BOOT_PENDING, start, sequence);
            }
        }
    }

    /* Work backwards from the selected slot, then forwards. */
    int loaded = -1;
    for (int index = start; index >= 0 && loaded < 0; index--) {
        if (fake_flash_slot_bootable(fake, index)) {
            loaded = index;
        }
    }
    for (int index = start + 1; index < FAKE_FLASH_SLOTS && loaded < 0; index++) {
        if (fake_flash_slot_bootable(fake, index)) {
            loaded = index;
        }
    }
    if (loaded < 0) {
        fake->powered = 0;
        return FAKE_BOOT_NOTHING_BOOTABLE;
    }
    if (loaded != start) {
        record(fake, FAKE_FLASH_BOOT_FALLBACK, loaded, (uint32_t)start);
    }
    if (first_contents) {
        if (rewrite_entry(fake, 0, (uint32_t)loaded + 1u, CHORUS_OTA_IMAGE_VALID) != 0) {
            return FAKE_BOOT_POWER_LOST;
        }
        record(fake, FAKE_FLASH_BOOT_FIRST, loaded, 0);
    }
    fake->running = loaded;
    record(fake, FAKE_FLASH_BOOTED, loaded, (uint32_t)fake_flash_state(fake, loaded));
    return FAKE_BOOT_OK;
}

/* --- the app side --------------------------------------------------------------- */

static int op_running_slot(void *context)
{
    const fake_flash_t *fake = context;
    return fake->powered ? fake->running : -1;
}

static chorus_ota_image_state_t op_slot_state(void *context, int slot)
{
    return fake_flash_state(context, slot);
}

static uint32_t op_slot_capacity(void *context, int slot)
{
    const fake_flash_t *fake = context;
    return (slot >= 0 && slot < FAKE_FLASH_SLOTS) ? fake->slot_bytes : 0u;
}

static int refuse(fake_flash_t *fake, int index, uint32_t what)
{
    record(fake, FAKE_FLASH_REFUSED, index, what);
    return -1;
}

static int op_begin(void *context, int slot, uint32_t size)
{
    fake_flash_t *fake = context;
    if (!fake->powered) {
        return -1;
    }
    if (slot < 0 || slot >= FAKE_FLASH_SLOTS || slot == fake->running || fake->writing ||
        size > fake->slot_bytes ||
        fake_flash_state(fake, fake->running) == CHORUS_OTA_IMAGE_PENDING_VERIFY) {
        return refuse(fake, slot, FAKE_FLASH_ERASE_SLOT);
    }
    if (fake->begin_refuses > 0) {
        fake->begin_refuses--;
        return refuse(fake, slot, FAKE_FLASH_ERASE_SLOT);
    }
    uint32_t erase = (size + SECTOR - 1u) / SECTOR * SECTOR;
    if (erase > fake->slot_bytes) {
        erase = fake->slot_bytes;
    }
    int lost = step_loses_power(fake);
    /* Power lost mid-erase leaves half the range erased. */
    uint32_t erased = lost ? erase / 2u : erase;
    memset(fake->slots[slot], 0xFF, erased);
    sync_slot(fake, slot, 0, erased);
    record(fake, FAKE_FLASH_ERASE_SLOT, slot, erased);
    if (lost) {
        return -1;
    }
    /* esp_ota_invalidate_inactive_ota_data_slot: the entry that is not the
     * active one, when it names a slot other than the running one. */
    int active = active_entry(fake);
    if (active >= 0) {
        int inactive = 1 - active;
        const fake_flash_otadata_t *e = &fake->otadata[inactive];
        if (e->sequence != ERASED && e->crc == sequence_crc(e->sequence) &&
            entry_slot(e) != fake->running) {
            if (step_loses_power(fake)) {
                return -1;
            }
            fake->otadata[inactive].sequence = ERASED;
            fake->otadata[inactive].state = ERASED;
            fake->otadata[inactive].crc = ERASED;
            sync_otadata(fake);
            record(fake, FAKE_FLASH_OTADATA_ERASE, inactive, 0);
        }
    }
    fake->writing = 1;
    fake->write_slot = slot;
    fake->written = 0;
    return 0;
}

static int op_write(void *context, uint32_t offset, const uint8_t *data, size_t length)
{
    fake_flash_t *fake = context;
    if (!fake->powered) {
        return -1;
    }
    if (!fake->writing || offset != fake->written || length > fake->slot_bytes - offset) {
        return refuse(fake, fake->write_slot, FAKE_FLASH_WRITE);
    }
    long this_write = fake->data_writes++;
    if (fake->write_error_at >= 0 && this_write == fake->write_error_at) {
        return refuse(fake, fake->write_slot, FAKE_FLASH_WRITE);
    }
    int lost = step_loses_power(fake);
    size_t landed = lost ? length / 2 : length;
    memcpy(fake->slots[fake->write_slot] + offset, data, landed);
    if (!lost && fake->corrupt_write_at >= 0 && this_write == fake->corrupt_write_at) {
        /* A bit that did not take: what is in the flash is not what was
         * sent, and only the medium's own check can see it. */
        fake->slots[fake->write_slot][offset + length / 2] ^= 0x10;
    }
    sync_slot(fake, fake->write_slot, offset, landed);
    record(fake, FAKE_FLASH_WRITE, fake->write_slot, offset);
    if (lost) {
        return -1;
    }
    fake->written += (uint32_t)length;
    return 0;
}

static int op_finish(void *context)
{
    fake_flash_t *fake = context;
    if (!fake->powered) {
        return -1;
    }
    if (!fake->writing) {
        return refuse(fake, -1, FAKE_FLASH_FINISH);
    }
    int slot = fake->write_slot;
    fake->writing = 0;
    fake->write_slot = -1;
    if (fake->finish_refuses > 0) {
        fake->finish_refuses--;
        return refuse(fake, slot, FAKE_FLASH_FINISH);
    }
    if (fake->written == 0 || !fake_flash_slot_bootable(fake, slot)) {
        return refuse(fake, slot, FAKE_FLASH_FINISH);
    }
    record(fake, FAKE_FLASH_FINISH, slot, fake->written);
    return 0;
}

static void op_abandon(void *context)
{
    fake_flash_t *fake = context;
    if (fake->powered && fake->writing) {
        record(fake, FAKE_FLASH_ABANDON, fake->write_slot, fake->written);
        fake->writing = 0;
        fake->write_slot = -1;
    }
}

static int op_set_boot(void *context, int slot)
{
    fake_flash_t *fake = context;
    if (!fake->powered) {
        return -1;
    }
    if (fake->set_boot_refuses > 0) {
        fake->set_boot_refuses--;
        return refuse(fake, slot, FAKE_FLASH_SET_BOOT);
    }
    if (slot < 0 || slot >= FAKE_FLASH_SLOTS || !fake_flash_slot_bootable(fake, slot)) {
        return refuse(fake, slot, FAKE_FLASH_SET_BOOT);
    }
    int active = active_entry(fake);
    int next;
    uint32_t sequence;
    if (active >= 0) {
        uint32_t current = fake->otadata[active].sequence;
        if (entry_slot(&fake->otadata[active]) == slot) {
            next = active;
            sequence = current;
        } else {
            next = 1 - active;
            /* compute_ota_seq_for_target_slot (esp_ota_ops.c:696-744). */
            uint32_t base = ((uint32_t)slot + 1u) % FAKE_FLASH_SLOTS;
            uint32_t i = 0;
            while (current > base + i * FAKE_FLASH_SLOTS) {
                i++;
            }
            sequence = base + i * FAKE_FLASH_SLOTS;
        }
    } else {
        next = 0;
        sequence = (uint32_t)slot + 1u;
    }
    if (rewrite_entry(fake, next, sequence, CHORUS_OTA_IMAGE_NEW) != 0) {
        return -1;
    }
    record(fake, FAKE_FLASH_SET_BOOT, slot, sequence);
    return 0;
}

static int op_confirm_running(void *context)
{
    fake_flash_t *fake = context;
    if (!fake->powered) {
        return -1;
    }
    if (fake->confirm_refuses > 0) {
        fake->confirm_refuses--;
        return refuse(fake, fake->running, FAKE_FLASH_CONFIRM);
    }
    int active = active_entry(fake);
    if (active < 0) {
        return refuse(fake, fake->running, FAKE_FLASH_CONFIRM);
    }
    if (fake->otadata[active].state != CHORUS_OTA_IMAGE_VALID) {
        if (rewrite_entry(fake, active, fake->otadata[active].sequence, CHORUS_OTA_IMAGE_VALID) !=
            0) {
            return -1;
        }
    }
    record(fake, FAKE_FLASH_CONFIRM, entry_slot(&fake->otadata[active]), 0);
    return 0;
}

static int op_invalidate_running_and_reboot(void *context)
{
    fake_flash_t *fake = context;
    if (!fake->powered) {
        return -1;
    }
    int active = active_entry(fake);
    if (active < 0) {
        return refuse(fake, fake->running, FAKE_FLASH_INVALIDATE);
    }
    /* esp_ota_check_rollback_is_possible: the other entry is valid and its
     * slot's image verifies. */
    const fake_flash_otadata_t *other = &fake->otadata[1 - active];
    if (!entry_valid(other) || !fake_flash_slot_bootable(fake, entry_slot(other))) {
        return refuse(fake, fake->running, FAKE_FLASH_INVALIDATE);
    }
    int slot = entry_slot(&fake->otadata[active]);
    if (rewrite_entry(fake, active, fake->otadata[active].sequence, CHORUS_OTA_IMAGE_INVALID) !=
        0) {
        return -1;
    }
    record(fake, FAKE_FLASH_INVALIDATE, slot, 0);
    fake->reboot_requested = 1;
    record(fake, FAKE_FLASH_REBOOT, -1, 0);
    return 0;
}

static void op_reboot(void *context)
{
    fake_flash_t *fake = context;
    if (fake->powered) {
        fake->reboot_requested = 1;
        record(fake, FAKE_FLASH_REBOOT, -1, 0);
    }
}

chorus_ota_flash_t fake_flash_ops(fake_flash_t *fake)
{
    chorus_ota_flash_t ops;
    memset(&ops, 0, sizeof(ops));
    ops.context = fake;
    ops.running_slot = op_running_slot;
    ops.slot_state = op_slot_state;
    ops.slot_capacity = op_slot_capacity;
    ops.begin = op_begin;
    ops.write = op_write;
    ops.finish = op_finish;
    ops.abandon = op_abandon;
    ops.set_boot = op_set_boot;
    ops.confirm_running = op_confirm_running;
    ops.invalidate_running_and_reboot = op_invalidate_running_and_reboot;
    ops.reboot = op_reboot;
    return ops;
}

/* --- the log ------------------------------------------------------------------- */

size_t fake_flash_count(const fake_flash_t *fake, fake_flash_event_kind_t kind)
{
    size_t n = 0;
    for (size_t i = 0; i < fake->event_count; i++) {
        n += fake->events[i].kind == kind;
    }
    return n;
}

int fake_flash_first(const fake_flash_t *fake, fake_flash_event_kind_t kind)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == kind) {
            return (int)i;
        }
    }
    return -1;
}

int fake_flash_last(const fake_flash_t *fake, fake_flash_event_kind_t kind)
{
    for (size_t i = fake->event_count; i > 0; i--) {
        if (fake->events[i - 1].kind == kind) {
            return (int)(i - 1);
        }
    }
    return -1;
}

const char *fake_flash_event_kind_name(fake_flash_event_kind_t kind)
{
    switch (kind) {
    case FAKE_FLASH_ERASE_SLOT:
        return "erase-slot";
    case FAKE_FLASH_WRITE:
        return "write";
    case FAKE_FLASH_FINISH:
        return "finish";
    case FAKE_FLASH_ABANDON:
        return "abandon";
    case FAKE_FLASH_OTADATA_ERASE:
        return "otadata-erase";
    case FAKE_FLASH_OTADATA_WRITE:
        return "otadata-write";
    case FAKE_FLASH_SET_BOOT:
        return "set-boot";
    case FAKE_FLASH_CONFIRM:
        return "confirm";
    case FAKE_FLASH_INVALIDATE:
        return "invalidate";
    case FAKE_FLASH_REBOOT:
        return "reboot";
    case FAKE_FLASH_REFUSED:
        return "refused";
    case FAKE_FLASH_POWER_LOST:
        return "power-lost";
    case FAKE_FLASH_BOOT_ABORTED:
        return "boot-marked-aborted";
    case FAKE_FLASH_BOOT_PENDING:
        return "boot-marked-pending-verify";
    case FAKE_FLASH_BOOT_FIRST:
        return "boot-first-contents";
    case FAKE_FLASH_BOOT_FALLBACK:
        return "boot-fell-back";
    case FAKE_FLASH_BOOTED:
        return "booted";
    }
    return "unknown";
}

void fake_flash_print(const fake_flash_t *fake)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        const fake_flash_event_t *e = &fake->events[i];
        printf("    [%zu] %s index=%d value=%u\n", i, fake_flash_event_kind_name(e->kind), e->index,
               (unsigned)e->value);
    }
}

/* --- the note ------------------------------------------------------------------ */

void fake_notes_init(fake_notes_t *fake, const char *path)
{
    memset(fake, 0, sizeof(*fake));
    if (path != NULL) {
        snprintf(fake->path, sizeof(fake->path), "%s", path);
    }
}

static int notes_load(void *context, uint8_t *out, size_t capacity, size_t *length)
{
    fake_notes_t *fake = context;
    if (fake->powered != NULL && !*fake->powered) {
        return -1;
    }
    if (fake->path[0] != '\0') {
        FILE *f = fopen(fake->path, "rb");
        if (f == NULL) {
            return 1;
        }
        size_t n = fread(out, 1, capacity, f);
        fclose(f);
        if (n == 0) {
            return 1;
        }
        *length = n;
        return 0;
    }
    if (!fake->present) {
        return 1;
    }
    if (fake->length > capacity) {
        return -1;
    }
    memcpy(out, fake->note, fake->length);
    *length = fake->length;
    return 0;
}

static int notes_save(void *context, const uint8_t *note, size_t length)
{
    fake_notes_t *fake = context;
    if (fake->powered != NULL && !*fake->powered) {
        return -1;
    }
    if (fake->save_refuses > 0) {
        fake->save_refuses--;
        return -1;
    }
    if (length > sizeof(fake->note)) {
        return -1;
    }
    fake->saves++;
    if (fake->path[0] != '\0') {
        FILE *f = fopen(fake->path, "wb");
        if (f == NULL) {
            return -1;
        }
        size_t n = fwrite(note, 1, length, f);
        return (fclose(f) == 0 && n == length) ? 0 : -1;
    }
    memcpy(fake->note, note, length);
    fake->length = length;
    fake->present = 1;
    return 0;
}

static int notes_clear(void *context)
{
    fake_notes_t *fake = context;
    if (fake->powered != NULL && !*fake->powered) {
        return -1;
    }
    fake->clears++;
    if (fake->path[0] != '\0') {
        (void)remove(fake->path);
        return 0;
    }
    fake->present = 0;
    fake->length = 0;
    return 0;
}

chorus_ota_notes_t fake_notes_ops(fake_notes_t *fake)
{
    chorus_ota_notes_t ops;
    ops.context = fake;
    ops.load = notes_load;
    ops.save = notes_save;
    ops.clear = notes_clear;
    return ops;
}
