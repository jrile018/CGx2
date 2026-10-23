#include "facade.h"
#include <torch/torch.h>
#include <cmath>
#include <exception>
#include <cstring>
#include <limits>
#include <new>
#include <stdexcept>
#include <utility>

namespace {
constexpr size_t MAX_ERROR_BYTES = 4096;
constexpr uint64_t MAX_MATRIX_ELEMENTS = 1048576;
thread_local char last_error[MAX_ERROR_BYTES + 1] = {};

void set_error(const char *message) noexcept {
    size_t length = 0;
    if (message) {
        while (length < MAX_ERROR_BYTES && message[length] != '\0') {
            last_error[length] = message[length];
            ++length;
        }
    }
    last_error[length] = '\0';
}

template <typename F> int32_t translate(F &&body) noexcept {
    try {
        last_error[0] = '\0';
        body();
        return 0;
    } catch (const std::exception &error) {
        set_error(error.what());
        return 1;
    } catch (...) {
        set_error("unknown native LibTorch exception");
        return 2;
    }
}

const torch::Tensor &checked(const void *raw) {
    if (!raw) throw std::invalid_argument("null tensor handle");
    const auto &tensor = *static_cast<const torch::Tensor *>(raw);
    if (!tensor.device().is_cpu() || tensor.scalar_type() != torch::kFloat64 || tensor.dim() != 1)
        throw std::invalid_argument("expected one-dimensional CPU float64 tensor");
    return tensor;
}

uint64_t matrix_elements(uint64_t rows, uint64_t cols) {
    if (!rows || !cols || rows > static_cast<uint64_t>(std::numeric_limits<int64_t>::max()) ||
        cols > static_cast<uint64_t>(std::numeric_limits<int64_t>::max()) ||
        rows > MAX_MATRIX_ELEMENTS / cols || rows > SIZE_MAX / sizeof(double) / cols)
        throw std::invalid_argument("invalid or excessive matrix shape");
    return rows * cols;
}

const torch::Tensor &checked_matrix(const void *raw) {
    if (!raw) throw std::invalid_argument("null matrix handle");
    const auto &tensor = *static_cast<const torch::Tensor *>(raw);
    if (!tensor.device().is_cpu() || tensor.scalar_type() != torch::kFloat64 || tensor.dim() != 2 ||
        !tensor.is_contiguous())
        throw std::invalid_argument("expected two-dimensional CPU float64 matrix");
    matrix_elements(static_cast<uint64_t>(tensor.size(0)), static_cast<uint64_t>(tensor.size(1)));
    return tensor;
}

const torch::Tensor &checked_matrix_f32(const void *raw) {
    if (!raw) throw std::invalid_argument("null float32 matrix handle");
    const auto &tensor = *static_cast<const torch::Tensor *>(raw);
    if (!tensor.device().is_cpu() || tensor.scalar_type() != torch::kFloat32 || tensor.dim() != 2 ||
        !tensor.is_contiguous())
        throw std::invalid_argument("expected two-dimensional CPU float32 matrix");
    const auto count = matrix_elements(static_cast<uint64_t>(tensor.size(0)),
                                       static_cast<uint64_t>(tensor.size(1)));
    if (count > SIZE_MAX / sizeof(float)) throw std::invalid_argument("float32 matrix byte overflow");
    return tensor;
}

void finite_f32(const float *values, uint64_t count) {
    for (uint64_t i = 0; i < count; ++i)
        if (!std::isfinite(values[i])) throw std::invalid_argument("nonfinite float32 matrix value");
}
}

extern "C" {
const char *cgx2_library_identity(void) { return "cgx2.libtorch.cpu"; }
const char *cgx2_library_version(void) { return CGX2_TORCH_VERSION; }
uint32_t cgx2_abi_version(void) { return 1; }
const char *cgx2_torch_last_error(void) { return last_error; }

int32_t cgx2_torch_f64_cpu(const double *values, uint64_t length, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        if (!output || (length && !values) || length > static_cast<uint64_t>(std::numeric_limits<int64_t>::max()) || length > SIZE_MAX / sizeof(double))
            throw std::invalid_argument("invalid tensor input");
        auto tensor = torch::empty({static_cast<int64_t>(length)}, torch::TensorOptions().dtype(torch::kFloat64).device(torch::kCPU));
        if (length) std::memcpy(tensor.data_ptr<double>(), values, static_cast<size_t>(length) * sizeof(double));
        *output = new torch::Tensor(std::move(tensor));
    });
}

