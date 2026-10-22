#include "facade.h"
#include <torch/torch.h>
#include <exception>
#include <cstring>
#include <limits>
#include <new>
#include <stdexcept>
#include <string>
#include <utility>

namespace {
thread_local std::string last_error;

template <typename F> int32_t translate(F &&body) noexcept {
    try {
        last_error.clear();
        body();
        return 0;
    } catch (const std::exception &error) {
        last_error = error.what();
        return 1;
    } catch (...) {
        last_error = "unknown native LibTorch exception";
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
}

extern "C" {
const char *cgx2_library_identity(void) { return "cgx2.libtorch.cpu"; }
const char *cgx2_library_version(void) { return CGX2_TORCH_VERSION; }
uint32_t cgx2_abi_version(void) { return 1; }
const char *cgx2_torch_last_error(void) { return last_error.c_str(); }

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
}
