//! Pure-Rust driver for libusdr's `PE_SYNC` clock-distribution and synchronization board.
//!
//! [`Device`] talks to the board through a [`lowlevel::Bus`], the register/SPI/I2C seam of
//! libusdr's `ls_op`, so its operations are checked against libusdr's on a simulated board
//! (`pe-sync-oracle`).
//!
//! **Unstable; unpublished.** Ported: libusdr's whole open ([`Device::open`]: power-on,
//! status check, clock-tree configuration with every lock wait and the output
//! synchronization) and close ([`Device::close`]). libusdr offers no control after open but
//! a sample rate it always refuses (`PE_SYNC` is not a sample source), which this driver
//! does not offer at all. See `PORTING.md` beside this crate.
//!
//! # The board
//!
//! An FPGA bridges the host link to the clock chips: GPO banks switch each chip's supplies
//! and the LEDs; SPI buses 2 to 5 reach two LMX2820 synthesizers, an LMX1204 clock buffer
//! and multiplier and an LMX1214 LO buffer; I²C core 0 reaches an LMK05318B network
//! synchronizer and two LMK1D1208I LVDS fan-out buffers, core 1 an LP87524 PMIC.
//!
//! Opening powers the board, then builds its clock tree: the LMK05318B (12.8 MHz XO) makes
//! 125, 150 and 10 MHz and 1 Hz; its 150 MHz drives LMX2820 #0 to 1 GHz and #1 to 550 MHz;
//! #0's 1 GHz becomes 4 GHz with SYSREF in the LMX1204, #1's 550 MHz is buffered by the
//! LMX1214; both LMK1D1208Is fan out from their first input. Nothing here is
//! hardware-qualified.
#![forbid(unsafe_code)]

mod device;
mod error;
pub mod lowlevel;
mod portable;

pub use device::Device;
pub use error::{Access, ClockPlanError, Error};
