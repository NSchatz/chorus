/* The endpoint's decoders against the shared fixtures in fixtures/codec.
 *
 * Each fixture is a stream as the wire carries it (NAME.fields: the
 * stream_format's fields and codec setup; NAME.chunks: per coded_chunk its
 * frames, the final range and the frame or packet) and its reference decode
 * (NAME.pcm). crates/client-linux/tests/codec_fixtures.rs reads the same files.
 *
 * FLAC is lossless, so the decode must equal the reference byte for byte and
 * its MD5 must be the STREAMINFO's. Opus is judged the way RFC 6716 section 6
 * and RFC 8251 judge a decoder: every packet's final range must equal the
 * vector's (bit-exact entropy decoding) and opus_compare must pass the decode
 * against the official reference decode. The decode must also be exactly the
 * one libopus 1.6.1 as chorus builds it gives (`decode_fnv1a64`), which is what
 * keeps this decoder and the Linux client's, the same code, from drifting.
 *
 * Usage: test_codec <opus_compare> [--bench]. CHORUS_CODEC_OUT names the
 * directory the Opus decodes are written to for opus_compare. --bench prints
 * the host decode cost per second of audio (docs/measurements/, source host). */

#include <dirent.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

#include "chorus/codec.h"
#include "harness.h"

#define MAX_CHUNKS 4096

typedef struct {
    char name[128];
    uint8_t codec;
    uint8_t sample_format;
    uint32_t rate;
    uint8_t channels;
    uint32_t frames_per_chunk;
    uint8_t config[64];
    size_t config_len;
    size_t chunks;
    uint64_t chunk_frames;
    uint64_t reference_frames;
    unsigned reference_channels;
    char md5[33];
    uint64_t fnv;
    /* NAME.chunks, parsed. */
    uint8_t *chunk_file;
    size_t chunk_file_len;
    uint32_t frames[MAX_CHUNKS];
    uint32_t range[MAX_CHUNKS];
    size_t offset[MAX_CHUNKS];
    uint32_t length[MAX_CHUNKS];
    /* NAME.pcm. */
    uint8_t *reference;
    size_t reference_len;
} fixture_t;

static uint8_t *slurp(const char *path, size_t *len)
{
    FILE *f = fopen(path, "rb");
    if (f == NULL) {
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    rewind(f);
    uint8_t *data = malloc(size > 0 ? (size_t)size : 1);
    *len = data != NULL ? fread(data, 1, (size_t)size, f) : 0;
    fclose(f);
    return data;
}

static uint32_t be32(const uint8_t *p)
{
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) | ((uint32_t)p[2] << 8) | p[3];
}

static uint64_t fnv1a64(const uint8_t *data, size_t len)
{
    uint64_t h = 0xcbf29ce484222325ull;
    for (size_t i = 0; i < len; i++) {
        h = (h ^ data[i]) * 0x100000001b3ull;
    }
    return h;
}

/* --- MD5 (RFC 1321), for the STREAMINFO check ------------------------------ */

typedef struct {
    uint32_t a, b, c, d;
    uint64_t len;
    uint8_t block[64];
    size_t used;
} md5_t;

static uint32_t rotl(uint32_t x, unsigned s)
{
    return (x << s) | (x >> (32 - s));
}

static void md5_block(md5_t *m, const uint8_t *p)
{
    /* RFC 1321 section 3.4: per-round shifts, and T[i] = floor(2^32 * |sin(i + 1)|). */
    static const unsigned shift[64] = {7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
                                       5, 9,  14, 20, 5, 9,  14, 20, 5, 9,  14, 20, 5, 9,  14, 20,
                                       4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
                                       6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21};
    static uint32_t t[64];
    if (t[0] == 0) {
        for (int i = 0; i < 64; i++) {
            t[i] = (uint32_t)(4294967296.0 * fabs(sin((double)(i + 1))));
        }
    }
    uint32_t x[16];
    for (int i = 0; i < 16; i++) {
        x[i] = (uint32_t)p[4 * i] | ((uint32_t)p[4 * i + 1] << 8) | ((uint32_t)p[4 * i + 2] << 16) |
               ((uint32_t)p[4 * i + 3] << 24);
    }
    uint32_t a = m->a, b = m->b, c = m->c, d = m->d;
    for (int i = 0; i < 64; i++) {
        uint32_t f;
        int g;
        if (i < 16) {
            f = (b & c) | (~b & d);
            g = i;
        } else if (i < 32) {
            f = (d & b) | (~d & c);
            g = (5 * i + 1) % 16;
        } else if (i < 48) {
            f = b ^ c ^ d;
            g = (3 * i + 5) % 16;
        } else {
            f = c ^ (b | ~d);
            g = (7 * i) % 16;
        }
        uint32_t next = d;
        d = c;
        c = b;
        b = b + rotl(a + f + t[i] + x[g], shift[i]);
        a = next;
    }
    m->a += a;
    m->b += b;
    m->c += c;
    m->d += d;
}

