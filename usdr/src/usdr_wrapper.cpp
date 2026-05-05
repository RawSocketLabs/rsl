#include "usdr_wrapper.hpp"
#include "usdr/src/lib.rs.h"

#include <stdexcept>
#include <utility>
#include <unistd.h>

#define USDR_SUCCESS 0
#define USDR_ERR_CREATE_DEVICE 1
#define USDR_ERR_POWER_ON 2
#define USDR_ERR_SET_SAMPLE_RATE 3
#define USDR_ERR_CREATE_RX_STREAM 4
#define USDR_ERR_GET_RX_STREAM_INFO 5
#define USDR_ERR_SYNC_OFF 6
#define USDR_ERR_RX_STREAM_PRE_CHARGE 7
#define USDR_ERR_NULL_DEVICE 8
#define USDR_ERR_SET_FREQ 9
#define USDR_ERR_SET_BANDWIDTH 10
#define USDR_ERR_SYNC_NONE 11
#define USDR_ERR_TOO_HOT 12

namespace {

void check_or_throw(int err, const char *op) {
  if (err < 0) {
    throw std::runtime_error(std::string("USDR error in ") + op + ": " +
                             std::to_string(err));
  }
}

// libusdr's auto RX-band cutoffs (m2_lm6_1: usdr_ctrl.c cfg_auto_rx[]).
// Crossing one of these toggles d->mexir_en, which fires
// si5332_set_port3_en — and that helper cycles the si5332 USYS_CTRL through
// READY → ACTIVE, blipping every output divider including the ADC sample
// clock. If the FPGA DMA ring is live during the blip it eventually trips
// pcie_uram_dma_wait_or_alloc with -ETIMEDOUT.
constexpr uint64_t BAND_LNAL_LNAW_HZ = 230'000'000ULL;
constexpr uint64_t BAND_LNAW_LNAH_HZ = 2'800'000'000ULL;

int rx_band_of(uint64_t hz) {
  if (hz < BAND_LNAL_LNAW_HZ) return 0; // LNAL (mexir/MXLO active)
  if (hz < BAND_LNAW_LNAH_HZ) return 1; // LNAW
  return 2;                              // LNAH
}

} // namespace

UsdrDevice::UsdrDevice(const std::string &device_string, int loglevel,
                       uint32_t samples_per_packet)
    : device_string_(device_string), samples_per_packet_(samples_per_packet) {
  usdrlog_setlevel(nullptr, loglevel);
  usdrlog_enablecolorize(nullptr);
}

UsdrDevice::~UsdrDevice() {
  if (dev_.dev) {
    (void)usdr_dms_op(dev_.strms[0], USDR_DMS_STOP, 0);
    usdr_dmd_close(dev_.dev);
    dev_.dev = nullptr;
  }
}

uint32_t UsdrDevice::init(uint32_t sample_rate) {
  fflush(stdin);
  int res;
  bool fresh_device = (dev_.dev == nullptr);
  if (fresh_device) {
    res = usdr_dmd_create_string(device_string_.c_str(), &dev_.dev);
    if (res < 0)
      return USDR_ERR_CREATE_DEVICE;
    // Cached state in member fields refers to the previous device handle;
    // a freshly created device starts unconfigured.
    sample_rate_ = 0;
  }

  // Bail before power-on / rate-set so hot chip isn't poked further.
  if (get_temperature() > 79.0f) {
    return USDR_ERR_TOO_HOT;
  }

  if (fresh_device) {
    res = usdr_dme_set_uint(dev_.dev, "/dm/power/en", 1);
    if (res < 0)
      return USDR_ERR_POWER_ON;
  }

  if (sample_rate != sample_rate_) {
    unsigned rates[4] = {sample_rate, 0, 0, 0};
    res = usdr_dme_set_uint(dev_.dev, "/dm/rate/rxtxadcdac", (uintptr_t)rates);
    if (res < 0)
      return USDR_ERR_SET_SAMPLE_RATE;
    sample_rate_ = sample_rate;
  }

  return USDR_SUCCESS;
}

