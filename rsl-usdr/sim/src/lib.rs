//! A simulated uSDR (`m2_lm6_1`) board.
//!
//! The board is modelled at libusdr's low-level operation seam (`ls_op`): FPGA register
//! reads and writes, 32-bit SPI transactions to the LMS6002D, and I2C transfers to the
//! board's support chips. Both libusdr (through `rsl-usdr-oracle`) and `rsl-usdr` drive
//! the same model, so their recorded [`Op`] traces and final chip state can be compared
//! without hardware.
//!
//! I2C data is exchanged in chip wire order; packing it into the FPGA I2C core's
//! readback word is a transport concern and lives with the caller.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod trace;

pub use board::{BoardRevision, I2cAddress, SimBoard, SimError};
pub use chips::{DcCalibration, PllLock};
pub use trace::Op;
