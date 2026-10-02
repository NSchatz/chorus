/* The firmware update's decisions (chorus/ota.h has the contract and the
 * ESP-IDF citations). Every path out of a function leaves the unit in a
 * state it can be asked anything from, and no path selects a slot whose
 * image was not written whole, digested and checked by the medium. */

#include "chorus/ota.h"

#include <psa/crypto.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

_Static_assert(sizeof(psa_hash_operation_t) <= sizeof(((chorus_ota_t *)0)->hash),
               "the hash storage in chorus_ota_t is too small for psa_hash_operation_t");
_Static_assert(_Alignof(psa_hash_operation_t) <= _Alignof(uint64_t),
               "the hash storage in chorus_ota_t is under-aligned for psa_hash_operation_t");

/* The note's first word: its format. A note in another format is not read. */
#define NOTE_FORMAT "ota1"

const char *chorus_ota_image_state_name(chorus_ota_image_state_t state)
{
    switch (state) {
    case CHORUS_OTA_IMAGE_NEW:
        return "new";
    case CHORUS_OTA_IMAGE_PENDING_VERIFY:
        return "pending-verify";
    case CHORUS_OTA_IMAGE_VALID:
        return "valid";
    case CHORUS_OTA_IMAGE_INVALID:
        return "invalid";
    case CHORUS_OTA_IMAGE_ABORTED:
        return "aborted";
    case CHORUS_OTA_IMAGE_UNDEFINED:
        return "undefined";
    }
    return "unknown";
}

const char *chorus_ota_state_name(chorus_ota_state_t state)
{
    switch (state) {
    case CHORUS_OTA_RUNNING_VALID:
        return "running-valid";
    case CHORUS_OTA_RECEIVING:
        return "receiving";
    case CHORUS_OTA_WRITTEN_UNVERIFIED:
        return "written-unverified";
    case CHORUS_OTA_PENDING_REBOOT:
        return "pending-reboot";
    case CHORUS_OTA_PENDING_VERIFY:
        return "pending-verify";
    case CHORUS_OTA_VALID:
        return "valid";
    case CHORUS_OTA_ROLLED_BACK:
        return "rolled-back";
    case CHORUS_OTA_REFUSED:
        return "refused";
    }
    return "unknown";
}

const char *chorus_ota_reason_name(chorus_ota_reason_t reason)
{
    switch (reason) {
    case CHORUS_OTA_REASON_NONE:
        return "none";
    case CHORUS_OTA_REASON_TOO_LARGE:
        return "too_large";
    case CHORUS_OTA_REASON_BAD_DIGEST:
        return "bad_digest";
    case CHORUS_OTA_REASON_WRITE_FAILED:
        return "write_failed";
    case CHORUS_OTA_REASON_BUSY:
        return "busy";
    case CHORUS_OTA_REASON_WRONG_BOARD:
        return "wrong_board";
    case CHORUS_OTA_REASON_NOT_CONFIRMED:
        return "not_confirmed";
    case CHORUS_OTA_REASON_BAD_OFFSET:
        return "bad_offset";
    case CHORUS_OTA_REASON_MEDIUM_REFUSED:
        return "medium_refused";
    }
    return "unknown";
}

static psa_hash_operation_t *hash_of(chorus_ota_t *ota)
{
    return (psa_hash_operation_t *)(void *)ota->hash.bytes;
}

static void hash_drop(chorus_ota_t *ota)
{
    if (ota->hashing) {
        psa_hash_abort(hash_of(ota));
        ota->hashing = 0;
    }
}

static int hash_start(chorus_ota_t *ota)
{
    hash_drop(ota);
    psa_hash_operation_t fresh = PSA_HASH_OPERATION_INIT;
    memcpy(ota->hash.bytes, &fresh, sizeof(fresh));
    if (psa_crypto_init() != PSA_SUCCESS ||
        psa_hash_setup(hash_of(ota), PSA_ALG_SHA_256) != PSA_SUCCESS) {
        return -1;
    }
    ota->hashing = 1;
    return 0;
}

static void copy_text(char *out, const char *text)
{
    snprintf(out, CHORUS_OTA_TEXT_MAX, "%s", (text == NULL) ? "" : text);
}

