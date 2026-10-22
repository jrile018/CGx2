/* Test-only C facade. This is not a LibTorch implementation. */
#include "facade.h"
#include <stddef.h>
#include <string.h>

#ifndef MOCK_VERSION
#define MOCK_VERSION "2.6.0"
#endif
#ifndef MOCK_ABI
#define MOCK_ABI 1
#endif
#ifndef MOCK_IDENTITY
#define MOCK_IDENTITY "cgx2.libtorch.cpu"
#endif

typedef struct {
    int alive;
    int profile;
    uint64_t length;
    double values[8];
} MockTensor;
static MockTensor slots[64];
static uint64_t next_slot;
static uint64_t live_count;
static uint64_t releases;
static const char *error_message;

const char *cgx2_library_identity(void) { return MOCK_IDENTITY; }
const char *cgx2_library_version(void) { return MOCK_VERSION; }
uint32_t cgx2_abi_version(void) { return MOCK_ABI; }
const char *cgx2_torch_last_error(void) { return error_message ? error_message : ""; }
uint64_t cgx2_mock_live_count(void) { return live_count; }
uint64_t cgx2_mock_release_count(void) { return releases; }

static MockTensor *checked(const void *raw) {
    for (uint64_t i = 0; i < next_slot; ++i) {
        if (raw == &slots[i] && slots[i].alive) return &slots[i];
    }
    error_message = "invalid or released handle";
    return NULL;
}
static MockTensor *allocate(void) {
    if (next_slot == 64) {
        error_message = "mock slots exhausted";
        return NULL;
    }
    MockTensor *tensor = &slots[next_slot++];
    tensor->alive = 1;
    ++live_count;
    return tensor;
}

int32_t cgx2_torch_f64_cpu(const double *values, uint64_t length, void **output) {
    if (output) *output = NULL;
    if (!output || (length && !values) || length > 8) {
        error_message = "invalid tensor input";
        return 1;
    }
    if (length && values[0] == -102) return 0; /* broken success: no handle */
    MockTensor *tensor = allocate();
    if (!tensor) return 1;
    tensor->length = length;
    tensor->profile = length && values[0] <= -103 && values[0] >= -108 ? (int)-values[0] : 0;
    if (length) memcpy(tensor->values, values, (size_t)length * sizeof(double));
    *output = tensor;
    if (length && values[0] == -101) {
        error_message = "injected create failure with handle";
        return 7;
    }
    return 0;
}
int32_t cgx2_torch_add(const void *left, const void *right, void **output) {
    if (output) *output = NULL;
    MockTensor *a = checked(left);
    MockTensor *b = checked(right);
    if (!output || !a || !b || a->length != b->length) {
        error_message = "shape mismatch or invalid handle";
        return 2;
    }
    MockTensor *sum = allocate();
    if (!sum) return 2;
    sum->length = a->length;
    for (uint64_t i = 0; i < a->length; ++i) sum->values[i] = a->values[i] + b->values[i];
    *output = sum;
    return 0;
}
int32_t cgx2_torch_length(const void *raw, uint64_t *output) {
    MockTensor *tensor = checked(raw);
    if (!tensor || !output) return 3;
    if (tensor->profile == 103 || tensor->profile == 104 || tensor->profile == 105) {
        error_message = tensor->profile == 103 ? "invalid dtype" :
                        tensor->profile == 104 ? "invalid device" : "invalid shape";
        return 3;
    }
    *output = tensor->profile == 108 ? UINT64_MAX :
              tensor->profile == 106 ? tensor->length + 1 : tensor->length;
    return 0;
}
#ifndef MOCK_MISSING_COPY
int32_t cgx2_torch_copy_f64(const void *raw, double *output, uint64_t capacity) {
    MockTensor *tensor = checked(raw);
    if (!tensor || capacity != tensor->length || (tensor->length && !output)) {
        error_message = "output buffer too small";
        return 4;
    }
    if (tensor->profile == 107) {
        error_message = "injected copy failure";
        return 5;
    }
    if (tensor->length) memcpy(output, tensor->values, (size_t)tensor->length * sizeof(double));
    return 0;
}
#endif
int32_t cgx2_torch_release(void *raw) {
    MockTensor *tensor = checked(raw);
    if (!tensor) return 6;
    tensor->alive = 0;
    --live_count;
    ++releases;
    return 0;
}
