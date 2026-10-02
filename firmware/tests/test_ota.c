/* The firmware update's state machine over a fault-injecting fake flash.
 *
 * This is the foundation line of goal 14: "The OTA state machine survives
 * every injected fault in make gate and rolls back a bad image". The unit
 * under test is firmware/src/ota.c, the code the board runs; the flash, the
 * otadata and the bootloader are firmware/tests/fake_flash.c, which models
 * ESP-IDF v6.1's rule from its source (the citations are in fake_flash.h).
 *
 * THE INVARIANT, asserted after EVERY injected fault: the board boots, the
 * slot it boots holds an image that verifies, and once the trial (if any) has
 * run its course the selected slot's record is VALID; a cold boot from there
 * lands on the same slot. And a bad image (one that never confirms, or that
 * dies before confirming) is rolled back to the version that ran before.
 *
 * Nothing here is timing evidence: the time is a number the test passes in. */

#include "chorus/ota.h"
#include "fake_flash.h"
#include "harness.h"

#include <psa/crypto.h>

#define SLOT_BYTES (8u * 1024u)
#define CONFIRM_NS (60ull * 1000000000ull)
#define BOARD "brick-s3-wired"

/* What an image does once it runs. The fake cannot execute an image, so the
 * rig reads the behaviour off the version it carries. */
static int never_confirms(const char *version)
{
    return strncmp(version, "bad-never", 9) == 0;
}

static int panics_before_confirm(const char *version)
{
    return strncmp(version, "bad-panic", 9) == 0;
}

typedef struct {
    fake_flash_t flash;
    fake_notes_t notes;
    chorus_ota_flash_t flash_ops;
    chorus_ota_notes_t notes_ops;
    chorus_ota_t ota;
    uint64_t now_ns;
    int bricked;
    uint32_t changes;
} rig_t;

static long faults_injected;
static long faults_survived;
static long bad_images_shipped;
static long bad_images_rolled_back;

static void on_change(void *context, const chorus_ota_status_t *status, chorus_ota_state_t state)
{
    (void)status;
    (void)state;
    rig_t *rig = context;
    rig->changes++;
}

/* Power up (again and again while power is lost inside the bootloader) and
 * start the unit, as app_main does after a reset. */
static void rig_power_on(rig_t *rig)
{
    fake_boot_t booted;
    int tries = 0;
    do {
        booted = fake_flash_boot(&rig->flash);
    } while (booted == FAKE_BOOT_POWER_LOST && ++tries < 8);
    if (booted != FAKE_BOOT_OK) {
        rig->bricked = 1;
        return;
    }
    chorus_ota_config_t config;
    memset(&config, 0, sizeof(config));
    config.flash = &rig->flash_ops;
    config.notes = &rig->notes_ops;
    if (fake_flash_slot_version(&rig->flash, rig->flash.running, config.version,
                                sizeof(config.version)) != 0) {
        snprintf(config.version, sizeof(config.version), "unknown");
    }
    snprintf(config.board, sizeof(config.board), "%s", BOARD);
    config.confirm_ns = CONFIRM_NS;
    config.never_confirm = never_confirms(config.version);
    config.on_change = on_change;
    config.change_context = rig;
    rig->now_ns += 1000000000ull;
    (void)chorus_ota_boot(&rig->ota, &config, rig->now_ns);
}

static size_t make(uint8_t *out, const char *version, size_t body, uint32_t seed)
{
    return fake_flash_make_image(out, SLOT_BYTES, version, body, seed);
}

/* A board flashed once over USB with `version` in slot 0, booted. */
static void rig_new(rig_t *rig, const char *version)
{
    static uint8_t image[SLOT_BYTES];
    memset(rig, 0, sizeof(*rig));
    if (fake_flash_init(&rig->flash, SLOT_BYTES) != 0) {
        rig->bricked = 1;
        return;
    }
    fake_notes_init(&rig->notes, NULL);
    rig->notes.powered = &rig->flash.powered;
    rig->flash_ops = fake_flash_ops(&rig->flash);
    rig->notes_ops = fake_notes_ops(&rig->notes);
    size_t length = make(image, version, 200, 1);
    (void)fake_flash_install(&rig->flash, 0, image, length);
    rig_power_on(rig);
}

static void rig_free(rig_t *rig)
{
    fake_flash_free(&rig->flash);
}

static void drain(rig_t *rig)
{
    chorus_ota_status_t status;
    while (chorus_ota_take_status(&rig->ota, &status)) {
    }
}

static chorus_ota_offer_t offer_for(const uint8_t *image, size_t length, const char *version,
                                    uint32_t transfer, uint16_t chunk_bytes)
{
    chorus_ota_offer_t offer;
    memset(&offer, 0, sizeof(offer));
    offer.transfer = transfer;
    offer.size = (uint32_t)length;
    offer.chunk_bytes = chunk_bytes;
    size_t got = 0;
    (void)psa_crypto_init();
    (void)psa_hash_compute(PSA_ALG_SHA_256, image, length, offer.sha256, sizeof(offer.sha256),
                           &got);
    snprintf(offer.version, sizeof(offer.version), "%s", version);
    snprintf(offer.board, sizeof(offer.board), "%s", BOARD);
    return offer;
}

/* Send chunks from `from` until the image ends, the unit stops receiving, or
 * power goes. Returns the offset reached. */
static uint32_t send_from(rig_t *rig, const uint8_t *image, size_t length, uint32_t transfer,
                          uint16_t chunk_bytes, uint32_t from, uint32_t stop_at)
{
    uint32_t at = from;
    while (at < length && at < stop_at && rig->flash.powered &&
           rig->ota.state == CHORUS_OTA_RECEIVING) {
        size_t n = length - at;
        if (n > chunk_bytes) {
            n = chunk_bytes;
        }
        chorus_ota_chunk(&rig->ota, transfer, at, image + at, n, rig->now_ns);
        at += (uint32_t)n;
    }
    return at;
}

/* The whole transfer, as a server that meets no trouble sends it. */
static void install(rig_t *rig, const uint8_t *image, size_t length, const char *version,
                    uint32_t transfer, uint16_t chunk_bytes)
{
    chorus_ota_offer_t offer = offer_for(image, length, version, transfer, chunk_bytes);
    chorus_ota_offer(&rig->ota, &offer, rig->now_ns);
    (void)send_from(rig, image, length, transfer, chunk_bytes, 0, UINT32_MAX);
}

/* Let whatever is in motion run its course: lost power comes back, a due
 * reboot happens, an image on trial confirms (a good one, once its session is
 * healthy), never confirms (the deadline passes) or dies (a reset). */
static void settle(rig_t *rig)
{
    for (int round = 0; round < 16 && !rig->bricked; round++) {
        if (!rig->flash.powered) {
            rig_power_on(rig);
            continue;
        }
        if (chorus_ota_reboot_due(&rig->ota)) {
            drain(rig);
            chorus_ota_reboot(&rig->ota);
        }
        if (rig->flash.reboot_requested) {
            rig_power_on(rig);
            continue;
        }
        if (rig->ota.state == CHORUS_OTA_PENDING_VERIFY && !rig->ota.rollback_impossible) {
            if (panics_before_confirm(rig->ota.config.version)) {
                /* It dies before its session is up: a reset, nothing more. */
                rig_power_on(rig);
                continue;
            }
            rig->now_ns += 5000000000ull;
            chorus_ota_session_healthy(&rig->ota, rig->now_ns);
            if (rig->ota.state == CHORUS_OTA_PENDING_VERIFY) {
                rig->now_ns += CONFIRM_NS;
                chorus_ota_tick(&rig->ota, rig->now_ns);
            }
            continue;
        }
        break;
    }
}

