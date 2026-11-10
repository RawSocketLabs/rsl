//! [`Access`]: the dongle's registers and the tuner's I2C port over a [`ControlPort`],
//! encoded as `rtl2832u` records them.

use std::time::Duration;

use crate::control::{BusError, ControlPort, Setup};
use crate::portable::rtl2832u::{
    Block, GATE, GATE_CLOSED, GATE_OPEN, Location, POST_WRITE_READ, Page, REQUEST_TYPE,
    VENDOR_REQUEST,
};

/// How long one control transfer may take. prov: none (the driver's own choice)
const TIMEOUT: Duration = Duration::from_millis(300);

/// A borrowed port, addressed as registers and I2C devices rather than raw transfers.
pub struct Access<'port> {
    /// The dongle's transport.
    port: &'port mut dyn ControlPort,
}

impl<'port> Access<'port> {
    /// Addresses `port`.
    pub fn new(port: &'port mut dyn ControlPort) -> Self {
        Self { port }
    }

    /// The vendor setup for `value` and `index`.
    const fn setup(value: u16, index: u16) -> Setup {
        Setup {
            request_type: REQUEST_TYPE,
            request: VENDOR_REQUEST,
            value,
            index,
        }
    }

    /// Writes `data` to `at`.
    fn write(&mut self, at: Location, data: &[u8]) -> Result<(), BusError> {
        let setup = Self::setup(at.value(), at.write_index());
        self.port.control_out(setup, data, TIMEOUT)
    }

    /// Reads into `data` from `at`; returns the bytes the dongle sent.
    fn read(&mut self, at: Location, data: &mut [u8]) -> Result<usize, BusError> {
        let setup = Self::setup(at.value(), at.index());
        self.port.control_in(setup, data, TIMEOUT)
    }

    /// Writes `data` from `register` of paged-block `page` on, then reads back the
    /// register every such write is followed by ([`POST_WRITE_READ`]).
    ///
    /// # Errors
    ///
    /// Any transport failure.
    pub fn write_paged(&mut self, page: Page, register: u8, data: &[u8]) -> Result<(), BusError> {
        self.write_paged_at(Location::paged(page, register), data)
    }

    /// Writes `data` at paged-block location `at`, then the read every such write is
    /// followed by, even a stalled one (as the captures show); the write's error wins.
    fn write_paged_at(&mut self, at: Location, data: &[u8]) -> Result<(), BusError> {
        let written = self.write(at, data);
        let read = self.read(POST_WRITE_READ, &mut [0]);
        written?;
        read?;
        Ok(())
    }

    /// Writes `data` at `address` of block `block`.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    pub fn write_block(&mut self, block: Block, address: u16, data: &[u8]) -> Result<(), BusError> {
        self.write(Location::block(block, address), data)
    }

    /// Opens (`true`) or closes the tuner's I2C port.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    pub fn set_gate(&mut self, open: bool) -> Result<(), BusError> {
        let byte = if open { GATE_OPEN } else { GATE_CLOSED };
        self.write_paged_at(GATE, &[byte])
    }

    /// Sends `data` to the I2C device at 7-bit `address`: for a register-file device,
    /// its first register, then the values.
    ///
    /// # Errors
    ///
    /// Any transport failure; a stall when no device acknowledges.
    pub fn i2c_write(&mut self, address: I2cAddress, data: &[u8]) -> Result<(), BusError> {
        self.write(address.0, data)
    }

    /// Reads `data` from the I2C device at 7-bit `address`; returns the bytes sent.
    ///
    /// # Errors
    ///
    /// Any transport failure; a stall when no device acknowledges.
    pub fn i2c_read(&mut self, address: I2cAddress, data: &mut [u8]) -> Result<usize, BusError> {
        self.read(address.0, data)
    }
}

/// A 7-bit I2C address, checked once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct I2cAddress(Location);

impl I2cAddress {
    /// Address `address`, if it fits 7 bits.
    #[must_use]
    pub const fn new(address: u8) -> Option<Self> {
        match Location::i2c(address) {
            Some(location) => Some(Self(location)),
            None => None,
        }
    }
}
