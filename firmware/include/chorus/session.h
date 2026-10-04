/* The session supervisor: join, play, and rejoin with no human action.
 *
 * A transport close and a transport that broke look identical to the peer:
 * both are a read returning zero. `stream_end` is what tells them apart, and
 * everything else is an outage. The supervisor treats every outage the same
 * way - link down, backoff, connect again, resume - because "the server or the
 * network disappears and returns" covers both and the endpoint cannot tell
 * which happened.
 *
 * It never gives up and it never spins: the backoff starts short because most
 * outages are a server restart, and it is capped so that a link down for an
 * hour is not a busy loop. There is no retry budget to exhaust, which is the
 * property the outage-of-minutes run exists to grade.
 *
 * Every connection is a chorus protocol v2 session (docs/protocol.md, "The
 * session, in order"): the endpoint opens the Noise XX handshake as the
 * initiator with its id and long-term key, checks the server's key against
 * its pin (pinning a server it has never met, refusing one whose key
 * changed), and from then on everything on the connection is a
 * secure_record: its hello and capabilities, the time-sync exchange, and the
 * server's stream_format, output_delay and audio. A server whose key changed
 * is not an outage: the endpoint sends session_refused key_changed and the
 * run ends, because only the owner changes a pin. */

#ifndef CHORUS_SESSION_H
#define CHORUS_SESSION_H

#include <stddef.h>
#include <stdint.h>

#include "chorus/controls.h"
#include "chorus/noise.h"
#include "chorus/ota.h"
#include "chorus/playout.h"
#include "chorus/protocol_v2.h"
#include "chorus/store.h"
#include "chorus/sync.h"
#include "chorus/telemetry.h"

#define CHORUS_SESSION_ADDRESS_MAX 128
#define CHORUS_SESSION_ID_MAX 256

/* The id used when the configuration names none. An id is what the server
 * pins this endpoint's key to, so a real endpoint names its own. */
#define CHORUS_SESSION_DEFAULT_ID "chorus-endpoint"

/* What the endpoint reports about itself that the session cannot read from
 * its own state (goal 15): how it reaches the network, the radio's signal,
 * a temperature, and the heap. The session hands the `health` seam below a
 * record in which EVERYTHING IS UNKNOWN, and the board fills only what it has
 * a source for; what stays unknown is sent as unknown (docs/protocol.md,
 * "Telemetry"), never as zero. */
typedef struct {
    /* 0 unknown, 1 wired, 2 wireless: the wire's `link`. */
    uint8_t link;
    /* INT8_MIN when unknown or wired. */
    int8_t rssi_dbm;
    /* Hundredths of a degree Celsius; INT16_MIN when unknown. */
    int16_t temperature_centi_c;
    /* Bytes; CHORUS_V2_TELEMETRY_HEAP_UNKNOWN when not reported. */
    uint32_t heap_free_bytes;
    uint32_t heap_min_free_bytes;
} chorus_session_health_t;

/* A health record in which nothing is known. */
void chorus_session_health_unknown(chorus_session_health_t *health);

/* --- the voice path (docs/protocol.md, "The voice role") ---------------------
 *
 * What the session does with a microphone: it declares the `voice` role when
 * the class has one, reports the gate (mic_state) once after capabilities and
 * at every change, keeps the server's last voice_control, and sends captured
 * samples as mic_audio. Three things must all allow a sample out, and this is
 * where they meet:
 *
 *   1. the class has a microphone (chorus_controls_profile);
 *   2. the gate is live: the hardware switch, debounced, in the live position
 *      (chorus/controls.h). Nothing the server sends opens it;
 *   3. the server's last voice_control in THIS session set `uplink`.
 *
 * Samples reach the encoder only by way of chorus_controls_mic_pass: the
 * capture source's samples are handed to the gate, and what is encoded is
 * the gate's output buffer and nothing else. Pure: no clock, no pin, no
 * socket. The capture source is a seam (chorus_session_config_t.mic_capture)
 * with no driver behind it yet; firmware/tests/test_controls.c drives a fake
 * one. */

/* The most one capture call takes: ten of the wire's largest chunk. More than
 * that in one call is refused whole (counted), never trimmed. */
