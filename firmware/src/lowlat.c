#include "chorus/lowlat.h"

#include <psa/crypto.h>
#include <string.h>

/* ASCII "CL", the first two bytes of every datagram. */
static const uint8_t MAGIC[2] = {0x43, 0x4C};

const char *chorus_lowlat_status_name(chorus_lowlat_status_t status)
{
    switch (status) {
    case CHORUS_LOWLAT_OK:
        return "ok";
    case CHORUS_LOWLAT_TOO_SHORT:
        return "too-short";
    case CHORUS_LOWLAT_TOO_LONG:
        return "too-long";
    case CHORUS_LOWLAT_BAD_MAGIC:
        return "bad-magic";
    case CHORUS_LOWLAT_BAD_VERSION:
        return "bad-version";
    case CHORUS_LOWLAT_BAD_KIND:
        return "bad-kind";
    case CHORUS_LOWLAT_WRONG_STREAM_TAG:
        return "wrong-stream-tag";
    case CHORUS_LOWLAT_REPLAYED:
        return "replayed";
    case CHORUS_LOWLAT_AUTH_FAILED:
        return "auth-failed";
    case CHORUS_LOWLAT_BAD_PARAMS:
        return "bad-params";
    case CHORUS_LOWLAT_ZERO_STREAM_TAG:
        return "zero-stream-tag";
    case CHORUS_LOWLAT_ZERO_KEY:
        return "zero-key";
    case CHORUS_LOWLAT_EXHAUSTED:
        return "exhausted";
    case CHORUS_LOWLAT_BUFFER_TOO_SMALL:
        return "buffer-too-small";
    case CHORUS_LOWLAT_CRYPTO_FAILED:
        return "crypto-failed";
    }
    return "unknown-status";
}

static void put_u32(uint8_t *out, uint32_t v)
{
    out[0] = (uint8_t)(v >> 24);
    out[1] = (uint8_t)((v >> 16) & 0xFF);
    out[2] = (uint8_t)((v >> 8) & 0xFF);
    out[3] = (uint8_t)(v & 0xFF);
}

static void put_u64(uint8_t *out, uint64_t v)
{
    for (int i = 0; i < 8; i++) {
        out[i] = (uint8_t)((v >> (56 - 8 * i)) & 0xFF);
    }
}

static uint32_t get_u32(const uint8_t *p)
{
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) | ((uint32_t)p[2] << 8) | p[3];
}

static uint64_t get_u64(const uint8_t *p)
{
    uint64_t v = 0;
    for (int i = 0; i < 8; i++) {
        v = (v << 8) | p[i];
    }
    return v;
}

/* --- the datagram ------------------------------------------------------------ */

void chorus_lowlat_header_write(const chorus_lowlat_header_t *header,
                                uint8_t out[CHORUS_LOWLAT_HEADER_LEN])
{
    out[0] = MAGIC[0];
    out[1] = MAGIC[1];
    out[2] = CHORUS_LOWLAT_VERSION;
    out[3] = header->kind;
    put_u32(out + 4, header->stream_tag);
    put_u64(out + 8, header->counter);
}

chorus_lowlat_status_t chorus_lowlat_header_parse(const uint8_t *datagram, size_t len,
                                                  chorus_lowlat_header_t *out)
{
    if (len < CHORUS_LOWLAT_HEADER_LEN + CHORUS_LOWLAT_TAG_LEN) {
        return CHORUS_LOWLAT_TOO_SHORT;
    }
    if (len > CHORUS_LOWLAT_MAX_DATAGRAM_LEN) {
        return CHORUS_LOWLAT_TOO_LONG;
    }
    if (datagram[0] != MAGIC[0] || datagram[1] != MAGIC[1]) {
        return CHORUS_LOWLAT_BAD_MAGIC;
    }
    if (datagram[2] != CHORUS_LOWLAT_VERSION) {
        return CHORUS_LOWLAT_BAD_VERSION;
    }
    if (datagram[3] != CHORUS_LOWLAT_KIND_DATA && datagram[3] != CHORUS_LOWLAT_KIND_PARITY) {
        return CHORUS_LOWLAT_BAD_KIND;
    }
    out->kind = datagram[3];
    out->stream_tag = get_u32(datagram + 4);
    out->counter = get_u64(datagram + 8);
    return CHORUS_LOWLAT_OK;
}

