//! A simulated uSDR (`m2_lm6_1`) board.
//!
//! The board is modelled at libusdr's low-level operation seam (`ls_op`): FPGA register
//! reads and writes, 32-bit SPI transactions to the LMS6002D, and I2C transfers to the
//! board's support chips, plus the FPGA's QSPI flash controller with a fake flash behind it
//! ([`SimBoard::with_flash`]). Both libusdr (through `rsl-usdr-oracle`) and `rsl-usdr` drive
//! the same model, so their recorded [`Op`] traces and final chip state can be compared
//! without hardware.
//!
//! I2C data is exchanged in chip wire order; packing it into the FPGA I2C core's
//! readback word is a transport concern and lives with the caller.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod flash;
mod stream;
mod trace;

pub use board::{BoardRevision, I2cAddress, SimBoard, SimError};
pub use chips::{DcCalibration, PllLock};
pub use flash::SimFlash;
pub use trace::Op;
