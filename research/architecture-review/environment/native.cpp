#include <ATen/ATen.h>
#include <ATen/Parallel.h>
#include <fstream>
#include <iostream>
#include <string>
#include <stdexcept>
int main() {
  at::set_num_threads(1);
  auto b=at::tensor({1,-2,3,4},at::TensorOptions().dtype(at::kInt).device(at::kCPU));
  auto alias=b.view({4}); auto before=b.clone();
  auto apply=[&](size_t budget) { if (b.numel()*b.element_size()>budget) return false;
    b.copy_(b*2+1); return true; };
  if (apply(15)||!at::equal(b,before)||!apply(16)||
      !at::equal(alias,at::tensor({3,-3,7,9},at::kInt))||alias.sum().item<int>()!=16)
    throw std::runtime_error("fixture mismatch");
  std::ifstream maps("/proc/self/maps"); std::string line; bool py=false;
  while(std::getline(maps,line)) if(line.find("libpython")!=std::string::npos) py=true;
  if(py) throw std::runtime_error("unexpected mapped libpython");
  std::cout << "PASS native_ATen_CPU int32 values=[3,-3,7,9] sum=16 failure_unchanged=true mapped_libpython=false\n";
}