/* RFC 8439's 96-bit nonce: stream_tag, then counter, both big-endian. A key
 * is fresh per offer and the counter never repeats under it. */
static void nonce_bytes(uint32_t stream_tag, uint64_t counter, uint8_t out[12])
{
    put_u32(out, stream_tag);
    put_u64(out + 4, counter);
}

/* The key, imported for one call and destroyed after it, as noise.c's
 * cipher state imports its own. */
static chorus_lowlat_status_t import_key(const uint8_t key[CHORUS_LOWLAT_KEY_LEN], psa_key_id_t *id)
{
    if (psa_crypto_init() != PSA_SUCCESS) {
        return CHORUS_LOWLAT_CRYPTO_FAILED;
    }
    psa_key_attributes_t attributes = PSA_KEY_ATTRIBUTES_INIT;
    psa_set_key_type(&attributes, PSA_KEY_TYPE_CHACHA20);
    psa_set_key_bits(&attributes, 256);
    psa_set_key_usage_flags(&attributes, PSA_KEY_USAGE_ENCRYPT | PSA_KEY_USAGE_DECRYPT);
    psa_set_key_algorithm(&attributes, PSA_ALG_CHACHA20_POLY1305);
    psa_status_t status = psa_import_key(&attributes, key, CHORUS_LOWLAT_KEY_LEN, id);
    psa_reset_key_attributes(&attributes);
    return (status == PSA_SUCCESS) ? CHORUS_LOWLAT_OK : CHORUS_LOWLAT_CRYPTO_FAILED;
}

chorus_lowlat_status_t chorus_lowlat_seal(const uint8_t key[CHORUS_LOWLAT_KEY_LEN],
                                          const chorus_lowlat_header_t *header,
                                          const uint8_t *plaintext, size_t plaintext_len,
                                          uint8_t *out, size_t out_cap, size_t *written)
{
    static const uint8_t none[1] = {0};
    if (header->stream_tag == 0) {
        return CHORUS_LOWLAT_ZERO_STREAM_TAG;
    }
    if (plaintext_len > CHORUS_LOWLAT_MAX_PLAINTEXT_LEN) {
        return CHORUS_LOWLAT_TOO_LONG;
    }
    size_t total = CHORUS_LOWLAT_HEADER_LEN + plaintext_len + CHORUS_LOWLAT_TAG_LEN;
    if (out_cap < total) {
        return CHORUS_LOWLAT_BUFFER_TOO_SMALL;
    }
    uint8_t head[CHORUS_LOWLAT_HEADER_LEN];
    chorus_lowlat_header_write(header, head);
    uint8_t nonce[12];
    nonce_bytes(header->stream_tag, header->counter, nonce);
    psa_key_id_t id = 0;
    chorus_lowlat_status_t imported = import_key(key, &id);
    if (imported != CHORUS_LOWLAT_OK) {
        return imported;
    }
    size_t got = 0;
    psa_status_t status =
        psa_aead_encrypt(id, PSA_ALG_CHACHA20_POLY1305, nonce, sizeof(nonce), head, sizeof(head),
                         (plaintext_len == 0) ? none : plaintext, plaintext_len,
                         out + CHORUS_LOWLAT_HEADER_LEN, out_cap - CHORUS_LOWLAT_HEADER_LEN, &got);
    psa_destroy_key(id);
    if (status != PSA_SUCCESS || got != plaintext_len + CHORUS_LOWLAT_TAG_LEN) {
        return CHORUS_LOWLAT_CRYPTO_FAILED;
    }
    memcpy(out, head, sizeof(head));
    *written = total;
    return CHORUS_LOWLAT_OK;
}