/* The invariant. Counts one injected fault and, when it held, one survived. */
static int survived(rig_t *rig)
{
    faults_injected++;
    settle(rig);
    int ok = !rig->bricked && rig->flash.powered && rig->flash.running >= 0 &&
             fake_flash_slot_bootable(&rig->flash, rig->flash.running) &&
             fake_flash_state(&rig->flash, rig->flash.running) == CHORUS_OTA_IMAGE_VALID;
    if (ok) {
        /* A cold boot from here: the same slot, still VALID. */
        int slot = rig->flash.running;
        rig_power_on(rig);
        settle(rig);
        ok = !rig->bricked && rig->flash.running == slot &&
             fake_flash_state(&rig->flash, slot) == CHORUS_OTA_IMAGE_VALID;
    }
    /* Rule 2 on the log: every selection follows the medium's own check of
     * that slot, with no write in between. */
    for (size_t i = 0; ok && i < rig->flash.event_count; i++) {
        if (rig->flash.events[i].kind != FAKE_FLASH_SET_BOOT) {
            continue;
        }
        int checked = 0;
        for (size_t j = i; j > 0; j--) {
            const fake_flash_event_t *e = &rig->flash.events[j - 1];
            if (e->kind == FAKE_FLASH_WRITE || e->kind == FAKE_FLASH_ERASE_SLOT) {
                break;
            }
            if (e->kind == FAKE_FLASH_FINISH && e->index == rig->flash.events[i].index) {
                checked = 1;
                break;
            }
        }
        ok = checked;
    }
    faults_survived += ok;
    return ok;
}

static int running_version_is(const rig_t *rig, const char *version)
{
    char found[64];
    return !rig->bricked &&
           fake_flash_slot_version(&rig->flash, rig->flash.running, found, sizeof(found)) == 0 &&
           strcmp(found, version) == 0;
}

/* A clean update to `version`, start to confirmed. 1 when it ends VALID on it. */
static int clean_update(rig_t *rig, const char *version, uint32_t transfer, uint32_t seed)
{
    static uint8_t image[SLOT_BYTES];
    size_t length = make(image, version, 900 + (seed % 700), seed);
    drain(rig);
    if (rig->ota.state == CHORUS_OTA_RECEIVING || rig->ota.state == CHORUS_OTA_REFUSED) {
        chorus_ota_cancel(&rig->ota);
    }
    install(rig, image, length, version, transfer, 128);
    settle(rig);
    return running_version_is(rig, version) &&
           fake_flash_state(&rig->flash, rig->flash.running) == CHORUS_OTA_IMAGE_VALID;
}

/* --- the plain path ----------------------------------------------------------- */

static void a_good_image_is_written_verified_tried_and_confirmed(void)
{
    chorus_section("a good image is written, verified, tried once and confirmed");
    static rig_t rig;
    static uint8_t image[SLOT_BYTES];
    rig_new(&rig, "1.0.0");
    chorus_check(rig.flash.running == 0 &&
                     fake_flash_state(&rig.flash, 0) == CHORUS_OTA_IMAGE_VALID,
                 "a board flashed once boots slot 0 and its record is written VALID (state %s)",
                 chorus_ota_image_state_name(fake_flash_state(&rig.flash, 0)));
    chorus_check(fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_UNDEFINED,
                 "the empty slot has no record: it reads undefined");
    chorus_check(rig.ota.state == CHORUS_OTA_RUNNING_VALID, "the unit starts running-valid");

    chorus_ota_status_t status;
    chorus_check(chorus_ota_take_status(&rig.ota, &status) == 0,
                 "nothing is queued before a session starts");
    chorus_ota_session_started(&rig.ota);
    int got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_IDLE && status.transfer == 0 &&
                     status.slot == 0 && strcmp(status.version, "1.0.0") == 0 &&
                     strcmp(status.board, BOARD) == 0,
                 "a session opens with one status: idle, slot 0, version 1.0.0, board %s",
                 status.board);

    size_t length = make(image, "2.0.0", 3000, 7);
    chorus_ota_offer_t offer = offer_for(image, length, "2.0.0", 41, 64);
    size_t writes_before = fake_flash_count(&rig.flash, FAKE_FLASH_WRITE);
    chorus_ota_chunk(&rig.ota, 41, 0, image, 64, rig.now_ns);
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_WRITE) == writes_before &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT) == 0 &&
                     rig.ota.chunks_ignored == 1,
                 "rule 1: a chunk with no offer before it writes and erases nothing");
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && rig.ota.state == CHORUS_OTA_RECEIVING &&
                     status.state == CHORUS_OTA_WIRE_RECEIVING && status.transfer == 41 &&
                     status.received == 0 && strcmp(status.image_version, "2.0.0") == 0,
                 "an offer that fits is accepted: receiving, transfer 41, received 0, image "
                 "2.0.0");
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT) == 1 &&
                     rig.flash.events[fake_flash_first(&rig.flash, FAKE_FLASH_ERASE_SLOT)].index ==
                         1,
                 "the slot that is NOT running (1) is the one erased");

    uint32_t at = send_from(&rig, image, length, 41, 64, 0, 64u * 20u);
    size_t acks = 0;
    uint32_t acked = 0;
    while (chorus_ota_take_status(&rig.ota, &status)) {
        acks++;
        acked = status.received;
    }
    chorus_check(at == 1280 && acks == 1 && acked == 64u * CHORUS_OTA_ACK_EVERY,
                 "20 chunks in: one acknowledgement, at chunk %u (received %u)",
                 (unsigned)CHORUS_OTA_ACK_EVERY, (unsigned)acked);
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0 &&
                     fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_UNDEFINED,
                 "rule 2: mid-download the boot selection has not moved");

    (void)send_from(&rig, image, length, 41, 64, at, UINT32_MAX);
    chorus_check(rig.ota.state == CHORUS_OTA_PENDING_REBOOT && chorus_ota_reboot_due(&rig.ota),
                 "the last byte: digest, the medium's check, selection; state %s, reboot due",
                 chorus_ota_state_name(rig.ota.state));
    int finish_at = fake_flash_last(&rig.flash, FAKE_FLASH_FINISH);
    int set_at = fake_flash_first(&rig.flash, FAKE_FLASH_SET_BOOT);
    int last_write = fake_flash_last(&rig.flash, FAKE_FLASH_WRITE);
    chorus_check(last_write < finish_at && finish_at < set_at &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 1,
                 "on the flash's own log: last write [%d], then the check [%d], then ONE "
                 "selection [%d]",
                 last_write, finish_at, set_at);
    chorus_check(rig.notes.present && rig.notes.saves == 1 &&
                     memcmp(rig.notes.note, "ota1 41 1 2.0.0", 15) == 0,
                 "the note names the image about to be tried: \"%.*s\"", (int)rig.notes.length,
                 (const char *)rig.notes.note);
    chorus_check(fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_NEW &&
                     fake_flash_state(&rig.flash, 0) == CHORUS_OTA_IMAGE_VALID,
                 "slot 1 is recorded NEW and slot 0 is still VALID behind it");
    chorus_check(memcmp(rig.flash.slots[1], image, length) == 0,
                 "slot 1 holds the offered image byte for byte");
    int verified = 0;
    while (chorus_ota_take_status(&rig.ota, &status)) {
        verified = status.state == CHORUS_OTA_WIRE_VERIFIED && status.received == length;
    }
    chorus_check(verified, "the last status before the reboot is verified, received %zu", length);

    chorus_ota_reboot(&rig.ota);
    chorus_check(chorus_ota_rebooting(&rig.ota) && rig.flash.reboot_requested,
                 "the unit reboots through the medium");
    rig_power_on(&rig);
    chorus_check(rig.flash.running == 1 &&
                     fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_PENDING_VERIFY &&
                     rig.ota.state == CHORUS_OTA_PENDING_VERIFY,
                 "rule 3: the bootloader boots slot 1 on trial and the unit is pending-verify");
    chorus_ota_session_started(&rig.ota);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_PENDING_VERIFY &&
                     status.transfer == 41 && status.slot == 1 &&
                     strcmp(status.version, "2.0.0") == 0,
                 "its session opens with pending_verify, transfer 41, slot 1, version 2.0.0");
    chorus_ota_tick(&rig.ota, rig.now_ns + CONFIRM_NS - 1);
    chorus_check(rig.ota.state == CHORUS_OTA_PENDING_VERIFY &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_INVALIDATE) == 0,
                 "one nanosecond before the deadline nothing is invalidated");
    chorus_ota_session_healthy(&rig.ota, rig.now_ns + 3000000000ull);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(rig.ota.state == CHORUS_OTA_VALID &&
                     fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_VALID && got == 1 &&
                     status.state == CHORUS_OTA_WIRE_CONFIRMED,
                 "the session reached its server: confirmed, slot 1 VALID, status confirmed");
    chorus_check(!rig.notes.present, "a confirmed image leaves no note");
    chorus_ota_tick(&rig.ota, rig.now_ns + 10 * CONFIRM_NS);
    chorus_check(rig.ota.state == CHORUS_OTA_VALID &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_INVALIDATE) == 0,
                 "a confirmed image is not rolled back by the clock");
    rig_power_on(&rig);
    chorus_check(rig.flash.running == 1 && rig.ota.state == CHORUS_OTA_RUNNING_VALID &&
                     running_version_is(&rig, "2.0.0"),
                 "the next boot is slot 1, running-valid, version 2.0.0");
    chorus_check(clean_update(&rig, "3.0.0", 42, 9) && rig.flash.running == 0,
                 "and the next update goes back into slot 0 and confirms there");
    rig_free(&rig);
}

