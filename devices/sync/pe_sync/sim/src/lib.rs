//! A simulated Wavelet Lab `PE_SYNC` board (libusdr's `pe_sync`): a clock-distribution and
//! synchronization board, not an SDR sample source.
//!
//! The board is modelled at libusdr's low-level operation seam (`ls_op`): FPGA register
//! reads and writes (GPO/GPI), 24-bit SPI words to the LMX2820 synthesizers and the
//! LMX1204/LMX1214 distribution buffers, and I2C transfers to the LMK05318 network
//! synchronizer, the two LMK1D1208I fan-out buffers and the LP87524 PMIC. libusdr (through
//! `pe-sync-oracle`) and, later, `pe-sync` drive the same model, so their recorded
//! [`Op`] traces can be compared without hardware.
//!
//! The model covers identification and libusdr's whole open (power-on, status read, clock
//! tree configuration) and close (power-off). Each modelling assumption is stated, with its
//! evidence, where it is encoded; `devices/sync/pe_sync/AGENTS.md` lists them all.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod trace;

pub use board::{I2cAddress, SimBoard, SimError};
pub use trace::Op;