static void md5_hex(const uint8_t *data, size_t len, char out[33])
{
    md5_t m = {0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0, {0}, 0};
    for (size_t i = 0; i < len; i++) {
        m.block[m.used++] = data[i];
        if (m.used == 64) {
            md5_block(&m, m.block);
            m.used = 0;
        }
    }
    uint64_t bits = (uint64_t)len * 8;
    m.block[m.used++] = 0x80;
    if (m.used > 56) {
        memset(m.block + m.used, 0, 64 - m.used);
        md5_block(&m, m.block);
        m.used = 0;
    }
    memset(m.block + m.used, 0, 56 - m.used);
    for (int i = 0; i < 8; i++) {
        m.block[56 + i] = (uint8_t)(bits >> (8 * i));
    }
    md5_block(&m, m.block);
    uint32_t words[4] = {m.a, m.b, m.c, m.d};
    for (int i = 0; i < 16; i++) {
        snprintf(out + 2 * i, 3, "%02x", (unsigned)(uint8_t)(words[i / 4] >> (8 * (i % 4))));
    }
}

/* --- reading a fixture ------------------------------------------------------ */

static int hex_bytes(const char *hex, uint8_t *out, size_t cap, size_t *len)
{
    size_t n = strlen(hex) / 2;
    if (n > cap) {
        return -1;
    }
    for (size_t i = 0; i < n; i++) {
        unsigned v;
        if (sscanf(hex + 2 * i, "%2x", &v) != 1) {
            return -1;
        }
        out[i] = (uint8_t)v;
    }
    *len = n;
    return 0;
}

static int load(fixture_t *fx, const char *dir, const char *name)
{
    memset(fx, 0, sizeof(*fx));
    snprintf(fx->name, sizeof(fx->name), "%.127s", name);
    char path[1024];
    snprintf(path, sizeof(path), "%.800s/%.127s.fields", dir, name);
    FILE *f = fopen(path, "r");
    if (f == NULL) {
        return -1;
    }
    char line[1024];
    while (fgets(line, sizeof(line), f) != NULL) {
        char key[64], value[512];
        if (line[0] == '#' || sscanf(line, " %63s = %511s", key, value) != 2) {
            continue;
        }
        if (strcmp(key, "codec") == 0) {
            fx->codec = strcmp(value, "flac") == 0 ? CHORUS_CODEC_FLAC : CHORUS_CODEC_OPUS;
        } else if (strcmp(key, "sample_format") == 0) {
            fx->sample_format =
                strcmp(value, "pcm_s16le") == 0 ? CHORUS_CODEC_S16 : CHORUS_CODEC_S24;
        } else if (strcmp(key, "sample_rate_hz") == 0) {
            fx->rate = (uint32_t)strtoul(value, NULL, 10);
        } else if (strcmp(key, "channels") == 0) {
            fx->channels = (uint8_t)strtoul(value, NULL, 10);
        } else if (strcmp(key, "frames_per_chunk") == 0) {
            fx->frames_per_chunk = (uint32_t)strtoul(value, NULL, 10);
        } else if (strcmp(key, "codec_config") == 0) {
            hex_bytes(value, fx->config, sizeof(fx->config), &fx->config_len);
        } else if (strcmp(key, "chunks") == 0) {
            fx->chunks = strtoul(value, NULL, 10);
        } else if (strcmp(key, "chunk_frames") == 0) {
            fx->chunk_frames = strtoull(value, NULL, 10);
        } else if (strcmp(key, "reference_frames") == 0) {
            fx->reference_frames = strtoull(value, NULL, 10);
        } else if (strcmp(key, "reference_channels") == 0) {
            fx->reference_channels = (unsigned)strtoul(value, NULL, 10);
        } else if (strcmp(key, "reference_md5") == 0) {
            snprintf(fx->md5, sizeof(fx->md5), "%.32s", value);
        } else if (strcmp(key, "decode_fnv1a64") == 0) {
            fx->fnv = strtoull(value, NULL, 16);
        }
    }
    fclose(f);
    snprintf(path, sizeof(path), "%.800s/%.127s.chunks", dir, name);
    fx->chunk_file = slurp(path, &fx->chunk_file_len);
    snprintf(path, sizeof(path), "%.800s/%.127s.pcm", dir, name);
    fx->reference = slurp(path, &fx->reference_len);
    if (fx->chunk_file == NULL || fx->reference == NULL || fx->chunks > MAX_CHUNKS) {
        return -1;
    }
    size_t at = 0, n = 0;
    while (at + 12 <= fx->chunk_file_len && n < MAX_CHUNKS) {
        fx->frames[n] = be32(fx->chunk_file + at);
        fx->range[n] = be32(fx->chunk_file + at + 4);
        fx->length[n] = be32(fx->chunk_file + at + 8);
        fx->offset[n] = at + 12;
        at += 12 + fx->length[n];
        n++;
    }
    return (at == fx->chunk_file_len && n == fx->chunks) ? 0 : -1;
}

