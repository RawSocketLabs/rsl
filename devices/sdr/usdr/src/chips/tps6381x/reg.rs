//! The register address map (SLVSEK4C §8.6).

use bnb::BitEnum;

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// See [`Control`](super::control::Control). `CONTROL`, §8.6.1.2.
    Control = 0x01,

    /// See [`DeviceId`](super::id::DeviceId). `DEVID`, §8.6.1.4.
    DeviceId = 0x03,

    /// Output voltage while the VSEL pin is low. `VOUT1`, §8.6.1.5; reset 0x3C (3.3 V).
    VoltageVselLow = 0x04,

    /// Output voltage while the VSEL pin is high. `VOUT2`, §8.6.1.6; reset 0x42 (3.45 V).
    VoltageVselHigh = 0x05,
}
