#pragma once

#include <cstdint>
#include <memory>
#include <string>

// This is your C API header
extern "C" {
//#include "simpleapi.h"
#include "usdr_logging.h"
#include "dm_dev.h"
#include "dm_rate.h"
#include "dm_stream.h"
}

struct sdr_data {
    pdm_dev_t dev;
    pusdr_dms_t strms[2]; // 0 - RX, 1 - TX
    usdr_dms_nfo_t snfo_rx;
    usdr_dms_nfo_t snfo_tx;
};
typedef struct sdr_data sdr_data_t;

class UsdrDevice {
public:
  UsdrDevice(const std::string& device_string,
             int loglevel,
             uint32_t samplerate_rx,
             uint32_t samples_per_packet);

  ~UsdrDevice();

  void start();
  void stop();

  // Frequency / BW
  void set_rx_freq(uint32_t hz);
  void set_rx_bandwidth(uint32_t hz);

  void receive_data(uint8_t* ch1,
                    uint8_t* ch2,
                    uint32_t samples);

  uint32_t rx_bytes_per_sample() const;

private:
  sdr_data_t dev_{};
};

std::unique_ptr<UsdrDevice> make_usdr_device(
    const std::string& device_string,
    int32_t loglevel,
    uint32_t samplerate_rx,
    uint32_t samples_per_packet);

