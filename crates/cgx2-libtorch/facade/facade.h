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
/* CGx2 facade ABI v1 over a version-pinned torch/torch.h build. This is not
 * LibTorch's stable C ABI. All tensor handles are opaque and facade-owned.
 * Callers borrow handles for add/length/copy and release each owned output
 * exactly once. All tensor operations return zero on success or nonzero on
 * failure; last_error is thread-local and valid until the next facade call.
 * create copies length doubles in, and copy requires capacity == length. */
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
