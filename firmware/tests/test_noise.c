/* The endpoint's key exchange, held to the published vector and to chorus's
 * own session vectors.
 *
 * docs/protocol.md: "The implementation is held to cacophony's published
 * vector for this protocol name (fixtures/protocol/v2/noise/cacophony_xx.fields)
 * in both roles." Here that is literal: an initiator and a responder built
 * from the vector's keys each WRITE their own messages, which must equal the
 * vector's ciphertexts byte for byte, and each READS the other's, recovering
 * the vector's payloads; both arrive at the vector's handshake hash, and the
 * three transport messages after Split() are reproduced in their directions.
 *
 * Then chorus's own: the handshake_init, handshake_response, handshake_finish
 * and secure_record vectors under fixtures/protocol/v2/ were made with the
 * public test keys their .fields files list. From those keys the endpoint
 * reproduces all four frames, so the endpoint's encryption passes the same
 * vectors on the host build as the Rust sides. Nothing here writes a
 * fixture. */

#include "chorus/noise.h"
#include "chorus/protocol_v2.h"
#include "fixture_text.h"
#include "harness.h"

#include <inttypes.h>

#define TEXT_CAP 16384
#define VALUE_CAP 4096

static char cacophony[TEXT_CAP];

static int fixed_bytes(const char *text, const char *key, uint8_t *out, size_t want)
{
    char value[VALUE_CAP];
    if (fixture_field(text, key, value, sizeof(value)) == NULL) {
        return 0;
    }
    return fixture_unhex(value, out, want) == (long)want;
}

static long field_bytes(const char *text, const char *key, uint8_t *out, size_t cap)
{
    char value[VALUE_CAP];
    if (fixture_field(text, key, value, sizeof(value)) == NULL) {
        return -1;
    }
    return fixture_unhex(value, out, cap);
}

static int load(const char *relative, char *out, size_t cap)
{
    char path[512];
    chorus_repo_path(path, sizeof(path), relative);
    return fixture_read(path, out, cap) >= 0;
}

/* --- cacophony, both roles ------------------------------------------------- */

typedef struct {
    int reproduced;
    int total;
} tally_t;

static void same(tally_t *tally, const char *what, const uint8_t *got, size_t got_len,
                 const uint8_t *want, size_t want_len)
{
    int ok = got_len == want_len && memcmp(got, want, want_len) == 0;
    tally->total++;
    tally->reproduced += ok;
    if (ok) {
        chorus_check(1, "%s: %zu bytes reproduced exactly", what, want_len);
    } else {
        char a[1024];
        char b[1024];
        fixture_hex(got, got_len, a, sizeof(a));
        fixture_hex(want, want_len, b, sizeof(b));
        chorus_check(0, "%s: produced %s, the vector says %s", what, a, b);
    }
}

