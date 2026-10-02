#include "esp_identity.h"

#include <stdio.h>
#include <string.h>

#include "esp_log.h"
#include "esp_store.h"

#include "chorus/identity.h"
#include "chorus/noise.h"

static const char *TAG = "chorus-identity";

int chorus_esp_identity_load(chorus_session_config_t *session)
{
    /* The store is brought up by whoever needs it first; a second call is
     * free. */
    (void)chorus_esp_store_init();
    const chorus_store_t *store = chorus_esp_store();
    if (store == NULL) {
        ESP_LOGE(TAG, "no store: this board cannot keep an identity, so no session is opened");
        return -1;
    }
    if (chorus_noise_setup() != CHORUS_NOISE_OK) {
        ESP_LOGE(TAG, "the crypto library did not start");
        return -1;
    }

    char id[CHORUS_IDENTITY_ID_LEN + 1];
    int id_created = 0;
    chorus_identity_status_t status = chorus_identity_id(
        store, session->random, session->random_ctx, id, sizeof(id), &id_created);
    if (status != CHORUS_IDENTITY_OK) {
        ESP_LOGE(TAG, "the endpoint id could not be read from or kept in the store: %s",
                 chorus_identity_status_name(status));
        return -1;
    }

    /* The key is read here only to say its fingerprint at boot and to stop
     * before the network if it cannot be kept; the session reads it again
     * from the same store when it runs. */
    uint8_t secret[CHORUS_NOISE_KEY_LEN];
    int key_created = 0;
    status =
        chorus_identity_secret(store, session->random, session->random_ctx, secret, &key_created);
    if (status != CHORUS_IDENTITY_OK) {
        ESP_LOGE(TAG, "the endpoint's key could not be read from or kept in the store: %s",
                 chorus_identity_status_name(status));
        return -1;
    }
    chorus_noise_keypair_t pair;
    chorus_noise_status_t paired = chorus_noise_keypair_from_secret(secret, &pair);
    memset(secret, 0, sizeof(secret));
    if (paired != CHORUS_NOISE_OK) {
        ESP_LOGE(TAG, "the stored key could not be used: %s", chorus_noise_status_name(paired));
        return -1;
    }
    char fingerprint[CHORUS_NOISE_FINGERPRINT_LEN];
    chorus_noise_fingerprint(pair.public_key, fingerprint);
    memset(&pair, 0, sizeof(pair));

    session->store = store;
    snprintf(session->endpoint_id, sizeof(session->endpoint_id), "%s", id);
    ESP_LOGI(TAG, "identity id=%s key=%s id_made_this_boot=%d key_made_this_boot=%d", id,
             fingerprint, id_created, key_created);
    return 0;
}