#define CHORUS_SESSION_VOICE_MAX_CAPTURE (10u * CHORUS_V2_MIC_MAX_SAMPLES)
/* The gate's state has not been told to this session's server yet. */
#define CHORUS_SESSION_VOICE_NOT_REPORTED 0xFFu

typedef struct {
    /* The endpoint's controls, whose gate this is. Read, never written. */
    const chorus_controls_t *controls;
    /* The server's last voice_control in this session; both 0 before one. */
    uint8_t uplink;
    uint8_t listening;
    /* The gate the server was last told, CHORUS_V2_MIC_GATE_*, or
     * CHORUS_SESSION_VOICE_NOT_REPORTED. */
    uint8_t reported;
    /* Whether the last capture was sent, and the next chunk's sequence: 0
     * for the first chunk after a stretch in which nothing was sent. */
    int streaming;
    uint32_t next_sequence;
    /* Since init: what was sent, and samples that did not leave (and why is
     * one of: gate closed, uplink off, gate not yet reported, no offset). */
    uint64_t chunks_sent;
    uint64_t samples_sent;
    uint64_t samples_withheld;
    uint32_t reports_sent;
    uint32_t refused_captures;
    /* The gate's output, and the same samples as the wire's bytes. */
    int16_t passed[CHORUS_V2_MIC_MAX_SAMPLES];
    uint8_t wire[CHORUS_V2_MIC_MAX_SAMPLES * CHORUS_V2_MIC_BYTES_PER_SAMPLE];
} chorus_session_voice_t;

/* `controls` outlives the voice path. */
void chorus_session_voice_init(chorus_session_voice_t *voice, const chorus_controls_t *controls);

/* CHORUS_V2_ROLE_VOICE when the class has a microphone, else 0: what the
 * session adds to hello.roles. NULL is 0. */
uint16_t chorus_session_voice_roles(const chorus_session_voice_t *voice);

/* A session began (or ended): uplink and listening are 0 again, the gate has
 * not been reported, and the next chunk is sequence 0. */
void chorus_session_voice_begin(chorus_session_voice_t *voice);

/* The server's voice_control. Skipped when the role was not declared. */
void chorus_session_voice_control(chorus_session_voice_t *voice,
                                  const chorus_v2_voice_control_t *control);

/* Whether the LED shows listening (chorus_led_inputs_t.listening). */
int chorus_session_voice_listening(const chorus_session_voice_t *voice);

/* The gate report: writes one mic_state frame to `out` and returns its
 * length when the gate differs from what this session's server was told (or
 * it was told nothing yet), else 0. A class without a microphone never
 * reports. */
size_t chorus_session_voice_report(chorus_session_voice_t *voice, uint8_t *out, size_t out_len);

/* `count` samples (16 kHz mono) whose first was digitized at `captured_ns`
 * on the endpoint's monotonic clock. Writes whole mic_audio frames to `out`
 * (one per CHORUS_V2_MIC_MAX_SAMPLES) and returns their total length; 0 when
 * nothing may leave: the gate is closed, the uplink is not requested, the
 * server has not been told the gate is live, or there is no sync offset yet
 * (`offset_known` 0: a timestamp is never guessed). `offset_ns` is the server
 * clock minus the endpoint's. */
size_t chorus_session_voice_capture(chorus_session_voice_t *voice, const int16_t *samples,
                                    size_t count, uint64_t captured_ns, int offset_known,
                                    int64_t offset_ns, uint8_t *out, size_t out_len);

