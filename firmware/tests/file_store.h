/* The store seam (chorus/store.h) over a directory of files: what the host
 * session binary keeps across runs, so a test can "reboot" an endpoint by
 * starting the binary again over the same directory.
 *
 * One file per key, mode 0600 in a directory of mode 0700, replaced by writing
 * a temporary beside it and renaming it over the old one: a set replaces the
 * whole value or does nothing, which is the seam's contract. Host-only and
 * outside firmware/src on purpose: the board's medium is NVS
 * (firmware/main/esp_store.c). */

#ifndef CHORUS_FILE_STORE_H
#define CHORUS_FILE_STORE_H

#include "chorus/store.h"

typedef struct {
    char directory[384];
} file_store_t;

/* Use `directory`, making it if it is absent. 0, or -1 with why in `detail`. */
int file_store_open(file_store_t *files, const char *directory, char *detail, size_t detail_len);

chorus_store_t file_store_as_store(file_store_t *files);

#endif /* CHORUS_FILE_STORE_H */