/* --- refusals by name --------------------------------------------------------- */

static void an_offer_that_cannot_be_taken_is_refused_by_name(void)
{
    chorus_section("an offer that cannot be taken is refused by name, and nothing is written");
    static rig_t rig;
    static uint8_t image[SLOT_BYTES];
    chorus_ota_status_t status;
    rig_new(&rig, "1.0.0");
    size_t length = make(image, "2.0.0", 1000, 3);

    chorus_ota_offer_t offer = offer_for(image, length, "2.0.0", 5, 128);
    offer.size = SLOT_BYTES + 1;
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    int got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_REFUSED &&
                     status.reason == CHORUS_OTA_REASON_TOO_LARGE && status.transfer == 5 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT) == 0,
                 "one byte more than the slot holds: refused %s, nothing erased",
                 chorus_ota_reason_name((chorus_ota_reason_t)status.reason));

    offer = offer_for(image, length, "2.0.0", 6, 128);
    snprintf(offer.board, sizeof(offer.board), "compact-s3-wifi");
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.reason == CHORUS_OTA_REASON_WRONG_BOARD &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT) == 0,
                 "an image built for another board: refused %s, nothing erased",
                 chorus_ota_reason_name((chorus_ota_reason_t)status.reason));

    chorus_ota_flash_t ops = fake_flash_ops(&rig.flash);
    chorus_check(ops.begin(ops.context, rig.flash.running, 1024) != 0 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT) == 0,
                 "the medium refuses to prepare the RUNNING slot, as esp_ota_begin does");
    offer = offer_for(image, length, "2.0.0", 7, 128);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    chorus_check(rig.ota.state == CHORUS_OTA_RECEIVING && rig.ota.target == 1 &&
                     rig.flash.running == 0,
                 "and the unit never asks: its target is the other slot (%d), not the running "
                 "one (%d)",
                 rig.ota.target, rig.flash.running);
    drain(&rig);

    /* A second offer mid-download. */
    (void)send_from(&rig, image, length, 7, 128, 0, 512);
    chorus_ota_offer_t second = offer_for(image, length, "2.0.1", 8, 128);
    second.sha256[0] ^= 1;
    size_t erases = fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT);
    chorus_ota_offer(&rig.ota, &second, rig.now_ns);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_REFUSED &&
                     status.reason == CHORUS_OTA_REASON_BUSY && status.transfer == 8 &&
                     rig.ota.state == CHORUS_OTA_RECEIVING && rig.ota.received == 512 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT) == erases,
                 "a second offer mid-download: refused %s; the first transfer keeps its 512 "
                 "bytes",
                 chorus_ota_reason_name((chorus_ota_reason_t)status.reason));
    faults_injected++;
    (void)send_from(&rig, image, length, 7, 128, 512, UINT32_MAX);
    settle(&rig);
    int landed = running_version_is(&rig, "2.0.0") &&
                 fake_flash_state(&rig.flash, rig.flash.running) == CHORUS_OTA_IMAGE_VALID;
    faults_survived += landed;
    chorus_check(landed, "and the first transfer still completes and confirms as 2.0.0");

    /* An offer during pending-verify. */
    length = make(image, "3.0.0", 1000, 4);
    install(&rig, image, length, "3.0.0", 9, 128);
    drain(&rig);
    chorus_ota_reboot(&rig.ota);
    rig_power_on(&rig);
    chorus_ota_offer_t third = offer_for(image, length, "4.0.0", 10, 128);
    erases = fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT);
    chorus_ota_offer(&rig.ota, &third, rig.now_ns);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_REFUSED &&
                     status.reason == CHORUS_OTA_REASON_NOT_CONFIRMED &&
                     rig.ota.state == CHORUS_OTA_PENDING_VERIFY &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT) == erases,
                 "an offer while the running image is on trial: refused %s; the fallback slot "
                 "is not erased",
                 chorus_ota_reason_name((chorus_ota_reason_t)status.reason));
    chorus_check(survived(&rig) && running_version_is(&rig, "3.0.0"),
                 "the trial then runs its course and 3.0.0 confirms");
    rig_free(&rig);
}

/* --- each named fault --------------------------------------------------------- */

