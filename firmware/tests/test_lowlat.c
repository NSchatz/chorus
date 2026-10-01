/* The endpoint's low-latency datagram layer, held to the shared vectors.
 *
 * fixtures/protocol/lowlat/ is read here and by the Rust implementation's
 * crates/protocol/tests/lowlat.rs, so the two cannot drift apart. This suite
 * WALKS that directory:
 *
 *   - every stream_*.fields: the chunks pushed through the FEC encoder and
 *     sealed (through PSA, as the image seals) give the committed plaintexts
 *     and datagrams byte for byte;
 *   - every case_*.fields: the committed datagrams dropped, reordered,
 *     repeated, altered or opened under another tag give exactly the chunks,
 *     the rebuilds and the counts the case names, and every chunk handed on
 *     is byte for byte the one that was sent.
 *
 * Then the rules the vectors do not spell out: the FEC parameters an offer
 * may carry, the replay window at its edges, and the header's checks in
 * their order. The vectors are READ and never written. */

#include "chorus/lowlat.h"
#include "fixture_text.h"
#include "harness.h"

#include <dirent.h>
#include <stdlib.h>

#define TEXT_CAP 65536
#define VALUE_CAP 4096
#define MAX_DATAGRAMS 64
#define MAX_CHUNKS 64
#define MAX_STEMS 64

typedef struct {
    uint8_t key[CHORUS_LOWLAT_KEY_LEN];
    uint32_t stream_tag;
    uint64_t first_counter;
    chorus_lowlat_fec_params_t params;
    size_t chunk_count;
    uint8_t chunks[MAX_CHUNKS][CHORUS_LOWLAT_MAX_DATA_PLAINTEXT_LEN];
    size_t chunk_len[MAX_CHUNKS];
    size_t datagram_count;
    uint8_t plaintexts[MAX_DATAGRAMS][CHORUS_LOWLAT_MAX_PLAINTEXT_LEN];
    size_t plaintext_len[MAX_DATAGRAMS];
    uint8_t datagrams[MAX_DATAGRAMS][CHORUS_LOWLAT_MAX_DATAGRAM_LEN];
    size_t datagram_len[MAX_DATAGRAMS];
    /* The data plaintexts, in chunk order: what each chunk was sent as. */
    size_t sent_of_chunk[MAX_CHUNKS];
} stream_t;

static char text[TEXT_CAP];

static int read_fields(const char *stem)
{
    char relative[256];
    char path[512];
    snprintf(relative, sizeof(relative), "fixtures/protocol/lowlat/%.200s.fields", stem);
    chorus_repo_path(path, sizeof(path), relative);
    return fixture_read(path, text, sizeof(text)) >= 0;
}

static int field_u64(const char *key, uint64_t *out)
{
    char value[64];
    if (fixture_field(text, key, value, sizeof(value)) == NULL) {
        return 0;
    }
    char *end = NULL;
    *out = strtoull(value, &end, 10);
    return end != value && *end == '\0';
}

static long field_bytes(const char *key, uint8_t *out, size_t cap)
{
    static char value[VALUE_CAP];
    if (fixture_field(text, key, value, sizeof(value)) == NULL) {
        return -1;
    }
    return fixture_unhex(value, out, cap);
}

/* Space separated numbers; returns the count, or -1 when the key is absent. */
static long field_list(const char *key, uint64_t *out, size_t cap)
{
    static char value[VALUE_CAP];
    if (fixture_field(text, key, value, sizeof(value)) == NULL) {
        return -1;
    }
    long n = 0;
    char *p = value;
    while (*p != '\0' && (size_t)n < cap) {
        char *end = NULL;
        unsigned long long v = strtoull(p, &end, 10);
        if (end == p) {
            break;
        }
        out[n++] = v;
        p = end;
    }
    return n;
}

