/* External-host handoff: sanitizer coverage of the fixture C++ ABI, not Rust
 * ownership/source integration or a fully instrumented external library. */
#include "../../sdk_v2.h"
#include <cassert>
extern "C" int32_t sdk_test_live();
int main() {
    for (int i = 0; i < 100; ++i) {
        void *raw = nullptr;
        assert(cgx2_sdk_create(42, &raw) == 0);
        int64_t value = 0;
        assert(cgx2_sdk_read(raw, &value) == 0 && value == 42);
        assert(cgx2_sdk_release(raw) == 0);
        raw = nullptr;
        assert(cgx2_sdk_create(-7, &raw) == 17 && raw != nullptr);
        assert(cgx2_sdk_release(raw) == 0);
        assert(sdk_test_live() == 0);
    }
}
