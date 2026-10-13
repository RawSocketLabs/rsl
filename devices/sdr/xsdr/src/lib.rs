//! Pure-Rust driver for libusdr's LMS7 family (`m2_lm7_1`): the XSDR first.
//!
//! [`Device`] talks to the board through a [`lowlevel::Bus`], the register/SPI/I2C seam of
//! libusdr's `ls_op`, so its operations are checked against libusdr's on a simulated board
//! (`rsl-xsdr-oracle`).
//!
//! **First slice; unstable; unpublished.** Ported: identification by HWID, the XSDR's supply
//! bring-up at open ([`Device::open`]) and the RFIC power-up ([`Device::power_up`]). Not yet
//! ported: the XSDR front-end probe libusdr runs at the end of its open, close and power-down,
//! the XTRX and sSDR variants (identified, then refused), and everything after power-up
//! (rate, tuning, gain, streams, calibration). See `PORTING.md` beside this crate.
//!
//! # The XSDR board
//!
//! An M.2 board: an FPGA bridges the host link (USB or PCI Express) to an LMS7002M RF
//! transceiver on SPI bus 0 and to an LP8758 power-management IC at I2C bus 0, address
//! 0x60. The FPGA's general-purpose outputs hold the LMS7002M in reset, switch its LDOs and
//! select the reference clock.
//!
//! Opening a [`Device`] reads the board's hardware ID, pulses the LMS7002M's power control,
//! checks the LP8758's revision, sets three of its rails and turns on all four converters,
//! then selects the internal 26 MHz reference. [`Device::power_up`] then enables the
//! LMS7002M's LDOs, releases its reset, loads its reset register programme and checks its
//! version.
#![forbid(unsafe_code)]

mod board;
mod device;
mod error;
pub mod lowlevel;
mod portable;

pub use device::Device;
pub use error::{Access, Error};
pub use portable::hwid::Variant;