/* What the wire says for the state the unit is in. */
static uint8_t wire_state(const chorus_ota_t *ota)
{
    switch (ota->state) {
    case CHORUS_OTA_RECEIVING:
    case CHORUS_OTA_WRITTEN_UNVERIFIED:
        return CHORUS_OTA_WIRE_RECEIVING;
    case CHORUS_OTA_PENDING_REBOOT:
        /* Either the new image is selected (verified), or the trial failed
         * and the reboot that rolls back is due (still pending_verify, with
         * the reason). */
        return (ota->reason == CHORUS_OTA_REASON_NOT_CONFIRMED) ? CHORUS_OTA_WIRE_PENDING_VERIFY
                                                                : CHORUS_OTA_WIRE_VERIFIED;
    case CHORUS_OTA_PENDING_VERIFY:
        return CHORUS_OTA_WIRE_PENDING_VERIFY;
    case CHORUS_OTA_VALID:
        return CHORUS_OTA_WIRE_CONFIRMED;
    case CHORUS_OTA_ROLLED_BACK:
        return CHORUS_OTA_WIRE_ROLLED_BACK;
    case CHORUS_OTA_REFUSED:
        return CHORUS_OTA_WIRE_REFUSED;
    case CHORUS_OTA_RUNNING_VALID:
        break;
    }
    return CHORUS_OTA_WIRE_IDLE;
}

void chorus_ota_status(const chorus_ota_t *ota, chorus_ota_status_t *out)
{
    memset(out, 0, sizeof(*out));
    out->state = wire_state(ota);
    out->reason = (uint8_t)ota->reason;
    copy_text(out->version, ota->config.version);
    copy_text(out->board, ota->config.board);
    out->slot = (ota->running == 0 || ota->running == 1) ? (uint8_t)ota->running : 255u;
    switch (ota->state) {
    case CHORUS_OTA_RECEIVING:
    case CHORUS_OTA_WRITTEN_UNVERIFIED:
    case CHORUS_OTA_REFUSED:
        out->transfer = ota->offer.transfer;
        out->received = ota->received;
        copy_text(out->image_version, ota->offer.version);
        break;
    case CHORUS_OTA_PENDING_REBOOT:
        if (ota->reason != CHORUS_OTA_REASON_NOT_CONFIRMED) {
            out->transfer = ota->offer.transfer;
            out->received = ota->received;
            copy_text(out->image_version, ota->offer.version);
        } else {
            out->transfer = ota->note_transfer;
        }
        break;
    case CHORUS_OTA_PENDING_VERIFY:
    case CHORUS_OTA_VALID:
        /* The image on trial, or just confirmed, IS the running one; the
         * transfer that brought it is the note's. */
        out->transfer = ota->note_transfer;
        break;
    case CHORUS_OTA_ROLLED_BACK:
        out->transfer = ota->note_transfer;
        copy_text(out->image_version, ota->note_version);
        break;
    case CHORUS_OTA_RUNNING_VALID:
        break;
    }
}

/* Queue a status. A full queue drops its oldest: the newest is the truth. */
static void queue_status(chorus_ota_t *ota, const chorus_ota_status_t *status)
{
    if (ota->queued == CHORUS_OTA_STATUS_QUEUE) {
        memmove(&ota->queue[0], &ota->queue[1],
                (CHORUS_OTA_STATUS_QUEUE - 1) * sizeof(ota->queue[0]));
        ota->queued--;
    }
    ota->queue[ota->queued++] = *status;
}

static void queue_current(chorus_ota_t *ota)
{
    chorus_ota_status_t status;
    chorus_ota_status(ota, &status);
    queue_status(ota, &status);
}

/* Move to `state` and tell the listener; `announce` queues its status too
 * (not at boot: every session opens with one of its own). */
static void enter(chorus_ota_t *ota, chorus_ota_state_t state, chorus_ota_reason_t reason,
                  int announce)
{
    ota->state = state;
    ota->reason = reason;
    chorus_ota_status_t status;
    chorus_ota_status(ota, &status);
    if (announce) {
        queue_status(ota, &status);
    }
    if (ota->config.on_change != NULL) {
        ota->config.on_change(ota->config.change_context, &status, state);
    }
}

static void change(chorus_ota_t *ota, chorus_ota_state_t state, chorus_ota_reason_t reason)
{
    enter(ota, state, reason, 1);
}

/* Give up whatever was being written and say why. The boot selection has not
 * been touched on any path that reaches here. */
