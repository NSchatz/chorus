/* The reference Opus decode for fixtures/decode: libopus 1.6.1 as chorus builds
 * it (third_party/opus/chorus-build.txt), called directly over the packets of an
 * Ogg Opus file that tools/decode-fixtures/generate.py took out of the Ogg pages
 * itself (its own page reader, not chorus-decode's and not Symphonia's).
 *
 * Usage: opus_pkt <in.pkts> <channels> <out.s24>
 * <in.pkts> is a run of packets, each a 32-bit big-endian length and then the
 * packet. Every packet is decoded with opus_decode24 at 48 kHz, with no output
 * gain (the generator refuses a file whose OpusHead carries one), and every
 * decoded sample is written as a 32-bit little-endian integer holding a 24-bit
 * value. Pre-skip and end trimming are the
 * caller's (RFC 7845 section 4). Prints the packet and frame counts. */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include "opus.h"

int main(int argc, char **argv)
{
    if (argc != 4) {
        fprintf(stderr, "usage: opus_pkt <in.pkts> <channels> <out.s24>\n");
        return 2;
    }
    FILE *in = fopen(argv[1], "rb");
    FILE *out = fopen(argv[3], "wb");
    if (in == NULL || out == NULL) {
        fprintf(stderr, "opus_pkt: cannot open %s or %s\n", argv[1], argv[3]);
        return 2;
    }
    int channels = atoi(argv[2]);
    int error = 0;
    OpusDecoder *dec = opus_decoder_create(48000, channels, &error);
    if (dec == NULL) {
        fprintf(stderr, "opus_pkt: %s\n", opus_strerror(error));
        return 2;
    }
    static unsigned char packet[1 << 16];
    static opus_int32 pcm[5760 * 2];
    long packets = 0, frames = 0;
    unsigned char head[4];
    while (fread(head, 1, 4, in) == 4) {
        uint32_t len = ((uint32_t)head[0] << 24) | ((uint32_t)head[1] << 16) |
                       ((uint32_t)head[2] << 8) | head[3];
        if (len > sizeof(packet) || fread(packet, 1, len, in) != len) {
            fprintf(stderr, "opus_pkt: truncated input at packet %ld\n", packets);
            return 2;
        }
        int n = opus_decode24(dec, packet, (opus_int32)len, pcm, 5760, 0);
        if (n < 0) {
            fprintf(stderr, "opus_pkt: packet %ld: %s\n", packets, opus_strerror(n));
            return 1;
        }
        for (long i = 0; i < (long)n * channels; i++) {
            uint32_t v = (uint32_t)pcm[i];
            unsigned char b[4] = {(unsigned char)v, (unsigned char)(v >> 8),
                                  (unsigned char)(v >> 16), (unsigned char)(v >> 24)};
            fwrite(b, 1, 4, out);
        }
        frames += n;
        packets++;
    }
    fclose(out);
    fclose(in);
    opus_decoder_destroy(dec);
    printf("packets=%ld frames=%ld\n", packets, frames);
    return 0;
}
