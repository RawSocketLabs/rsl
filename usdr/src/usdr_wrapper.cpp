#include "usdr_wrapper.hpp"
#include "usdr/src/lib.rs.h"

#include <stdexcept>
#include <utility>

namespace {

void check_or_throw(int err, const char* op) {
  if (err < 0) {
    throw std::runtime_error(std::string("USDR error in ") + op + ": " + std::to_string(err));
  }
}

}

UsdrDevice::UsdrDevice(const std::string& device_string,
                       int loglevel,
                       uint32_t samplerate_rx,
                       uint32_t samples_per_packet)
    : device_string_(device_string),
      samplerate_rx_(samplerate_rx),
      samples_per_packet_(samples_per_packet)
{
  usdrlog_setlevel(nullptr, loglevel);
  usdrlog_enablecolorize(nullptr);
  init();
}

UsdrDevice::~UsdrDevice() {
  if (dev_.dev) {
    (void)usdr_dms_op(dev_.strms[0], USDR_DMS_STOP, 0);
    usdr_dmd_close(dev_.dev);
    dev_.dev = nullptr;
  }
}

void UsdrDevice::init() {
  if (dev_.dev != nullptr) {
    return;  // Already initialized
  }

  const unsigned chmsk = 0x1u;
  const std::string format = "ci16";
  // rates: [rx_rate, tx_rate, adc_rate, dac_rate]
  unsigned rates[4] = { samplerate_rx_, 0, 0, 0 };
  int res;

  res = usdr_dmd_create_string(device_string_.c_str(), &dev_.dev);
  check_or_throw(res, "create device");

  res = usdr_dme_set_uint(dev_.dev, "/dm/power/en", 1);
  check_or_throw(res, "power on");

  res = usdr_dme_set_uint(dev_.dev, "/dm/rate/rxtxadcdac", (uintptr_t)rates);
  check_or_throw(res, "set samplerate");

  res = usdr_dms_create_ex(dev_.dev, "/ll/srx/0", format.c_str(), chmsk, samples_per_packet_, 0, &dev_.strms[0]);
  check_or_throw(res, "create rx stream");

  res = usdr_dms_info(dev_.strms[0], &dev_.snfo_rx);
  check_or_throw(res, "get rx stream info");

  res = usdr_dms_sync(dev_.dev, "off", 2, dev_.strms);
  check_or_throw(res, "sync off");

  res = usdr_dms_op(dev_.strms[0], USDR_DMS_START, 0);
  check_or_throw(res, "rx stream precharge");
}

void UsdrDevice::start() {
  init();

  if (dev_.dev == nullptr) {
    throw std::runtime_error("Device is null after init");
  }

  // Apply stored RX frequency if set
  if (rx_freq_ != 0) {
    int res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/freqency", rx_freq_);
    check_or_throw(res, "set rx freq");
  }

  // Apply stored RX bandwidth if set
  if (rx_bandwidth_ != 0) {
    int res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/bandwidth", rx_bandwidth_);
    check_or_throw(res, "set rx bandwidth");
  }

  int res = usdr_dms_sync(dev_.dev, "none", 2, dev_.strms);
  check_or_throw(res, "sync start");
}

void UsdrDevice::stop() {
  if (dev_.strms[0]) {
    int res = usdr_dms_op(dev_.strms[0], USDR_DMS_STOP, 0);
    check_or_throw(res, "rx stream stop");

    usdr_dms_destroy(dev_.strms[0]);
    dev_.strms[0] = nullptr;
  }

  if (dev_.dev) {
    usdr_dmd_close(dev_.dev);
    dev_.dev = nullptr;
  }
}

void UsdrDevice::set_rx_freq(uint32_t hz) {
  rx_freq_ = hz;

  if (dev_.dev != nullptr) {
    int res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/freqency", hz);
    check_or_throw(res, "set rx freq");
  }
}

void UsdrDevice::set_rx_bandwidth(uint32_t hz) {
  rx_bandwidth_ = hz;

  if (dev_.dev != nullptr) {
    int res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/bandwidth", hz);
    check_or_throw(res, "set rx bandwidth");
  }
}

void UsdrDevice::receive_data(uint8_t* ch1, uint8_t* ch2, uint32_t samples) {
  if (dev_.strms[0] == nullptr) {
    throw std::runtime_error("RX stream is null - call start() first");
  }

  void* buffers[2] = { ch1, ch2 };
  int res = usdr_dms_recv(dev_.strms[0], buffers, samples, nullptr);
  check_or_throw(res, "receive data");
}

uint32_t UsdrDevice::rx_bytes_per_sample() const {
  return dev_.snfo_rx.pktbszie / dev_.snfo_rx.pktsyms;
}

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