static void refuse_transfer(chorus_ota_t *ota, chorus_ota_reason_t reason)
{
    hash_drop(ota);
    if (ota->target >= 0 && ota->config.flash->abandon != NULL) {
        ota->config.flash->abandon(ota->config.flash->context);
    }
    ota->target = -1;
    ota->offers_refused++;
    change(ota, CHORUS_OTA_REFUSED, reason);
}

/* --- the note ----------------------------------------------------------------
 *
 * "ota1 <transfer> <slot> <version>": the image that is about to be tried.
 * Written BEFORE the boot selection changes, so that a trial that ends in a
 * rollback is always known to have happened; a note whose slot was never
 * selected (power lost between the two) describes nothing and is cleared at
 * the next boot. */

static int note_save(chorus_ota_t *ota)
{
    const chorus_ota_notes_t *notes = ota->config.notes;
    if (notes == NULL) {
        return 0;
    }
    char text[CHORUS_OTA_NOTE_MAX];
    int n = snprintf(text, sizeof(text), NOTE_FORMAT " %lu %d %s",
                     (unsigned long)ota->offer.transfer, ota->target, ota->offer.version);
    if (n < 0 || (size_t)n >= sizeof(text)) {
        return -1;
    }
    return notes->save(notes->context, (const uint8_t *)text, (size_t)n);
}

static void note_clear(chorus_ota_t *ota)
{
    const chorus_ota_notes_t *notes = ota->config.notes;
    if (notes != NULL && ota->have_note) {
        /* A clear that fails leaves a note the next boot reads again and
         * judges again by the slot's state: reported twice, never wrong. */
        (void)notes->clear(notes->context);
    }
    ota->have_note = 0;
}

static void note_load(chorus_ota_t *ota)
{
    ota->have_note = 0;
    ota->note_transfer = 0;
    ota->note_slot = -1;
    ota->note_version[0] = '\0';
    const chorus_ota_notes_t *notes = ota->config.notes;
    if (notes == NULL) {
        return;
    }
    uint8_t raw[CHORUS_OTA_NOTE_MAX];
    size_t length = 0;
    if (notes->load(notes->context, raw, sizeof(raw) - 1, &length) != 0 || length >= sizeof(raw)) {
        return;
    }
    raw[length] = '\0';
    char *text = (char *)raw;
    const size_t tag = strlen(NOTE_FORMAT);
    if (strncmp(text, NOTE_FORMAT " ", tag + 1) != 0) {
        return;
    }
    char *end = NULL;
    unsigned long transfer = strtoul(text + tag + 1, &end, 10);
    if (end == NULL || *end != ' ') {
        return;
    }
    long slot = strtol(end + 1, &end, 10);
    if (end == NULL || *end != ' ' || (slot != 0 && slot != 1)) {
        return;
    }
    ota->have_note = 1;
    ota->note_transfer = (uint32_t)transfer;
    ota->note_slot = (int)slot;
    copy_text(ota->note_version, end + 1);
}

/* --- boot --------------------------------------------------------------------- */

int chorus_ota_boot(chorus_ota_t *ota, const chorus_ota_config_t *config, uint64_t now_ns)
{
    memset(ota, 0, sizeof(*ota));
    if (config == NULL || config->flash == NULL) {
        return -1;
    }
    ota->config = *config;
    ota->target = -1;
    const chorus_ota_flash_t *flash = config->flash;
    ota->running = flash->running_slot(flash->context);
    chorus_ota_image_state_t running_state = (ota->running == 0 || ota->running == 1)
                                                 ? flash->slot_state(flash->context, ota->running)
                                                 : CHORUS_OTA_IMAGE_UNDEFINED;
    note_load(ota);

    if (running_state == CHORUS_OTA_IMAGE_PENDING_VERIFY) {
        /* The bootloader selected a NEW image and marked it on trial
         * (bootloader_utility.c:443-449). The clock starts now. */
        ota->confirm_deadline_ns = now_ns + config->confirm_ns;
        if (ota->have_note && ota->note_slot != ota->running) {
            /* A note about the other slot is not about this trial. */
            note_clear(ota);
            ota->note_transfer = 0;
        }
        enter(ota, CHORUS_OTA_PENDING_VERIFY, CHORUS_OTA_REASON_NONE, 0);
        return 0;
    }
    if (ota->have_note) {
        if (ota->note_slot == ota->running) {
            /* The noted image runs and is not on trial: it was confirmed and
             * the note's clear did not land. Nothing to report. */
            note_clear(ota);
            ota->note_transfer = 0;
        } else {
            chorus_ota_image_state_t tried = flash->slot_state(flash->context, ota->note_slot);
            if (tried == CHORUS_OTA_IMAGE_INVALID || tried == CHORUS_OTA_IMAGE_ABORTED) {
                /* It was selected, it did not confirm, and the bootloader
                 * came back here (bootloader_common_loader.c:78-86). */
                enter(ota, CHORUS_OTA_ROLLED_BACK, CHORUS_OTA_REASON_NOT_CONFIRMED, 0);
                return 0;
            }
            /* Never selected (power was lost before the selection landed),
             * or selected and not yet booted. Either way no trial ended. */
            if (tried != CHORUS_OTA_IMAGE_NEW) {
                note_clear(ota);
                ota->note_transfer = 0;
            }
        }
    }
    enter(ota, CHORUS_OTA_RUNNING_VALID, CHORUS_OTA_REASON_NONE, 0);
    return 0;
}

