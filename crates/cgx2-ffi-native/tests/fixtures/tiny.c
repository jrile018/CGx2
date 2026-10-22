#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static int64_t live_handles = 0;

const char *cgx2_library_identity(void) { return "cgx2.test.tiny"; }
const char *cgx2_library_version(void) { return "1.0.0"; }
uint32_t cgx2_abi_version(void) { return 1; }

int32_t tiny_double(int64_t input, int64_t *output) {
    if (input < 0 || input > INT64_MAX / 2) return 17;
    *output = input * 2;
    return 0;
}

int32_t tiny_bad_result(int64_t input, int64_t *output) {
    (void)input;
    (void)output;
    return 23;
}

int32_t tiny_copy(const uint8_t *input, uint64_t length, uint8_t *output, uint64_t capacity) {
    if (length > capacity) return 18;
    if (length) memcpy(output, input, (size_t)length);
    return 0;
}

int32_t tiny_create(int64_t value, void **output) {
    int64_t *handle = malloc(sizeof(int64_t));
    if (!handle) return 19;
    *handle = value;
    *output = handle;
    live_handles++;
    return 0;
}

int32_t tiny_create_partial_failure(int64_t value, void **output) {
    int32_t status = tiny_create(value, output);
    return status ? status : 20;
}

int32_t tiny_read(void *handle, int64_t *output) {
    if (!handle) return 21;
    *output = *(int64_t *)handle;
    return 0;
}

int32_t tiny_release(void *handle) {
    if (!handle) return 22;
    free(handle);
    live_handles--;
    return 0;
}

int32_t tiny_release_retry(void *handle) {
    static int first_failure = 1;
    if (first_failure) {
        first_failure = 0;
        return 24;
    }
    return tiny_release(handle);
}

int32_t tiny_live(int64_t unused, int64_t *output) {
    (void)unused;
    *output = live_handles;
    return 0;
}
