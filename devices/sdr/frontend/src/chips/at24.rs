//! AT24 serial EEPROM's security registers: the factory serial number XMASS revision 1a
//! carries, which libusdr only logs.
//!
//! # What it does
//!
//! The security registers answer at their own I2C address, with a one-byte register
//! pointer; the 16-byte factory serial number starts at 0x80.
//!
//! # Sources
//!
//! libusdr's `hw/at24/at24.c` and `at24.h`: `at24_saddr_mem_get` reads in chunks of at
//! most four bytes (the FPGA I2C core's limit), each from its own pointer, and stops
//! reading at the first failure; `AT24_SECURE_SERIAL_OFF` is 0x80.

use hal::{Bus, I2cAddr};

use crate::error::{BusContext, Error};

/// Register addresses (`at24.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// The first of the factory serial number's 16 bytes (`AT24_SECURE_SERIAL_OFF`).
    SerialNumber = 0x80,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// An AT24's security registers on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct At24Security {
    /// Where they answer.
    dev: I2cAddr,
}

impl At24Security {
    /// The chip's name in errors.
    const NAME: &'static str = "AT24";

    /// The security registers at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Reads the 16-byte serial number in four-byte chunks, in wire order
    /// (`at24_saddr_mem_get(.., AT24_SECURE_SERIAL_OFF, 16, ..)`).
    pub(crate) fn serial_number(self, bus: &mut dyn Bus) -> Result<[u8; 16], Error> {
        let mut serial = [0; 16];
        for (pointer, chunk) in (u8::from(Reg::SerialNumber)..)
            .step_by(4)
            .zip(serial.chunks_exact_mut(4))
        {
            bus.i2c(self.dev, &[pointer], chunk)
                .reading(Self::NAME, pointer)?;
        }
        Ok(serial)
    }
}