/* --- offers and chunks ------------------------------------------------------- */

static int same_transfer(const chorus_ota_t *ota, const chorus_ota_offer_t *offer)
{
    return ota->offer.transfer == offer->transfer && ota->offer.size == offer->size &&
           memcmp(ota->offer.sha256, offer->sha256, CHORUS_OTA_SHA256_LEN) == 0;
}

/* A refusal that leaves the transfer in progress exactly as it is: the
 * status names the refused offer, the state does not move. */
static void refuse_and_keep(chorus_ota_t *ota, const chorus_ota_offer_t *offer,
                            chorus_ota_reason_t reason)
{
    chorus_ota_status_t status;
    chorus_ota_status(ota, &status);
    status.transfer = offer->transfer;
    status.state = CHORUS_OTA_WIRE_REFUSED;
    status.reason = (uint8_t)reason;
    status.received = 0;
    copy_text(status.image_version, offer->version);
    queue_status(ota, &status);
    ota->offers_refused++;
}

void chorus_ota_offer(chorus_ota_t *ota, const chorus_ota_offer_t *offer, uint64_t now_ns)
{
    (void)now_ns;
    const chorus_ota_flash_t *flash = ota->config.flash;
    if (offer->transfer == 0) {
        chorus_ota_cancel(ota);
        return;
    }
    switch (ota->state) {
    case CHORUS_OTA_RECEIVING:
        if (same_transfer(ota, offer)) {
            /* The server came back to the transfer it began: carry on from
             * the bytes already written. */
            queue_current(ota);
            return;
        }
        refuse_and_keep(ota, offer, CHORUS_OTA_REASON_BUSY);
        return;
    case CHORUS_OTA_WRITTEN_UNVERIFIED:
    case CHORUS_OTA_PENDING_REBOOT:
        refuse_and_keep(ota, offer, CHORUS_OTA_REASON_BUSY);
        return;
    case CHORUS_OTA_PENDING_VERIFY:
        /* ESP-IDF's own write refuses too (esp_ota_ops.c:178-185): an image
         * that has not proved itself does not get to replace its only
         * fallback. */
        refuse_and_keep(ota, offer, CHORUS_OTA_REASON_NOT_CONFIRMED);
        return;
    case CHORUS_OTA_RUNNING_VALID:
    case CHORUS_OTA_VALID:
    case CHORUS_OTA_ROLLED_BACK:
    case CHORUS_OTA_REFUSED:
        break;
    }

    ota->offer = *offer;
    ota->offer.version[CHORUS_OTA_TEXT_MAX - 1] = '\0';
    ota->offer.board[CHORUS_OTA_TEXT_MAX - 1] = '\0';
    ota->received = 0;
    ota->chunks_since_ack = 0;
    ota->gap_reported = 0;
    ota->target = -1;

    if (strcmp(ota->offer.board, ota->config.board) != 0) {
        refuse_transfer(ota, CHORUS_OTA_REASON_WRONG_BOARD);
        return;
    }
    if (ota->running != 0 && ota->running != 1) {
        /* Without knowing which slot runs there is no telling which one is
         * safe to erase. */
        refuse_transfer(ota, CHORUS_OTA_REASON_MEDIUM_REFUSED);
        return;
    }
    if (flash->slot_state(flash->context, ota->running) == CHORUS_OTA_IMAGE_PENDING_VERIFY) {
        refuse_transfer(ota, CHORUS_OTA_REASON_NOT_CONFIRMED);
        return;
    }
    int target = 1 - ota->running;
    if (offer->size == 0 || offer->size > flash->slot_capacity(flash->context, target) ||
        offer->chunk_bytes == 0 || offer->chunk_bytes > CHORUS_OTA_MAX_CHUNK_BYTES) {
        refuse_transfer(ota, (offer->size == 0 || offer->chunk_bytes == 0 ||
                              offer->chunk_bytes > CHORUS_OTA_MAX_CHUNK_BYTES)
                                 ? CHORUS_OTA_REASON_BAD_OFFSET
                                 : CHORUS_OTA_REASON_TOO_LARGE);
        return;
    }
    if (hash_start(ota) != 0 || flash->begin(flash->context, target, offer->size) != 0) {
        refuse_transfer(ota, CHORUS_OTA_REASON_MEDIUM_REFUSED);
        return;
    }
    ota->target = target;
    change(ota, CHORUS_OTA_RECEIVING, CHORUS_OTA_REASON_NONE);
}

