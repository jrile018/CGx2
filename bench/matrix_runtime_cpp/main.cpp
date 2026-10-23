#include <torch/torch.h>
#include <torch/version.h>
#include <chrono>
#include <cmath>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <dlfcn.h>
#include <iomanip>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>
#include <sys/resource.h>
#include <time.h>
#include <vector>

using Clock = std::chrono::steady_clock;
using FnCreate = int32_t (*)(const double *, uint64_t, uint64_t, void **);
using FnMul = int32_t (*)(const void *, const void *, void **);
using FnCopy = int32_t (*)(const void *, double *, uint64_t);
using FnRelease = int32_t (*)(void *);

static uint64_t cpu_ns() {
  timespec ts{};
  if (clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &ts)) throw std::runtime_error("clock_gettime failed");
  return uint64_t(ts.tv_sec) * 1000000000ULL + uint64_t(ts.tv_nsec);
}
static uint64_t wall_ns(Clock::time_point start) {
  return std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - start).count();
}
static long rss_kib() {
  rusage usage{};
  if (getrusage(RUSAGE_SELF, &usage)) throw std::runtime_error("getrusage failed");
  return usage.ru_maxrss;
}
static void check(int32_t status, const char *operation) {
  if (status) throw std::runtime_error(std::string(operation) + " failed");
}
template <typename T> static T symbol(void *library, const char *name) {
  auto raw = dlsym(library, name);
  if (!raw) throw std::runtime_error(std::string("missing facade symbol: ") + name);
  return reinterpret_cast<T>(raw);
}
struct Facade {
  void *library;
  FnCreate create;
  FnMul multiply;
  FnCopy copy;
  FnRelease release;
  explicit Facade(const std::string &path) : library(dlopen(path.c_str(), RTLD_NOW | RTLD_LOCAL)) {
    if (!library) throw std::runtime_error(dlerror());
    auto identity = symbol<const char *(*)()>(library, "cgx2_library_identity");
    auto version = symbol<const char *(*)()>(library, "cgx2_library_version");
    auto abi = symbol<uint32_t (*)()>(library, "cgx2_abi_version");
    auto matrix_abi = symbol<uint32_t (*)()>(library, "cgx2_torch_matrix_api_version");
    if (std::string(identity()) != "cgx2.libtorch.cpu" || std::string(version()) != "2.11.0" || abi() != 1 || matrix_abi() != 2)
      throw std::runtime_error("facade identity/version/API mismatch");
    create = symbol<FnCreate>(library, "cgx2_torch_matrix_f64_cpu");
    multiply = symbol<FnMul>(library, "cgx2_torch_matmul");
    copy = symbol<FnCopy>(library, "cgx2_torch_matrix_copy_f64");
    release = symbol<FnRelease>(library, "cgx2_torch_release");
  }
  ~Facade() { if (library) dlclose(library); }
};