static void each_named_fault_leaves_a_valid_slot(void)
{
    chorus_section("each named fault leaves a VALID slot selected");
    static rig_t rig;
    static uint8_t image[SLOT_BYTES];
    static uint8_t wrong[SLOT_BYTES];
    chorus_ota_status_t status;
    size_t length;

    /* A corrupted byte in transit: the digest. */
    rig_new(&rig, "1.0.0");
    length = make(image, "2.0.0", 2000, 11);
    memcpy(wrong, image, length);
    wrong[length / 2] ^= 0x01;
    chorus_ota_offer_t offer = offer_for(image, length, "2.0.0", 20, 128);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    (void)send_from(&rig, wrong, length, 20, 128, 0, UINT32_MAX);
    int refused = 0;
    while (chorus_ota_take_status(&rig.ota, &status)) {
        refused = status.state == CHORUS_OTA_WIRE_REFUSED &&
                  status.reason == CHORUS_OTA_REASON_BAD_DIGEST;
    }
    chorus_check(refused && rig.ota.state == CHORUS_OTA_REFUSED &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_FINISH) == 0 && !rig.notes.present,
                 "one corrupted byte in transit: refused bad_digest before the medium's check, "
                 "no selection, no note");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"),
                 "1.0.0 still runs, VALID, and a cold boot agrees");
    chorus_check(clean_update(&rig, "2.0.0", 21, 12), "and the board still takes a good image");
    rig_free(&rig);

    /* A bit that did not take in the flash: the medium's own check. */
    rig_new(&rig, "1.0.0");
    rig.flash.corrupt_write_at = 3;
    install(&rig, image, length, "2.0.0", 22, 128);
    refused = 0;
    while (chorus_ota_take_status(&rig.ota, &status)) {
        refused = status.state == CHORUS_OTA_WIRE_REFUSED &&
                  status.reason == CHORUS_OTA_REASON_MEDIUM_REFUSED;
    }
    chorus_check(refused && fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0,
                 "a flash bit that did not take (the digest of what was SENT is good): the "
                 "medium's check refuses, medium_refused, no selection");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"), "1.0.0 still runs, VALID");
    rig_free(&rig);

    /* A short image: the offer promises more than arrives. */
    rig_new(&rig, "1.0.0");
    offer = offer_for(image, length, "2.0.0", 23, 128);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    (void)send_from(&rig, image, length, 23, 128, 0, (uint32_t)length - 300);
    chorus_check(rig.ota.state == CHORUS_OTA_RECEIVING &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0,
                 "a short image (300 bytes never arrive): still receiving, nothing selected");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"),
                 "1.0.0 still runs, VALID; the half-written slot is never booted");
    rig_free(&rig);

    /* An image whose bytes are all there but which is not an image. */
    rig_new(&rig, "1.0.0");
    memset(wrong, 0x5A, 1024);
    install(&rig, wrong, 1024, "2.0.0", 24, 128);
    refused = 0;
    while (chorus_ota_take_status(&rig.ota, &status)) {
        refused = status.reason == CHORUS_OTA_REASON_MEDIUM_REFUSED;
    }
    chorus_check(refused && fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0,
                 "1024 bytes with a good digest that are not an application image: the "
                 "medium's check refuses, no selection");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"), "1.0.0 still runs, VALID");
    rig_free(&rig);

    /* A write error. */
    rig_new(&rig, "1.0.0");
    rig.flash.write_error_at = 5;
    install(&rig, image, length, "2.0.0", 25, 128);
    refused = 0;
    while (chorus_ota_take_status(&rig.ota, &status)) {
        refused = status.state == CHORUS_OTA_WIRE_REFUSED &&
                  status.reason == CHORUS_OTA_REASON_WRITE_FAILED;
    }
    chorus_check(refused && fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ABANDON) == 1,
                 "a write error at the sixth chunk: refused write_failed, the write abandoned, "
                 "no selection");
    size_t ignored = rig.ota.chunks_ignored;
    chorus_ota_chunk(&rig.ota, 25, 768, image + 768, 128, rig.now_ns);
    chorus_check(rig.ota.chunks_ignored == ignored + 1,
                 "the chunks still in flight after the refusal are ignored");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"), "1.0.0 still runs, VALID");
    chorus_check(clean_update(&rig, "2.0.0", 26, 13), "and the board still takes a good image");
    rig_free(&rig);

    /* The medium refuses its check, the preparation, the selection, the note. */
    static const char *const what[] = {"finish", "begin", "set_boot", "note"};
    for (int which = 0; which < 4; which++) {
        rig_new(&rig, "1.0.0");
        rig.flash.finish_refuses = which == 0;
        rig.flash.begin_refuses = which == 1;
        rig.flash.set_boot_refuses = which == 2;
        rig.notes.save_refuses = which == 3;
        install(&rig, image, length, "2.0.0", 30u + (uint32_t)which, 128);
        refused = 0;
        while (chorus_ota_take_status(&rig.ota, &status)) {
            refused = status.state == CHORUS_OTA_WIRE_REFUSED &&
                      status.reason == CHORUS_OTA_REASON_MEDIUM_REFUSED;
        }
        chorus_check(
            refused && fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0 && !rig.notes.present,
            "a %s refusal: refused medium_refused, no selection, no note left", what[which]);
        chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0") &&
                         rig.ota.state == CHORUS_OTA_RUNNING_VALID,
                     "after a %s refusal 1.0.0 still runs, VALID, and no rollback is reported",
                     what[which]);
        chorus_check(clean_update(&rig, "2.0.0", 40u + (uint32_t)which, 14),
                     "and the board still takes a good image");
        rig_free(&rig);
    }

    /* Power lost between the medium's check and the selection, and between
     * the selection and the reboot. */
    rig_new(&rig, "1.0.0");
    offer = offer_for(image, length, "2.0.0", 50, 128);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    uint32_t last = (uint32_t)(length - (length % 128 == 0 ? 128 : length % 128));
    (void)send_from(&rig, image, length, 50, 128, 0, last);
    /* The next medium step after the last data write is set_boot's erase. */
    rig.flash.power_loss_at = rig.flash.steps + 1;
    (void)send_from(&rig, image, length, 50, 128, last, UINT32_MAX);
    chorus_check(!rig.flash.powered && fake_flash_count(&rig.flash, FAKE_FLASH_FINISH) == 1 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0 && rig.notes.present,
                 "power lost between the medium's check and the selection (the note is "
                 "already written)");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0") &&
                     rig.ota.state == CHORUS_OTA_RUNNING_VALID && !rig.notes.present,
                 "1.0.0 boots, VALID; the note of a trial that never began is cleared, not "
                 "reported as a rollback");
    rig_free(&rig);

    rig_new(&rig, "1.0.0");
    install(&rig, image, length, "2.0.0", 51, 128);
    chorus_check(rig.ota.state == CHORUS_OTA_PENDING_REBOOT, "the image is selected");
    rig.flash.powered = 0; /* the plug is pulled before the unit reboots */
    chorus_check(survived(&rig) && running_version_is(&rig, "2.0.0"),
                 "power lost between the selection and the reboot: 2.0.0 boots on trial, "
                 "confirms, VALID");
    rig_free(&rig);

    /* The server gone mid-download: resume, or abandon. Never half-activate. */
    rig_new(&rig, "1.0.0");
    offer = offer_for(image, length, "2.0.0", 52, 128);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    (void)send_from(&rig, image, length, 52, 128, 0, 1024);
    drain(&rig);
    chorus_ota_session_started(&rig.ota); /* the session came back */
    int got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_RECEIVING && status.transfer == 52 &&
                     status.received == 1024,
                 "the server gone mid-download: the next session opens with receiving, "
                 "transfer 52, received 1024");
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    got = chorus_ota_take_status(&rig.ota, &status);
    size_t erases = fake_flash_count(&rig.flash, FAKE_FLASH_ERASE_SLOT);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_RECEIVING && status.received == 1024 &&
                     erases == 1,
                 "the same offer again resumes: received 1024, the slot is not erased again");
    faults_injected++;
    (void)send_from(&rig, image, length, 52, 128, 1024, UINT32_MAX);
    settle(&rig);
    int resumed = running_version_is(&rig, "2.0.0") &&
                  fake_flash_state(&rig.flash, rig.flash.running) == CHORUS_OTA_IMAGE_VALID;
    faults_survived += resumed;
    chorus_check(resumed, "resumed from byte 1024, it verifies and confirms as 2.0.0");
    rig_free(&rig);

    rig_new(&rig, "1.0.0");
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    (void)send_from(&rig, image, length, 52, 128, 0, 1024);
    chorus_ota_cancel(&rig.ota);
    chorus_check(rig.ota.state == CHORUS_OTA_RUNNING_VALID &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_ABANDON) == 1 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_SET_BOOT) == 0,
                 "abandoned by the cancel: running-valid, the write given up, no selection");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"), "1.0.0 still runs, VALID");
    chorus_check(clean_update(&rig, "2.0.0", 53, 15),
                 "and a new offer afterwards installs from byte 0");
    rig_free(&rig);

    /* A duplicated and an out-of-order chunk. */
    rig_new(&rig, "1.0.0");
    offer = offer_for(image, length, "2.0.0", 54, 128);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    (void)send_from(&rig, image, length, 54, 128, 0, 512);
    drain(&rig);
    size_t writes = fake_flash_count(&rig.flash, FAKE_FLASH_WRITE);
    chorus_ota_chunk(&rig.ota, 54, 384, image + 384, 128, rig.now_ns);
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_WRITE) == writes &&
                     rig.ota.received == 512 && chorus_ota_take_status(&rig.ota, &status) == 0,
                 "a duplicated chunk is not written twice, and raises nothing");
    chorus_ota_chunk(&rig.ota, 54, 768, image + 768, 128, rig.now_ns);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_WRITE) == writes && got == 1 &&
                     status.state == CHORUS_OTA_WIRE_RECEIVING &&
                     status.reason == CHORUS_OTA_REASON_BAD_OFFSET && status.received == 512,
                 "a chunk past a gap is not written; one status says bad_offset, resume at 512");
    chorus_ota_chunk(&rig.ota, 54, 896, image + 896, 128, rig.now_ns);
    chorus_check(chorus_ota_take_status(&rig.ota, &status) == 0,
                 "the chunks behind it are ignored without a status each");
    faults_injected++;
    (void)send_from(&rig, image, length, 54, 128, 512, UINT32_MAX);
    settle(&rig);
    resumed = running_version_is(&rig, "2.0.0") &&
              fake_flash_state(&rig.flash, rig.flash.running) == CHORUS_OTA_IMAGE_VALID &&
              memcmp(rig.flash.slots[rig.flash.running], image, length) == 0;
    faults_survived += resumed;
    chorus_check(resumed, "resent from 512 in order, the image lands byte for byte and confirms");
    rig_free(&rig);

    /* A chunk longer than the offer's chunk size. */
    rig_new(&rig, "1.0.0");
    offer = offer_for(image, length, "2.0.0", 55, 64);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    drain(&rig);
    chorus_ota_chunk(&rig.ota, 55, 0, image, 128, rig.now_ns);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_REFUSED &&
                     status.reason == CHORUS_OTA_REASON_BAD_OFFSET &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_WRITE) == 0,
                 "a chunk longer than the offer's chunk size: refused bad_offset, not written");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"), "1.0.0 still runs, VALID");
    rig_free(&rig);

    /* A reboot mid-download: the unit restarts clean. */
    rig_new(&rig, "1.0.0");
    offer = offer_for(image, length, "2.0.0", 56, 128);
    chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
    (void)send_from(&rig, image, length, 56, 128, 0, 1024);
    rig_power_on(&rig);
    chorus_ota_session_started(&rig.ota);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(rig.ota.state == CHORUS_OTA_RUNNING_VALID && got == 1 &&
                     status.state == CHORUS_OTA_WIRE_IDLE && status.received == 0 &&
                     status.transfer == 0,
                 "a reboot mid-download: the unit restarts clean (idle, no transfer, received "
                 "0)");
    ignored = rig.ota.chunks_ignored;
    writes = fake_flash_count(&rig.flash, FAKE_FLASH_WRITE);
    chorus_ota_chunk(&rig.ota, 56, 1024, image + 1024, 128, rig.now_ns);
    chorus_check(rig.ota.chunks_ignored == ignored + 1 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_WRITE) == writes,
                 "the old transfer's next chunk is ignored: nothing is written without an "
                 "offer");
    chorus_check(survived(&rig) && running_version_is(&rig, "1.0.0"), "1.0.0 still runs, VALID");
    chorus_check(clean_update(&rig, "2.0.0", 57, 16), "and a new offer installs from byte 0");
    rig_free(&rig);

    /* The confirm itself refused once, then accepted. */
    rig_new(&rig, "1.0.0");
    install(&rig, image, length, "2.0.0", 58, 128);
    drain(&rig);
    chorus_ota_reboot(&rig.ota);
    rig_power_on(&rig);
    rig.flash.confirm_refuses = 1;
    chorus_ota_session_healthy(&rig.ota, rig.now_ns + 1000000000ull);
    chorus_check(rig.ota.state == CHORUS_OTA_PENDING_VERIFY,
                 "a confirm the medium refuses leaves the image on trial");
    chorus_ota_session_healthy(&rig.ota, rig.now_ns + 2000000000ull);
    chorus_check(rig.ota.state == CHORUS_OTA_VALID, "and the next healthy session confirms it");
    chorus_check(survived(&rig) && running_version_is(&rig, "2.0.0"), "2.0.0 runs, VALID");
    rig_free(&rig);
}