typedef struct {
    char server[CHORUS_SESSION_ADDRESS_MAX];
    uint32_t first_backoff_ms;
    uint32_t max_backoff_ms;
    /* How long to run for. Zero runs until told to stop. */
    uint32_t run_seconds;
    /* How often to run a time-sync exchange, in milliseconds. */
    uint32_t sync_interval_ms;
    /* Exchanges in the minimum-round-trip window, and the smoother's weight.
     * Both come from config/sync.conf, so both endpoints filter the same way. */
    uint32_t filter_window;
    double smoothing_alpha;
    /* Where to write one telemetry line per event. NULL prints nothing. */
    const char *event_log_path;

    /* What the endpoint keeps across boots (goal 14, chorus/store.h and
     * chorus/identity.h), optional. With a store the long-term key, the
     * pinned servers and, when `endpoint_id` is empty, the id itself are the
     * store's (`key_path` and `server_pins_path` are then not read), a key or
     * pin the store did not keep is refused rather than used, and the address
     * of each server a handshake completes with is kept for discovery's
     * fallback. NULL is the run as it was: the file paths below. */
    const chorus_store_t *store;

    /* Who this endpoint is: its id (1 to 255 bytes; empty uses the store's
     * id when there is a store, else CHORUS_SESSION_DEFAULT_ID) and where its long-term X25519
     * secret lives (64 hex digits and a newline, mode 0600, made from the random source when the
     * file is absent). NULL keeps a key for this run only. */
    char endpoint_id[CHORUS_SESSION_ID_MAX];
    const char *key_path;
    /* Where the pinned servers live, in the adoption store's text form (one
     * `pinned <public key hex> <server id>` line per server). NULL keeps the
     * pins for this run only. */
    const char *server_pins_path;
    /* How long the server has to answer handshake_init. Zero uses 2000 ms. */
    uint32_t handshake_timeout_ms;
    /* The random source for keys. NULL uses chorus_noise_system_random; a
     * test hands in fixed keys. */
    chorus_noise_random_fn random;
    void *random_ctx;
    /* The playout path (firmware/include/chorus/playout.h): audio chunks, PCM
     * or decoded, go into its jitter buffer and the filtered offset goes to
     * its loop. NULL counts chunks as received and plays nothing (the host
     * session binary). */
    chorus_playout_t *playout;

    /* The endpoint console's two seams (audit A-13), both optional. NULL
     * leaves the run exactly as it was: `server` above for its whole length,
     * and telemetry only in the event log.
     *
     * `server_update` is asked before every connection attempt; it returns 1
     * and writes a `host:port` into `address` when the console set a new one,
     * 0 otherwise. An address that does not split is refused by name in the
     * event log and the old one kept. `on_telemetry` is handed the telemetry
     * at every event the run publishes, which is what the console's `status`
     * prints. Both run on the session's own task. */
    int (*server_update)(void *ctx, char *address, size_t address_len);
    void *server_update_ctx;
    void (*on_telemetry)(void *ctx, const chorus_telemetry_t *telemetry);
    void *telemetry_ctx;

    /* Discovery's seam (goal 14, chorus/discovery.h), optional. Asked after
     * every third connection attempt in a row that reached no server: it
     * returns 1 and writes a `host:port` into `address` when it FOUND the
     * server somewhere, 0 otherwise. A different address applies from the
     * next attempt and is published as `server-relocated`; the same one, or
     * one that does not split, changes nothing. Runs on the session's own
     * task and may take as long as a browse does: nothing is connected while
     * it is asked. NULL never asks. */
    int (*relocate)(void *ctx, char *address, size_t address_len);
    void *relocate_ctx;

    /* Handed every `sound` (0x39) the server sends, after the decoder has
     * held it to its ranges and the run has kept it in the result
     * (chorus_session_last_sound) and handed it to the playout path's sound
     * chain (chorus_playout_set_sound, chorus/endpoint_dsp.h). Optional: NULL
     * hands it nowhere else (docs/decisions/0081-*). Runs on the session's
     * own task. */
    void (*on_sound)(void *ctx, const chorus_v2_sound_t *sound);
    void *sound_ctx;

    /* The firmware update unit (chorus/ota.h), already started for this
     * boot. With it the endpoint sets capabilities.features `ota`, opens
     * every session with a firmware_status, hands firmware_offer and
     * firmware_chunk to the unit, confirms an image on trial once the
     * server's first record opens, and ends the run when the unit reboots.
     * NULL: no `ota` feature, and the three types are stepped over. */
    chorus_ota_t *ota;

    /* The endpoint's health (goal 15), optional: asked once per `telemetry`
     * (about once a second) on the session's own task, with a record in
     * which everything is unknown, and it fills what the board knows
     * (firmware/main/esp_hal.c on the board; the host programs fill it from
     * their command line, as stated fakes). NULL reports all of it unknown
     * and sends no heap block. It must not block. */
    void (*health)(void *ctx, chorus_session_health_t *health);
    void *health_ctx;

    /* The voice path (above), optional. With it, and a class that has a
     * microphone, the endpoint declares the `voice` role, reports its gate
     * after capabilities and at every change, and takes voice_control. NULL:
     * no role, and voice_control is stepped over.
     *
     * `mic_capture` is the capture source's seam: asked on the session's own
     * task, it writes up to `max` samples (16 kHz mono) and the monotonic
     * instant the first was digitized, and returns how many (0: none now).
     * It must not block. No driver stands behind it yet (the microphone part
     * is not chosen); NULL sends no audio, and the gate is still reported.
     * The task that feeds the controls' switch readings and this one share
     * the gate's one flag; the binding that adds a second task serialises
     * them. */
    chorus_session_voice_t *voice;
    size_t (*mic_capture)(void *ctx, int16_t *samples, size_t max, uint64_t *captured_ns);
    void *mic_capture_ctx;
} chorus_session_config_t;