chorus_lowlat_status_t chorus_lowlat_open(const uint8_t key[CHORUS_LOWLAT_KEY_LEN],
                                          const uint8_t *datagram, size_t len,
                                          chorus_lowlat_header_t *header, uint8_t *out,
                                          size_t out_cap, size_t *plaintext_len)
{
    static uint8_t sink[1];
    chorus_lowlat_status_t parsed = chorus_lowlat_header_parse(datagram, len, header);
    if (parsed != CHORUS_LOWLAT_OK) {
        return parsed;
    }
    size_t plain = len - CHORUS_LOWLAT_HEADER_LEN - CHORUS_LOWLAT_TAG_LEN;
    if (out_cap < plain) {
        return CHORUS_LOWLAT_BUFFER_TOO_SMALL;
    }
    uint8_t nonce[12];
    nonce_bytes(header->stream_tag, header->counter, nonce);
    psa_key_id_t id = 0;
    chorus_lowlat_status_t imported = import_key(key, &id);
    if (imported != CHORUS_LOWLAT_OK) {
        return imported;
    }
    size_t got = 0;
    psa_status_t status = psa_aead_decrypt(
        id, PSA_ALG_CHACHA20_POLY1305, nonce, sizeof(nonce), datagram, CHORUS_LOWLAT_HEADER_LEN,
        datagram + CHORUS_LOWLAT_HEADER_LEN, len - CHORUS_LOWLAT_HEADER_LEN,
        (plain == 0) ? sink : out, (plain == 0) ? sizeof(sink) : out_cap, &got);
    psa_destroy_key(id);
    if (status == PSA_ERROR_INVALID_SIGNATURE) {
        return CHORUS_LOWLAT_AUTH_FAILED;
    }
    if (status != PSA_SUCCESS || got != plain) {
        return CHORUS_LOWLAT_CRYPTO_FAILED;
    }
    *plaintext_len = plain;
    return CHORUS_LOWLAT_OK;
}

/* --- the replay window -------------------------------------------------------- */

#define WINDOW_WORDS (CHORUS_LOWLAT_REPLAY_WINDOW / 64u)

void chorus_lowlat_replay_init(chorus_lowlat_replay_t *window)
{
    memset(window, 0, sizeof(*window));
}

int chorus_lowlat_replay_accepts(const chorus_lowlat_replay_t *window, uint64_t counter)
{
    if (!window->any || counter > window->top) {
        return 1;
    }
    uint64_t age = window->top - counter;
    if (age >= CHORUS_LOWLAT_REPLAY_WINDOW) {
        return 0;
    }
    return (window->bits[age / 64u] & ((uint64_t)1 << (age % 64u))) == 0;
}

static void shift(chorus_lowlat_replay_t *window, uint64_t by)
{
    if (by >= CHORUS_LOWLAT_REPLAY_WINDOW) {
        memset(window->bits, 0, sizeof(window->bits));
        return;
    }
    size_t words = (size_t)(by / 64u);
    unsigned bits = (unsigned)(by % 64u);
    for (size_t i = WINDOW_WORDS; i-- > 0;) {
        uint64_t v = 0;
        if (i >= words) {
            v = window->bits[i - words] << bits;
            if (bits > 0 && i > words) {
                v |= window->bits[i - words - 1] >> (64u - bits);
            }
        }
        window->bits[i] = v;
    }
}

/* Called only after the tag verified, so a forged packet cannot move the
 * window (RFC 4303 section 3.4.3). */
void chorus_lowlat_replay_commit(chorus_lowlat_replay_t *window, uint64_t counter)
{
    if (!window->any) {
        window->any = 1;
        window->top = counter;
        memset(window->bits, 0, sizeof(window->bits));
    } else if (counter > window->top) {
        shift(window, counter - window->top);
        window->top = counter;
    }
    uint64_t age = window->top - counter;
    if (age < CHORUS_LOWLAT_REPLAY_WINDOW) {
        window->bits[age / 64u] |= (uint64_t)1 << (age % 64u);
    }
}

/* --- one stream's sender and receiver ----------------------------------------- */

chorus_lowlat_status_t chorus_lowlat_sealer_init(chorus_lowlat_sealer_t *sealer,
                                                 const uint8_t key[CHORUS_LOWLAT_KEY_LEN],
                                                 uint32_t stream_tag, uint64_t first_counter)
{
    uint8_t any = 0;
    for (size_t i = 0; i < CHORUS_LOWLAT_KEY_LEN; i++) {
        any |= key[i];
    }
    if (stream_tag == 0) {
        return CHORUS_LOWLAT_ZERO_STREAM_TAG;
    }
    if (any == 0) {
        return CHORUS_LOWLAT_ZERO_KEY;
    }
    memcpy(sealer->key, key, CHORUS_LOWLAT_KEY_LEN);
    sealer->stream_tag = stream_tag;
    sealer->next_counter = first_counter;
    return CHORUS_LOWLAT_OK;
}

