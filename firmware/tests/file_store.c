#include "file_store.h"

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

int file_store_open(file_store_t *files, const char *directory, char *detail, size_t detail_len)
{
    if (strlen(directory) >= sizeof(files->directory)) {
        snprintf(detail, detail_len, "the store directory's path is too long");
        return -1;
    }
    snprintf(files->directory, sizeof(files->directory), "%s", directory);
    if (mkdir(directory, 0700) != 0 && errno != EEXIST) {
        snprintf(detail, detail_len, "%s could not be made: %s", directory, strerror(errno));
        return -1;
    }
    struct stat found;
    if (stat(directory, &found) != 0 || !S_ISDIR(found.st_mode)) {
        snprintf(detail, detail_len, "%s is not a directory", directory);
        return -1;
    }
    return 0;
}

/* The key was held to [a-z0-9_] by chorus_store_* before it came here, so it
 * names a file in the directory and nothing outside it. */
static int path_of(const file_store_t *files, const char *key, const char *suffix, char *out,
                   size_t capacity)
{
    int wrote = snprintf(out, capacity, "%s/%s%s", files->directory, key, suffix);
    return (wrote > 0 && (size_t)wrote < capacity) ? 0 : -1;
}

static chorus_store_status_t file_get(void *context, const char *key, void *out, size_t capacity,
                                      size_t *length)
{
    const file_store_t *files = (const file_store_t *)context;
    char path[512];
    if (path_of(files, key, "", path, sizeof(path)) != 0) {
        return CHORUS_STORE_FAILED;
    }
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        return (errno == ENOENT) ? CHORUS_STORE_MISSING : CHORUS_STORE_FAILED;
    }
    size_t got = (capacity > 0) ? fread(out, 1, capacity, file) : 0;
    int more = fgetc(file) != EOF;
    int failed = ferror(file);
    fclose(file);
    if (failed) {
        return CHORUS_STORE_FAILED;
    }
    if (more) {
        return CHORUS_STORE_TOO_LARGE;
    }
    *length = got;
    return CHORUS_STORE_OK;
}

static chorus_store_status_t file_set(void *context, const char *key, const void *value,
                                      size_t length)
{
    const file_store_t *files = (const file_store_t *)context;
    char path[512];
    char temporary[512];
    if (path_of(files, key, "", path, sizeof(path)) != 0 ||
        path_of(files, key, ".writing", temporary, sizeof(temporary)) != 0) {
        return CHORUS_STORE_FAILED;
    }
    int fd = open(temporary, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) {
        return CHORUS_STORE_FAILED;
    }
    ssize_t wrote = (length > 0) ? write(fd, value, length) : 0;
    int synced = fsync(fd);
    int closed = close(fd);
    if (wrote != (ssize_t)length || synced != 0 || closed != 0 || rename(temporary, path) != 0) {
        (void)unlink(temporary);
        return CHORUS_STORE_FAILED;
    }
    return CHORUS_STORE_OK;
}

static chorus_store_status_t file_erase(void *context, const char *key)
{
    const file_store_t *files = (const file_store_t *)context;
    char path[512];
    if (path_of(files, key, "", path, sizeof(path)) != 0) {
        return CHORUS_STORE_FAILED;
    }
    if (unlink(path) != 0 && errno != ENOENT) {
        return CHORUS_STORE_FAILED;
    }
    return CHORUS_STORE_OK;
}

chorus_store_t file_store_as_store(file_store_t *files)
{
    chorus_store_t store;
    store.context = files;
    store.get = file_get;
    store.set = file_set;
    store.erase = file_erase;
    return store;
}
