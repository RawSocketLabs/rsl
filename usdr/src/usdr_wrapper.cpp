#include "usdr_wrapper.hpp"
#include "usdr/src/lib.rs.h"

#include <stdexcept>
#include <utility>

static inline void check_or_throw(int err, const char* op) {
  if (err < 0) {
    // strerror expects positive errno-like code in your C sample usage
    // but we don't have strerror(-err) available in a portable way here without <cstring>.
    // Keep the numeric code; you can improve message formatting later.
    throw std::runtime_error(std::string("USDR error in ") + op + ": " + std::to_string(err));
  }
}

UsdrDevice::UsdrDevice(const std::string& device_string,
                       int loglevel,
                       uint32_t samplerate_rx,
                       uint32_t samples_per_packet)
{

  unsigned chmsk = 0x1u;
  // rates must be an array of 4: [rx_rate, tx_rate, adc_rate, dac_rate]
  unsigned rates[4] = { samplerate_rx, 0, 0, 0 };

  usdrlog_setlevel(nullptr, loglevel);
  usdrlog_enablecolorize(nullptr);

  int res = 0;
  const std::string &format = "ci16";

  res = usdr_dmd_create_string(device_string.c_str(), &dev_.dev);
  check_or_throw(res, "initialize");

  res = usdr_dme_set_uint(dev_.dev, "/dm/power/en", 1);
  check_or_throw(res, "power on");

  res = usdr_dme_set_uint(dev_.dev, "/dm/rate/rxtxadcdac", (uintptr_t)rates);
  check_or_throw(res, "set samplerate");

  res = usdr_dms_create(dev_.dev, "/ll/stx/0", format.c_str(), chmsk, 4096, &dev_.strms[1]);
  check_or_throw(res, "tx stream");

  res = usdr_dms_create_ex(dev_.dev, "/ll/srx/0", format.c_str(), chmsk, samples_per_packet, 0, &dev_.strms[0]);
  check_or_throw(res, "rx stream");

  res = usdr_dms_info(dev_.strms[1], &dev_.snfo_tx);
  check_or_throw(res, "tx info");

  res = usdr_dms_info(dev_.strms[0], &dev_.snfo_rx);
  check_or_throw(res, "rx info");

  res = usdr_dms_sync(dev_.dev, "off", 2, dev_.strms);
  check_or_throw(res, "tx & rx synchronization off");

  res = usdr_dms_op(dev_.strms[0], USDR_DMS_START, 0);
  check_or_throw(res, "rx stream precharge");

  res = usdr_dms_op(dev_.strms[1], USDR_DMS_START, 0);
  check_or_throw(res, "tx stream precharge");
}

UsdrDevice::~UsdrDevice() {
  if (dev_.dev) {
    // stop streams best-effort
    (void)usdr_dms_op(dev_.strms[0], USDR_DMS_STOP, 0);
    (void)usdr_dms_op(dev_.strms[1], USDR_DMS_STOP, 0);
    usdr_dmd_close(dev_.dev);
    dev_.dev = nullptr;
  }
}

void UsdrDevice::start() {
  int res = usdr_dms_sync(dev_.dev, "none", 2, dev_.strms);
  check_or_throw(res, "start");
}

void UsdrDevice::stop() {
  int res = usdr_dms_op(dev_.strms[0], USDR_DMS_STOP, 0);
  check_or_throw(res, "rx stream stop");
  res = usdr_dms_op(dev_.strms[1], USDR_DMS_STOP, 0);
  check_or_throw(res, "tx stream stop");
}

void UsdrDevice::set_rx_freq(uint32_t hz) {
  int res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/freqency", hz);
  check_or_throw(res, "SetRxFreq");
}

void UsdrDevice::set_rx_bandwidth(uint32_t hz) {
  int res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/bandwidth", hz);
  check_or_throw(res, "SetRxBandwidth");
}

void UsdrDevice::receive_data(uint8_t* ch1, uint8_t* ch2, uint32_t samples) {
  void* buffers[2] = { ch1, ch2 };
  int res = usdr_dms_recv(dev_.strms[0], buffers, samples, nullptr);
  check_or_throw(res, "ReceiveData");
}

uint32_t UsdrDevice::rx_bytes_per_sample() const {
  // pktbszie = packet size in bytes per channel
  // pktsyms = packet size in symbols per channel
  // bytes per sample = pktbszie / pktsyms
  return dev_.snfo_rx.pktbszie / dev_.snfo_rx.pktsyms;
}

// Factory function for CXX bridge
std::unique_ptr<UsdrDevice> make_usdr_device(
    const std::string& device_string,
    int32_t loglevel,
    uint32_t samplerate_rx,
    uint32_t samples_per_packet) {
  return std::make_unique<UsdrDevice>(
      device_string,
      loglevel,
      samplerate_rx,
      samples_per_packet);
}

