#ifndef CGX2_LIBTORCH_FACADE_H
#define CGX2_LIBTORCH_FACADE_H
#include <stdint.h>
#if defined(__GNUC__)
#define CGX2_EXPORT __attribute__((visibility("default")))
#else
#define CGX2_EXPORT
#endif
#ifdef __cplusplus
extern "C" {
#endif
CGX2_EXPORT const char *cgx2_library_identity(void);
CGX2_EXPORT const char *cgx2_library_version(void);
CGX2_EXPORT uint32_t cgx2_abi_version(void);
CGX2_EXPORT const char *cgx2_torch_last_error(void);
CGX2_EXPORT int32_t cgx2_torch_f64_cpu(const double *values, uint64_t length, void **output);
CGX2_EXPORT int32_t cgx2_torch_add(const void *left, const void *right, void **output);
CGX2_EXPORT int32_t cgx2_torch_length(const void *tensor, uint64_t *output);
CGX2_EXPORT int32_t cgx2_torch_copy_f64(const void *tensor, double *output, uint64_t capacity);
CGX2_EXPORT int32_t cgx2_torch_release(void *tensor);
#ifdef __cplusplus
}
#endif
#endif
