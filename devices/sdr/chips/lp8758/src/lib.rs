//! TI LP8758 four-channel step-down converter (PMIC): its register values and identity,
//! shared by the drivers whose boards carry one (the uSDR; the LMS7 family's XSDR, XTRX and
//! sSDR boards in libusdr).
//!
//! Register meanings: TI SNVSAC6B, "LP8758-E0", §7.6: the address map ([`Reg`]), the
//! per-channel registers ([`BuckControl`], [`BuckVoltage`] per [`Buck`]), the chip-wide
//! [`Config`] and the revision ([`Revision`]). Values implement `rsl-lowlevel`'s typed
//! register traits, so a driver writes them without naming an address. Where a board wires
//! the chip and which voltages it sets stay with that board's driver.
//!
//! `no_std`; `bnb`'s bitfields need `alloc`.
#![no_std]
#![forbid(unsafe_code)]

mod buck;
mod config;
mod id;
mod reg;

pub use buck::{Buck, BuckControl, BuckVoltage};
pub use config::Config;
pub use id::{REVISION, Revision};
pub use reg::Reg;

/// The chip's name in errors.
pub const NAME: &str = "LP8758";
