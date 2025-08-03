#pragma once

#include "dsd_decoder.h"
#include "rust/cxx.h"
#include <cstdint>
#include <memory>

namespace rust_dsdcc {

class DSDDecoder {
public:
  DSDDecoder();
  ~DSDDecoder();
  void run(int16_t sample) const;

  std::unique_ptr<::DSDcc::DSDDecoder> dsddecoder;
};

std::unique_ptr<::rust_dsdcc::DSDDecoder> create_dsddecoder();

} // namespace rust_dsdcc