static int load_stream(const char *stem, stream_t *s)
{
    if (!read_fields(stem)) {
        return 0;
    }
    uint64_t tag = 0;
    uint64_t k = 0;
    uint64_t depth = 0;
    uint64_t chunks = 0;
    uint64_t datagrams = 0;
    if (field_bytes("key", s->key, sizeof(s->key)) != CHORUS_LOWLAT_KEY_LEN ||
        !field_u64("stream_tag", &tag) || !field_u64("first_counter", &s->first_counter) ||
        !field_u64("fec_k", &k) || !field_u64("fec_depth", &depth) ||
        !field_u64("chunks", &chunks) || !field_u64("datagrams", &datagrams) ||
        chunks > MAX_CHUNKS || datagrams > MAX_DATAGRAMS ||
        chorus_lowlat_fec_params((uint8_t)k, (uint8_t)depth, &s->params) != CHORUS_LOWLAT_OK) {
        return 0;
    }
    s->stream_tag = (uint32_t)tag;
    s->chunk_count = (size_t)chunks;
    s->datagram_count = (size_t)datagrams;
    char key[64];
    for (size_t i = 0; i < s->chunk_count; i++) {
        snprintf(key, sizeof(key), "chunk.%zu", i);
        long n = field_bytes(key, s->chunks[i], sizeof(s->chunks[i]));
        if (n < 0) {
            return 0;
        }
        s->chunk_len[i] = (size_t)n;
    }
    size_t data = 0;
    for (size_t j = 0; j < s->datagram_count; j++) {
        snprintf(key, sizeof(key), "plaintext.%zu", j);
        long n = field_bytes(key, s->plaintexts[j], sizeof(s->plaintexts[j]));
        snprintf(key, sizeof(key), "datagram.%zu", j);
        long m = field_bytes(key, s->datagrams[j], sizeof(s->datagrams[j]));
        if (n < 0 || m < 4) {
            return 0;
        }
        s->plaintext_len[j] = (size_t)n;
        s->datagram_len[j] = (size_t)m;
        if (s->datagrams[j][3] == CHORUS_LOWLAT_KIND_DATA && data < MAX_CHUNKS) {
            s->sent_of_chunk[data++] = j;
        }
    }
    return data == s->chunk_count;
}

static int compare_stems(const void *a, const void *b)
{
    return strcmp((const char *)a, (const char *)b);
}

static size_t list_stems(const char *prefix, char stems[][128])
{
    char dir_path[512];
    chorus_repo_path(dir_path, sizeof(dir_path), "fixtures/protocol/lowlat");
    DIR *dir = opendir(dir_path);
    chorus_check(dir != NULL, "fixtures/protocol/lowlat/ is readable");
    if (dir == NULL) {
        return 0;
    }
    size_t n = 0;
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        const char *dot = strrchr(entry->d_name, '.');
        if (entry->d_name[0] == '.') {
            continue;
        }
        chorus_check(dot != NULL && strcmp(dot, ".fields") == 0,
                     "%s: only .fields files live in fixtures/protocol/lowlat", entry->d_name);
        if (dot == NULL || strncmp(entry->d_name, prefix, strlen(prefix)) != 0) {
            continue;
        }
        size_t len = (size_t)(dot - entry->d_name);
        if (n >= MAX_STEMS || len >= 128) {
            chorus_check(0, "more vectors than this suite has room for");
            break;
        }
        memcpy(stems[n], entry->d_name, len);
        stems[n][len] = '\0';
        n++;
    }
    closedir(dir);
    qsort(stems, n, sizeof(stems[0]), compare_stems);
    return n;
}

static stream_t stream;

static void the_encoder_and_the_sealer_reproduce_every_stream(void)
{
    chorus_section("fixtures/protocol/lowlat: every stream, encoded and sealed byte for byte");
    static char stems[MAX_STEMS][128];
    size_t count = list_stems("stream_", stems);
    int passed = 0;
    size_t total = 0;
    for (size_t i = 0; i < count; i++) {
        if (!load_stream(stems[i], &stream)) {
            chorus_check(0, "%s.fields reads as a stream", stems[i]);
            continue;
        }
        static chorus_lowlat_fec_encoder_t encoder;
        chorus_lowlat_fec_encoder_init(&encoder, &stream.params);
        chorus_lowlat_sealer_t sealer;
        chorus_check(chorus_lowlat_sealer_init(&sealer, stream.key, stream.stream_tag,
                                               stream.first_counter) == CHORUS_LOWLAT_OK,
                     "%s: the sealer takes its key and tag", stems[i]);
        size_t j = 0;
        int same = 1;
        for (size_t c = 0; c < stream.chunk_count; c++) {
            static uint8_t payload[CHORUS_LOWLAT_MAX_DATA_PLAINTEXT_LEN];
            static uint8_t parity[CHORUS_LOWLAT_MAX_PLAINTEXT_LEN];
            memcpy(payload, stream.chunks[c], stream.chunk_len[c]);
            size_t parity_len = 0;
            if (chorus_lowlat_fec_encode(&encoder, payload, stream.chunk_len[c], parity,
                                         sizeof(parity), &parity_len) != CHORUS_LOWLAT_OK) {
                same = 0;
                break;
            }
            const uint8_t *out[2] = {payload, parity};
            size_t out_len[2] = {stream.chunk_len[c], parity_len};
            uint8_t kinds[2] = {CHORUS_LOWLAT_KIND_DATA, CHORUS_LOWLAT_KIND_PARITY};
            for (int d = 0; d < 2 && same; d++) {
                if (d == 1 && parity_len == 0) {
                    break;
                }
                uint8_t sealed[CHORUS_LOWLAT_MAX_DATAGRAM_LEN];
                size_t sealed_len = 0;
                int ok =
                    j < stream.datagram_count && out_len[d] == stream.plaintext_len[j] &&
                    memcmp(out[d], stream.plaintexts[j], out_len[d]) == 0 &&
                    chorus_lowlat_sealer_seal(&sealer, kinds[d], out[d], out_len[d], sealed,
                                              sizeof(sealed), &sealed_len) == CHORUS_LOWLAT_OK &&
                    sealed_len == stream.datagram_len[j] &&
                    memcmp(sealed, stream.datagrams[j], sealed_len) == 0;
                chorus_check(ok, "%s: datagram.%zu is the committed plaintext and bytes", stems[i],
                             j);
                same &= ok;
                j++;
            }
        }
        same &= j == stream.datagram_count;
        chorus_check(same, "%s: %zu chunks give the committed %zu datagrams exactly", stems[i],
                     stream.chunk_count, stream.datagram_count);
        passed += same;
        total += j;
    }
    printf("\nlowlat streams: %d of %zu byte for byte (%zu datagrams)\n", passed, count, total);
    chorus_check(passed == (int)count && count >= 3, "every committed stream is reproduced");
}