static void cacophony_in_both_roles(void)
{
    chorus_section("cacophony Noise_XX_25519_ChaChaPoly_SHA256, both roles");
    int read_ok =
        load("fixtures/protocol/v2/noise/cacophony_xx.fields", cacophony, sizeof(cacophony));
    chorus_check(read_ok, "fixtures/protocol/v2/noise/cacophony_xx.fields is readable");
    if (!read_ok) {
        return;
    }
    char name[128];
    chorus_check(fixture_field(cacophony, "protocol_name", name, sizeof(name)) != NULL &&
                     strcmp(name, "Noise_XX_25519_ChaChaPoly_SHA256") == 0,
                 "the vector is the one for Noise_XX_25519_ChaChaPoly_SHA256");

    uint8_t prologue[256];
    long prologue_len = field_bytes(cacophony, "prologue", prologue, sizeof(prologue));
    uint8_t secrets[4][32];
    const char *keys[4] = {"init_static", "init_ephemeral", "resp_static", "resp_ephemeral"};
    for (int i = 0; i < 4; i++) {
        chorus_check(fixed_bytes(cacophony, keys[i], secrets[i], 32), "%s is a 32-byte key",
                     keys[i]);
    }
    chorus_noise_keypair_t is;
    chorus_noise_keypair_t ie;
    chorus_noise_keypair_t rs;
    chorus_noise_keypair_t re;
    chorus_check(chorus_noise_keypair_from_secret(secrets[0], &is) == CHORUS_NOISE_OK &&
                     chorus_noise_keypair_from_secret(secrets[1], &ie) == CHORUS_NOISE_OK &&
                     chorus_noise_keypair_from_secret(secrets[2], &rs) == CHORUS_NOISE_OK &&
                     chorus_noise_keypair_from_secret(secrets[3], &re) == CHORUS_NOISE_OK,
                 "the four key pairs derive through psa_raw_key_agreement's key type");

    chorus_noise_handshake_t initiator;
    chorus_noise_handshake_t responder;
    chorus_noise_handshake_init(&initiator, 1, prologue, (size_t)prologue_len, &is, &ie);
    chorus_noise_handshake_init(&responder, 0, prologue, (size_t)prologue_len, &rs, &re);

    tally_t tally = {0, 0};
    static uint8_t out[1024];
    static uint8_t payload[1024];
    static uint8_t want[1024];
    static uint8_t want_payload[1024];

    for (int n = 0; n < 3; n++) {
        char key[64];
        snprintf(key, sizeof(key), "message_%d_payload", n);
        long p_len = field_bytes(cacophony, key, want_payload, sizeof(want_payload));
        snprintf(key, sizeof(key), "message_%d_ciphertext", n);
        long c_len = field_bytes(cacophony, key, want, sizeof(want));
        chorus_noise_handshake_t *writer = (n % 2 == 0) ? &initiator : &responder;
        chorus_noise_handshake_t *reader = (n % 2 == 0) ? &responder : &initiator;
        const char *who = (n % 2 == 0) ? "initiator" : "responder";
        const char *other = (n % 2 == 0) ? "responder" : "initiator";
        size_t written = 0;
        chorus_noise_status_t status = chorus_noise_write_message(
            writer, want_payload, (size_t)p_len, out, sizeof(out), &written);
        char what[128];
        snprintf(what, sizeof(what), "handshake message %d written by the %s", n, who);
        chorus_check(status == CHORUS_NOISE_OK, "%s (%s)", what, chorus_noise_status_name(status));
        same(&tally, what, out, written, want, (size_t)c_len);
        size_t got = 0;
        status =
            chorus_noise_read_message(reader, want, (size_t)c_len, payload, sizeof(payload), &got);
        snprintf(what, sizeof(what), "handshake message %d read by the %s", n, other);
        chorus_check(status == CHORUS_NOISE_OK, "%s (%s)", what, chorus_noise_status_name(status));
        same(&tally, what, payload, got, want_payload, (size_t)p_len);
    }

    chorus_noise_transport_t ti;
    chorus_noise_transport_t tr;
    chorus_check(chorus_noise_split(&initiator, &ti) == CHORUS_NOISE_OK &&
                     chorus_noise_split(&responder, &tr) == CHORUS_NOISE_OK,
                 "both sides split after message 3");
    uint8_t hash[32];
    fixed_bytes(cacophony, "handshake_hash", hash, 32);
    same(&tally, "the initiator's handshake hash", ti.handshake_hash, 32, hash, 32);
    same(&tally, "the responder's handshake hash", tr.handshake_hash, 32, hash, 32);
    same(&tally, "the initiator's view of the responder's static key", ti.remote_static, 32,
         rs.public_key, 32);
    same(&tally, "the responder's view of the initiator's static key", tr.remote_static, 32,
         is.public_key, 32);

    /* Transport messages alternate from the initiator's turn after the
     * handshake: 3 and 5 are responder to initiator, 4 initiator to
     * responder, all with empty associated data. */
    for (int n = 3; n < 6; n++) {
        char key[64];
        snprintf(key, sizeof(key), "message_%d_payload", n);
        long p_len = field_bytes(cacophony, key, want_payload, sizeof(want_payload));
        snprintf(key, sizeof(key), "message_%d_ciphertext", n);
        long c_len = field_bytes(cacophony, key, want, sizeof(want));
        int from_responder = (n % 2 == 1);
        chorus_noise_cipher_t *send = from_responder ? &tr.send : &ti.send;
        chorus_noise_cipher_t *receive = from_responder ? &ti.receive : &tr.receive;
        size_t written = 0;
        chorus_noise_encrypt(send, NULL, 0, want_payload, (size_t)p_len, out, sizeof(out),
                             &written);
        char what[128];
        snprintf(what, sizeof(what), "transport message %d sealed by the %s", n,
                 from_responder ? "responder" : "initiator");
        same(&tally, what, out, written, want, (size_t)c_len);
        size_t got = 0;
        chorus_noise_status_t status = chorus_noise_decrypt(receive, NULL, 0, want, (size_t)c_len,
                                                            payload, sizeof(payload), &got);
        snprintf(what, sizeof(what), "transport message %d opened by the %s", n,
                 from_responder ? "initiator" : "responder");
        chorus_check(status == CHORUS_NOISE_OK, "%s (%s)", what, chorus_noise_status_name(status));
        same(&tally, what, payload, got, want_payload, (size_t)p_len);
    }
    chorus_noise_transport_clear(&ti);
    chorus_noise_transport_clear(&tr);

    printf("cacophony XX vector: %d of %d values reproduced (both handshake hashes and static "
           "keys, 6 of 6 message "
           "ciphertexts and payloads, as initiator and as responder)\n",
           tally.reproduced, tally.total);
    chorus_check(tally.reproduced == tally.total && tally.total == 16,
                 "every value of the published vector was reproduced in both roles");
}