/* --- the bad image ------------------------------------------------------------ */

static void a_bad_image_is_rolled_back_to_the_previous_version(void)
{
    chorus_section("a bad image is rolled back to the previous version");
    static rig_t rig;
    static uint8_t image[SLOT_BYTES];
    chorus_ota_status_t status;

    /* It never confirms. */
    rig_new(&rig, "1.0.0");
    size_t length = make(image, "bad-never-2.0.0", 1500, 21);
    install(&rig, image, length, "bad-never-2.0.0", 60, 128);
    drain(&rig);
    chorus_ota_reboot(&rig.ota);
    rig_power_on(&rig);
    bad_images_shipped++;
    chorus_check(rig.flash.running == 1 && rig.ota.state == CHORUS_OTA_PENDING_VERIFY &&
                     running_version_is(&rig, "bad-never-2.0.0"),
                 "an image that never confirms is selected and boots on trial in slot 1");
    uint64_t booted_ns = rig.now_ns;
    chorus_ota_session_started(&rig.ota);
    drain(&rig);
    chorus_ota_session_healthy(&rig.ota, booted_ns + 1000000000ull);
    chorus_check(rig.ota.state == CHORUS_OTA_PENDING_VERIFY &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_CONFIRM) == 0,
                 "it does not confirm even with a healthy session");
    chorus_ota_tick(&rig.ota, booted_ns + CONFIRM_NS - 1);
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_INVALIDATE) == 0,
                 "before the deadline it is left alone");
    chorus_ota_tick(&rig.ota, booted_ns + CONFIRM_NS);
    int got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_INVALIDATE) == 1 &&
                     rig.flash.reboot_requested && chorus_ota_rebooting(&rig.ota) &&
                     fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_INVALID && got == 1 &&
                     status.reason == CHORUS_OTA_REASON_NOT_CONFIRMED,
                 "at the deadline (%llu s after boot) it marks itself INVALID and reboots",
                 (unsigned long long)(CONFIRM_NS / 1000000000ull));
    rig_power_on(&rig);
    int back = rig.flash.running == 0 && running_version_is(&rig, "1.0.0") &&
               fake_flash_state(&rig.flash, 0) == CHORUS_OTA_IMAGE_VALID;
    bad_images_rolled_back += back;
    chorus_check(back, "the bootloader boots slot 0 again: version 1.0.0, VALID");
    chorus_check(rig.ota.state == CHORUS_OTA_ROLLED_BACK, "the unit knows: rolled-back");
    chorus_ota_session_started(&rig.ota);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(got == 1 && status.state == CHORUS_OTA_WIRE_ROLLED_BACK &&
                     status.reason == CHORUS_OTA_REASON_NOT_CONFIRMED && status.transfer == 60 &&
                     strcmp(status.image_version, "bad-never-2.0.0") == 0 &&
                     strcmp(status.version, "1.0.0") == 0 && status.slot == 0,
                 "rule 4: the first session says rolled_back, transfer 60, image %s, running "
                 "%s",
                 status.image_version, status.version);
    chorus_check(rig.notes.present, "the note stays until that status has been sent");
    chorus_ota_reported(&rig.ota);
    chorus_check(!rig.notes.present, "and is cleared once it has");
    rig_power_on(&rig);
    chorus_check(rig.ota.state == CHORUS_OTA_RUNNING_VALID,
                 "the boot after that is quiet: running-valid");
    chorus_check(survived(&rig), "the invariant holds after the rollback");
    chorus_check(clean_update(&rig, "3.0.0", 61, 22) && rig.flash.running == 1,
                 "and the slot the bad image sat in takes the next good one (3.0.0)");
    rig_free(&rig);

    /* It dies before it confirms. */
    rig_new(&rig, "1.0.0");
    length = make(image, "bad-panic-2.0.0", 1500, 23);
    install(&rig, image, length, "bad-panic-2.0.0", 62, 128);
    drain(&rig);
    chorus_ota_reboot(&rig.ota);
    rig_power_on(&rig);
    bad_images_shipped++;
    chorus_check(rig.flash.running == 1 &&
                     fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_PENDING_VERIFY,
                 "an image that panics before confirming boots on trial in slot 1");
    rig_power_on(&rig); /* the panic's reset */
    back = rig.flash.running == 0 && running_version_is(&rig, "1.0.0") &&
           fake_flash_state(&rig.flash, 0) == CHORUS_OTA_IMAGE_VALID &&
           fake_flash_state(&rig.flash, 1) == CHORUS_OTA_IMAGE_ABORTED;
    bad_images_rolled_back += back;
    chorus_check(back && fake_flash_count(&rig.flash, FAKE_FLASH_BOOT_ABORTED) == 1,
                 "its one reset is its last: the bootloader marks it ABORTED and boots slot 0, "
                 "1.0.0, VALID");
    chorus_ota_session_started(&rig.ota);
    got = chorus_ota_take_status(&rig.ota, &status);
    chorus_check(rig.ota.state == CHORUS_OTA_ROLLED_BACK && got == 1 &&
                     status.state == CHORUS_OTA_WIRE_ROLLED_BACK && status.transfer == 62 &&
                     strcmp(status.image_version, "bad-panic-2.0.0") == 0,
                 "and the first session says rolled_back, naming bad-panic-2.0.0");
    chorus_check(survived(&rig), "the invariant holds after the rollback");
    rig_free(&rig);

    /* A rollback that is not possible is not attempted twice. */
    rig_new(&rig, "1.0.0");
    length = make(image, "bad-never-2.0.0", 1500, 24);
    install(&rig, image, length, "bad-never-2.0.0", 63, 128);
    drain(&rig);
    chorus_ota_reboot(&rig.ota);
    rig_power_on(&rig);
    memset(rig.flash.slots[0], 0xFF, 64); /* the fallback image is gone */
    chorus_ota_tick(&rig.ota, rig.now_ns + CONFIRM_NS);
    chorus_ota_tick(&rig.ota, rig.now_ns + 2 * CONFIRM_NS);
    chorus_check(fake_flash_count(&rig.flash, FAKE_FLASH_INVALIDATE) == 0 &&
                     !rig.flash.reboot_requested && rig.ota.rollback_impossible &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_REFUSED) == 1,
                 "with no image to go back to the medium refuses the rollback, and the unit "
                 "asks once, keeps running and does not reboot into nothing");
    rig_free(&rig);
}

