//! The `TPS6381x` driver: where the uSDR wires it. Identification and start-up run in the
//! portable `PowerRails` machine.

use crate::lowlevel::I2cAddr;

/// A `TPS6381x` on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tps6381x {
    /// Where the chip answers.
    addr: I2cAddr,
}

impl Tps6381x {
    /// The converter at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { addr: dev }
    }

    /// The converter where the uSDR wires it: FPGA I2C bus 0, address 0x75, the part's fixed
    /// address (`I2C_DEV_DCDCBOOST` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x75))
    }

    /// Where the chip answers, for a sequence machine's plan.
    pub(crate) const fn dev(self) -> I2cAddr {
        self.addr
    }
}
