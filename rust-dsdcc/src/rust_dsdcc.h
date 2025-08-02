#pragma once

#include <memory>
#include <cstdint>
#include <string>
#include "rust/cxx.h"
#include "dsd_decoder.h"

class Decoder {
public:
    Decoder();
    void run(int16_t sample);
    const std::string& get_frame_type_text();

private:
    std::unique_ptr<DSDcc::DSDDecoder> inner_;
};

std::unique_ptr<Decoder> new_decoder();