chorus_lowlat_status_t chorus_lowlat_sealer_seal(chorus_lowlat_sealer_t *sealer, uint8_t kind,
                                                 const uint8_t *plaintext, size_t plaintext_len,
                                                 uint8_t *out, size_t out_cap, size_t *written)
{
    if (sealer->next_counter == UINT64_MAX) {
        return CHORUS_LOWLAT_EXHAUSTED;
    }
    chorus_lowlat_header_t header = {kind, sealer->stream_tag, sealer->next_counter};
    chorus_lowlat_status_t status =
        chorus_lowlat_seal(sealer->key, &header, plaintext, plaintext_len, out, out_cap, written);
    if (status == CHORUS_LOWLAT_OK) {
        sealer->next_counter++;
    }
    return status;
}

void chorus_lowlat_opener_init(chorus_lowlat_opener_t *opener,
                               const uint8_t key[CHORUS_LOWLAT_KEY_LEN], uint32_t stream_tag)
{
    memset(opener, 0, sizeof(*opener));
    memcpy(opener->key, key, CHORUS_LOWLAT_KEY_LEN);
    opener->stream_tag = stream_tag;
    chorus_lowlat_replay_init(&opener->window);
}

static chorus_lowlat_status_t open_inner(chorus_lowlat_opener_t *opener, const uint8_t *datagram,
                                         size_t len, chorus_lowlat_header_t *header, uint8_t *out,
                                         size_t out_cap, size_t *plaintext_len)
{
    chorus_lowlat_status_t status = chorus_lowlat_header_parse(datagram, len, header);
    if (status != CHORUS_LOWLAT_OK) {
        return status;
    }
    if (header->stream_tag != opener->stream_tag) {
        return CHORUS_LOWLAT_WRONG_STREAM_TAG;
    }
    if (!chorus_lowlat_replay_accepts(&opener->window, header->counter)) {
        return CHORUS_LOWLAT_REPLAYED;
    }
    status = chorus_lowlat_open(opener->key, datagram, len, header, out, out_cap, plaintext_len);
    if (status == CHORUS_LOWLAT_OK) {
        chorus_lowlat_replay_commit(&opener->window, header->counter);
    }
    return status;
}

chorus_lowlat_status_t chorus_lowlat_opener_open(chorus_lowlat_opener_t *opener,
                                                 const uint8_t *datagram, size_t len,
                                                 chorus_lowlat_header_t *header, uint8_t *out,
                                                 size_t out_cap, size_t *plaintext_len)
{
    chorus_lowlat_status_t status =
        open_inner(opener, datagram, len, header, out, out_cap, plaintext_len);
    switch (status) {
    case CHORUS_LOWLAT_OK:
        opener->stats.opened++;
        break;
    case CHORUS_LOWLAT_WRONG_STREAM_TAG:
        opener->stats.wrong_stream_tag++;
        break;
    case CHORUS_LOWLAT_REPLAYED:
        opener->stats.replayed++;
        break;
    case CHORUS_LOWLAT_AUTH_FAILED:
        opener->stats.auth_failed++;
        break;
    case CHORUS_LOWLAT_TOO_SHORT:
    case CHORUS_LOWLAT_TOO_LONG:
    case CHORUS_LOWLAT_BAD_MAGIC:
    case CHORUS_LOWLAT_BAD_VERSION:
    case CHORUS_LOWLAT_BAD_KIND:
        opener->stats.malformed++;
        break;
    default:
        /* A buffer or the crypto library: the caller's fault, not the
         * datagram's, and not counted against the stream. */
        break;
    }
    return status;
}

/* --- the reserved block and the FEC shape ------------------------------------- */

void chorus_lowlat_chunk_info_write(const chorus_lowlat_chunk_info_t *info,
                                    uint8_t reserved[CHORUS_LOWLAT_RESERVED_LEN])
{
    memset(reserved, 0, CHORUS_LOWLAT_RESERVED_LEN);
    reserved[0] = CHORUS_LOWLAT_LL_MARKER;
    reserved[1] = info->fec_k;
    reserved[2] = info->fec_depth;
    reserved[3] = info->group_index;
    put_u32(reserved + 4, info->group);
}

int chorus_lowlat_chunk_info_read(const uint8_t *payload, size_t len,
                                  chorus_lowlat_chunk_info_t *out)
{
    if (len < CHORUS_LOWLAT_RESERVED_OFFSET + CHORUS_LOWLAT_RESERVED_LEN) {
        return -1;
    }
    const uint8_t *r = payload + CHORUS_LOWLAT_RESERVED_OFFSET;
    if (r[0] != CHORUS_LOWLAT_LL_MARKER) {
        return -1;
    }
    for (size_t i = 8; i < CHORUS_LOWLAT_RESERVED_LEN; i++) {
        if (r[i] != 0) {
            return -1;
        }
    }
    out->fec_k = r[1];
    out->fec_depth = r[2];
    out->group_index = r[3];
    out->group = get_u32(r + 4);
    return 0;
}

