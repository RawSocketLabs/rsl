//! The hardware seam the pure-Rust device drivers share: libusdr's `ls_op` operations
//! (gateware registers, SPI words, I²C transfers) and the register sequences built on them.
//!
//! **Unstable; unpublished.** Always available, without std:
//!
//! - bus identities and bounds: [`I2cAddr`], [`SpiAddr`], [`I2C_MAX_WRITE`], [`I2C_MAX_READ`];
//! - typed registers: [`Register`] and [`IndexedRegister`];
//! - the sequence seam: a [`Sequence`] names one [`SequenceAction`] at a time and stops
//!   with a [`SequenceFailure`] that keeps the executor's error unchanged;
//!   [`RegisterAccess::context`] and [`FpgaAccess::context`] decode a failed access into the
//!   [`Access`] direction and register a driver's error reports.
//!
//! Feature `std` adds the blocking `Bus` trait, its `BusError` and the
//! `run_sequence` executor that drives a sequence over a bus.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(any(feature = "std", test))]
extern crate std;

mod address;
#[cfg(feature = "std")]
mod bus;
mod register;
mod sequence;

pub use address::{I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
#[cfg(feature = "std")]
pub use bus::{Bus, BusError, run_sequence};
pub use register::{IndexedRegister, Register};
pub use sequence::{
    Access, FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure, i2c_byte,
};