void chorus_ota_cancel(chorus_ota_t *ota)
{
    if (ota->state != CHORUS_OTA_RECEIVING && ota->state != CHORUS_OTA_REFUSED) {
        /* Nothing to cancel. A selected image is past cancelling: the trial
         * decides. */
        queue_current(ota);
        return;
    }
    hash_drop(ota);
    if (ota->state == CHORUS_OTA_RECEIVING && ota->config.flash->abandon != NULL) {
        ota->config.flash->abandon(ota->config.flash->context);
    }
    ota->target = -1;
    ota->received = 0;
    memset(&ota->offer, 0, sizeof(ota->offer));
    change(ota, CHORUS_OTA_RUNNING_VALID, CHORUS_OTA_REASON_NONE);
}

/* Rule 2, in its order: the digest of what was written, then the medium's
 * own check, then the note, and only then the boot selection. */
static void activate(chorus_ota_t *ota)
{
    const chorus_ota_flash_t *flash = ota->config.flash;
    ota->state = CHORUS_OTA_WRITTEN_UNVERIFIED;
    uint8_t digest[CHORUS_OTA_SHA256_LEN];
    size_t got = 0;
    psa_status_t hashed = psa_hash_finish(hash_of(ota), digest, sizeof(digest), &got);
    ota->hashing = 0;
    if (hashed != PSA_SUCCESS || got != sizeof(digest) ||
        memcmp(digest, ota->offer.sha256, sizeof(digest)) != 0) {
        refuse_transfer(ota, CHORUS_OTA_REASON_BAD_DIGEST);
        return;
    }
    if (flash->finish(flash->context) != 0) {
        /* finish ends the medium's write whatever it answers. */
        ota->target = -1;
        refuse_transfer(ota, CHORUS_OTA_REASON_MEDIUM_REFUSED);
        return;
    }
    int target = ota->target;
    if (note_save(ota) != 0) {
        ota->target = -1;
        refuse_transfer(ota, CHORUS_OTA_REASON_MEDIUM_REFUSED);
        return;
    }
    ota->have_note = (ota->config.notes != NULL);
    ota->note_transfer = ota->offer.transfer;
    ota->note_slot = target;
    copy_text(ota->note_version, ota->offer.version);
    if (flash->set_boot(flash->context, target) != 0) {
        /* The selection did not change, so the note describes nothing. */
        note_clear(ota);
        ota->target = -1;
        refuse_transfer(ota, CHORUS_OTA_REASON_MEDIUM_REFUSED);
        return;
    }
    ota->reboot_due = 1;
    change(ota, CHORUS_OTA_PENDING_REBOOT, CHORUS_OTA_REASON_NONE);
}