typedef struct {
    const stream_t *s;
    uint64_t delivered[MAX_CHUNKS * 2];
    size_t delivered_count;
    uint64_t recovered[MAX_CHUNKS];
    size_t recovered_count;
    int bytes_ok;
} collected_t;

static void collect(void *ctx, uint64_t chunk_index, uint32_t group, uint8_t group_index,
                    int recovered, const uint8_t *payload, size_t len)
{
    (void)group;
    (void)group_index;
    collected_t *c = (collected_t *)ctx;
    size_t j = (chunk_index < c->s->chunk_count) ? c->s->sent_of_chunk[chunk_index] : 0;
    if (chunk_index >= c->s->chunk_count || len != c->s->plaintext_len[j] ||
        memcmp(payload, c->s->plaintexts[j], len) != 0) {
        c->bytes_ok = 0;
    }
    if (c->delivered_count < MAX_CHUNKS * 2) {
        c->delivered[c->delivered_count++] = chunk_index;
    }
    if (recovered && c->recovered_count < MAX_CHUNKS) {
        c->recovered[c->recovered_count++] = chunk_index;
    }
}

static int compare_u64(const void *a, const void *b)
{
    uint64_t x = *(const uint64_t *)a;
    uint64_t y = *(const uint64_t *)b;
    return (x > y) - (x < y);
}

static int same_list(const uint64_t *a, size_t n, const uint64_t *b, long m)
{
    if (m < 0 || (size_t)m != n) {
        return 0;
    }
    for (size_t i = 0; i < n; i++) {
        if (a[i] != b[i]) {
            return 0;
        }
    }
    return 1;
}