int32_t cgx2_torch_add(const void *left, const void *right, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        if (!output) throw std::invalid_argument("null output");
        const auto &a = checked(left);
        const auto &b = checked(right);
        if (a.size(0) != b.size(0)) throw std::invalid_argument("shape mismatch");
        *output = new torch::Tensor(a + b);
    });
}

int32_t cgx2_torch_length(const void *raw, uint64_t *output) {
    return translate([&] {
        if (!output) throw std::invalid_argument("null output");
        *output = static_cast<uint64_t>(checked(raw).size(0));
    });
}

int32_t cgx2_torch_copy_f64(const void *raw, double *output, uint64_t capacity) {
    return translate([&] {
        const auto &tensor = checked(raw);
        auto length = static_cast<uint64_t>(tensor.size(0));
        if (capacity != length || (length && !output)) throw std::invalid_argument("output buffer length mismatch");
        if (length) std::memcpy(output, tensor.data_ptr<double>(), static_cast<size_t>(length) * sizeof(double));
    });
}

int32_t cgx2_torch_release(void *raw) {
    return translate([&] {
        if (!raw) throw std::invalid_argument("null tensor handle");
        delete static_cast<torch::Tensor *>(raw);
    });
}

uint32_t cgx2_torch_matrix_api_version(void) { return 2; }

int32_t cgx2_torch_matrix_f64_cpu(const double *values, uint64_t rows, uint64_t cols, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        const auto count = matrix_elements(rows, cols);
        if (!output || !values) throw std::invalid_argument("invalid matrix input");
        auto tensor = torch::empty({static_cast<int64_t>(rows), static_cast<int64_t>(cols)},
                                   torch::TensorOptions().dtype(torch::kFloat64).device(torch::kCPU));
        std::memcpy(tensor.data_ptr<double>(), values, static_cast<size_t>(count) * sizeof(double));
        *output = new torch::Tensor(std::move(tensor));
    });
}

int32_t cgx2_torch_matmul(const void *left, const void *right, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        if (!output) throw std::invalid_argument("null output");
        const auto &a = checked_matrix(left);
        const auto &b = checked_matrix(right);
        if (a.size(1) != b.size(0)) throw std::invalid_argument("matrix shape mismatch");
        matrix_elements(static_cast<uint64_t>(a.size(0)), static_cast<uint64_t>(b.size(1)));
        *output = new torch::Tensor(torch::matmul(a, b));
    });
}

int32_t cgx2_torch_matrix_shape(const void *raw, uint64_t *rows, uint64_t *cols) {
    return translate([&] {
        if (!rows || !cols) throw std::invalid_argument("null shape output");
        const auto &matrix = checked_matrix(raw);
        *rows = static_cast<uint64_t>(matrix.size(0));
        *cols = static_cast<uint64_t>(matrix.size(1));
    });
}

int32_t cgx2_torch_matrix_copy_f64(const void *raw, double *output, uint64_t capacity) {
    return translate([&] {
        const auto &matrix = checked_matrix(raw);
        const auto count = matrix_elements(static_cast<uint64_t>(matrix.size(0)),
                                           static_cast<uint64_t>(matrix.size(1)));
        if (!output || capacity != count) throw std::invalid_argument("matrix output capacity mismatch");
        std::memcpy(output, matrix.data_ptr<double>(), static_cast<size_t>(count) * sizeof(double));
    });
}

uint32_t cgx2_torch_matrix_f32_api_version(void) { return 3; }

