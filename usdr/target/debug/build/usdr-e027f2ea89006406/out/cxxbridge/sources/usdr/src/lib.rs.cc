#include "usdr_wrapper.hpp"
#include <cstddef>
#include <cstdint>
#include <exception>
#include <memory>
#include <new>
#include <string>
#include <type_traits>
#include <utility>

#ifdef __GNUC__
#pragma GCC diagnostic ignored "-Wmissing-declarations"
#pragma GCC diagnostic ignored "-Wshadow"
#ifdef __clang__
#pragma clang diagnostic ignored "-Wdollar-in-identifier-extension"
#endif // __clang__
#endif // __GNUC__

namespace rust {
inline namespace cxxbridge1 {
// #include "rust/cxx.h"

#ifndef CXXBRIDGE1_IS_COMPLETE
#define CXXBRIDGE1_IS_COMPLETE
namespace detail {
namespace {
template <typename T, typename = std::size_t>
struct is_complete : std::false_type {};
template <typename T>
struct is_complete<T, decltype(sizeof(T))> : std::true_type {};
} // namespace
} // namespace detail
#endif // CXXBRIDGE1_IS_COMPLETE

namespace repr {
struct PtrLen final {
  void *ptr;
  ::std::size_t len;
};
} // namespace repr

namespace detail {
class Fail final {
  ::rust::repr::PtrLen &throw$;
public:
  Fail(::rust::repr::PtrLen &throw$) noexcept : throw$(throw$) {}
  void operator()(char const *) noexcept;
  void operator()(std::string const &) noexcept;
};
} // namespace detail

namespace {
template <bool> struct deleter_if {
  template <typename T> void operator()(T *) {}
};
template <> struct deleter_if<true> {
  template <typename T> void operator()(T *ptr) { ptr->~T(); }
};
} // namespace
} // namespace cxxbridge1

namespace behavior {
class missing {};
missing trycatch(...);

template <typename Try, typename Fail>
static typename ::std::enable_if<::std::is_same<
    decltype(trycatch(::std::declval<Try>(), ::std::declval<Fail>())),
    missing>::value>::type
trycatch(Try &&func, Fail &&fail) noexcept try {
  func();
} catch (::std::exception const &e) {
  fail(e.what());
}
} // namespace behavior
} // namespace rust

using UsdrDevice = ::UsdrDevice;

extern "C" {
::rust::repr::PtrLen cxxbridge1$196$make_usdr_device(::std::string const &device_string, ::std::int32_t loglevel, ::std::uint32_t samples_per_packet, ::UsdrDevice **return$) noexcept {
  ::std::unique_ptr<::UsdrDevice> (*make_usdr_device$)(::std::string const &, ::std::int32_t, ::std::uint32_t) = ::make_usdr_device;
  ::rust::repr::PtrLen throw$;
  ::rust::behavior::trycatch(
      [&] {
        new (return$) ::UsdrDevice *(make_usdr_device$(device_string, loglevel, samples_per_packet).release());
        throw$.ptr = nullptr;
      },
      ::rust::detail::Fail(throw$));
  return throw$;
}

::std::uint32_t cxxbridge1$196$UsdrDevice$init(::UsdrDevice &self, ::std::uint32_t sample_rate) noexcept {
  ::std::uint32_t (::UsdrDevice::*init$)(::std::uint32_t) = &::UsdrDevice::init;
  return (self.*init$)(sample_rate);
}

::std::uint32_t cxxbridge1$196$UsdrDevice$start(::UsdrDevice &self, ::std::uint32_t rate) noexcept {
  ::std::uint32_t (::UsdrDevice::*start$)(::std::uint32_t) = &::UsdrDevice::start;
  return (self.*start$)(rate);
}

void cxxbridge1$196$UsdrDevice$stop(::UsdrDevice &self) noexcept {
  void (::UsdrDevice::*stop$)() = &::UsdrDevice::stop;
  (self.*stop$)();
}

void cxxbridge1$196$UsdrDevice$set_rx_freq(::UsdrDevice &self, ::std::uint64_t hz) noexcept {
  void (::UsdrDevice::*set_rx_freq$)(::std::uint64_t) = &::UsdrDevice::set_rx_freq;
  (self.*set_rx_freq$)(hz);
}

void cxxbridge1$196$UsdrDevice$set_rx_bandwidth(::UsdrDevice &self, ::std::uint32_t hz) noexcept {
  void (::UsdrDevice::*set_rx_bandwidth$)(::std::uint32_t) = &::UsdrDevice::set_rx_bandwidth;
  (self.*set_rx_bandwidth$)(hz);
}

::rust::repr::PtrLen cxxbridge1$196$UsdrDevice$get_temperature(::UsdrDevice &self, float *return$) noexcept {
  float (::UsdrDevice::*get_temperature$)() = &::UsdrDevice::get_temperature;
  ::rust::repr::PtrLen throw$;
  ::rust::behavior::trycatch(
      [&] {
        new (return$) float((self.*get_temperature$)());
        throw$.ptr = nullptr;
      },
      ::rust::detail::Fail(throw$));
  return throw$;
}

void cxxbridge1$196$UsdrDevice$receive_data(::UsdrDevice &self, ::std::uint8_t *ch1, ::std::uint8_t *ch2, ::std::uint32_t samples) noexcept {
  void (::UsdrDevice::*receive_data$)(::std::uint8_t *, ::std::uint8_t *, ::std::uint32_t) = &::UsdrDevice::receive_data;
  (self.*receive_data$)(ch1, ch2, samples);
}

::std::uint32_t cxxbridge1$196$UsdrDevice$rx_bytes_per_sample(::UsdrDevice const &self) noexcept {
  ::std::uint32_t (::UsdrDevice::*rx_bytes_per_sample$)() const = &::UsdrDevice::rx_bytes_per_sample;
  return (self.*rx_bytes_per_sample$)();
}

static_assert(::rust::detail::is_complete<::std::remove_extent<::UsdrDevice>::type>::value, "definition of `::UsdrDevice` is required");
static_assert(sizeof(::std::unique_ptr<::UsdrDevice>) == sizeof(void *), "");
static_assert(alignof(::std::unique_ptr<::UsdrDevice>) == alignof(void *), "");
void cxxbridge1$unique_ptr$UsdrDevice$null(::std::unique_ptr<::UsdrDevice> *ptr) noexcept {
  ::new (ptr) ::std::unique_ptr<::UsdrDevice>();
}
void cxxbridge1$unique_ptr$UsdrDevice$raw(::std::unique_ptr<::UsdrDevice> *ptr, ::std::unique_ptr<::UsdrDevice>::pointer raw) noexcept {
  ::new (ptr) ::std::unique_ptr<::UsdrDevice>(raw);
}
::std::unique_ptr<::UsdrDevice>::element_type const *cxxbridge1$unique_ptr$UsdrDevice$get(::std::unique_ptr<::UsdrDevice> const &ptr) noexcept {
  return ptr.get();
}
::std::unique_ptr<::UsdrDevice>::pointer cxxbridge1$unique_ptr$UsdrDevice$release(::std::unique_ptr<::UsdrDevice> &ptr) noexcept {
  return ptr.release();
}
void cxxbridge1$unique_ptr$UsdrDevice$drop(::std::unique_ptr<::UsdrDevice> *ptr) noexcept {
  ::rust::deleter_if<::rust::detail::is_complete<::UsdrDevice>::value>{}(ptr);
}
} // extern "C"
