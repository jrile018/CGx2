#include <stdint.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef FIXTURE_MULTIPLIER
#define FIXTURE_MULTIPLIER 2
#endif

__attribute__((constructor)) static void fixture_constructor(void) {
    const char *path = getenv("CGX2_NATIVE_CONSTRUCTOR_MARKER");
    if (path == NULL) return;
    FILE *file = fopen(path, "w");
    if (file != NULL) {
        fputs("loaded\n", file);
        fclose(file);
    }
}

const char *cgx2_library_identity(void) { return "native_fixture"; }
const char *cgx2_library_version(void) { return "1.0.0"; }
uint32_t cgx2_abi_version(void) { return 1; }

int32_t fixture_double(int64_t input, int64_t *output) {
    if (input < 0 || input > INT64_MAX / FIXTURE_MULTIPLIER) return 17;
    *output = input * FIXTURE_MULTIPLIER;
    return 0;
}
