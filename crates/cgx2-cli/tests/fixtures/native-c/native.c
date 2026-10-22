#include <stdint.h>
#include <limits.h>

const char *cgx2_library_identity(void) { return "native_fixture"; }
const char *cgx2_library_version(void) { return "1.0.0"; }
uint32_t cgx2_abi_version(void) { return 1; }

int32_t fixture_double(int64_t input, int64_t *output) {
    if (input < 0 || input > INT64_MAX / 2) return 17;
    *output = input * 2;
    return 0;
}
