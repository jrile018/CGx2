#include "../../facade/facade.h"

#ifndef MOCK_VERSION
#define MOCK_VERSION "2.6.0"
#endif

extern int cgx2_closure_dependency(void);
static double value;
static int alive;

const char *cgx2_library_identity(void) { return "cgx2.libtorch.cpu"; }
const char *cgx2_library_version(void) { return MOCK_VERSION; }
uint32_t cgx2_abi_version(void) { return 1; }
const char *cgx2_torch_last_error(void) { return "mock error"; }
int32_t cgx2_torch_f64_cpu(const double *values, uint64_t length, void **output) {
    if (!output || !values || length != 1 || cgx2_closure_dependency() != 7) return 1;
    value = values[0]; alive = 1; *output = &value; return 0;
}
int32_t cgx2_torch_add(const void *a, const void *b, void **output) {
    if (!a || !b || !output) return 1;
    value += *(const double *)b; *output = &value; return 0;
}
int32_t cgx2_torch_length(const void *handle, uint64_t *output) {
    if (!alive || handle != &value || !output) return 1;
    *output = 1; return 0;
}
int32_t cgx2_torch_copy_f64(const void *handle, double *output, uint64_t length) {
    if (!alive || handle != &value || !output || length != 1) return 1;
    output[0] = value; return 0;
}
int32_t cgx2_torch_release(void *handle) {
    if (!alive || handle != &value) return 1;
    alive = 0; return 0;
}
