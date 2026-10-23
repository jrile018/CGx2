#include "../../sdk_v2.h"
#include <cstring>
#include <stdexcept>
#ifndef SDK_VERSION
#define SDK_VERSION 2
#endif
#ifndef SDK_ID
#define SDK_ID "sdk_fixture"
#endif
#ifndef SDK_ABI
#define SDK_ABI 1
#endif
static int live = 0;
static int releases = 0;
static int64_t *last = nullptr;
template <typename F> static int32_t translated(F body) noexcept {
    try { return body(); } catch (...) { return 17; }
}
extern "C" {
const char *cgx2_library_identity() { return SDK_ID; }
const char *cgx2_library_version() { return "1.0.0"; }
uint32_t cgx2_abi_version() { return SDK_ABI; }
uint32_t cgx2_sdk_api_version() { return SDK_VERSION; }
int32_t cgx2_sdk_copy_bytes(const uint8_t *input, uint64_t length, uint8_t *out, uint64_t cap, uint64_t *written) {
    return translated([&]() -> int32_t {
        if (length && input[0] == '!') throw std::runtime_error("contained");
        if (length && input[0] == '?') { *written = cap + 1; return 0; }
        if (length > cap / 2) return 3;
        std::memcpy(out, input, length);
        std::memcpy(out + length, input, length);
        *written = length * 2;
        return 0;
    });
}
#ifndef MISSING_UTF8
int32_t cgx2_sdk_copy_utf8(const uint8_t *input, uint64_t length, uint8_t *out, uint64_t cap, uint64_t *written) {
    if (length && input[0] == '~' && cap) { out[0] = 255; *written = 1; return 0; }
    return cgx2_sdk_copy_bytes(input, length, out, cap, written);
}
#endif
int32_t cgx2_sdk_create(int64_t value, void **output) {
    *output = nullptr;
    return translated([&]() -> int32_t {
        if (value == -8) { *output = last; return 0; }
        *output = last = new int64_t(value);
        ++live;
        if (value == -7) throw std::runtime_error("owned output before exception");
        return 0;
    });
}
int32_t cgx2_sdk_read(const void *input, int64_t *output) {
    *output = *static_cast<const int64_t *>(input);
    return 0;
}
int32_t cgx2_sdk_release(void *input) {
    auto *value = static_cast<int64_t *>(input);
    int status = *value == -9 ? 9 : 0;
    delete value;
    --live;
    ++releases;
    return status;
}
int32_t sdk_test_live() { return live; }
int32_t sdk_test_releases() { return releases; }
}
