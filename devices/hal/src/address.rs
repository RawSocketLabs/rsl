//! FPGA bus identities and bounds, without transport or operating-system ownership.

/// Most bytes one I2C transfer can write: the FPGA I2C core's command word holds three.
pub const I2C_MAX_WRITE: usize = 3;
/// Most bytes one I2C transfer can read: the FPGA I2C core returns one 32-bit word.
pub const I2C_MAX_READ: usize = 4;

/// An I2C device: the FPGA I2C core and bus it sits on, and its 7-bit address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct I2cAddr {
    /// FPGA I2C core (controller instance); boards with one core use 0.
    core: u8,
    /// FPGA I2C bus number on that core.
    bus: u8,
    /// 7-bit device address.
    addr: u8,
}

impl I2cAddr {
    /// The device at 7-bit address `addr` on FPGA I2C bus `bus` of core 0.
    ///
    /// # Panics
    ///
    /// If `addr` does not fit in 7 bits (at compile time when used in a `const`).
    #[must_use]
    pub const fn new(bus: u8, addr: u8) -> Self {
        Self::on_core(0, bus, addr)
    }

    /// The device at 7-bit address `addr` on bus `bus` of FPGA I2C core `core`, for boards
    /// with several I2C controllers (libusdr's `PE_SYNC` has two).
    ///
    /// # Panics
    ///
    /// If `addr` does not fit in 7 bits (at compile time when used in a `const`).
    #[must_use]
    pub const fn on_core(core: u8, bus: u8, addr: u8) -> Self {
        assert!(addr <= 0x7f, "I2C addresses are 7 bits");
        Self { core, bus, addr }
    }

    /// The FPGA I2C core (controller instance).
    #[must_use]
    pub const fn core(self) -> u8 {
        self.core
    }

    /// The FPGA I2C bus number on that core.
    #[must_use]
    pub const fn bus(self) -> u8 {
        self.bus
    }

    /// The 7-bit device address.
    #[must_use]
    pub const fn addr(self) -> u8 {
        self.addr
    }
}

/// An SPI target as libusdr encodes it: bus number in bits 7:0, core configuration in
/// bits 31:16 (unused on the uSDR).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpiAddr(pub u32);

impl SpiAddr {
    /// The SPI bus number.
    #[must_use]
    pub fn bus(self) -> u8 {
        self.0.to_le_bytes()[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_without_a_core_is_on_core_zero() {
        let dev = I2cAddr::new(1, 0x60);
        assert_eq!((dev.core(), dev.bus(), dev.addr()), (0, 1, 0x60));
        assert_eq!(dev, I2cAddr::on_core(0, 1, 0x60));
        assert_ne!(dev, I2cAddr::on_core(1, 1, 0x60));
    }
}
