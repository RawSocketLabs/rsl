//! The Si5332 driver: where the uSDR wires it. Its identification, power-up and sample-clock
//! plans run as the portable `ClockSequence` machine, and the band crossing's output-3
//! enable as part of the portable `SelectRxBand`.

use crate::lowlevel::I2cAddr;
use crate::portable::radio::wiring;

/// A Si5332 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Si5332 {
    /// Where the chip answers.
    addr: I2cAddr,
}

impl Si5332 {
    /// The clock generator at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { addr: dev }
    }

    /// The clock generator where the uSDR wires it: FPGA I2C bus 0, address 0x6A
    /// (`I2C_DEV_CLKGEN` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(wiring::CLOCK)
    }

    /// Where the chip answers, for a sequence machine.
    pub(crate) const fn dev(self) -> I2cAddr {
        self.addr
    }
}
