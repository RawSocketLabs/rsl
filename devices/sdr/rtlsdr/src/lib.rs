//! Clean-room pure-Rust driver for RTL2832U RTL-SDR dongles (Rafael R820T2 and R828D tuners).
//!
//! **Unstable; unpublished; not hardware-qualified.** Licence MIT OR Apache-2.0: no GPL
//! RTL-SDR source, nor anything derived from one, is read for this crate. Every register
//! address, value, request code and sequence comes from an admissible source recorded in
//! `PROVENANCE.md` beside this crate, and every constant names its ledger row with a
//! `prov: <ID>` tag. A value the evidence does not yet give is a [`Pending`] placeholder,
//! which refuses at runtime with its ledger row ([`Gap`]) instead of guessing.
//!
//! What exists: the transport seam ([`control::ControlPort`]: vendor control transfers, a
//! bulk RX stream and time), its usbfs implementation (feature `usb`), a [`Device`] whose
//! open waits on the captures, and the R820T2 register maths the register description
//! defines ([`r820t2`]). The RTL2832U side (request codes, register map, I2C repeater, init
//! order, the dongles' VID:PID) awaits USB captures; see `CAPTURES.md`.
#![forbid(unsafe_code)]

pub mod control;
mod device;
mod error;
mod portable;
#[cfg(all(feature = "usb", target_os = "linux"))]
pub mod transport;

pub use device::Device;
pub use error::Error;
pub use portable::pending::{Gap, Pending};
pub use portable::{r820t2, rtl2832u};
