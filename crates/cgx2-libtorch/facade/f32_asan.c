/* Standalone dynamic-tool harness for the mock C facade boundary. */
#include "facade.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    const float left[] = {1, 2, 3, 4, 5, 6};
    const float right[] = {7, 8, 9, 10, 11, 12};
    for (int cycle = 0; cycle < 12; ++cycle) {
        void *a = 0, *b = 0, *result = 0;
        uint64_t rows = 0, cols = 0;
        uint32_t dtype = 0;
        float output[6] = {-9, 0, 0, 0, 0, -9};
        assert(cgx2_torch_matrix_f32_cpu(left, 2, 3, &a) == 0);
        assert(cgx2_torch_matrix_f32_cpu(right, 3, 2, &b) == 0);
        assert(cgx2_torch_matmul_f32(a, b, &result) == 0);
        assert(cgx2_torch_matrix_f32_info(result, &rows, &cols, &dtype) == 0);
        assert(rows == 2 && cols == 2 && dtype == CGX2_TORCH_DTYPE_F32);
        assert(cgx2_torch_matrix_copy_f32(result, output + 1, 3) != 0);
        assert(cgx2_torch_matrix_copy_f32(result, output + 1, 4) == 0);
        assert(output[0] == -9 && output[5] == -9);
        assert(fabsf(output[1] - 58) < 1e-5f && fabsf(output[4] - 154) < 1e-5f);
        assert(cgx2_torch_release(result) == 0);
        assert(cgx2_torch_release(b) == 0);
        assert(cgx2_torch_release(a) == 0);
    }
    FILE *maps = fopen("/proc/self/maps", "r");
    assert(maps);
    char line[4096];
    while (fgets(line, sizeof line, maps)) assert(strstr(line, "libpython") == NULL);
    assert(fclose(maps) == 0);
    return 0;
}