chorus_lowlat_status_t chorus_lowlat_fec_params(uint8_t k, uint8_t depth,
                                                chorus_lowlat_fec_params_t *out)
{
    if (k != 0 && (k < 2 || k > CHORUS_LOWLAT_FEC_K_MAX)) {
        return CHORUS_LOWLAT_BAD_PARAMS;
    }
    if (depth < 1 || depth > CHORUS_LOWLAT_FEC_DEPTH_MAX) {
        return CHORUS_LOWLAT_BAD_PARAMS;
    }
    if (k == 0 && depth != 1) {
        return CHORUS_LOWLAT_BAD_PARAMS;
    }
    out->k = k;
    out->depth = depth;
    return CHORUS_LOWLAT_OK;
}

uint32_t chorus_lowlat_group_len(const chorus_lowlat_fec_params_t *params)
{
    return (params->k == 0) ? 1u : params->k;
}

static uint64_t block_len(const chorus_lowlat_fec_params_t *params)
{
    return (uint64_t)chorus_lowlat_group_len(params) * params->depth;
}

int chorus_lowlat_locate(const chorus_lowlat_fec_params_t *params, uint64_t n, uint32_t *group,
                         uint8_t *index)
{
    uint64_t block = n / block_len(params);
    uint64_t offset = n % block_len(params);
    uint64_t g = block * params->depth + offset % params->depth;
    if (g > UINT32_MAX) {
        return -1;
    }
    *group = (uint32_t)g;
    *index = (uint8_t)(offset / params->depth);
    return 0;
}

uint64_t chorus_lowlat_chunk_index(const chorus_lowlat_fec_params_t *params, uint32_t group,
                                   uint8_t index)
{
    uint64_t block = group / params->depth;
    uint64_t column = group % params->depth;
    return block * block_len(params) + column + (uint64_t)index * params->depth;
}

/* --- the encoder -------------------------------------------------------------- */

static void xor_into(uint8_t *acc, size_t *acc_len, const uint8_t *bytes, size_t len)
{
    if (*acc_len < len) {
        memset(acc + *acc_len, 0, len - *acc_len);
        *acc_len = len;
    }
    for (size_t i = 0; i < len; i++) {
        acc[i] ^= bytes[i];
    }
}

void chorus_lowlat_fec_encoder_init(chorus_lowlat_fec_encoder_t *encoder,
                                    const chorus_lowlat_fec_params_t *params)
{
    memset(encoder, 0, sizeof(*encoder));
    encoder->params = *params;
}

chorus_lowlat_status_t chorus_lowlat_fec_encode(chorus_lowlat_fec_encoder_t *encoder,
                                                uint8_t *payload, size_t len, uint8_t *parity,
                                                size_t parity_cap, size_t *parity_len)
{
    *parity_len = 0;
    if (len < CHORUS_LOWLAT_MIN_DATA_PLAINTEXT_LEN) {
        return CHORUS_LOWLAT_TOO_SHORT;
    }
    if (len > CHORUS_LOWLAT_MAX_DATA_PLAINTEXT_LEN) {
        return CHORUS_LOWLAT_TOO_LONG;
    }
    chorus_lowlat_chunk_info_t info;
    if (chorus_lowlat_locate(&encoder->params, encoder->next, &info.group, &info.group_index) !=
        0) {
        return CHORUS_LOWLAT_EXHAUSTED;
    }
    const chorus_lowlat_fec_params_t *p = &encoder->params;
    chorus_lowlat_column_t *col = &encoder->columns[info.group % p->depth];
    /* A parity that would not fit the caller's buffer is refused before the
     * chunk is numbered, so nothing changes. */
    size_t parity_needed = CHORUS_LOWLAT_PARITY_HEADER_LEN + (col->len > len ? col->len : len);
    if (p->k != 0 && col->count + 1 == p->k && parity_cap < parity_needed) {
        return CHORUS_LOWLAT_BUFFER_TOO_SMALL;
    }
    info.fec_k = p->k;
    info.fec_depth = p->depth;
    chorus_lowlat_chunk_info_write(&info, payload + CHORUS_LOWLAT_RESERVED_OFFSET);
    encoder->next++;
    if (p->k == 0) {
        return CHORUS_LOWLAT_OK;
    }
    xor_into(col->xor_bytes, &col->len, payload, len);
    col->len_xor ^= (uint16_t)len;
    col->count++;
    if (col->count == p->k) {
        put_u32(parity, info.group);
        parity[4] = p->k;
        parity[5] = p->depth;
        parity[6] = (uint8_t)(col->len_xor >> 8);
        parity[7] = (uint8_t)(col->len_xor & 0xFF);
        memcpy(parity + CHORUS_LOWLAT_PARITY_HEADER_LEN, col->xor_bytes, col->len);
        *parity_len = CHORUS_LOWLAT_PARITY_HEADER_LEN + col->len;
        memset(col, 0, sizeof(*col));
    }
    return CHORUS_LOWLAT_OK;
}

