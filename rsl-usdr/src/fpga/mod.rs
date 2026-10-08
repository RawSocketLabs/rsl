//! The uSDR FPGA gateware's host-visible registers.

mod gpio;

pub(crate) use gpio::{Gpi, Gpo, Hwid};