/* --- chorus's own session vectors ----------------------------------------- */

static const uint8_t PROLOGUE[7] = {0x43, 0x48, 0x52, 0x53, 0x00, 0x02, 0x01};

static long committed_frame(const char *stem, uint8_t *out, size_t cap)
{
    static char text[TEXT_CAP];
    char relative[256];
    snprintf(relative, sizeof(relative), "fixtures/protocol/v2/%s.hex", stem);
    if (!load(relative, text, sizeof(text))) {
        return -1;
    }
    return fixture_parse_hex(text, out, cap);
}

static size_t id_payload(const char *id, uint8_t *out)
{
    size_t len = strlen(id);
    out[0] = (uint8_t)len;
    memcpy(out + 1, id, len);
    return len + 1;
}

static void the_chorus_session_vectors_are_reproduced(void)
{
    chorus_section("chorus v2 session vectors from the public test keys");
    static char fields[TEXT_CAP];
    static char record_fields[TEXT_CAP];
    int ok =
        load("fixtures/protocol/v2/handshake_init.fields", fields, sizeof(fields)) &&
        load("fixtures/protocol/v2/secure_record.fields", record_fields, sizeof(record_fields));
    chorus_check(ok, "the handshake and record .fields files are readable");
    if (!ok) {
        return;
    }
    uint8_t secrets[4][32];
    const char *keys[4] = {"endpoint_static", "endpoint_ephemeral", "server_static",
                           "server_ephemeral"};
    for (int i = 0; i < 4; i++) {
        chorus_check(fixed_bytes(fields, keys[i], secrets[i], 32), "%s is a 32-byte test key",
                     keys[i]);
    }
    char endpoint_id[256];
    char server_id[256];
    fixture_field(fields, "endpoint_id", endpoint_id, sizeof(endpoint_id));
    fixture_field(fields, "server_id", server_id, sizeof(server_id));

    chorus_noise_keypair_t es;
    chorus_noise_keypair_t ee;
    chorus_noise_keypair_t ss;
    chorus_noise_keypair_t se;
    chorus_noise_keypair_from_secret(secrets[0], &es);
    chorus_noise_keypair_from_secret(secrets[1], &ee);
    chorus_noise_keypair_from_secret(secrets[2], &ss);
    chorus_noise_keypair_from_secret(secrets[3], &se);

    chorus_noise_handshake_t endpoint;
    chorus_noise_handshake_t server;
    chorus_noise_handshake_init(&endpoint, 1, PROLOGUE, sizeof(PROLOGUE), &es, &ee);
    chorus_noise_handshake_init(&server, 0, PROLOGUE, sizeof(PROLOGUE), &ss, &se);

    tally_t tally = {0, 0};
    static uint8_t noise[1024];
    static uint8_t frame[70000];
    static uint8_t want[70000];
    static uint8_t payload[1024];
    size_t noise_len = 0;
    size_t frame_len = 0;
    size_t got = 0;

    /* -> e: the endpoint's handshake_init. */
    chorus_noise_write_message(&endpoint, NULL, 0, noise, sizeof(noise), &noise_len);
    chorus_v2_message_t m;
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_HANDSHAKE_INIT;
    m.as.handshake_init.protocol_version = CHORUS_V2_PROTOCOL_VERSION;
    m.as.handshake_init.suite = CHORUS_V2_SUITE_NOISE_XX_25519_CHACHAPOLY_SHA256;
    m.as.handshake_init.noise.data = noise;
    m.as.handshake_init.noise.len = noise_len;
    chorus_v2_encode(&m, frame, sizeof(frame), &frame_len, NULL);
    long want_len = committed_frame("handshake_init", want, sizeof(want));
    same(&tally, "handshake_init, as the endpoint sends it", frame, frame_len, want,
         (size_t)want_len);
    chorus_noise_read_message(&server, noise, noise_len, payload, sizeof(payload), &got);

    /* <- e, ee, s, es with the server's id. */
    uint8_t id[256];
    size_t id_len = id_payload(server_id, id);
    chorus_noise_write_message(&server, id, id_len, noise, sizeof(noise), &noise_len);
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_HANDSHAKE_RESPONSE;
    m.as.handshake_response.noise.data = noise;
    m.as.handshake_response.noise.len = noise_len;
    chorus_v2_encode(&m, frame, sizeof(frame), &frame_len, NULL);
    want_len = committed_frame("handshake_response", want, sizeof(want));
    same(&tally, "handshake_response, with the server's id", frame, frame_len, want,
         (size_t)want_len);
    chorus_noise_status_t status = chorus_noise_read_message(
        &endpoint, want + CHORUS_FRAME_HEADER_LEN, (size_t)want_len - CHORUS_FRAME_HEADER_LEN,
        payload, sizeof(payload), &got);
    chorus_check(status == CHORUS_NOISE_OK && got == id_len && memcmp(payload, id, id_len) == 0,
                 "the endpoint reads the committed handshake_response and finds the server id "
                 "'%s' (%s)",
                 server_id, chorus_noise_status_name(status));

    /* -> s, se with the endpoint's id. */
    id_len = id_payload(endpoint_id, id);
    chorus_noise_write_message(&endpoint, id, id_len, noise, sizeof(noise), &noise_len);
    memset(&m, 0, sizeof(m));
    m.type = CHORUS_V2_HANDSHAKE_FINISH;
    m.as.handshake_finish.noise.data = noise;
    m.as.handshake_finish.noise.len = noise_len;
    chorus_v2_encode(&m, frame, sizeof(frame), &frame_len, NULL);
    want_len = committed_frame("handshake_finish", want, sizeof(want));
    same(&tally, "handshake_finish, with the endpoint's id", frame, frame_len, want,
         (size_t)want_len);
    status = chorus_noise_read_message(&server, noise, noise_len, payload, sizeof(payload), &got);
    chorus_check(status == CHORUS_NOISE_OK && got == id_len && memcmp(payload, id, id_len) == 0,
                 "the server side reads the endpoint id '%s'", endpoint_id);

    chorus_noise_transport_t te;
    chorus_noise_transport_t ts;
    chorus_noise_split(&endpoint, &te);
    chorus_noise_split(&server, &ts);
    chorus_check(memcmp(te.handshake_hash, ts.handshake_hash, 32) == 0,
                 "both sides hold the same handshake hash");
    chorus_check(memcmp(te.remote_static, ss.public_key, 32) == 0,
                 "the endpoint learned the server's static key from the handshake");

    /* The endpoint's first record: hello_endpoint then capabilities, nonce 0,
     * the record's own header as associated data. */
    static uint8_t plaintext[70000];
    long plain_len = field_bytes(record_fields, "plaintext", plaintext, sizeof(plaintext));
    static uint8_t hello[1024];
    static uint8_t caps[1024];
    long hello_len = committed_frame("hello_endpoint", hello, sizeof(hello));
    long caps_len = committed_frame("capabilities", caps, sizeof(caps));
    chorus_check(plain_len == hello_len + caps_len &&
                     memcmp(plaintext, hello, (size_t)hello_len) == 0 &&
                     memcmp(plaintext + hello_len, caps, (size_t)caps_len) == 0,
                 "the record's plaintext is the hello_endpoint frame then the capabilities "
                 "frame, as its .fields file says");
    chorus_noise_seal_record(&te.send, plaintext, (size_t)plain_len, frame, sizeof(frame),
                             &frame_len);
    want_len = committed_frame("secure_record", want, sizeof(want));
    same(&tally, "secure_record, the endpoint's first", frame, frame_len, want, (size_t)want_len);
    static uint8_t opened[70000];
    status = chorus_noise_open_record(&ts.receive, want + CHORUS_FRAME_HEADER_LEN,
                                      (size_t)want_len - CHORUS_FRAME_HEADER_LEN, opened,
                                      sizeof(opened), &got);
    chorus_check(status == CHORUS_NOISE_OK && got == (size_t)plain_len &&
                     memcmp(opened, plaintext, got) == 0,
                 "the server side opens the committed record into the same frames");

    /* A replay of the same record, and an altered one, end the session. */
    status = chorus_noise_open_record(&ts.receive, want + CHORUS_FRAME_HEADER_LEN,
                                      (size_t)want_len - CHORUS_FRAME_HEADER_LEN, opened,
                                      sizeof(opened), &got);
    chorus_check(status == CHORUS_NOISE_DECRYPT_FAILED, "a replayed record does not decrypt (%s)",
                 chorus_noise_status_name(status));
    chorus_noise_transport_clear(&te);
    chorus_noise_transport_clear(&ts);

    printf("chorus v2 session vectors: %d of %d reproduced from the public test keys "
           "(handshake_init, handshake_response, handshake_finish, secure_record)\n",
           tally.reproduced, tally.total);
    chorus_check(tally.reproduced == tally.total && tally.total == 4,
                 "the endpoint's encryption passes the same vectors as the Rust sides");
}

