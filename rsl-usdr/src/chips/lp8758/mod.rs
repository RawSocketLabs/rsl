//! LP8758 four-channel step-down converter (PMIC), which supplies the board's core, I/O
//! and RF chip rails.
//!
//! # What it does
//!
//! The LP8758 has four buck converters, each turning the board's input supply into one
//! lower, regulated voltage. TI programs each variant's defaults into one-time-programmable
//! memory (OTP). The "-E0" variant on the uSDR resets with every channel enabled under
//! EN-pin control (see [`BuckControl`]'s reset values), which is how the board's rails are
//! up before any software runs. Over I2C the driver can then move each channel's voltage,
//! turn it on or off, and choose how it switches.
//!
//! On the uSDR the channels feed ([`Buck`] has the detail):
//!
//! | Channel | Rail | Set by the driver |
//! |---------|------|-------------------|
//! | 0 | 1.0 V (reset value 0x4D) | no |
//! | 1 | FPGA GPIO bank | 1.8 V |
//! | 2 | 1.2 V, per a libusdr comment | no |
//! | 3 | LMS6002D digital I/O | 1.8 V, 1.925 V at high sample rates |
//!
//! libusdr's comment calls channel 1 "2v5", but its code sets 1.8 V; this follows the
//! code.
//!
//! # How the driver uses it
//!
//! Power-up checks the revision ([`Lp8758::check_revision`]), turns spread spectrum off
//! and the 105 °C warning on ([`Config`]), sets channels 1 and 3 ([`BuckVoltage`]), then
//! runs all four in forced PWM ([`BuckControl`]). Forced PWM keeps each converter
//! switching at one fixed frequency even at light load. In the other mode, PFM, the
//! switching frequency moves with the load to save power, so its ripple can land anywhere,
//! including inside the band being received. libusdr does not state its reason; this is
//! the usual one for a radio.
//!
//! # Sources
//!
//! Register meanings: TI SNVSAC6B, "LP8758-E0", §7.6 (the board reports OTP revision 0xE0).
//! Values written: libusdr `hw/lp8758/lp8758.c`.
//!
//! # Register map
//!
//! One flat address space ([`reg`]); the value types are split into the per-channel
//! registers ([`buck`]) and the chip-wide [`config`].

mod buck;
mod config;
mod lp8758;
mod reg;

pub(crate) use buck::{Buck, BuckControl, BuckVoltage};
pub(crate) use config::Config;
pub(crate) use lp8758::Lp8758;