int32_t cgx2_torch_matrix_f32_cpu(const float *values, uint64_t rows, uint64_t cols, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        const auto count = matrix_elements(rows, cols);
        if (!output || !values || count > SIZE_MAX / sizeof(float))
            throw std::invalid_argument("invalid float32 matrix input");
        finite_f32(values, count);
        auto tensor = torch::empty({static_cast<int64_t>(rows), static_cast<int64_t>(cols)},
                                   torch::TensorOptions().dtype(torch::kFloat32).device(torch::kCPU));
        std::memcpy(tensor.data_ptr<float>(), values, static_cast<size_t>(count) * sizeof(float));
        *output = new torch::Tensor(std::move(tensor));
    });
}

int32_t cgx2_torch_matmul_f32(const void *left, const void *right, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        if (!output) throw std::invalid_argument("null output");
        const auto &a = checked_matrix_f32(left);
        const auto &b = checked_matrix_f32(right);
        if (a.size(1) != b.size(0)) throw std::invalid_argument("matrix shape mismatch");
        const auto count = matrix_elements(static_cast<uint64_t>(a.size(0)), static_cast<uint64_t>(b.size(1)));
        auto product = torch::matmul(a, b).contiguous();
        finite_f32(product.data_ptr<float>(), count);
        *output = new torch::Tensor(std::move(product));
    });
}

int32_t cgx2_torch_matrix_f32_info(const void *raw, uint64_t *rows, uint64_t *cols, uint32_t *dtype) {
    return translate([&] {
        if (!rows || !cols || !dtype) throw std::invalid_argument("null matrix info output");
        if (!raw) throw std::invalid_argument("null matrix handle");
        const auto &matrix = *static_cast<const torch::Tensor *>(raw);
        if (!matrix.device().is_cpu() || matrix.dim() != 2 || !matrix.is_contiguous())
            throw std::invalid_argument("expected contiguous two-dimensional CPU matrix");
        matrix_elements(static_cast<uint64_t>(matrix.size(0)), static_cast<uint64_t>(matrix.size(1)));
        *rows = static_cast<uint64_t>(matrix.size(0));
        *cols = static_cast<uint64_t>(matrix.size(1));
        *dtype = matrix.scalar_type() == torch::kFloat32 ? CGX2_TORCH_DTYPE_F32 :
                 matrix.scalar_type() == torch::kFloat64 ? 2u : 0u;
    });
}

int32_t cgx2_torch_matrix_copy_f32(const void *raw, float *output, uint64_t capacity) {
    return translate([&] {
        const auto &matrix = checked_matrix_f32(raw);
        const auto count = matrix_elements(static_cast<uint64_t>(matrix.size(0)),
                                           static_cast<uint64_t>(matrix.size(1)));
        if (!output || capacity != count) throw std::invalid_argument("matrix output capacity mismatch");
        finite_f32(matrix.data_ptr<float>(), count);
        std::memcpy(output, matrix.data_ptr<float>(), static_cast<size_t>(count) * sizeof(float));
    });
}
uint32_t cgx2_torch_inference_api_version(void) { return 4; }
int32_t cgx2_torch_matrix_add_f32(const void *left, const void *right, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        c10::InferenceMode guard;
        if (!output) throw std::invalid_argument("null output");
        const auto &a = checked_matrix_f32(left);
        const auto &b = checked_matrix_f32(right);
        if (a.sizes() != b.sizes()) throw std::invalid_argument("add shape mismatch");
        auto result = (a + b).contiguous();
        finite_f32(result.data_ptr<float>(), static_cast<uint64_t>(result.numel()));
        *output = new torch::Tensor(std::move(result));
    });
}
int32_t cgx2_torch_matrix_relu_f32(const void *input, void **output) {
    if (output) *output = nullptr;
    return translate([&] {
        c10::InferenceMode guard;
        if (!output) throw std::invalid_argument("null output");
        const auto &a = checked_matrix_f32(input);
        // Check before ReLU too: it must not conceal a nonfinite input.
        finite_f32(a.data_ptr<float>(), static_cast<uint64_t>(a.numel()));
        auto result = torch::relu(a).contiguous();
        *output = new torch::Tensor(std::move(result));
    });
}

}
