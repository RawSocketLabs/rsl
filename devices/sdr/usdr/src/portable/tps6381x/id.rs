//! The device ID register: manufacturer and silicon revision.

use bnb::{bitfield, u2, u4};

use super::super::register::Register;
use super::reg::Reg;

/// Manufacturer and silicon revision. `DEVID`, §8.6.1.4 (read-only).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DeviceId {
    /// Manufacturer: 0 is Texas Instruments. `MANUFACTURER`, bits 7:4.
    #[bits(4..=7)]
    manufacturer: u4,

    /// Major silicon revision: 0 is A, 1 is B. `MAJOR`, bits 3:2.
    #[bits(2..=3)]
    major: u2,

    /// Minor silicon revision. `MINOR`, bits 1:0.
    #[bits(0..=1)]
    minor: u2,
}
impl Register for DeviceId {
    type Map = Reg;
    const ADDR: Reg = Reg::DeviceId;
}

/// The ID libusdr requires: Texas Instruments, silicon revision B0 (0x04).
pub(crate) const DEVICE_ID: DeviceId = DeviceId::new().with_major(u2::new(1));