static void unload(fixture_t *fx)
{
    free(fx->chunk_file);
    free(fx->reference);
}

static chorus_codec_stream_t stream_of(const fixture_t *fx, uint8_t sample_format)
{
    chorus_codec_stream_t s = {fx->codec,    sample_format, fx->rate,
                               fx->channels, fx->config,    fx->config_len};
    return s;
}

/* Decode every chunk of `fx` at `sample_format`. Returns the PCM (caller frees)
 * and its length, and counts the Opus final ranges that matched. */
static uint8_t *decode_all(const fixture_t *fx, uint8_t sample_format, size_t *len,
                           size_t *ranges_ok, int *failed)
{
    chorus_codec_stream_t s = stream_of(fx, sample_format);
    chorus_decoder_t *d = NULL;
    char detail[256] = "";
    *len = 0;
    *ranges_ok = 0;
    *failed = 0;
    if (chorus_codec_open(&d, &s, detail, sizeof(detail)) != CHORUS_CODEC_OK) {
        printf("  %s: open failed: %s\n", fx->name, detail);
        *failed = 1;
        return NULL;
    }
    size_t bytes = chorus_codec_frame_bytes(d);
    size_t cap = (size_t)fx->chunk_frames * bytes + 1;
    uint8_t *out = malloc(cap);
    for (size_t i = 0; i < fx->chunks && out != NULL; i++) {
        uint32_t got = 0;
        chorus_codec_status_t st =
            chorus_codec_decode(d, fx->chunk_file + fx->offset[i], fx->length[i], fx->frames[i],
                                out + *len, cap - *len, &got, detail, sizeof(detail));
        if (st != CHORUS_CODEC_OK) {
            printf("  %s: chunk %zu: %s\n", fx->name, i, detail);
            *failed = 1;
            break;
        }
        *len += (size_t)got * bytes;
        if (chorus_codec_final_range(d) == fx->range[i]) {
            (*ranges_ok)++;
        }
    }
    chorus_codec_close(d);
    return out;
}

static int write_file(const char *path, const uint8_t *data, size_t len)
{
    FILE *f = fopen(path, "wb");
    if (f == NULL) {
        return -1;
    }
    size_t n = fwrite(data, 1, len, f);
    fclose(f);
    return n == len ? 0 : -1;
}

/* --- the checks ------------------------------------------------------------- */

