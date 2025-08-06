#pragma once

#include "dsd_decoder.h"
#include "rust/cxx.h"
#include <cstdint>
#include <memory>

typedef DSDcc::DSDDecoder::DSDDecodeMode DSDDecodeMode;

class DSDDecoder {
public:
  DSDDecoder();
  ~DSDDecoder();
  void run(short sample) const;
  void setQuiet() const;
  void setDecodeMode(DSDDecodeMode mode, bool on) const;
  
  const std::unique_ptr<::DSDcc::DSDDecoder> dsddecoder;
};

std::unique_ptr<::DSDDecoder> create_dsddecoder();
