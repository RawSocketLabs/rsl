#include "rust_dsdcc.h"

DSDDecoder::DSDDecoder() {
  dsddecoder = std::make_unique<::DSDcc::DSDDecoder>();
}
DSDDecoder::~DSDDecoder() {}

void DSDDecoder::run(short sample) const { dsddecoder->run(sample); }

std::unique_ptr<::DSDDecoder> create_dsddecoder() {
  return std::make_unique<DSDDecoder>();
}

void DSDDecoder::setDecodeMode(DSDDecodeMode mode, bool on) const {
  dsddecoder->setDecodeMode(mode, on);
}

rust::String DSDDecoder::getSlot0Text() const {
  return std::string(dsddecoder->getDMRDecoder().getSlot0Text());
}

rust::String DSDDecoder::getSlot1Text() const {
  return std::string(dsddecoder->getDMRDecoder().getSlot1Text());
}

rust::String DSDDecoder::getFrameTypeText() const {
  return std::string(dsddecoder->getFrameTypeText());
}

rust::String DSDDecoder::getFrameSubtypeText() const {
  return std::string(dsddecoder->getFrameSubtypeText());
}

DSDcc::DSDDecoder::DSDSyncType DSDDecoder::getSyncType() const {
  return dsddecoder->getSyncType();
}

DSDcc::DSDDecoder::DSDStationType DSDDecoder::getStationType() const {
  return dsddecoder->getStationType();
}

void DSDDecoder::setQuiet() const { dsddecoder->setQuiet(); }