static std::vector<double> values(int rows, int cols, bool left, bool tiny) {
  if (tiny) return left ? std::vector<double>{1, 2, 3, 4, 5, 6} : std::vector<double>{7, 8, 9, 10, 11, 12};
  std::vector<double> out(size_t(rows) * cols);
  for (size_t i = 0; i < out.size(); ++i)
    out[i] = left ? (double(int(i % 7) - 3) / 8.0) : (double(int(i % 5) - 2) / 7.0);
  return out;
}
static torch::Tensor copied(const std::vector<double> &input, int rows, int cols) {
  auto t = torch::empty({rows, cols}, torch::TensorOptions().dtype(torch::kFloat64).device(torch::kCPU));
  std::memcpy(t.data_ptr<double>(), input.data(), input.size() * sizeof(double));
  return t;
}
static std::vector<double> baseline(const std::vector<double> &a, const std::vector<double> &b, int m, int k, int n, uint64_t *math_time) {
  auto left = copied(a, m, k), right = copied(b, k, n);
  auto start = Clock::now();
  auto output = torch::matmul(left, right);
  *math_time = wall_ns(start);
  std::vector<double> result(size_t(m) * n);
  std::memcpy(result.data(), output.data_ptr<double>(), result.size() * sizeof(double));
  return result;
}
static std::vector<double> facade_call(Facade &f, const std::vector<double> &a, const std::vector<double> &b, int m, int k, int n, uint64_t *math_time) {
  void *left = nullptr, *right = nullptr, *output = nullptr;
  try {
    check(f.create(a.data(), m, k, &left), "create left");
    check(f.create(b.data(), k, n, &right), "create right");
    auto start = Clock::now();
    check(f.multiply(left, right, &output), "matmul");
    *math_time = wall_ns(start);
    std::vector<double> result(size_t(m) * n);
    check(f.copy(output, result.data(), result.size()), "copy output");
    check(f.release(output), "release output"); output = nullptr;
    check(f.release(right), "release right"); right = nullptr;
    check(f.release(left), "release left"); left = nullptr;
    return result;
  } catch (...) {
    if (output) f.release(output);
    if (right) f.release(right);
    if (left) f.release(left);
    throw;
  }
}
static void print_result(const std::vector<double> &result) {
  std::cout << "{\"version\":\"" << TORCH_VERSION << "\",\"output\":[";
  std::cout << std::setprecision(17);
  for (size_t i = 0; i < result.size(); ++i) {
    if (i) std::cout << ',';
    if (!std::isfinite(result[i])) throw std::runtime_error("nonfinite output");
    std::cout << result[i];
  }
  std::cout << ']';
}
int main(int argc, char **argv) {
  try {
    if (std::string(TORCH_VERSION) != "2.11.0") throw std::runtime_error("header Torch version differs from 2.11.0");
    if (argc != 9) throw std::runtime_error("usage: baseline|facade cold|warm M K N WARMUP REPS FACADE_PATH");
    std::string route = argv[1], phase = argv[2];
    int m = std::stoi(argv[3]), k = std::stoi(argv[4]), n = std::stoi(argv[5]);
    int warmup = std::stoi(argv[6]), reps = std::stoi(argv[7]);
    if ((route != "baseline" && route != "facade") || (phase != "cold" && phase != "warm") ||
        m < 1 || k < 1 || n < 1 || m > 1024 || k > 1024 || n > 1024 || warmup < 0 || reps < 1)
      throw std::runtime_error("invalid arguments");
    if (std::string(std::getenv("OMP_NUM_THREADS") ? std::getenv("OMP_NUM_THREADS") : "") != "1" ||
        std::string(std::getenv("MKL_NUM_THREADS") ? std::getenv("MKL_NUM_THREADS") : "") != "1")
      throw std::runtime_error("OMP_NUM_THREADS and MKL_NUM_THREADS must be 1");
    torch::set_num_threads(1);
    torch::set_num_interop_threads(1);
    auto a = values(m, k, true, m == 2 && k == 3 && n == 2);
    auto b = values(k, n, false, m == 2 && k == 3 && n == 2);
    std::unique_ptr<Facade> f;
    if (route == "facade") f = std::make_unique<Facade>(argv[8]);
    auto call = [&](uint64_t *math) { return route == "facade" ? facade_call(*f, a, b, m, k, n, math) : baseline(a, b, m, k, n, math); };
    uint64_t math = 0;
    auto answer = call(&math);
    print_result(answer);
    if (phase == "warm") {
      for (int i = 0; i < warmup; ++i) call(&math);
      std::cout << ",\"samples\":[";
      for (int i = 0; i < reps; ++i) {
        uint64_t c0 = cpu_ns(); auto start = Clock::now();
        auto got = call(&math);
        auto elapsed = wall_ns(start); auto cpu = cpu_ns() - c0;
        if (got.size() != answer.size()) throw std::runtime_error("output size changed");
        for (size_t j = 0; j < got.size(); ++j)
          if (std::abs(got[j] - answer[j]) > 1e-9) throw std::runtime_error("output changed");
        if (i) std::cout << ',';
        std::cout << "{\"wall_ns\":" << elapsed << ",\"cpu_ns\":" << cpu
                  << ",\"matmul_wall_ns\":" << math << ",\"peak_rss_kib\":" << rss_kib() << '}';
      }
      std::cout << ']';
    }
    std::cout << "}\n";
  } catch (const std::exception &e) {
    std::cerr << e.what() << '\n';
    return 2;
  }
}
