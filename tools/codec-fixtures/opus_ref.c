/* The reference Opus decode for fixtures/codec: libopus 1.6.1 as chorus builds
 * it (third_party/opus/chorus-build.txt), called directly, without the
 * endpoint's seam in between, over an RFC 6716 / RFC 8251 test vector.
 *
 * A vector (.bit) is a run of packets, each a 32-bit big-endian length, the
 * encoder's 32-bit big-endian final range, then the packet (the layout
 * libopus's opus_demo writes and reads, and the one the official vectors use).
 *
 * Usage: opus_ref <vector.bit> <channels> <packets or 0 for all> <skip> <out.pcm>
 * Decodes that many packets to 16-bit PCM at 48 kHz, drops the first `skip`
 * frames, writes the rest, and exits non-zero if any packet's final range
 * differs from the vector's. Prints the packet count, the frames written, and
 * the FNV-1a 64 of the bytes written. */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include "opus.h"

static uint32_t be32(const unsigned char *p)
{
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) | ((uint32_t)p[2] << 8) | p[3];
}

int main(int argc, char **argv)
{
    if (argc != 6) {
        fprintf(stderr, "usage: opus_ref <vector.bit> <channels> <packets|0> <skip> <out.pcm>\n");
        return 2;
    }
    FILE *in = fopen(argv[1], "rb");
    FILE *out = fopen(argv[5], "wb");
    if (in == NULL || out == NULL) {
        fprintf(stderr, "opus_ref: cannot open %s or %s\n", argv[1], argv[5]);
        return 2;
    }
    int channels = atoi(argv[2]);
    long want = atol(argv[3]);
    long skip = atol(argv[4]);
    int error = 0;
    OpusDecoder *dec = opus_decoder_create(48000, channels, &error);
    if (dec == NULL) {
        fprintf(stderr, "opus_ref: %s\n", opus_strerror(error));
        return 2;
    }
    static unsigned char packet[1 << 16];
    static opus_int16 pcm[5760 * 2];
    uint64_t hash = 0xcbf29ce484222325ull;
    long packets = 0, written = 0, mismatches = 0;
    unsigned char head[8];
    while ((want == 0 || packets < want) && fread(head, 1, 8, in) == 8) {
        uint32_t len = be32(head);
        uint32_t range = be32(head + 4);
        if (len > sizeof(packet) || fread(packet, 1, len, in) != len) {
            fprintf(stderr, "opus_ref: truncated vector at packet %ld\n", packets);
            return 2;
        }
        int n = opus_decode(dec, packet, (opus_int32)len, pcm, 5760, 0);
        if (n < 0) {
            fprintf(stderr, "opus_ref: packet %ld: %s\n", packets, opus_strerror(n));
            return 1;
        }
        opus_uint32 got = 0;
        opus_decoder_ctl(dec, OPUS_GET_FINAL_RANGE(&got));
        if (got != range) {
            mismatches++;
        }
        for (long f = 0; f < n; f++) {
            if (skip > 0) {
                skip--;
                continue;
            }
            for (int c = 0; c < channels; c++) {
                uint16_t v = (uint16_t)pcm[f * channels + c];
                unsigned char b[2] = {(unsigned char)v, (unsigned char)(v >> 8)};
                fwrite(b, 1, 2, out);
                for (int k = 0; k < 2; k++) {
                    hash = (hash ^ b[k]) * 0x100000001b3ull;
                }
            }
            written++;
        }
        packets++;
    }
    fclose(out);
    fclose(in);
    opus_decoder_destroy(dec);
    printf("packets=%ld frames=%ld final_range_mismatches=%ld fnv1a64=%016llx\n", packets, written,
           mismatches, (unsigned long long)hash);
    return mismatches == 0 ? 0 : 1;
}