static int check_flac(const fixture_t *fx)
{
    size_t len, ranges;
    int failed;
    uint8_t *pcm = decode_all(fx, fx->sample_format, &len, &ranges, &failed);
    int exact =
        !failed && pcm != NULL && len == fx->reference_len && memcmp(pcm, fx->reference, len) == 0;
    chorus_check(exact, "%s: %zu chunks decode to %zu bytes, bit-exact with flac -d's %zu",
                 fx->name, fx->chunks, len, fx->reference_len);
    char md5[33] = "";
    if (pcm != NULL) {
        md5_hex(pcm, len, md5);
    }
    char streaminfo_md5[33];
    for (int i = 0; i < 16; i++) {
        snprintf(streaminfo_md5 + 2 * i, 3, "%02x", fx->config[18 + i]);
    }
    int md5_ok = strcmp(md5, streaminfo_md5) == 0 && strcmp(md5, fx->md5) == 0;
    chorus_check(md5_ok, "%s: MD5 of the decode %s equals the STREAMINFO MD5 %s", fx->name, md5,
                 streaminfo_md5);
    free(pcm);
    return exact && md5_ok;
}

static int check_opus(const fixture_t *fx, const char *compare, const char *out_dir)
{
    size_t len, ranges;
    int failed;
    uint8_t *pcm = decode_all(fx, CHORUS_CODEC_S16, &len, &ranges, &failed);
    int ranges_ok = !failed && ranges == fx->chunks;
    chorus_check(ranges_ok, "%s: final range equal to the vector's in %zu of %zu packets", fx->name,
                 ranges, fx->chunks);
    size_t want = (size_t)fx->reference_frames * fx->channels * 2;
    uint64_t fnv = pcm != NULL ? fnv1a64(pcm, len) : 0;
    int exact = !failed && len == want && fnv == fx->fnv;
    chorus_check(exact,
                 "%s: %zu bytes after the pre-skip, FNV-1a %016llx (libopus 1.6.1 fixed "
                 "point: %016llx)",
                 fx->name, len, (unsigned long long)fnv, (unsigned long long)fx->fnv);

    /* opus_compare, libopus's own conformance judge, against the official decode. */
    char out_path[1024], ref_path[1024], command[4096];
    snprintf(out_path, sizeof(out_path), "%s/%s.pcm", out_dir, fx->name);
    snprintf(ref_path, sizeof(ref_path), "%s/fixtures/codec/%s.pcm", CHORUS_REPO_ROOT, fx->name);
    int quality = 0;
    if (pcm != NULL && write_file(out_path, pcm, len) == 0) {
        snprintf(command, sizeof(command), "'%s' %s '%s' '%s' 2>&1", compare,
                 fx->channels == 2 ? "-s" : "", ref_path, out_path);
        FILE *p = popen(command, "r");
        char line[256] = "", last[256] = "";
        while (p != NULL && fgets(line, sizeof(line), p) != NULL) {
            line[strcspn(line, "\n")] = '\0';
            if (strstr(line, "PASSES") != NULL) {
                quality = 1;
            }
            snprintf(last, sizeof(last), "%s", line);
        }
        int status = p != NULL ? pclose(p) : -1;
        quality = quality && status == 0;
        chorus_check(quality, "%s: opus_compare against the official decode: %s", fx->name, last);
    } else {
        chorus_check(0, "%s: the decode could not be written to %s", fx->name, out_path);
    }

    /* The 24-bit output is the same decode at full resolution: rounded to 16 bits
     * as libopus rounds (RES2INT16: add half, shift 8, saturate) it is the 16-bit
     * output exactly. */
    size_t len24, r24;
    int failed24;
    uint8_t *pcm24 = decode_all(fx, CHORUS_CODEC_S24, &len24, &r24, &failed24);
    int same = !failed24 && pcm != NULL && pcm24 != NULL && len24 / 3 == len / 2;
    for (size_t i = 0; same && i < len / 2; i++) {
        int32_t v = (int32_t)((uint32_t)pcm24[3 * i] | ((uint32_t)pcm24[3 * i + 1] << 8) |
                              ((uint32_t)pcm24[3 * i + 2] << 16));
        v = (v ^ 0x800000) - 0x800000;
        int32_t r = (v + 128) >> 8;
        r = r > 32767 ? 32767 : (r < -32768 ? -32768 : r);
        int16_t s = (int16_t)(uint16_t)(pcm[2 * i] | (pcm[2 * i + 1] << 8));
        same = r == s;
    }
    chorus_check(same, "%s: the pcm_s24le decode rounds to the pcm_s16le decode sample for sample",
                 fx->name);
    free(pcm);
    free(pcm24);
    return ranges_ok && exact && quality && same;
}

/* --- refusals and recovery --------------------------------------------------- */