/* --- the decoder -------------------------------------------------------------- */

void chorus_lowlat_fec_decoder_init(chorus_lowlat_fec_decoder_t *decoder,
                                    const chorus_lowlat_fec_params_t *params)
{
    memset(decoder, 0, sizeof(*decoder));
    decoder->params = *params;
    decoder->slot_count = CHORUS_LOWLAT_OPEN_BLOCKS * params->depth;
}

static chorus_lowlat_slot_t *slot_of(chorus_lowlat_fec_decoder_t *d, uint32_t group)
{
    return &d->slots[group % d->slot_count];
}

static void close_through(chorus_lowlat_fec_decoder_t *d, uint32_t limit, uint64_t chunks_sent)
{
    if (limit <= d->next_close) {
        return;
    }
    uint32_t k = chorus_lowlat_group_len(&d->params);
    uint32_t held_end = d->next_close + d->slot_count;
    if (held_end < d->next_close) {
        held_end = UINT32_MAX;
    }
    if (held_end > limit) {
        held_end = limit;
    }
    for (uint32_t g = d->next_close; g < held_end; g++) {
        chorus_lowlat_slot_t *slot = slot_of(d, g);
        int held = slot->used && slot->group == g;
        uint32_t have = held ? slot->have : 0;
        for (uint32_t i = 0; i < k; i++) {
            if ((have & (1u << i)) == 0 &&
                chorus_lowlat_chunk_index(&d->params, g, (uint8_t)i) < chunks_sent) {
                d->stats.unrecoverable++;
            }
        }
        if (held) {
            memset(slot, 0, sizeof(*slot));
        }
    }
    /* A jump past every held slot: those groups had no packet at all. */
    if (limit > held_end) {
        d->stats.unrecoverable += (uint64_t)(limit - held_end) * k;
    }
    d->next_close = limit;
}

/* The one missing chunk of a group, once its parity and the other k - 1 are
 * in; never a guess: a rebuild that does not carry its own group's fields is
 * rejected. */
static void try_recover(chorus_lowlat_fec_decoder_t *d, uint32_t group,
                        chorus_lowlat_deliver_fn deliver, void *ctx)
{
    uint32_t k = chorus_lowlat_group_len(&d->params);
    chorus_lowlat_slot_t *slot = slot_of(d, group);
    uint32_t count = 0;
    for (uint32_t i = 0; i < k; i++) {
        count += (slot->have >> i) & 1u;
    }
    if (d->params.k == 0 || !slot->parity || slot->failed || count != k - 1) {
        return;
    }
    uint8_t missing = 0;
    while (slot->have & (1u << missing)) {
        missing++;
    }
    size_t len = slot->len_xor;
    chorus_lowlat_chunk_info_t info;
    int ok = len >= CHORUS_LOWLAT_MIN_DATA_PLAINTEXT_LEN && len <= slot->acc_len &&
             chorus_lowlat_chunk_info_read(slot->acc, len, &info) == 0 &&
             info.fec_k == d->params.k && info.fec_depth == d->params.depth &&
             info.group_index == missing && info.group == group;
    if (!ok) {
        slot->failed = 1;
        d->stats.rejected++;
        return;
    }
    slot->have |= 1u << missing;
    d->stats.recovered++;
    d->stats.delivered++;
    deliver(ctx, chorus_lowlat_chunk_index(&d->params, group, missing), group, missing, 1,
            slot->acc, len);
}