/* The key_changed refusal names two fingerprints; the endpoint computes them
 * the way the vector's .fields file says they were made. */
static void the_fingerprints_in_the_refusal_vector_are_reproduced(void)
{
    chorus_section("fingerprints");
    static char init_fields[TEXT_CAP];
    static char refused[TEXT_CAP];
    int ok =
        load("fixtures/protocol/v2/handshake_init.fields", init_fields, sizeof(init_fields)) &&
        load("fixtures/protocol/v2/session_refused_key_changed.fields", refused, sizeof(refused));
    chorus_check(ok, "the refusal's .fields file is readable");
    if (!ok) {
        return;
    }
    uint8_t pinned_secret[32];
    uint8_t offered_secret[32];
    fixed_bytes(init_fields, "endpoint_static", pinned_secret, 32);
    fixed_bytes(refused, "offered_static", offered_secret, 32);
    chorus_noise_keypair_t pinned;
    chorus_noise_keypair_t offered;
    chorus_noise_keypair_from_secret(pinned_secret, &pinned);
    chorus_noise_keypair_from_secret(offered_secret, &offered);
    char pinned_fp[CHORUS_NOISE_FINGERPRINT_LEN];
    char offered_fp[CHORUS_NOISE_FINGERPRINT_LEN];
    chorus_noise_fingerprint(pinned.public_key, pinned_fp);
    chorus_noise_fingerprint(offered.public_key, offered_fp);
    char endpoint_id[256];
    fixture_field(init_fields, "endpoint_id", endpoint_id, sizeof(endpoint_id));
    char expected[512];
    snprintf(expected, sizeof(expected),
             "%s presented key %s but key %s has been pinned since its adoption; refused",
             endpoint_id, offered_fp, pinned_fp);
    char detail[512];
    fixture_field(refused, "detail", detail, sizeof(detail));
    chorus_check(strcmp(expected, detail) == 0,
                 "the key_changed detail is reproduced from the two keys: \"%s\"", expected);
}