/* --- the bootloader model itself ----------------------------------------------- */

static void the_bootloader_model_follows_esp_idf(void)
{
    chorus_section("the bootloader model follows ESP-IDF v6.1's rule");
    static rig_t rig;
    static uint8_t image[SLOT_BYTES];
    rig_new(&rig, "1.0.0");
    size_t length = make(image, "2.0.0", 500, 31);
    chorus_check(fake_flash_image_ok(image, length, NULL) &&
                     !fake_flash_image_ok(image, length - 1, NULL),
                 "a made image verifies (header, segment, checksum, SHA-256), and one byte "
                 "short of it does not");
    image[40] ^= 0x80;
    chorus_check(!fake_flash_image_ok(image, length, NULL),
                 "one flipped bit in the image fails its own check");
    image[40] ^= 0x80;

    /* The selected image is corrupt: the loader falls back to the other slot
     * (bootloader_utility.c:591-617). */
    install(&rig, image, length, "2.0.0", 70, 128);
    rig.flash.slots[1][100] ^= 0xFF;
    rig_power_on(&rig);
    chorus_check(rig.flash.running == 0 &&
                     fake_flash_count(&rig.flash, FAKE_FLASH_BOOT_FALLBACK) == 1,
                 "a selected slot whose image no longer verifies is not booted: the loader "
                 "falls back to slot 0");
    rig_free(&rig);

    /* Two blank slots: nothing to boot, and the model says so. */
    static fake_flash_t blank;
    (void)fake_flash_init(&blank, SLOT_BYTES);
    chorus_check(fake_flash_boot(&blank) == FAKE_BOOT_NOTHING_BOOTABLE,
                 "a flash with no image is NOT bootable: the property below can be false");
    fake_flash_free(&blank);

    /* The sequence numbers alternate between the two entries. */
    rig_new(&rig, "1.0.0");
    int ok = 1;
    static const uint32_t expected_sequence[] = {2, 3, 4, 5};
    for (uint32_t i = 0; i < 4 && ok; i++) {
        char version[32];
        snprintf(version, sizeof(version), "%u.0.0", (unsigned)(i + 2));
        ok = clean_update(&rig, version, 71 + i, 40 + i);
        int entry = (rig.flash.otadata[0].sequence == expected_sequence[i]) ? 0 : 1;
        ok = ok && rig.flash.otadata[entry].sequence == expected_sequence[i] &&
             rig.flash.running == (int)((expected_sequence[i] - 1) % 2);
    }
    chorus_check(ok, "four updates in a row alternate slots 1, 0, 1, 0 with otadata sequences "
                     "2, 3, 4, 5");
    rig_free(&rig);
}

/* --- power loss at every write ------------------------------------------------- */

typedef enum {
    BASE_FRESH,
    BASE_UPDATED,
    BASE_ROLLED_BACK
} base_t;

