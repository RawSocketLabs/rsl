#include "rust_dsdcc.h"

Decoder::Decoder() : inner_(std::make_unique<DSDcc::DSDDecoder>()) {
  inner_->setDecodeMode(DSDcc::DSDDecoder::DSDDecodeDMR, true);
  inner_->setDecodeMode(DSDcc::DSDDecoder::DSDDecodeNXDN48, false);
  inner_->setDecodeMode(DSDcc::DSDDecoder::DSDDecodeNXDN96, false);
}

void Decoder::run(int16_t sample) { inner_->run(sample); }

const std::string &Decoder::get_frame_type_text() {
  const char *cstr = inner_->getFrameTypeText();
  return cstr ? std::string(cstr) : std::string();
}

std::unique_ptr<Decoder> new_decoder() { return std::make_unique<Decoder>(); }