static void the_handshake_fails_closed(void)
{
    chorus_section("failures");
    uint8_t a[32];
    uint8_t b[32];
    uint8_t c[32];
    uint8_t d[32];
    memset(a, 1, 32);
    memset(b, 2, 32);
    memset(c, 3, 32);
    memset(d, 4, 32);
    chorus_noise_keypair_t ka;
    chorus_noise_keypair_t kb;
    chorus_noise_keypair_t kc;
    chorus_noise_keypair_t kd;
    chorus_noise_keypair_from_secret(a, &ka);
    chorus_noise_keypair_from_secret(b, &kb);
    chorus_noise_keypair_from_secret(c, &kc);
    chorus_noise_keypair_from_secret(d, &kd);

    /* A prologue that differs (a version or suite changed in transit) fails
     * the handshake at message 2. */
    chorus_noise_handshake_t i;
    chorus_noise_handshake_t r;
    chorus_noise_handshake_init(&i, 1, (const uint8_t *)"chorus", 6, &ka, &kb);
    chorus_noise_handshake_init(&r, 0, (const uint8_t *)"other", 5, &kc, &kd);
    uint8_t m[256];
    uint8_t p[256];
    size_t n = 0;
    size_t got = 0;
    chorus_noise_write_message(&i, NULL, 0, m, sizeof(m), &n);
    chorus_noise_read_message(&r, m, n, p, sizeof(p), &got);
    chorus_noise_write_message(&r, NULL, 0, m, sizeof(m), &n);
    chorus_noise_status_t status = chorus_noise_read_message(&i, m, n, p, sizeof(p), &got);
    chorus_check(status == CHORUS_NOISE_DECRYPT_FAILED,
                 "a different prologue fails the handshake (%s)", chorus_noise_status_name(status));
    chorus_check(chorus_noise_write_message(&i, NULL, 0, m, sizeof(m), &n) ==
                     CHORUS_NOISE_OUT_OF_ORDER,
                 "a failed handshake cannot be continued");
    chorus_noise_handshake_clear(&i);
    chorus_noise_handshake_clear(&r);

    /* A low-order public key (all zeros) gives an all-zero DH result, which
     * is refused rather than keyed with. */
    chorus_noise_handshake_init(&i, 1, NULL, 0, &ka, &kb);
    chorus_noise_write_message(&i, NULL, 0, m, sizeof(m), &n);
    uint8_t evil[96];
    memset(evil, 0, sizeof(evil));
    status = chorus_noise_read_message(&i, evil, sizeof(evil), p, sizeof(p), &got);
    chorus_check(status == CHORUS_NOISE_WEAK_KEY || status == CHORUS_NOISE_CRYPTO_FAILED,
                 "an all-zero ephemeral key from the peer is refused (%s)",
                 chorus_noise_status_name(status));
    chorus_noise_handshake_clear(&i);

    /* A message too short for its tokens. */
    chorus_noise_handshake_init(&r, 0, NULL, 0, &kc, &kd);
    status = chorus_noise_read_message(&r, m, 10, p, sizeof(p), &got);
    chorus_check(status == CHORUS_NOISE_BAD_LENGTH, "a 10-byte message 1 is refused (%s)",
                 chorus_noise_status_name(status));
    chorus_noise_handshake_clear(&r);

    /* An altered record. */
    chorus_noise_handshake_init(&i, 1, NULL, 0, &ka, &kb);
    chorus_noise_handshake_init(&r, 0, NULL, 0, &kc, &kd);
    chorus_noise_write_message(&i, NULL, 0, m, sizeof(m), &n);
    chorus_noise_read_message(&r, m, n, p, sizeof(p), &got);
    chorus_noise_write_message(&r, NULL, 0, m, sizeof(m), &n);
    chorus_noise_read_message(&i, m, n, p, sizeof(p), &got);
    chorus_noise_write_message(&i, NULL, 0, m, sizeof(m), &n);
    chorus_noise_read_message(&r, m, n, p, sizeof(p), &got);
    chorus_noise_transport_t ti;
    chorus_noise_transport_t tr;
    chorus_check(chorus_noise_split(&i, &ti) == CHORUS_NOISE_OK &&
                     chorus_noise_split(&r, &tr) == CHORUS_NOISE_OK,
                 "a handshake with empty payloads completes");
    uint8_t frames[8] = {0x7F, 0x00, 0x05, 1, 2, 3, 4, 5};
    uint8_t record[64];
    size_t record_len = 0;
    chorus_noise_seal_record(&ti.send, frames, sizeof(frames), record, sizeof(record), &record_len);
    record[5] ^= 0x01;
    status = chorus_noise_open_record(&tr.receive, record + 3, record_len - 3, p, sizeof(p), &got);
    chorus_check(status == CHORUS_NOISE_DECRYPT_FAILED, "an altered record does not decrypt (%s)",
                 chorus_noise_status_name(status));
    record[5] ^= 0x01;
    status = chorus_noise_open_record(&tr.receive, record + 3, record_len - 3, p, sizeof(p), &got);
    chorus_check(status == CHORUS_NOISE_OK && got == sizeof(frames),
                 "the unaltered record still opens: a failure did not advance the nonce");
    /* The session opens records in place (goal 8: no second 64 KiB buffer);
     * PSA Crypto API 1.2 section 5.4.4 lets an output buffer overlap an input
     * buffer with the same result. */
    chorus_noise_seal_record(&ti.send, frames, sizeof(frames), record, sizeof(record), &record_len);
    status = chorus_noise_open_record(&tr.receive, record + 3, record_len - 3, record + 3,
                                      record_len - 3, &got);
    chorus_check(status == CHORUS_NOISE_OK && got == sizeof(frames) &&
                     memcmp(record + 3, frames, sizeof(frames)) == 0,
                 "a record opens in place, over its own ciphertext, to the same plaintext");
    chorus_noise_transport_clear(&ti);
    chorus_noise_transport_clear(&tr);
}

int main(void)
{
    chorus_check(chorus_noise_setup() == CHORUS_NOISE_OK, "psa_crypto_init");
    cacophony_in_both_roles();
    the_chorus_session_vectors_are_reproduced();
    the_fingerprints_in_the_refusal_vector_are_reproduced();
    the_handshake_fails_closed();
    return chorus_test_report("test_noise");
}