static void every_case_delivers_rebuilds_and_counts_as_committed(void)
{
    chorus_section("fixtures/protocol/lowlat: every case, delivered and counted as committed");
    static char stems[MAX_STEMS][128];
    size_t count = list_stems("case_", stems);
    int passed = 0;
    for (size_t i = 0; i < count; i++) {
        char stream_stem[128];
        static char case_text[TEXT_CAP];
        if (!read_fields(stems[i]) ||
            fixture_field(text, "stream", stream_stem, sizeof(stream_stem)) == NULL) {
            chorus_check(0, "%s.fields names its stream", stems[i]);
            continue;
        }
        memcpy(case_text, text, sizeof(case_text));
        if (!load_stream(stream_stem, &stream)) {
            chorus_check(0, "%s: its stream %s loads", stems[i], stream_stem);
            continue;
        }
        memcpy(text, case_text, sizeof(text));
        uint64_t tag = stream.stream_tag;
        uint64_t named_tag = 0;
        if (field_u64("receiver_stream_tag", &named_tag)) {
            tag = named_tag;
        }
        uint64_t tamper[3] = {0, 0, 0};
        long tampered = field_list("tamper", tamper, 3);
        uint64_t deliver[MAX_DATAGRAMS * 2];
        long deliveries = field_list("deliver", deliver, MAX_DATAGRAMS * 2);
        uint64_t want_delivered[MAX_CHUNKS * 2];
        long want_delivered_n = field_list("expect_delivered", want_delivered, MAX_CHUNKS * 2);
        uint64_t want_recovered[MAX_CHUNKS];
        long want_recovered_n = field_list("expect_recovered", want_recovered, MAX_CHUNKS);
        static const char *const COUNTS[] = {"opened",   "malformed",   "wrong_stream_tag",
                                             "replayed", "auth_failed", "unrecoverable",
                                             "duplicate"};
        uint64_t want[7];
        int have_all = deliveries >= 0;
        for (size_t c = 0; c < 7; c++) {
            char key[64];
            snprintf(key, sizeof(key), "expect_%s", COUNTS[c]);
            have_all &= field_u64(key, &want[c]);
        }
        if (!have_all) {
            chorus_check(0, "%s.fields names deliver and every expectation", stems[i]);
            continue;
        }

        static chorus_lowlat_opener_t opener;
        chorus_lowlat_opener_init(&opener, stream.key, (uint32_t)tag);
        static chorus_lowlat_fec_decoder_t decoder;
        chorus_lowlat_fec_decoder_init(&decoder, &stream.params);
        static collected_t got;
        memset(&got, 0, sizeof(got));
        got.s = &stream;
        got.bytes_ok = 1;
        for (long d = 0; d < deliveries; d++) {
            size_t j = (size_t)deliver[d];
            if (j >= stream.datagram_count) {
                chorus_check(0, "%s: deliver names datagram %zu of %zu", stems[i], j,
                             stream.datagram_count);
                continue;
            }
            uint8_t bytes[CHORUS_LOWLAT_MAX_DATAGRAM_LEN];
            memcpy(bytes, stream.datagrams[j], stream.datagram_len[j]);
            if (tampered == 3 && tamper[0] == j && tamper[1] < stream.datagram_len[j]) {
                bytes[tamper[1]] ^= (uint8_t)tamper[2];
            }
            chorus_lowlat_header_t header;
            static uint8_t plain[CHORUS_LOWLAT_MAX_PLAINTEXT_LEN];
            size_t plain_len = 0;
            if (chorus_lowlat_opener_open(&opener, bytes, stream.datagram_len[j], &header, plain,
                                          sizeof(plain), &plain_len) == CHORUS_LOWLAT_OK) {
                chorus_lowlat_fec_decode(&decoder, header.kind, plain, plain_len, collect, &got);
            }
        }
        chorus_lowlat_fec_finish(&decoder, stream.chunk_count);
        qsort(got.delivered, got.delivered_count, sizeof(uint64_t), compare_u64);
        uint64_t counts[7] = {opener.stats.opened,           opener.stats.malformed,
                              opener.stats.wrong_stream_tag, opener.stats.replayed,
                              opener.stats.auth_failed,      decoder.stats.unrecoverable,
                              decoder.stats.duplicate};
        int ok = got.bytes_ok;
        chorus_check(got.bytes_ok, "%s: every chunk handed on is the chunk that was sent",
                     stems[i]);
        int lists =
            same_list(got.delivered, got.delivered_count, want_delivered, want_delivered_n) &&
            same_list(got.recovered, got.recovered_count, want_recovered, want_recovered_n);
        chorus_check(lists, "%s: %zu chunks handed on, %zu rebuilt, as committed", stems[i],
                     got.delivered_count, got.recovered_count);
        ok &= lists;
        for (size_t c = 0; c < 7; c++) {
            chorus_check(counts[c] == want[c], "%s: %s = %llu (committed %llu)", stems[i],
                         COUNTS[c], (unsigned long long)counts[c], (unsigned long long)want[c]);
            ok &= counts[c] == want[c];
        }
        ok &= decoder.stats.late == 0 && decoder.stats.rejected == 0;
        passed += ok;
    }
    printf("\nlowlat cases: %d of %zu as committed\n", passed, count);
    chorus_check(passed == (int)count && count >= 10, "every committed case holds");
}

