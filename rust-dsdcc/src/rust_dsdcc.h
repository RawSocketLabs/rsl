#pragma once

#include "dsd_decoder.h"
#include "rust/cxx.h"
#include <cstdint>
#include <memory>
#include <string>

typedef DSDcc::DSDDecoder::DSDDecodeMode DSDDecodeMode;
typedef DSDcc::DSDDecoder::DSDSyncType DSDSyncType;
typedef DSDcc::DSDDecoder::DSDStationType DSDStationType;

class DSDDecoder {
public:
  DSDDecoder();
  ~DSDDecoder();
  void run(short sample) const;
  void setQuiet() const;
  void setDecodeMode(DSDDecodeMode mode, bool on) const;
  rust::String getSlot0Text() const;
  rust::String getSlot1Text() const;
  rust::String getFrameTypeText() const;
  rust::String getFrameSubtypeText() const;
  DSDcc::DSDDecoder::DSDSyncType getSyncType() const;
  DSDcc::DSDDecoder::DSDStationType getStationType() const;

  
  std::unique_ptr<::DSDcc::DSDDecoder> dsddecoder;
};

std::unique_ptr<::DSDDecoder> create_dsddecoder();