static const char *base_name(base_t base)
{
    switch (base) {
    case BASE_FRESH:
        return "a freshly flashed board (slot 0)";
    case BASE_UPDATED:
        return "a board updated once (slot 1)";
    case BASE_ROLLED_BACK:
        return "a board that rolled a bad image back (slot 0, slot 1 ABORTED)";
    }
    return "?";
}

/* Bring a rig to one of the three states an update can start from. Returns
 * the version it runs. */
static const char *prepare(rig_t *rig, base_t base)
{
    static uint8_t image[SLOT_BYTES];
    rig_new(rig, "1.0.0");
    if (base == BASE_UPDATED) {
        (void)clean_update(rig, "1.1.0", 1, 2);
        return "1.1.0";
    }
    if (base == BASE_ROLLED_BACK) {
        size_t length = make(image, "bad-panic-1.1.0", 400, 3);
        install(rig, image, length, "bad-panic-1.1.0", 1, 128);
        settle(rig);
        chorus_ota_session_started(rig->ota.state == CHORUS_OTA_ROLLED_BACK ? &rig->ota
                                                                            : &rig->ota);
        drain(rig);
        chorus_ota_reported(&rig->ota);
    }
    return "1.0.0";
}

/* One whole update from `base`, with power lost at medium step `lose_at`
 * counted from the offer (-1: never). Returns the steps the scenario took
 * when nothing was lost. */
static long one_update(rig_t *rig, base_t base, const char *version, long lose_at,
                       const char **previous)
{
    static uint8_t image[SLOT_BYTES];
    *previous = prepare(rig, base);
    size_t length = make(image, version, 1100, 77);
    long before = rig->flash.steps;
    if (lose_at >= 0) {
        rig->flash.power_loss_at = before + lose_at;
    }
    install(rig, image, length, version, 100, 64);
    settle(rig);
    return rig->flash.steps - before;
}

static void power_lost_at_every_write_index(void)
{
    chorus_section("power lost at every write index of an update, exhaustively");
    static rig_t rig;
    static const struct {
        const char *version;
        const char *kind;
    } images[] = {
        {"2.0.0", "a good image"},
        {"bad-never-2.0.0", "an image that never confirms"},
        {"bad-panic-2.0.0", "an image that panics before confirming"},
    };
    for (size_t which = 0; which < sizeof(images) / sizeof(images[0]); which++) {
        for (int b = BASE_FRESH; b <= BASE_ROLLED_BACK; b++) {
            base_t base = (base_t)b;
            const char *previous = NULL;
            long steps = one_update(&rig, base, images[which].version, -1, &previous);
            int good = which == 0;
            int dry =
                !rig.bricked && running_version_is(&rig, good ? images[which].version : previous);
            rig_free(&rig);
            long held = 0;
            long updatable = 0;
            long rolled = 0;
            long shipped = 0;
            /* Every step of the dry run: each of these cuts fires. */
            long points = steps;
            long fired = 0;
            for (long lose_at = 0; lose_at < points; lose_at++) {
                (void)one_update(&rig, base, images[which].version, lose_at, &previous);
                fired += fake_flash_count(&rig.flash, FAKE_FLASH_POWER_LOST) == 1;
                int ok = survived(&rig);
                /* Whatever the cut left, the board runs either the version
                 * before or (a good image only) the new one. */
                int on_previous = running_version_is(&rig, previous);
                int on_new = running_version_is(&rig, images[which].version);
                ok = ok && (on_previous || (good && on_new));
                held += ok;
                if (!good) {
                    /* Did the bad image ever boot? Then it was rolled back. */
                    int tried = fake_flash_count(&rig.flash, FAKE_FLASH_BOOT_PENDING) >
                                (base == BASE_ROLLED_BACK ? 1u : (base == BASE_UPDATED ? 1u : 0u));
                    shipped += tried;
                    rolled += tried && on_previous;
                }
                updatable += clean_update(&rig, "9.9.9", 900, 5);
                rig_free(&rig);
            }
            if (!good) {
                bad_images_shipped += shipped;
                bad_images_rolled_back += rolled;
            }
            /* One step past the end the cut is still armed: the loop above
             * covered the whole update and nothing after it. */
            (void)one_update(&rig, base, images[which].version, steps, &previous);
            int covered = rig.flash.power_loss_at >= 0 &&
                          fake_flash_count(&rig.flash, FAKE_FLASH_POWER_LOST) == 0;
            rig_free(&rig);
            chorus_check(dry && covered && steps > 20 && fired == points && held == points &&
                             updatable == points && rolled == shipped,
                         "%s onto %s: %ld medium steps, power lost at each of the %ld, %ld "
                         "left a VALID bootable slot, %ld then took a clean update%s",
                         images[which].kind, base_name(base), steps, points, held, updatable,
                         good ? "" : ", every trial boot of the bad image rolled back");
        }
    }
}

/* --- the seeded campaign -------------------------------------------------------- */

static uint32_t rng_state;

static uint32_t rng(void)
{
    rng_state ^= rng_state << 13;
    rng_state ^= rng_state >> 17;
    rng_state ^= rng_state << 5;
    return rng_state;
}

static uint32_t below(uint32_t n)
{
    return rng() % n;
}

#define CAMPAIGN_SEED 0x0C4A0514u
#define CAMPAIGN_RUNS 3000

