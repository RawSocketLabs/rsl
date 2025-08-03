#include "rust_dsdcc.h"

namespace rust_dsdcc {

DSDDecoder::DSDDecoder() : dsddecoder(std::make_unique<::DSDcc::DSDDecoder>()) {}
DSDDecoder::~DSDDecoder() {}

void DSDDecoder::run(int16_t sample) const { dsddecoder->run(sample); }
std::unique_ptr<::rust_dsdcc::DSDDecoder> create_dsddecoder() {
  return std::make_unique<DSDDecoder>();
}
} // namespace rust_dsdcc