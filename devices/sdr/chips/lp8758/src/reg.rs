//! The register address map (SNVSAC6B §7.6).

use bnb::BitEnum;

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub enum Reg {
    /// Device revision. libusdr reads it as `DEV_REV`; SNVSAC6B's map does not list 0x00.
    DeviceRevision = 0x00,

    /// OTP image revision. `OTP_REV`, §7.6.1.1.
    OtpRevision = 0x01,

    /// See [`BuckControl`](super::buck::BuckControl), channel 0. `BUCK0_CTRL1`.
    Buck0Control = 0x02,

    /// Channel 1 control. `BUCK1_CTRL1`.
    Buck1Control = 0x04,

    /// Channel 2 control. `BUCK2_CTRL1`.
    Buck2Control = 0x06,

    /// Channel 3 control. `BUCK3_CTRL1`.
    Buck3Control = 0x08,

    /// See [`BuckVoltage`](super::buck::BuckVoltage), channel 0. `BUCK0_VOUT`.
    Buck0Voltage = 0x0a,

    /// Channel 1 output voltage. `BUCK1_VOUT`.
    Buck1Voltage = 0x0c,

    /// Channel 2 output voltage. `BUCK2_VOUT`.
    Buck2Voltage = 0x0e,

    /// Channel 3 output voltage. `BUCK3_VOUT`.
    Buck3Voltage = 0x10,

    /// See [`Config`](super::config::Config). `CONFIG`, §7.6.1.23.
    Config = 0x17,

    /// Channels 0 and 1 status; libusdr's `lp8758_check_pg` takes power-good from bits 2
    /// (channel 0) and 6 (channel 1). `BUCK_0_1_STAT` in libusdr's map.
    Buck01Status = 0x1c,

    /// Channels 2 and 3 status, power-good in bits 2 and 6 as for
    /// [`Self::Buck01Status`]. `BUCK_2_3_STAT` in libusdr's map.
    Buck23Status = 0x1d,
}