void chorus_lowlat_fec_decode(chorus_lowlat_fec_decoder_t *d, uint8_t kind,
                              const uint8_t *plaintext, size_t len,
                              chorus_lowlat_deliver_fn deliver, void *ctx)
{
    const chorus_lowlat_fec_params_t *p = &d->params;
    uint32_t group = 0;
    uint8_t index = 0;
    if (kind == CHORUS_LOWLAT_KIND_DATA) {
        chorus_lowlat_chunk_info_t info;
        if (len < CHORUS_LOWLAT_MIN_DATA_PLAINTEXT_LEN ||
            len > CHORUS_LOWLAT_MAX_DATA_PLAINTEXT_LEN ||
            chorus_lowlat_chunk_info_read(plaintext, len, &info) != 0 || info.fec_k != p->k ||
            info.fec_depth != p->depth || info.group_index >= chorus_lowlat_group_len(p)) {
            d->stats.rejected++;
            return;
        }
        group = info.group;
        index = info.group_index;
    } else if (kind == CHORUS_LOWLAT_KIND_PARITY) {
        if (p->k == 0 ||
            len < CHORUS_LOWLAT_PARITY_HEADER_LEN + CHORUS_LOWLAT_MIN_DATA_PLAINTEXT_LEN ||
            len > CHORUS_LOWLAT_MAX_PLAINTEXT_LEN || plaintext[4] != p->k ||
            plaintext[5] != p->depth) {
            d->stats.rejected++;
            return;
        }
        group = get_u32(plaintext);
    } else {
        d->stats.rejected++;
        return;
    }
    uint32_t block = group / p->depth;
    if (!d->started) {
        d->started = 1;
        d->next_close = block * p->depth;
        d->newest_block = block;
    }
    if (group < d->next_close) {
        d->stats.late++;
        return;
    }
    if (block > d->newest_block) {
        d->newest_block = block;
        if (block >= CHORUS_LOWLAT_OPEN_BLOCKS - 1) {
            uint64_t first_open = (uint64_t)(block - (CHORUS_LOWLAT_OPEN_BLOCKS - 1)) * p->depth;
            close_through(d, first_open > UINT32_MAX ? UINT32_MAX : (uint32_t)first_open,
                          UINT64_MAX);
        }
    }
    chorus_lowlat_slot_t *slot = slot_of(d, group);
    if (!slot->used || slot->group != group) {
        memset(slot, 0, sizeof(*slot));
        slot->used = 1;
        slot->group = group;
    }
    if (kind == CHORUS_LOWLAT_KIND_DATA) {
        uint32_t bit = 1u << index;
        if (slot->have & bit) {
            d->stats.duplicate++;
            return;
        }
        d->stats.data++;
        xor_into(slot->acc, &slot->acc_len, plaintext, len);
        slot->len_xor ^= (uint16_t)len;
        slot->have |= bit;
        d->stats.delivered++;
        deliver(ctx, chorus_lowlat_chunk_index(p, group, index), group, index, 0, plaintext, len);
    } else {
        if (slot->parity) {
            d->stats.duplicate++;
            return;
        }
        d->stats.parity++;
        xor_into(slot->acc, &slot->acc_len, plaintext + CHORUS_LOWLAT_PARITY_HEADER_LEN,
                 len - CHORUS_LOWLAT_PARITY_HEADER_LEN);
        slot->len_xor ^= (uint16_t)(((uint16_t)plaintext[6] << 8) | plaintext[7]);
        slot->parity = 1;
    }
    try_recover(d, group, deliver, ctx);
}

void chorus_lowlat_fec_expire_before(chorus_lowlat_fec_decoder_t *decoder, uint32_t group)
{
    if (decoder->started) {
        close_through(decoder, group, UINT64_MAX);
    }
}

void chorus_lowlat_fec_finish(chorus_lowlat_fec_decoder_t *decoder, uint64_t chunks_sent)
{
    if (!decoder->started || chunks_sent == 0) {
        return;
    }
    uint32_t last_group = 0;
    uint8_t index = 0;
    if (chorus_lowlat_locate(&decoder->params, chunks_sent - 1, &last_group, &index) != 0) {
        return;
    }
    uint64_t end = ((uint64_t)(last_group / decoder->params.depth) + 1) * decoder->params.depth;
    close_through(decoder, end > UINT32_MAX ? UINT32_MAX : (uint32_t)end, chunks_sent);
}
