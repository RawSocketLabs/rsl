//! The register address map (SNVSAC6B §7.6).

use bnb::BitEnum;

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
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
}