/* Why a run ended. Never "the server went away": that is not an end, it is a
 * reconnect. */
typedef enum {
    CHORUS_SESSION_RAN_ITS_TIME = 0,
    CHORUS_SESSION_ADDRESS_UNUSABLE,
    CHORUS_SESSION_STOPPED_ON_AMP_FAULT,
    CHORUS_SESSION_LOG_UNWRITABLE,
    /* The endpoint's key or its pin store could not be read or made. */
    CHORUS_SESSION_IDENTITY_UNUSABLE,
    /* A server presented a key other than the one pinned to its id. Refused
     * with session_refused key_changed, and the run ends: only the owner
     * changes a pin. */
    CHORUS_SESSION_SERVER_KEY_CHANGED,
    /* The update unit rebooted (a new image selected, or one on trial that
     * did not confirm). Only a host build sees this end: on the board the
     * reboot does not return. Not a failure. */
    CHORUS_SESSION_REBOOTING
} chorus_session_end_t;

const char *chorus_session_end_name(chorus_session_end_t end);

typedef struct {
    chorus_session_end_t end;
    chorus_telemetry_t telemetry;
    /* The longest gap between the link going down and coming back up again,
     * in nanoseconds on the endpoint's monotonic clock. This is the outage the
     * endpoint actually rode through, measured rather than assumed. */
    uint64_t longest_outage_ns;
    /* How many distinct connections carried audio, counted from
     * `chunks_received`: carried to the endpoint, not played by it (audit
     * A-9). The name stays because firmware/tests/session-outage.sh reads it
     * off the summary line by that key. */
    uint32_t connections_that_played;
    /* Protocol v2: handshakes completed, servers pinned by this run,
     * refusals received from the server, and records each way. */
    uint32_t handshakes;
    uint32_t servers_pinned;
    uint32_t refusals_received;
    uint64_t records_received;
    uint64_t records_sent;
    /* Frames inside records that were rejected (one frame each, never the
     * session: a record's frame boundaries are known). */
    uint64_t rejected_frames;
    /* The last `sound` the server sent, kept across reconnects for the whole
     * run (a new session is not a reason to forget the room's sound, as a
     * new session is not a reason to play louder, ADR 0074), and how many
     * arrived. Read through chorus_session_last_sound. */
    uint8_t have_sound;
    chorus_v2_sound_t sound;
    uint32_t sounds_received;
    /* The fingerprint of the endpoint's own public key. */
    char key_fingerprint[CHORUS_NOISE_FINGERPRINT_LEN];
    char detail[512];
} chorus_session_result_t;

/* Run one session. Returns 0 when it ended because it ran its time, and -1
 * otherwise with `end` and `detail` saying why. */
int chorus_session_run(const chorus_session_config_t *config, chorus_session_result_t *out);

/* Keep `sound` as the last one received: what the session does with every
 * `sound` it decodes, exposed so a test can hold the store to it. */
void chorus_session_keep_sound(chorus_session_result_t *result, const chorus_v2_sound_t *sound);

/* The last `sound` received: 1 with `*out` set, or 0 when none has arrived
 * this run (the endpoint then plays flat, the catalog's defaults being the
 * server's to send). */
int chorus_session_last_sound(const chorus_session_result_t *result, chorus_v2_sound_t *out);

#endif /* CHORUS_SESSION_H */
