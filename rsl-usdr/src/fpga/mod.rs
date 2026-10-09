//! The uSDR FPGA gateware's host-visible registers.

mod fir_tables;
mod gpio;
mod phy;

pub(crate) use gpio::{Gpi, Gpo, Hwid};
pub(crate) use phy::{Decimation, Nco, Phy};
