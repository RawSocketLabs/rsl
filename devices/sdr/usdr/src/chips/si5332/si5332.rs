//! The Si5332 driver: where the uSDR wires it, and the mixer-LO enable the board flips in
//! ACTIVE. Its identification, power-up and sample-clock plans run as the portable
//! `ClockSequence` machine.

use crate::chips::register::I2cChip;
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};
use crate::portable::si5332::{self, output::OutputEnables0to3};

/// A Si5332 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Si5332 {
    /// Where the chip answers.
    addr: I2cAddr,
}

impl I2cChip for Si5332 {
    const NAME: &'static str = si5332::NAME;

    fn addr(&self) -> I2cAddr {
        self.addr
    }
}

impl Si5332 {
    /// The clock generator at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { addr: dev }
    }

    /// The clock generator where the uSDR wires it: FPGA I2C bus 0, address 0x6A
    /// (`I2C_DEV_CLKGEN` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x6a))
    }

    /// Where the chip answers, for a sequence machine.
    pub(crate) const fn dev(self) -> I2cAddr {
        self.addr
    }

    /// Switches output 3, the mixer LO, on or off with its output-enable bit alone, while
    /// the chip stays ACTIVE.
    ///
    /// Intentional divergence from libusdr's `si5332_set_port3_en`, which requests READY to
    /// change the power-down registers; in READY the chip drives no outputs, so the sample
    /// clocks stop. Skyworks' Si5332 Reference Manual (rev 0.6, "Si5332 32-QFN Specific
    /// Registers", p. 52) lists `OUT3_OE` as writable in ACTIVE; the uSDR's six-output part
    /// uses that map, where `OUT3_OE` is bit 6, as in libusdr.
    ///
    /// Output 3's divider and driver are powered from power-up (`ClockSequence::program`) on, and
    /// each sample rate (`ClockSequence::layout`) configures them, so the enable is all that changes: off, they
    /// stay powered but disabled, where libusdr powers them down. As in libusdr, output 3
    /// runs from the first sample rate until the first band crossing, because power-up
    /// enables every output, and is silent before any rate. Output 2, the TX clock, keeps
    /// its power-up enable, where libusdr turns it off whenever TX is not streaming;
    /// revisit [`OutputEnables0to3::ALL`] here when TX is ported.
    pub(crate) fn set_mixer_lo(self, bus: &mut dyn Bus, on: bool) -> Result<(), Error> {
        self.on(bus).write(OutputEnables0to3::ALL.with_output3(on))
    }
}
