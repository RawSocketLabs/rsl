//! A simulated `LSDr` board (libusdr's `m2_da09_4_ad45_2` factory, device name `lsdr`).
//!
//! The board is modelled at libusdr's low-level operation seam (`ls_op`): FPGA register
//! reads and writes (GPO), 32-bit SPI transactions to the `ADS42LBx9` ADCs and the LMK04832
//! clock generator, and I2C transfers to the TPS6594 PMIC and the Si549 oscillator.
//! libusdr (through `lsdr-oracle`) and, later, `lsdr` drive the same model, so
//! their recorded [`Op`] traces can be compared without hardware.
//!
//! The model covers open (`usdr_device_m2_d09_4_ad45_2_initialize`), ADC power-up
//! (`/dm/power/en`) and close, and fe1005000 on the front-end connector
//! ([`SimBoard::with_fe1005000`]). The board is receive-only: libusdr declares no TX channel
//! and no DAC, and neither does the sim. Each modelling assumption is stated, with its
//! evidence, where it is encoded; see the family's `AGENTS.md` for the full list.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod trace;

pub use board::{I2cAddress, SimBoard, SimError, Variant};
pub use trace::Op;
