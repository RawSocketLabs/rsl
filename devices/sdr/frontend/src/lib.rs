//! libusdr's front-end board probe (`device/device_fe.c`) and the boards it brings up,
//! shared by the pure-Rust SDR drivers whose devices have a front-end connector: the uSDR
//! and XSDR (M.2 key A+E) and the `LSDr`.
//!
//! **Unstable; unpublished.** After a device's own bring-up, libusdr tries each front-end
//! board its connector admits, in a fixed order, and keeps the first that comes up; an
//! `fe=` hint names one board instead. The boards reach the device through the connector's
//! GPIO core (FPGA register 3, [`gpio`]) and its I2C or SPI bus.
//!
//! Always available, without std:
//!
//! - the boards and their order, which connector admits which ([`Board`], [`Connector`]),
//!   and what a device reports once one came up ([`FrontEndKind`]);
//! - the connector GPIO core's commands as register writes ([`gpio`]);
//! - fe1005000's register-only identification, a portable sequence
//!   ([`fe1005000::Fe1005000Probe`]).
//!
//! Feature `std` adds the blocking probe over `hal`'s `Bus`:
//! `Connector::probe` with the `fe=` hint (`FrontEndSelection`) and the driver's part of
//! it (`BringUp`), every board's identification, and the bring-ups this crate owns whole:
//! the `PCIe` front end (`pciefe`, with its controls) and the M.2-to-`PCIe` breakout
//! (`exm2pe`). The XMASS, `SuperSync` and `SimpleSync` bring-ups need the LMK05318 and
//! LMK5C33216 drivers and stay in `usdr`; `xmass` drives XMASS's control expander for
//! it. Identification mixes connector writes with I2C transfers wider than one register,
//! which the portable `Sequence` seam does not express, so it runs as blocking code.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(any(feature = "std", test))]
extern crate std;

mod board;
#[cfg(feature = "std")]
mod chips;
#[cfg(feature = "std")]
mod error;
#[cfg(feature = "std")]
pub mod exm2pe;
pub mod fe1005000;
#[cfg(feature = "std")]
mod fe4ch;
pub mod gpio;
#[cfg(feature = "std")]
pub mod pciefe;
#[cfg(feature = "std")]
mod probe;
#[cfg(feature = "std")]
mod selection;
#[cfg(feature = "std")]
pub mod simplesync;
#[cfg(feature = "std")]
pub mod supersync;
#[cfg(feature = "std")]
pub mod xmass;

pub use board::{Board, Connector, FrontEndKind};
#[cfg(feature = "std")]
pub use error::Error;
#[cfg(feature = "std")]
pub use exm2pe::{Exm2pe, Exm2peOptions};
#[cfg(feature = "std")]
pub use pciefe::{
    Pciefe, PciefeAttenuation, PciefeDuplexer, PciefeOptions, PciefePath, PciefeRevision,
    PciefeRxFilter, PciefeSetting, PciefeTxFilter,
};
#[cfg(feature = "std")]
pub use probe::{BringUp, Detected, Failure};
#[cfg(feature = "std")]
pub use selection::FrontEndSelection;
#[cfg(feature = "std")]
pub use xmass::XmassCalibrationPath;

/// The FPGA I2C bus of the M.2 key A+E connector's boards (libusdr's `I2C_BUS_FRONTEND`,
/// core 0).
#[cfg(feature = "std")]
const FRONT_END_BUS: u8 = 1;