void chorus_ota_chunk(chorus_ota_t *ota, uint32_t transfer, uint32_t offset, const uint8_t *data,
                      size_t length, uint64_t now_ns)
{
    (void)now_ns;
    const chorus_ota_flash_t *flash = ota->config.flash;
    if (ota->state != CHORUS_OTA_RECEIVING || transfer != ota->offer.transfer) {
        /* A chunk of no transfer in progress: the tail of one that was
         * cancelled or refused. Nothing is written without an offer. */
        ota->chunks_ignored++;
        return;
    }
    if (offset != ota->received) {
        ota->chunks_ignored++;
        if (offset > ota->received && !ota->gap_reported) {
            /* A gap: say where to resume, once, until the stream is back in
             * order. A duplicate (offset below) is simply stepped over. */
            ota->gap_reported = 1;
            chorus_ota_status_t status;
            chorus_ota_status(ota, &status);
            status.reason = CHORUS_OTA_REASON_BAD_OFFSET;
            queue_status(ota, &status);
        }
        return;
    }
    if (length == 0 || length > ota->offer.chunk_bytes ||
        length > (size_t)(ota->offer.size - ota->received)) {
        /* In order, and not a chunk this transfer can hold: the sender and
         * the offer disagree, so nothing more of it is believed. */
        refuse_transfer(ota, CHORUS_OTA_REASON_BAD_OFFSET);
        return;
    }
    if (flash->write(flash->context, offset, data, length) != 0) {
        refuse_transfer(ota, CHORUS_OTA_REASON_WRITE_FAILED);
        return;
    }
    if (psa_hash_update(hash_of(ota), data, length) != PSA_SUCCESS) {
        refuse_transfer(ota, CHORUS_OTA_REASON_BAD_DIGEST);
        return;
    }
    ota->received += (uint32_t)length;
    ota->chunks_written++;
    ota->gap_reported = 0;
    if (ota->received == ota->offer.size) {
        activate(ota);
        return;
    }
    if (++ota->chunks_since_ack >= CHORUS_OTA_ACK_EVERY) {
        ota->chunks_since_ack = 0;
        queue_current(ota);
    }
}

/* --- the trial ---------------------------------------------------------------- */

void chorus_ota_session_healthy(chorus_ota_t *ota, uint64_t now_ns)
{
    if (ota->state != CHORUS_OTA_PENDING_VERIFY || ota->config.never_confirm ||
        now_ns >= ota->confirm_deadline_ns) {
        return;
    }
    const chorus_ota_flash_t *flash = ota->config.flash;
    if (flash->confirm_running(flash->context) != 0) {
        /* Still on trial; the next healthy session tries again, and the
         * deadline still stands. */
        return;
    }
    note_clear(ota);
    change(ota, CHORUS_OTA_VALID, CHORUS_OTA_REASON_NONE);
}

void chorus_ota_tick(chorus_ota_t *ota, uint64_t now_ns)
{
    if (ota->state != CHORUS_OTA_PENDING_VERIFY || ota->rollback_impossible ||
        now_ns < ota->confirm_deadline_ns) {
        return;
    }
    const chorus_ota_flash_t *flash = ota->config.flash;
    if (flash->invalidate_running_and_reboot(flash->context) != 0) {
        /* ESP_ERR_OTA_ROLLBACK_FAILED: there is no other image to go back to
         * (esp_ota_ops.c:1199-1203). Rebooting would only run this one
         * again, so it keeps running, unconfirmed, and says so. */
        ota->rollback_impossible = 1;
        ota->reason = CHORUS_OTA_REASON_NOT_CONFIRMED;
        queue_current(ota);
        return;
    }
    /* Only a fake returns from that. */
    ota->rebooting = 1;
    change(ota, CHORUS_OTA_PENDING_REBOOT, CHORUS_OTA_REASON_NOT_CONFIRMED);
}

void chorus_ota_session_started(chorus_ota_t *ota)
{
    queue_current(ota);
}

void chorus_ota_reported(chorus_ota_t *ota)
{
    if (ota->state == CHORUS_OTA_ROLLED_BACK) {
        note_clear(ota);
    }
}

int chorus_ota_take_status(chorus_ota_t *ota, chorus_ota_status_t *out)
{
    if (ota->queued == 0) {
        return 0;
    }
    *out = ota->queue[0];
    ota->queued--;
    memmove(&ota->queue[0], &ota->queue[1], ota->queued * sizeof(ota->queue[0]));
    return 1;
}

int chorus_ota_reboot_due(const chorus_ota_t *ota)
{
    return ota->reboot_due && !ota->rebooting;
}

void chorus_ota_reboot(chorus_ota_t *ota)
{
    ota->rebooting = 1;
    ota->config.flash->reboot(ota->config.flash->context);
}

int chorus_ota_rebooting(const chorus_ota_t *ota)
{
    return ota->rebooting;
}
