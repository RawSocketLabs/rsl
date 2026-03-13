#include "usdr_wrapper.hpp"
#include "usdr/src/lib.rs.h"

#include <stdexcept>
#include <utility>

#define USDR_SUCCESS                 0
#define USDR_ERR_CREATE_DEVICE       1
#define USDR_ERR_POWER_ON            2
#define USDR_ERR_SET_SAMPLE_RATE     3
#define USDR_ERR_CREATE_RX_STREAM    4
#define USDR_ERR_GET_RX_STREAM_INFO  5
#define USDR_ERR_SYNC_OFF            6
#define USDR_ERR_RX_STREAM_PRE_CHARGE 7
#define USDR_ERR_NULL_DEVICE         8
#define USDR_ERR_SET_FREQ            9
#define USDR_ERR_SET_BANDWIDTH       10
#define USDR_ERR_SYNC_NONE           11
#define USDR_ERR_TOO_HOT             12

namespace {

void check_or_throw(int err, const char* op) {
  if (err < 0) {
    throw std::runtime_error(std::string("USDR error in ") + op + ": " + std::to_string(err));
  }
}

}

UsdrDevice::UsdrDevice(const std::string& device_string,
                       int loglevel,
                       uint32_t samples_per_packet)
    : device_string_(device_string),
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

uint32_t UsdrDevice::init() {
  fflush(stdin);
  int res;
  if (dev_.dev == nullptr) {
      res = usdr_dmd_create_string(device_string_.c_str(), &dev_.dev);
      if (res < 0) return USDR_ERR_CREATE_DEVICE;
  }

  return USDR_SUCCESS;
}

uint32_t UsdrDevice::start(uint32_t rate) {
  const unsigned chmsk = 0x1u;
  const std::string format = "ci16";
  // rates: [rx_rate, tx_rate, adc_rate, dac_rate]
  unsigned rates[4] = { rate, 0, 0, 0 };

  int res = init();
  if (res != 0) return res;
  if (dev_.dev == nullptr) return USDR_ERR_NULL_DEVICE;

  res = usdr_dme_set_uint(dev_.dev, "/dm/power/en", 1);
  if (res < 0) return USDR_ERR_POWER_ON;

  float temp = get_temperature();
  if (temp > 79.0) {
    return USDR_ERR_TOO_HOT;
  }

  res = usdr_dme_set_uint(dev_.dev, "/dm/rate/rxtxadcdac", (uintptr_t)rates);
  if (res < 0) return USDR_ERR_SET_SAMPLE_RATE;
  res = usdr_dms_create_ex(dev_.dev, "/ll/srx/0", format.c_str(), chmsk, samples_per_packet_, 0, &dev_.strms[0]);
  if (res < 0) return USDR_ERR_CREATE_RX_STREAM;
  res = usdr_dms_info(dev_.strms[0], &dev_.snfo_rx);
  if (res < 0) return USDR_ERR_GET_RX_STREAM_INFO;
  res = usdr_dms_op(dev_.strms[0], USDR_DMS_START, 0);
  if (res < 0) return USDR_ERR_RX_STREAM_PRE_CHARGE;

  // Apply stored RX frequency if set
  if (rx_freq_ != 0) {
    res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/freqency", rx_freq_);
    if (res < 0) return USDR_ERR_SET_FREQ;
  }

  // Apply stored RX bandwidth if set
  if (rx_bandwidth_ != 0) {
    res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/bandwidth", rx_bandwidth_);
    if (res < 0) return USDR_ERR_SET_BANDWIDTH;
  }

  res = usdr_dms_sync(dev_.dev, "none", 2, dev_.strms);
  if (res < 0) return USDR_ERR_SYNC_NONE;

  return USDR_SUCCESS;
}

void UsdrDevice::stop() {
  if (dev_.strms[0]) {
    int res = usdr_dms_op(dev_.strms[0], USDR_DMS_STOP, 0);
    check_or_throw(res, "rx stream stop");

    res = usdr_dme_set_uint(dev_.dev, "/dm/power/en", 0);
    check_or_throw(res, "power off");

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

float UsdrDevice::get_temperature() {
  bool was_closed = (dev_.dev == nullptr);
  if (was_closed) {
    int res = usdr_dmd_create_string(device_string_.c_str(), &dev_.dev);
    if (res < 0) {
      throw std::runtime_error("Failed to open device for temperature read");
    }
  }

  uint64_t temp;
  int res = usdr_dme_get_uint(dev_.dev, "/dm/sensor/temp", &temp);

  check_or_throw(res, "get temperature");
  return static_cast<float>(temp) / 256.0f;
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
    uint32_t samples_per_packet) {
  return std::make_unique<UsdrDevice>(
      device_string,
      loglevel,
      samples_per_packet);
}
