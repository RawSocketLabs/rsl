#pragma once
#include "rust_dsdcc.h"
#include <array>
#include <cstdint>
#include <memory>
#include <string>
#include <type_traits>

namespace rust {
inline namespace cxxbridge1 {
// #include "rust/cxx.h"

struct unsafe_bitcopy_t;

#ifndef CXXBRIDGE1_RUST_STRING
#define CXXBRIDGE1_RUST_STRING
class String final {
public:
  String() noexcept;
  String(const String &) noexcept;
  String(String &&) noexcept;
  ~String() noexcept;

  String(const std::string &);
  String(const char *);
  String(const char *, std::size_t);
  String(const char16_t *);
  String(const char16_t *, std::size_t);
#ifdef __cpp_char8_t
  String(const char8_t *s);
  String(const char8_t *s, std::size_t len);
#endif

  static String lossy(const std::string &) noexcept;
  static String lossy(const char *) noexcept;
  static String lossy(const char *, std::size_t) noexcept;
  static String lossy(const char16_t *) noexcept;
  static String lossy(const char16_t *, std::size_t) noexcept;

  String &operator=(const String &) & noexcept;
  String &operator=(String &&) & noexcept;

  explicit operator std::string() const;

  const char *data() const noexcept;
  std::size_t size() const noexcept;
  std::size_t length() const noexcept;
  bool empty() const noexcept;

  const char *c_str() noexcept;

  std::size_t capacity() const noexcept;
  void reserve(size_t new_cap) noexcept;

  using iterator = char *;
  iterator begin() noexcept;
  iterator end() noexcept;

  using const_iterator = const char *;
  const_iterator begin() const noexcept;
  const_iterator end() const noexcept;
  const_iterator cbegin() const noexcept;
  const_iterator cend() const noexcept;

  bool operator==(const String &) const noexcept;
  bool operator!=(const String &) const noexcept;
  bool operator<(const String &) const noexcept;
  bool operator<=(const String &) const noexcept;
  bool operator>(const String &) const noexcept;
  bool operator>=(const String &) const noexcept;

  void swap(String &) noexcept;

  String(unsafe_bitcopy_t, const String &) noexcept;

private:
  struct lossy_t;
  String(lossy_t, const char *, std::size_t) noexcept;
  String(lossy_t, const char16_t *, std::size_t) noexcept;
  friend void swap(String &lhs, String &rhs) noexcept { lhs.swap(rhs); }

  std::array<std::uintptr_t, 3> repr;
};
#endif // CXXBRIDGE1_RUST_STRING
} // namespace cxxbridge1
} // namespace rust

using DSDDecoder = ::DSDDecoder;
using DSDDecodeMode = ::DSDDecodeMode;
using DSDSyncType = ::DSDSyncType;
using DSDStationType = ::DSDStationType;

static_assert(::std::is_enum<DSDStationType>::value, "expected enum");
static_assert(sizeof(DSDStationType) == sizeof(::std::uint32_t), "incorrect size");
static_assert(static_cast<::std::uint32_t>(DSDStationType::DSDStationTypeNotApplicable) == 0, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDStationType::DSDBaseStation) == 1, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDStationType::DSDMobileStation) == 2, "disagrees with the value in #[cxx::bridge]");

static_assert(::std::is_enum<DSDDecodeMode>::value, "expected enum");
static_assert(sizeof(DSDDecodeMode) == sizeof(::std::uint32_t), "incorrect size");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeAuto) == 0, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeNone) == 1, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeP25P1) == 2, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeDStar) == 3, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeNXDN48) == 4, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeNXDN96) == 5, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeProVoice) == 6, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeDMR) == 7, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeX2TDMA) == 8, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeDPMR) == 9, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDDecodeMode::DSDDecodeYSF) == 10, "disagrees with the value in #[cxx::bridge]");

static_assert(::std::is_enum<DSDSyncType>::value, "expected enum");
static_assert(sizeof(DSDSyncType) == sizeof(::std::uint32_t), "incorrect size");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncP25p1P) == 0, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncP25p1N) == 1, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncX2TDMADataP) == 2, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncX2TDMAVoiceN) == 3, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncX2TDMAVoiceP) == 4, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncX2TDMADataN) == 5, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDStarP) == 6, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDStarN) == 7, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncNXDNP) == 8, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncNXDNN) == 9, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDMRDataP) == 10, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDMRDataMS) == 11, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDMRVoiceP) == 12, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDMRVoiceMS) == 13, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncProVoiceP) == 14, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncProVoiceN) == 15, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncNXDNDataP) == 16, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncNXDNDataN) == 17, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDStarHeaderP) == 18, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDStarHeaderN) == 19, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDPMR) == 20, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDPMRPacket) == 21, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDPMRPayload) == 22, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncDPMREnd) == 23, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncYSF) == 24, "disagrees with the value in #[cxx::bridge]");
static_assert(static_cast<::std::uint32_t>(DSDSyncType::DSDSyncNone) == 25, "disagrees with the value in #[cxx::bridge]");
