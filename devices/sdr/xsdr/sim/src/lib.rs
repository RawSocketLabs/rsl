//! A simulated LMS7 family board (libusdr's `m2_lm7_1`: XSDR, XTRX, sSDR, sSDR Pro).
//!
//! The board is modelled at libusdr's low-level operation seam (`ls_op`): FPGA register
//! reads and writes (GPO/GPI), 32-bit SPI transactions to the LMS7002M, and I2C transfers
//! to the board's support chips (LP8758 PMICs, the XTRX DAC80501). libusdr (through
//! `xsdr-oracle`) and, later, `xsdr` drive the same model, so their recorded [`Op`]
//! traces can be compared without hardware.
//!
//! The model covers identification, `xsdr_init` and RFIC power-up/close for XSDR and XTRX.
//! sSDR variants are identified but carry no LMS8001, so libusdr refuses to open them.
//! Each modelling assumption is stated, with its evidence, where it is encoded; see the
//! family's `AGENTS.md` for the full list.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod trace;

pub use board::{I2cAddress, SimBoard, SimError, Variant};
pub use trace::Op;
