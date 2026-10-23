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
#ifndef MOCK_MATRIX_API_VERSION
#define MOCK_MATRIX_API_VERSION 2
#endif

typedef struct {
    int alive;
    int profile;
    uint64_t length;
    uint64_t rows;
    uint64_t cols;
    double values[8];
} MockTensor;
static MockTensor slots[64];
static uint64_t next_slot;
static uint64_t live_count;
static uint64_t releases;
static const char *error_message;
static char long_error[8193];

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
    if (length && values[0] == -113) {
        memset(long_error, 'x', sizeof(long_error) - 1);
        error_message = long_error;
        return 7;
    }
    if (length && (values[0] == -109 || values[0] == -110) && next_slot) {
        *output = &slots[0]; /* deliberately violate distinct-handle ABI */
        if (values[0] == -110) {
            error_message = "injected failure with aliased handle";
            return 7;
        }
        return 0;
    }
    if (length && values[0] == -102) return 0; /* broken success: no handle */
    MockTensor *tensor = allocate();
    if (!tensor) return 1;
    tensor->length = length;
    tensor->profile = length && values[0] <= -103 && values[0] >= -112 ? (int)-values[0] : 0;
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
    if (a->profile == 112) {
        *output = a; /* deliberately violate distinct-handle ABI */
        return 0;
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
    if (tensor->profile == 111) {
        error_message = "release failed after teardown";
        return 6;
    }
    return 0;
}

uint32_t cgx2_torch_matrix_api_version(void) { return MOCK_MATRIX_API_VERSION; }

int32_t cgx2_torch_matrix_f64_cpu(const double *values, uint64_t rows, uint64_t cols, void **output) {
    if (output) *output = NULL;
    if (!output || !values || !rows || !cols || rows > 8 / cols) {
        error_message = "invalid matrix input";
        return 1;
    }
    if (values[0] == -203 && next_slot) {
        *output = &slots[0];
        return 0;
    }
    MockTensor *matrix = allocate();
    if (!matrix) return 1;
    matrix->rows = rows;
    matrix->cols = cols;
    matrix->length = rows * cols;
    matrix->profile = values[0] <= -201 && values[0] >= -206 ? (int)-values[0] : 0;
    memcpy(matrix->values, values, (size_t)matrix->length * sizeof(double));
    *output = matrix;
    if (matrix->profile == 204) {
        error_message = "injected matrix create failure with handle";
        return 7;
    }
    return 0;
}

int32_t cgx2_torch_matmul(const void *left, const void *right, void **output) {
    if (output) *output = NULL;
    MockTensor *a = checked(left);
    MockTensor *b = checked(right);
    if (!output || !a || !b || !a->rows || !b->rows || a->cols != b->rows || a->rows > 8 / b->cols) {
        error_message = "matrix shape mismatch";
        return 2;
    }
    if (a->profile == 205) {
        *output = a; /* deliberately violate distinct-handle ABI */
        return 0;
    }
    MockTensor *product = allocate();
    if (!product) return 2;
    product->rows = a->rows;
    product->cols = b->cols;
    product->length = product->rows * product->cols;
    for (uint64_t r = 0; r < product->rows; ++r)
        for (uint64_t c = 0; c < product->cols; ++c) {
            double sum = 0;
            for (uint64_t k = 0; k < a->cols; ++k)
                sum += a->values[r * a->cols + k] * b->values[k * b->cols + c];
            product->values[r * product->cols + c] = sum;
        }
    *output = product;
    if (a->profile == 206) {
        error_message = "injected matmul failure with handle";
        return 7;
    }
    return 0;
}

int32_t cgx2_torch_matrix_shape(const void *raw, uint64_t *rows, uint64_t *cols) {
    MockTensor *matrix = checked(raw);
    if (!matrix || !rows || !cols || !matrix->rows) return 3;
    *rows = matrix->rows + (matrix->profile == 202);
    *cols = matrix->cols;
    return 0;
}

#ifndef MOCK_MISSING_MATRIX_COPY
int32_t cgx2_torch_matrix_copy_f64(const void *raw, double *output, uint64_t capacity) {
    MockTensor *matrix = checked(raw);
    if (!matrix || !matrix->rows || !output || capacity != matrix->length) {
        error_message = "matrix capacity mismatch";
        return 4;
    }
    if (matrix->profile == 201) {
        error_message = "injected matrix copy failure";
        return 5;
    }
    memcpy(output, matrix->values, (size_t)capacity * sizeof(double));
    return 0;
}
#endif
