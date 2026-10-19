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
//!   ([`fe1005000::Fe1005000Probe`]), and its settings ([`Fe1005000`]), whose tunings
//!   are register-only too.
//!
//! Feature `std` adds the blocking probe over `hal`'s `Bus`:
//! `Connector::probe` with the `fe=` hint (`FrontEndSelection`), logging the events
//! libusdr's probe logs (through `tracing`, in this crate's words and levels), and the board that came up (`FrontEnd`), brought up whole with its
//! controls: the `PCIe` front end (`pciefe`), XMASS (`xmass`), `SuperSync` (`supersync`),
//! `SimpleSync` (`simplesync`), the M.2-to-`PCIe` breakout (`exm2pe`) and the 100–5000 MHz
//! front end (`fe1005000`), with the chips they carry (the LMK05318's bus code over
//! `lmk05318`'s plans, the LMK5C33216, ...). fe4ch4007200 is identified only: its
//! chips sit on an I2C core no device this crate serves has. A driver keeps the `FrontEnd`
//! and offers the controls libusdr's device offers. The I2C bring-ups mix connector writes
//! with transfers wider than one register, which the portable `Sequence` seam does not
//! express, so they run as blocking code.
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
mod simplesync;
#[cfg(feature = "std")]
mod supersync;
#[cfg(feature = "std")]
mod xmass;

pub use board::{Board, Connector, FrontEndKind};
#[cfg(feature = "std")]
pub use error::Error;
#[cfg(feature = "std")]
pub use exm2pe::{Exm2pe, Exm2peOptions};
pub use fe1005000::{
    Fe1005000, Fe1005000Band, Fe1005000Control, Fe1005000IfFilter, Fe1005000Manual,
    Fe1005000Setting,
};
#[cfg(feature = "std")]
pub use lmk05318::plan::ClockPlanError;
#[cfg(feature = "std")]
pub use pciefe::{
    Pciefe, PciefeAttenuation, PciefeDuplexer, PciefeOptions, PciefePath, PciefeRevision,
    PciefeRxFilter, PciefeSetting, PciefeTxFilter,
};
#[cfg(feature = "std")]
pub use probe::FrontEnd;
#[cfg(feature = "std")]
pub use selection::FrontEndSelection;
#[cfg(feature = "std")]
pub use simplesync::SimpleSync;
#[cfg(feature = "std")]
pub use supersync::SuperSync;
#[cfg(feature = "std")]
pub use xmass::{Xmass, XmassCalibrationPath};

/// The FPGA I2C bus of the M.2 key A+E connector's boards (libusdr's `I2C_BUS_FRONTEND`,
/// core 0).
#[cfg(feature = "std")]
const FRONT_END_BUS: u8 = 1;
