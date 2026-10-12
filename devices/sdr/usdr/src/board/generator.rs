//! The TX chain's constant generator: `dev_m2_lm6_1_sdr_tfe_gen_const_set` in
//! `device/m2_lm6_1/m2_lm6_1.c`, through `usdr_tx_gen_set` in `usdr_ctrl.c`.
//!
//! The FPGA's TX chain can replace the host's samples with one fixed I/Q pair, on both
//! channels: a DC level into the DAC, which the TX NCOs turn into a tone. libusdr's LO
//! leakage calibration uses it as its test signal.

use num_complex::Complex;

use super::Board;
use crate::error::Error;
use crate::fpga::Phy;

impl Board {
    /// Replaces the TX samples with `constant` on both channels, or with `None` passes
    /// them again (`usdr_tx_gen_set` with channel mask 3).
    pub(crate) fn set_tx_constant(&mut self, constant: Option<Complex<i16>>) -> Result<(), Error> {
        Phy::set_tx_constant(self.bus.as_mut(), constant)
    }
}