static void a_seeded_campaign_of_random_faults(void)
{
    chorus_section("a seeded campaign of random faults");
    static rig_t rig;
    static uint8_t image[SLOT_BYTES];
    static uint8_t other[SLOT_BYTES];
    static const char *const kinds[] = {
        "power loss at a random step",
        "a corrupted byte in transit",
        "a short image",
        "a write error",
        "a finish refusal",
        "a begin refusal",
        "a set_boot refusal",
        "a note save refusal",
        "a flash bit that fails",
        "an image that never confirms",
        "an image that panics",
        "a duplicated chunk",
        "an out-of-order chunk",
        "a second offer mid-download",
        "a reboot mid-download",
        "the server gone, then back",
        "two power losses",
        "a confirm refusal",
    };
    enum {
        KINDS = sizeof(kinds) / sizeof(kinds[0])
    };
    long per_kind[KINDS] = {0};
    long held = 0;
    long updatable = 0;
    long shipped = 0;
    long rolled = 0;
    rng_state = CAMPAIGN_SEED;
    for (int run = 0; run < CAMPAIGN_RUNS; run++) {
        base_t base = (base_t)below(3);
        const char *previous = prepare(&rig, base);
        uint32_t kind = below(KINDS);
        per_kind[kind]++;
        const char *version = (kind == 9)    ? "bad-never-2.0.0"
                              : (kind == 10) ? "bad-panic-2.0.0"
                                             : "2.0.0";
        size_t length = make(image, version, 300 + below(5000), rng());
        uint16_t chunk = (uint16_t)(16 + below(500));
        uint32_t transfer = 1000u + (uint32_t)run;
        chorus_ota_offer_t offer = offer_for(image, length, version, transfer, chunk);
        uint32_t cut = below((uint32_t)length);
        long boots_before = (long)fake_flash_count(&rig.flash, FAKE_FLASH_BOOT_PENDING);
        int expect_new = 0;
        switch (kind) {
        case 0:
        case 16:
            /* Anywhere from the offer to well past the confirm. */
            rig.flash.power_loss_at =
                rig.flash.steps + (long)below((uint32_t)(length / chunk) + 12);
            install(&rig, image, length, version, transfer, chunk);
            if (kind == 16) {
                settle(&rig);
                rig.flash.power_loss_at = rig.flash.steps + (long)below(6);
                (void)clean_update(&rig, "2.0.0", transfer + 100000u, rng());
            }
            break;
        case 1:
            memcpy(other, image, length);
            other[cut] ^= (uint8_t)(1u << below(8));
            chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
            (void)send_from(&rig, other, length, transfer, chunk, 0, UINT32_MAX);
            break;
        case 2:
            chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
            (void)send_from(&rig, image, length, transfer, chunk, 0, cut);
            break;
        case 3:
            rig.flash.write_error_at =
                rig.flash.data_writes + (long)below((uint32_t)(length / chunk) + 1);
            install(&rig, image, length, version, transfer, chunk);
            break;
        case 4:
            rig.flash.finish_refuses = 1;
            install(&rig, image, length, version, transfer, chunk);
            break;
        case 5:
            rig.flash.begin_refuses = 1;
            install(&rig, image, length, version, transfer, chunk);
            break;
        case 6:
            rig.flash.set_boot_refuses = 1;
            install(&rig, image, length, version, transfer, chunk);
            break;
        case 7:
            rig.notes.save_refuses = 1;
            install(&rig, image, length, version, transfer, chunk);
            break;
        case 8:
            rig.flash.corrupt_write_at =
                rig.flash.data_writes + (long)below((uint32_t)(length / chunk) + 1);
            install(&rig, image, length, version, transfer, chunk);
            break;
        case 9:
        case 10:
            install(&rig, image, length, version, transfer, chunk);
            break;
        case 11:
        case 12: {
            chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
            uint32_t at = send_from(&rig, image, length, transfer, chunk, 0, cut);
            if (kind == 11 && at >= chunk) {
                chorus_ota_chunk(&rig.ota, transfer, at - chunk, image + at - chunk, chunk,
                                 rig.now_ns);
            } else if (at + 2u * chunk < length) {
                chorus_ota_chunk(&rig.ota, transfer, at + chunk, image + at + chunk, chunk,
                                 rig.now_ns);
            }
            (void)send_from(&rig, image, length, transfer, chunk, at, UINT32_MAX);
            expect_new = 1;
            break;
        }
        case 13: {
            chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
            uint32_t at = send_from(&rig, image, length, transfer, chunk, 0, cut);
            size_t second_length = make(other, "2.0.1", 300 + below(2000), rng());
            chorus_ota_offer_t second =
                offer_for(other, second_length, "2.0.1", transfer + 500000u, chunk);
            chorus_ota_offer(&rig.ota, &second, rig.now_ns);
            (void)send_from(&rig, other, second_length, second.transfer, chunk, 0, UINT32_MAX);
            (void)send_from(&rig, image, length, transfer, chunk, at, UINT32_MAX);
            expect_new = 1;
            break;
        }
        case 14:
            chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
            (void)send_from(&rig, image, length, transfer, chunk, 0, cut);
            rig_power_on(&rig);
            (void)send_from(&rig, image, length, transfer, chunk, cut, UINT32_MAX);
            break;
        case 15: {
            chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
            uint32_t at = send_from(&rig, image, length, transfer, chunk, 0, cut);
            chorus_ota_session_started(&rig.ota);
            if (below(2) == 0) {
                chorus_ota_offer(&rig.ota, &offer, rig.now_ns);
                (void)send_from(&rig, image, length, transfer, chunk, at, UINT32_MAX);
                expect_new = 1;
            } else {
                chorus_ota_cancel(&rig.ota);
            }
            break;
        }
        default:
            install(&rig, image, length, version, transfer, chunk);
            drain(&rig);
            if (chorus_ota_reboot_due(&rig.ota)) {
                chorus_ota_reboot(&rig.ota);
                rig_power_on(&rig);
                rig.flash.confirm_refuses = 1;
                chorus_ota_session_healthy(&rig.ota, rig.now_ns + 1);
            }
            expect_new = 1;
            break;
        }
        int ok = survived(&rig);
        /* A cut drawn past the end of what this run did never fired. */
        rig.flash.power_loss_at = -1;
        int on_previous = running_version_is(&rig, previous);
        int on_new = running_version_is(&rig, "2.0.0");
        ok = ok && (on_previous || on_new) && (!expect_new || on_new);
        if (kind == 9 || kind == 10) {
            int tried = (long)fake_flash_count(&rig.flash, FAKE_FLASH_BOOT_PENDING) > boots_before;
            shipped += tried;
            rolled += tried && on_previous && rig.ota.state == CHORUS_OTA_ROLLED_BACK;
            ok = ok && tried && on_previous;
        }
        held += ok;
        if (!ok && held + 5 > run) {
            printf("    run %d (%s onto %s) did not hold; the flash's log:\n", run, kinds[kind],
                   base_name(base));
            fake_flash_print(&rig.flash);
        }
        updatable += clean_update(&rig, "9.9.9", 900000u + (uint32_t)run, rng());
        rig_free(&rig);
    }
    bad_images_shipped += shipped;
    bad_images_rolled_back += rolled;
    int every_kind = 1;
    for (int k = 0; k < KINDS; k++) {
        every_kind = every_kind && per_kind[k] > 0;
        printf("    %-32s %ld runs\n", kinds[k], per_kind[k]);
    }
    chorus_check(every_kind, "seed 0x%08X, %d runs: every one of the %d fault kinds was drawn",
                 (unsigned)CAMPAIGN_SEED, CAMPAIGN_RUNS, (int)KINDS);
    chorus_check(held == CAMPAIGN_RUNS,
                 "%ld of %d runs left a VALID bootable slot on the version before or the new "
                 "one",
                 held, CAMPAIGN_RUNS);
    chorus_check(updatable == CAMPAIGN_RUNS,
                 "%ld of %d boards then took a clean update to 9.9.9 and confirmed it", updatable,
                 CAMPAIGN_RUNS);
    chorus_check(shipped > 0 && rolled == shipped,
                 "%ld bad images were selected and booted in the campaign; %ld were rolled back "
                 "and reported",
                 shipped, rolled);
}

int main(void)
{
    a_good_image_is_written_verified_tried_and_confirmed();
    an_offer_that_cannot_be_taken_is_refused_by_name();
    each_named_fault_leaves_a_valid_slot();
    a_bad_image_is_rolled_back_to_the_previous_version();
    the_bootloader_model_follows_esp_idf();
    power_lost_at_every_write_index();
    a_seeded_campaign_of_random_faults();

    chorus_section("the foundation line");
    chorus_check(faults_injected > 0 && faults_survived == faults_injected,
                 "%ld faults injected, %ld survived", faults_injected, faults_survived);
    chorus_check(bad_images_shipped > 0 && bad_images_rolled_back == bad_images_shipped,
                 "%ld bad images booted on trial, %ld rolled back to the previous version",
                 bad_images_shipped, bad_images_rolled_back);
    int failed = chorus_test_report("test_ota");
    if (failed == 0) {
        printf("ota faults: %ld injected, %ld survived (a VALID slot bootable after each), bad "
               "image rolled back\n",
               faults_injected, faults_survived);
    } else {
        printf("ota faults: %ld injected, %ld survived: NOT every fault was survived\n",
               faults_injected, faults_survived);
    }
    return failed;
}
