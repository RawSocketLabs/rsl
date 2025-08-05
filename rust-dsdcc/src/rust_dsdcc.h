#pragma once

#include "dsd_decoder.h"
#include "rust/cxx.h"
#include <cstdint>
#include <memory>

namespace rust_dsdcc {
  enum DSDDecodeMode {
    DSDDecodeDMR,
    DSDDecodeNXDN48,
    DSDDecodeNXDN96,
  };

class DSDDecoder {
public:
  DSDDecoder();
  ~DSDDecoder();
  void run(short sample) const;
  void setQuiet() const;
  void setDecodeMode(rust_dsdcc::DSDDecodeMode mode, bool on) const;

  std::unique_ptr<::DSDcc::DSDDecoder> dsddecoder;
};

std::unique_ptr<::rust_dsdcc::DSDDecoder> create_dsddecoder();

} // namespace rust_dsdcc