static void the_offers_fec_rules_hold(void)
{
    chorus_section("the FEC parameters an offer may carry");
    chorus_lowlat_fec_params_t p;
    chorus_check(chorus_lowlat_fec_params(0, 1, &p) == CHORUS_LOWLAT_OK, "no FEC");
    chorus_check(chorus_lowlat_fec_params(4, 1, &p) == CHORUS_LOWLAT_OK, "the defaults");
    chorus_check(chorus_lowlat_fec_params(16, 8, &p) == CHORUS_LOWLAT_OK, "the largest");
    chorus_check(chorus_lowlat_fec_params(1, 1, &p) == CHORUS_LOWLAT_BAD_PARAMS, "k 1 refused");
    chorus_check(chorus_lowlat_fec_params(17, 1, &p) == CHORUS_LOWLAT_BAD_PARAMS, "k 17 refused");
    chorus_check(chorus_lowlat_fec_params(4, 0, &p) == CHORUS_LOWLAT_BAD_PARAMS, "depth 0 refused");
    chorus_check(chorus_lowlat_fec_params(4, 9, &p) == CHORUS_LOWLAT_BAD_PARAMS, "depth 9 refused");
    chorus_check(chorus_lowlat_fec_params(0, 2, &p) == CHORUS_LOWLAT_BAD_PARAMS,
                 "no FEC with an interleave refused");
    /* Every chunk number maps to a group and back. */
    chorus_lowlat_fec_params(3, 4, &p);
    int round_trip = 1;
    for (uint64_t n = 0; n < 1000; n++) {
        uint32_t g = 0;
        uint8_t idx = 0;
        round_trip &= chorus_lowlat_locate(&p, n, &g, &idx) == 0 && idx < 3 &&
                      chorus_lowlat_chunk_index(&p, g, idx) == n;
    }
    chorus_check(round_trip, "chunk number to (group, position) and back, k 3 depth 4");
}

static void the_replay_window_holds_at_its_edges(void)
{
    chorus_section("the replay window (RFC 4303 section 3.4.3), 1024 wide");
    chorus_lowlat_replay_t w;
    chorus_lowlat_replay_init(&w);
    const uint64_t first[] = {5, 3, 9, 4, 2000, 1000};
    int ok = 1;
    for (size_t i = 0; i < sizeof(first) / sizeof(first[0]); i++) {
        ok &= chorus_lowlat_replay_accepts(&w, first[i]);
        chorus_lowlat_replay_commit(&w, first[i]);
    }
    chorus_check(ok, "new counters, in and out of order, are accepted");
    const uint64_t again[] = {5, 3, 9, 2000, 1000, 976};
    ok = 1;
    for (size_t i = 0; i < sizeof(again) / sizeof(again[0]); i++) {
        ok &= !chorus_lowlat_replay_accepts(&w, again[i]);
    }
    chorus_check(ok, "a repeat, and a counter older than the window, are refused");
    chorus_check(chorus_lowlat_replay_accepts(&w, 977) && chorus_lowlat_replay_accepts(&w, 1999),
                 "the oldest counter still inside the window, and an unseen one, are accepted");
    chorus_lowlat_replay_commit(&w, 5000);
    chorus_check(!chorus_lowlat_replay_accepts(&w, 1999) && chorus_lowlat_replay_accepts(&w, 4000),
                 "a jump past the window forgets what fell out of it");
}

static void the_header_is_checked_in_order(void)
{
    chorus_section("the datagram header's checks, in order");
    uint8_t d[CHORUS_LOWLAT_HEADER_LEN + CHORUS_LOWLAT_TAG_LEN + 4];
    memset(d, 0, sizeof(d));
    chorus_lowlat_header_t h = {CHORUS_LOWLAT_KIND_DATA, 9, 3};
    chorus_lowlat_header_write(&h, d);
    chorus_lowlat_header_t got;
    chorus_check(chorus_lowlat_header_parse(d, 31, &got) == CHORUS_LOWLAT_TOO_SHORT,
                 "31 bytes are too short");
    chorus_check(chorus_lowlat_header_parse(d, sizeof(d), &got) == CHORUS_LOWLAT_OK &&
                     got.kind == h.kind && got.stream_tag == 9 && got.counter == 3,
                 "a header reads back");
    d[0] = 0;
    chorus_check(chorus_lowlat_header_parse(d, sizeof(d), &got) == CHORUS_LOWLAT_BAD_MAGIC,
                 "a bad magic");
    d[0] = 0x43;
    d[2] = 2;
    chorus_check(chorus_lowlat_header_parse(d, sizeof(d), &got) == CHORUS_LOWLAT_BAD_VERSION,
                 "another version");
    d[2] = 1;
    d[3] = 3;
    chorus_check(chorus_lowlat_header_parse(d, sizeof(d), &got) == CHORUS_LOWLAT_BAD_KIND,
                 "an undefined kind");
    chorus_check(chorus_lowlat_header_parse(d, CHORUS_LOWLAT_MAX_DATAGRAM_LEN + 1, &got) ==
                     CHORUS_LOWLAT_TOO_LONG,
                 "a datagram past 1472 bytes");
}

int main(void)
{
    the_encoder_and_the_sealer_reproduce_every_stream();
    every_case_delivers_rebuilds_and_counts_as_committed();
    the_offers_fec_rules_hold();
    the_replay_window_holds_at_its_edges();
    the_header_is_checked_in_order();
    return chorus_test_report("test_lowlat");
}
