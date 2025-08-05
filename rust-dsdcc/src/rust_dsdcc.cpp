#include "rust_dsdcc.h"

namespace rust_dsdcc {

DSDDecoder::DSDDecoder() : dsddecoder(std::make_unique<::DSDcc::DSDDecoder>()) {}
DSDDecoder::~DSDDecoder() {}

void DSDDecoder::run(short sample) const { dsddecoder->run(sample); }

std::unique_ptr<::rust_dsdcc::DSDDecoder> create_dsddecoder() {
  return std::make_unique<DSDDecoder>();
}

void DSDDecoder::setDecodeMode(DSDDecodeMode mode, bool on) const {
  DSDcc::DSDDecoder::DSDDecodeMode dsd_mode;
  switch (mode) {
    case DSDDecodeDMR:
      dsd_mode = DSDcc::DSDDecoder::DSDDecodeMode::DSDDecodeDMR;
      break;
    case DSDDecodeNXDN48:
      dsd_mode = DSDcc::DSDDecoder::DSDDecodeMode::DSDDecodeNXDN48;
      break;
    case DSDDecodeNXDN96:
      dsd_mode = DSDcc::DSDDecoder::DSDDecodeMode::DSDDecodeNXDN96;
      break;
    default:
      return;
  } 
  dsddecoder->setDecodeMode(dsd_mode, on);
}

void DSDDecoder::setQuiet() const { dsddecoder->setQuiet(); }
} // namespace rust_dsdcc