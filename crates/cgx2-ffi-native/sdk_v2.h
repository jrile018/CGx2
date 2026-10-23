#ifndef CGX2_NATIVE_SDK_V2_H
#define CGX2_NATIVE_SDK_V2_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Metadata: valid immutable NUL-terminated strings for DSO lifetime; ABI=1,
 * SDK API=2. All functions calling-thread-only, no callbacks or unwinding.
 * Rust: catch_unwind INSIDE extern C; C++: catch (...) INSIDE every entry.
 * Inputs borrowed for this call; copy outputs never retained or foreign-owned.
 * Success: written <= capacity. Failure: no published result. No size probes.
 * create: distinct owned output, null initially; any nonnull failure output is
 * caller owned too. read borrows. release ALWAYS consumes, even on error.
 * Shared ownership is immutable adapter-side aliasing; final alias releases. */
const char *cgx2_library_identity(void);
const char *cgx2_library_version(void);
uint32_t cgx2_abi_version(void);
uint32_t cgx2_sdk_api_version(void);
int32_t cgx2_sdk_copy_bytes(const uint8_t *, uint64_t, uint8_t *, uint64_t, uint64_t *written);
int32_t cgx2_sdk_copy_utf8(const uint8_t *, uint64_t, uint8_t *, uint64_t, uint64_t *written);
int32_t cgx2_sdk_create(int64_t, void **);
int32_t cgx2_sdk_read(const void *, int64_t *);
int32_t cgx2_sdk_release(void *);
#ifdef __cplusplus
}
#endif
#endif