static void check_refusals(const fixture_t *flac, const fixture_t *opus)
{
    chorus_section("refusals and recovery");
    char detail[256];
    chorus_decoder_t *d = NULL;

    chorus_codec_stream_t s = stream_of(flac, flac->sample_format);
    s.config_len = 33;
    chorus_check(chorus_codec_open(&d, &s, detail, sizeof(detail)) == CHORUS_CODEC_UNSUPPORTED &&
                     d == NULL,
                 "a 33-byte FLAC setup is refused: %s", detail);
    s = stream_of(flac, flac->sample_format);
    s.sample_rate_hz = flac->rate + 1;
    chorus_check(chorus_codec_open(&d, &s, detail, sizeof(detail)) == CHORUS_CODEC_UNSUPPORTED,
                 "a STREAMINFO rate that is not the stream's is refused: %s", detail);
    s = stream_of(flac, 3);
    chorus_check(chorus_codec_open(&d, &s, detail, sizeof(detail)) == CHORUS_CODEC_UNSUPPORTED,
                 "pcm_f32le output is refused: %s", detail);

    chorus_codec_stream_t o = stream_of(opus, CHORUS_CODEC_S16);
    uint8_t head[64];
    memcpy(head, opus->config, opus->config_len);
    head[18] = 1;
    o.config = head;
    chorus_check(chorus_codec_open(&d, &o, detail, sizeof(detail)) == CHORUS_CODEC_UNSUPPORTED,
                 "an Opus mapping family 1 stream is refused by name: %s", detail);
    o = stream_of(opus, CHORUS_CODEC_S16);
    o.sample_rate_hz = 44100;
    chorus_check(chorus_codec_open(&d, &o, detail, sizeof(detail)) == CHORUS_CODEC_UNSUPPORTED,
                 "an Opus stream at 44100 Hz is refused: %s", detail);

    /* A damaged FLAC frame is refused, and the next chunk decodes exactly as it
     * would have: frames carry no state from one to the next. */
    s = stream_of(flac, flac->sample_format);
    if (chorus_codec_open(&d, &s, detail, sizeof(detail)) != CHORUS_CODEC_OK) {
        chorus_check(0, "open for the recovery check: %s", detail);
        return;
    }
    size_t bytes = chorus_codec_frame_bytes(d);
    size_t cap = (size_t)chorus_codec_max_frames(d) * bytes;
    uint8_t *pcm = malloc(cap);
    uint8_t *bad = malloc(flac->length[1]);
    if (pcm == NULL || bad == NULL) {
        chorus_check(0, "memory for the recovery check");
        free(pcm);
        free(bad);
        chorus_codec_close(d);
        return;
    }
    memcpy(bad, flac->chunk_file + flac->offset[1], flac->length[1]);
    bad[flac->length[1] / 2] ^= 0x5a;
    uint32_t got = 0;
    chorus_codec_status_t st =
        chorus_codec_decode(d, flac->chunk_file + flac->offset[0], flac->length[0], flac->frames[0],
                            pcm, cap, &got, detail, sizeof(detail));
    st = st == CHORUS_CODEC_OK ? chorus_codec_decode(d, bad, flac->length[1], flac->frames[1], pcm,
                                                     cap, &got, detail, sizeof(detail))
                               : st;
    chorus_check(st == CHORUS_CODEC_BAD_CHUNK, "a FLAC frame with a flipped byte is refused: %s",
                 detail);
    st = chorus_codec_decode(d, flac->chunk_file + flac->offset[2], flac->length[2],
                             flac->frames[2], pcm, cap, &got, detail, sizeof(detail));
    size_t at = ((size_t)flac->frames[0] + flac->frames[1]) * bytes;
    chorus_check(st == CHORUS_CODEC_OK && got == flac->frames[2] &&
                     memcmp(pcm, flac->reference + at, (size_t)got * bytes) == 0,
                 "the frame after it decodes bit-exact (%u frames)", (unsigned)got);
    st = chorus_codec_decode(d, flac->chunk_file + flac->offset[3], flac->length[3],
                             flac->frames[3] - 1, pcm, cap, &got, detail, sizeof(detail));
    chorus_check(st == CHORUS_CODEC_BAD_CHUNK,
                 "a chunk whose `frames` is not what its frame holds is refused: %s", detail);
    st = chorus_codec_decode(d, flac->chunk_file + flac->offset[3], flac->length[3],
                             flac->frames[3], pcm, 16, &got, detail, sizeof(detail));
    chorus_check(st == CHORUS_CODEC_NO_ROOM, "a buffer too small is said so: %s", detail);
    free(bad);
    free(pcm);
    chorus_codec_close(d);
}

