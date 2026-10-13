//! Pure-Rust driver for libusdr's `LimeSDR` family (`u3_limesdr`): the LimeSDR-mini.
//!
//! [`Device`] talks to the board through a [`control::ControlPort`], which carries the
//! firmware's LMS64C control frames ([`lms64c`]), so its operations are checked against
//! libusdr's on a simulated board (`rsl-limesdr-oracle`).
//!
//! **Unstable; unpublished.** Ported: the open ([`Device::open`], libusdr's `limesdr_init`),
//! the power-up and the close, which on this family do nothing on the bus. Not yet ported:
//! the FT601 USB transport (not in the pinned libusdr), and everything after the open (rate,
//! tuning, gain, streams). The LMS7002M's register values and chip operations are
//! `rsl-lms7002m`'s. See `PORTING.md` beside this crate.
//!
//! # The LimeSDR-mini
//!
//! An FT601 USB 3 bridge in front of gateware with a 16-bit register file, firmware that
//! answers LMS64C frames, and an LMS7002M RF transceiver. Nothing on the wire identifies
//! the board: libusdr selects the family by USB VID:PID and reports an emulated revision.
//!
//! Opening a [`Device`] measures the reference clock against the bridge's 100 MHz clock,
//! pulses the LMS7002M's reset, requires its version word to be 0x3841, loads its reset
//! register programme and stops streaming. Which LMS64C command carries each register
//! access is an assumption shared with the oracle, until libusdr's FT601 transport is
//! available to check it against.
#![forbid(unsafe_code)]

pub mod control;
mod device;
mod error;
mod portable;

pub use device::Device;
pub use error::{Access, Error, ExchangeError};
pub use portable::lms64c;
