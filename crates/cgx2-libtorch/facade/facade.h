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
 * Each successful create/add returns a distinct owned handle, including when
 * tensor storage is shared. create copies length doubles in, and copy requires
 * capacity == length. */
CGX2_EXPORT const char *cgx2_library_identity(void);
CGX2_EXPORT const char *cgx2_library_version(void);
CGX2_EXPORT uint32_t cgx2_abi_version(void);
CGX2_EXPORT const char *cgx2_torch_last_error(void);
CGX2_EXPORT int32_t cgx2_torch_f64_cpu(const double *values, uint64_t length, void **output);
CGX2_EXPORT int32_t cgx2_torch_add(const void *left, const void *right, void **output);
CGX2_EXPORT int32_t cgx2_torch_length(const void *tensor, uint64_t *output);
CGX2_EXPORT int32_t cgx2_torch_copy_f64(const void *tensor, double *output, uint64_t capacity);
CGX2_EXPORT int32_t cgx2_torch_release(void *tensor);
/* Additive matrix API version 2 on facade ABI v1. Matrix handles are owned
 * outputs, borrowed by math/shape/copy, and released by cgx2_torch_release.
 * CPU float64 values are contiguous row-major. Dimensions are nonzero and
 * rows*cols is at most 1,048,576; copy capacity must equal rows*cols. */
CGX2_EXPORT uint32_t cgx2_torch_matrix_api_version(void);
CGX2_EXPORT int32_t cgx2_torch_matrix_f64_cpu(const double *values, uint64_t rows, uint64_t cols, void **output);
CGX2_EXPORT int32_t cgx2_torch_matmul(const void *left, const void *right, void **output);
CGX2_EXPORT int32_t cgx2_torch_matrix_shape(const void *matrix, uint64_t *rows, uint64_t *cols);
CGX2_EXPORT int32_t cgx2_torch_matrix_copy_f64(const void *matrix, double *output, uint64_t capacity);
/* Additive matrix API v3. Dtype code 1 is CPU float32. Values are copied in
 * and out; each successful output is a distinct owned handle. Capacity must
 * equal rows * cols and handles use the existing release entry point. */
#define CGX2_TORCH_DTYPE_F32 1u
CGX2_EXPORT uint32_t cgx2_torch_matrix_f32_api_version(void);
CGX2_EXPORT int32_t cgx2_torch_matrix_f32_cpu(const float *values, uint64_t rows, uint64_t cols, void **output);
CGX2_EXPORT int32_t cgx2_torch_matmul_f32(const void *left, const void *right, void **output);
CGX2_EXPORT int32_t cgx2_torch_matrix_f32_info(const void *matrix, uint64_t *rows, uint64_t *cols, uint32_t *dtype);
CGX2_EXPORT int32_t cgx2_torch_matrix_copy_f32(const void *matrix, float *output, uint64_t capacity);
/* Additive inference API v4 on ABI v1: exact-shape f32 CPU add and ReLU.
 * Inputs borrowed; outputs distinct/owned and finite. No broadcasting, retained
 * caller buffers, autograd, callbacks or foreign exceptions across this ABI. */
CGX2_EXPORT uint32_t cgx2_torch_inference_api_version(void);
CGX2_EXPORT int32_t cgx2_torch_matrix_add_f32(const void *, const void *, void **);
CGX2_EXPORT int32_t cgx2_torch_matrix_relu_f32(const void *, void **);
#ifdef __cplusplus
}
#endif
#endif