/* --- the host measurement ---------------------------------------------------- */

static double now_s(void)
{
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec + (double)t.tv_nsec / 1e9;
}

static void bench(const fixture_t *fx)
{
    double audio_s = (double)fx->chunk_frames / fx->rate;
    int rounds = (int)(20.0 / audio_s) + 1; /* about 20 s of audio per fixture */
    double best = 1e9;
    for (int rep = 0; rep < 5; rep++) {
        double start = now_s();
        for (int r = 0; r < rounds; r++) {
            size_t len, ranges;
            int failed;
            free(decode_all(fx, fx->sample_format, &len, &ranges, &failed));
        }
        double per = (now_s() - start) / (rounds * audio_s);
        best = per < best ? per : best;
    }
    printf("bench %s: %.3f ms of host CPU per second of audio (best of 5 runs of %.1f s of "
           "audio; %u Hz, %u channels)\n",
           fx->name, best * 1e3, rounds * audio_s, (unsigned)fx->rate, (unsigned)fx->channels);
}

static int by_name(const void *a, const void *b)
{
    return strcmp((const char *)a, (const char *)b);
}

int main(int argc, char **argv)
{
    if (argc < 2) {
        fprintf(stderr, "usage: test_codec <opus_compare> [--bench]\n");
        return 2;
    }
    int bench_only = argc > 2 && strcmp(argv[2], "--bench") == 0;
    const char *out_dir = getenv("CHORUS_CODEC_OUT");
    if (out_dir == NULL) {
        out_dir = "/tmp";
    }
    mkdir(out_dir, 0755);
    char dir[1024];
    chorus_repo_path(dir, sizeof(dir), "fixtures/codec");
    DIR *listing = opendir(dir);
    if (listing == NULL) {
        printf("FAIL fixtures/codec could not be read at %s\n", dir);
        return 1;
    }
    char names[32][128];
    size_t count = 0;
    struct dirent *e;
    while ((e = readdir(listing)) != NULL && count < 32) {
        size_t n = strlen(e->d_name);
        if (n > 7 && strcmp(e->d_name + n - 7, ".fields") == 0) {
            snprintf(names[count], sizeof(names[count]), "%.*s", (int)(n - 7), e->d_name);
            count++;
        }
    }
    closedir(listing);
    qsort(names, count, sizeof(names[0]), by_name);

    static fixture_t fixtures[32];
    int flac_total = 0, flac_ok = 0, opus_total = 0, opus_ok = 0;
    const fixture_t *a_flac = NULL, *an_opus = NULL;
    for (size_t i = 0; i < count; i++) {
        fixture_t *fx = &fixtures[i];
        int loaded = load(fx, dir, names[i]) == 0;
        chorus_check(loaded, "%s: fields, chunks and reference read", names[i]);
        if (!loaded) {
            continue;
        }
        if (bench_only) {
            bench(fx);
            continue;
        }
        chorus_section(fx->name);
        if (fx->codec == CHORUS_CODEC_FLAC) {
            flac_total++;
            flac_ok += check_flac(fx);
            a_flac = a_flac != NULL ? a_flac : fx;
        } else {
            opus_total++;
            opus_ok += check_opus(fx, argv[1], out_dir);
            an_opus = an_opus != NULL ? an_opus : fx;
        }
    }
    if (bench_only) {
        return 0;
    }
    chorus_check(flac_total >= 3 && opus_total >= 2, "fixtures/codec holds %d FLAC and %d Opus",
                 flac_total, opus_total);
    if (a_flac != NULL && an_opus != NULL) {
        check_refusals(a_flac, an_opus);
    }
    printf("\ncodec fixtures: flac %d of %d bit-exact and MD5, opus %d of %d final-range, exact "
           "and opus_compare pass\n",
           flac_ok, flac_total, opus_ok, opus_total);
    for (size_t i = 0; i < count; i++) {
        unload(&fixtures[i]);
    }
    return chorus_test_report("test_codec");
}
