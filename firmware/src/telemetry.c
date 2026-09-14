#include "chorus/telemetry.h"

#include <inttypes.h>
#include <stdio.h>
#include <string.h>

const char *chorus_link_state_name(chorus_link_state_t state)
{
    switch (state) {
    case CHORUS_LINK_DOWN:
        return "down";
    case CHORUS_LINK_CONNECTING:
        return "connecting";
    case CHORUS_LINK_UP:
        return "up";
    }
    return "unknown";
}

const char *chorus_audio_state_name(chorus_audio_state_t state)
{
    switch (state) {
    case CHORUS_AUDIO_RUNNING:
        return "running";
    case CHORUS_AUDIO_STOPPED_ON_AMP_FAULT:
        return "stopped-on-amp-fault";
    case CHORUS_AUDIO_IDLE:
        return "idle";
    }
    return "unknown";
}

void chorus_telemetry_init(chorus_telemetry_t *telemetry)
{
    memset(telemetry, 0, sizeof(*telemetry));
    telemetry->link = CHORUS_LINK_DOWN;
    telemetry->audio = CHORUS_AUDIO_IDLE;
    telemetry->amp = CHORUS_AMP_OK;
    /* Wired until the committed configuration says otherwise. An endpoint that
     * has not been told which link it is on is not a wireless endpoint, and a
     * default of wireless would publish radio fields about a radio nobody
     * declared. */
    telemetry->transport = CHORUS_TRANSPORT_WIRED;
    telemetry->wifi = CHORUS_WIFI_NOT_WIRELESS;
    telemetry->wifi_ps_declared = CHORUS_WIFI_PS_UNKNOWN;
    telemetry->wifi_ps_in_force = CHORUS_WIFI_PS_UNKNOWN;
}

void chorus_telemetry_line(const chorus_telemetry_t *telemetry, char *out, size_t out_len)
{
    char offset[64];
    char round_trip[64];
    char bound[64];
    if (telemetry->offset_known) {
        snprintf(offset, sizeof(offset), "%" PRId64, telemetry->offset_ns);
        snprintf(round_trip, sizeof(round_trip), "%" PRIu64, telemetry->round_trip_ns);
        snprintf(bound, sizeof(bound), "%" PRIu64, telemetry->bound_ns);
    } else {
        /* `none` and never `0`: a bound of zero is a claim that the offset is
         * exact, and a client that has never completed an exchange is making
         * no claim at all. */
        snprintf(offset, sizeof(offset), "none");
        snprintf(round_trip, sizeof(round_trip), "none");
        snprintf(bound, sizeof(bound), "none");
    }

    char sequence[32];
    if (telemetry->have_sequence) {
        snprintf(sequence, sizeof(sequence), "%" PRIu32, telemetry->last_sequence);
    } else {
        snprintf(sequence, sizeof(sequence), "none");
    }

    char amp_fault[48];
    if (telemetry->amp_fault_read) {
        snprintf(amp_fault, sizeof(amp_fault), "0x%02x", telemetry->amp_fault_bits);
    } else {
        snprintf(amp_fault, sizeof(amp_fault), "none");
    }

    /* The radio. A wired endpoint prints `not-applicable` in all four fields
     * rather than omitting them, because a field that disappears cannot be told
     * from one that was never set; it is the same rule the three sync fields
     * follow when they print `none` rather than `0`. */
    char wifi[48];
    char ps_declared[32];
    char ps_in_force[32];
    char wireless_bound[32];
    if (telemetry->transport == CHORUS_TRANSPORT_WIRELESS) {
        snprintf(wifi, sizeof(wifi), "%s", chorus_wifi_status_name(telemetry->wifi));
        snprintf(ps_declared, sizeof(ps_declared), "%s",
                 chorus_wifi_ps_name(telemetry->wifi_ps_declared));
        if (telemetry->wifi_ps_read) {
            snprintf(ps_in_force, sizeof(ps_in_force), "%s",
                     chorus_wifi_ps_name(telemetry->wifi_ps_in_force));
        } else {
            /* `not-read` and never a mode name: a mode nobody asked the
             * platform for is not a mode in force. */
            snprintf(ps_in_force, sizeof(ps_in_force), "not-read");
        }
        /* `publishable` is a statement about configuration and readback, never
         * about a measured distribution. Nothing here has measured anything. */
        snprintf(wireless_bound, sizeof(wireless_bound), "%s",
                 telemetry->wireless_bound_publishable ? "publishable" : "withheld");
    } else {
        snprintf(wifi, sizeof(wifi), "not-applicable");
        snprintf(ps_declared, sizeof(ps_declared), "not-applicable");
        snprintf(ps_in_force, sizeof(ps_in_force), "not-applicable");
        snprintf(wireless_bound, sizeof(wireless_bound), "not-applicable");
    }

    snprintf(out, out_len,
             "chorus-endpoint: link=%s transport=%s wifi=%s wifi_ps_declared=%s "
             "wifi_ps_in_force=%s wireless_bound=%s rejoins=%" PRIu32 " attempts=%" PRIu32
             " audio=%s chunks=%" PRIu64 " frames=%" PRIu64 " sequence=%s skipped=%" PRIu64
             " exchanges=%" PRIu32 " offset_ns=%s round_trip_ns=%s bound_ns=%s amp=%s "
             "amp_fault_bits=%s",
             chorus_link_state_name(telemetry->link),
             chorus_transport_name(telemetry->transport), wifi, ps_declared, ps_in_force,
             wireless_bound, telemetry->rejoins, telemetry->connect_attempts,
             chorus_audio_state_name(telemetry->audio), telemetry->chunks_played,
             telemetry->frames_played, sequence, telemetry->skipped_frames, telemetry->exchanges,
             offset, round_trip, bound, chorus_amp_status_name(telemetry->amp), amp_fault);
}

void chorus_telemetry_record_wifi(chorus_telemetry_t *telemetry,
                                  const chorus_wifi_report_t *report)
{
    telemetry->transport = report->transport;
    telemetry->wifi = report->status;
    telemetry->wifi_ps_declared = report->declared;
    telemetry->wifi_ps_read = report->mode_read;
    telemetry->wifi_ps_in_force = report->in_force;
    telemetry->wireless_bound_publishable = report->bound_publishable;
    if (!report->link_up) {
        /* The whole of AC-17's last clause, in one line: a bring-up that
         * refused reports the link DOWN. There is no state of this struct that
         * refuses to join and publishes a link that is up. */
        telemetry->link = CHORUS_LINK_DOWN;
    }
}

void chorus_telemetry_record_amp(chorus_telemetry_t *telemetry, const chorus_amp_report_t *report)
{
    telemetry->amp = report->status;
    telemetry->amp_fault_read = report->fault_read;
    telemetry->amp_fault_bits = report->fault_bits;
    if (report->status != CHORUS_AMP_OK) {
        /* The whole of AC-5, in one line: a reported fault stops the audio. A
         * telemetry surface that recorded the fault and left `audio=running`
         * would be a system that surfaced a fault and carried on playing,
         * which is the thing the criterion forbids. */
        telemetry->audio = CHORUS_AUDIO_STOPPED_ON_AMP_FAULT;
    } else if (telemetry->audio == CHORUS_AUDIO_STOPPED_ON_AMP_FAULT) {
        telemetry->audio = CHORUS_AUDIO_IDLE;
    }
}