uint32_t UsdrDevice::start(uint32_t rate) {
  const unsigned chmsk = 0x1u;
  const std::string format = "ci16";

  int res = init(rate);
  if (res != 0)
    return res;
  if (dev_.dev == nullptr)
    return USDR_ERR_NULL_DEVICE;

  res = usdr_dms_create_ex(dev_.dev, "/ll/srx/0", format.c_str(), chmsk,
                           samples_per_packet_, 0, &dev_.strms[0]);
  if (res < 0)
    return USDR_ERR_CREATE_RX_STREAM;
  res = usdr_dms_info(dev_.strms[0], &dev_.snfo_rx);
  if (res < 0)
    return USDR_ERR_GET_RX_STREAM_INFO;
  res = usdr_dms_op(dev_.strms[0], USDR_DMS_START, 0);
  if (res < 0)
    return USDR_ERR_RX_STREAM_PRE_CHARGE;

  // Apply stored RX frequency if set
  if (rx_freq_ != 0) {
    res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/freqency", rx_freq_);
    if (res < 0)
      return USDR_ERR_SET_FREQ;
  }

  // Bandwidth defaults to sample rate if caller didn't set one
  uint32_t bw = (rx_bandwidth_ != 0) ? rx_bandwidth_ : sample_rate_;
  res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/bandwidth", bw);
  if (res < 0)
    return USDR_ERR_SET_BANDWIDTH;

  res = usdr_dms_sync(dev_.dev, "none", 2, dev_.strms);
  if (res < 0)
    return USDR_ERR_SYNC_NONE;

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

void UsdrDevice::set_rx_freq(uint64_t hz) {
  // If the new freq lives in a different RX band cfg than the previous one,
  // libusdr will toggle d->mexir_en and call si5332_set_port3_en mid-tune.
  // That helper cycles the si5332 chip through READY/ACTIVE which momentarily
  // halts the ADC sample clock — the FPGA RX engine and kernel DMA ring
  // desync from the in-flight sample stream and recv eventually times out.
  //
  // STOP/START alone (`usdr_dms_op`) is not sufficient: it pauses the FPGA
  // engine but leaves DMA descriptors in place, and the descriptors stay
  // poisoned across the chip blip. Full destroy + recreate of the stream is
  // required to rebuild the descriptor ring and resync the FPGA front-end.
  bool band_cross =
      (rx_freq_ != 0) && (rx_band_of(rx_freq_) != rx_band_of(hz));
  rx_freq_ = hz;

  if (dev_.dev == nullptr) {
    return;
  }

  bool full_reset = band_cross && (dev_.strms[0] != nullptr);

  if (!full_reset) {
    int res = usdr_dme_set_uint(dev_.dev, "/dm/sdr/0/rx/freqency", hz);
    check_or_throw(res, "set rx freq");
    return;
  }

  // In-place teardown + rebuild of the stream is not enough to recover from
  // the si5332 USYS_CTRL READY/ACTIVE blip — DMA fails immediately after the
  // chip toggle (the 4 s lag before the throw is just recv timeout). Mirror
  // the working /api/scan/start flow: full device close + reopen + reinit.
  // start() runs init() which checks temp; if hot it returns USDR_ERR_TOO_HOT
  // and we retry after a cool-down sleep, matching libsdr's start_streaming
  // loop so the wrapper handles the rare hot case without the caller having
  // to know it happened mid-tune.
  //
  // Cache sample_rate_ before stop()/start(): init() resets sample_rate_ to 0
  // every fresh-device pass (so the next set_rate actually fires), which
  // means passing the live member would feed 0 back to subsequent iterations
  // and skip the rate set entirely — leaving the device with no MXLO clock
  // plan when sub-200 MHz tuning is requested.
  uint32_t cached_rate = sample_rate_;
  stop();
  while (true) {
    uint32_t rc = start(cached_rate);
    if (rc == 0) {
      break;
    }
    if (rc != USDR_ERR_TOO_HOT) {
      throw std::runtime_error("USDR start after band cross: " +
                               std::to_string(rc));
    }
    stop();
    sleep(5);
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
  // Leave dev_.dev in the state we found it. If we open it just to read the
  // sensor, we must close it again — otherwise the next init() sees dev_.dev
  // != nullptr and skips power-on + sample-rate setup (which is what
  // establishes the LMS6002D mixer offset / clock plan needed for sub-200 MHz
  // tuning), producing a half-initialized device that DMA-times-out at recv.
  bool was_closed = (dev_.dev == nullptr);
  if (was_closed) {
    int res = usdr_dmd_create_string(device_string_.c_str(), &dev_.dev);
    if (res < 0) {
      throw std::runtime_error("Failed to open device for temperature read");
    }
  }

  uint64_t temp;
  int res = usdr_dme_get_uint(dev_.dev, "/dm/sensor/temp", &temp);

  if (was_closed) {
    usdr_dmd_close(dev_.dev);
    dev_.dev = nullptr;
  }

  check_or_throw(res, "get temperature");
  return static_cast<float>(temp) / 256.0f;
}

void UsdrDevice::receive_data(uint8_t *ch1, uint8_t *ch2, uint32_t samples) {
  if (dev_.strms[0] == nullptr) {
    throw std::runtime_error("RX stream is null - call start() first");
  }

  void *buffers[2] = {ch1, ch2};
  int res = usdr_dms_recv(dev_.strms[0], buffers, samples, nullptr);
  check_or_throw(res, "receive data");
}

uint32_t UsdrDevice::rx_bytes_per_sample() const {
  return dev_.snfo_rx.pktbszie / dev_.snfo_rx.pktsyms;
}

std::unique_ptr<UsdrDevice> make_usdr_device(const std::string &device_string,
                                             int32_t loglevel,
                                             uint32_t samples_per_packet) {
  return std::make_unique<UsdrDevice>(device_string, loglevel,
                                      samples_per_packet);
}
