//! TI LMK5C33216 network synchronizer clock: the `SuperSync` front end's clock tree.
//!
//! # What it does
//!
//! Three DPLL/APLL pairs that discipline references and fan them out on sixteen outputs.
//! Registers are bytes behind a 16-bit address, sent most significant byte first.
//!
//! # Sources
//!
//! libusdr's `hw/lmk5c33216/lmk5c33216.c`: `lmk5c33216_create` reads four bytes from
//! address 0 (`VNDRID_BY1`, `VNDRID_BY0`, `PRODID`, `REVID` in `lmk5c33216.yaml`) and
//! requires `10 0B 40 05`, then writes the register load in [`rom`] (a fixed
//! configuration; libusdr notes a soft reset as still to do). The identification is the
//! `SuperSync` probe's, in `sdr-frontend`; this driver writes the load.

mod rom;

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// An LMK5C33216 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lmk5c33216 {
    /// Where the chip answers.
    dev: I2cAddr,
}

impl Lmk5c33216 {
    /// The chip's name in errors.
    const NAME: &'static str = "LMK5C33216";

    /// The chip at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Writes libusdr's register load, entry by entry (`lmk5c33216_reg_wr_n` over
    /// `lmk5c33216_rom`).
    pub(crate) fn load(self, bus: &mut dyn Bus) -> Result<(), Error> {
        for (reg, value) in rom::ROM {
            let [high, low] = reg.to_be_bytes();
            bus.i2c(self.dev, &[high, low, value], &mut [])
                .writing(Self::NAME, reg)?;
        }
        Ok(())
    }
}
