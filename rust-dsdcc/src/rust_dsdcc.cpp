#include "rust_dsdcc.h"

DSDDecoder::DSDDecoder() : dsddecoder(std::make_unique<::DSDcc::DSDDecoder>()) {}
DSDDecoder::~DSDDecoder() {}

void DSDDecoder::run(short sample) const { dsddecoder->run(sample); }

std::unique_ptr<::DSDDecoder> create_dsddecoder() {
  return std::make_unique<DSDDecoder>();
}

void DSDDecoder::setDecodeMode(DSDDecodeMode mode, bool on) const {
  dsddecoder->setDecodeMode(mode, on);
}

void DSDDecoder::setQuiet() const { dsddecoder->setQuiet(); }
