//! Pure-Rust driver for libusdr's LMS7 family (`m2_lm7_1`): the XSDR first.
//!
//! [`Device`] talks to the board through a [`lowlevel::Bus`], the register/SPI/I2C seam of
//! libusdr's `ls_op`, so its operations are checked against libusdr's on a simulated board
//! (`xsdr-oracle`).
//!
//! **Unstable; unpublished.** Ported: identification by HWID, the XSDR's supply bring-up and
//! front-end board probe at open ([`Device::open`], [`Device::front_end`]; the probe is
//! `sdr-frontend`'s), the RFIC power-up ([`Device::power_up`]), the sample rate on
//! gateware with a TX MMCM ([`Device::set_sample_rate`]), RX/TX LO tuning, gains and
//! baseband bandwidths, explicit RX/TX paths with the RF loopback, TDD tuning, the
//! baseband NCOs, the DC and I/Q corrections, the TX test generator and mute, the
//! baseband and digital loopbacks, the RX and TX streams in `ci16`
//! ([`Device::create_rx_stream`], [`Device::receive`], [`Device::create_tx_stream`],
//! [`Device::send`]; the stream engine's registers are `sdr-stream`'s, shared with the
//! uSDR) with the TX MMCM, the TX phase calibration and the TX quality check, and the
//! close ([`Device::close`]); the probe brings up every board the connector admits, as
//! libusdr does. Not yet ported: the front ends' controls, the XTRX and sSDR variants
//! (identified, then refused), gateware with an RX or shared MMCM, a DUC/DDC or the
//! extended TX front end, and the RF calibrations. The LMS7002M's register values,
//! chip operations and control code are `lms7002m`'s. See `PORTING.md` beside this crate.
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
//! version. [`Device::close`] brings the LMS7002M's paths down, powers its converters
//! down, holds it in reset with its supplies off and powers the TX MMCM down.
#![forbid(unsafe_code)]

mod board;
mod device;
mod error;
mod fe_probe;
pub mod lowlevel;
mod portable;

pub use device::{
    Device, IqCorrection, LmlRxMode, RxChannels, RxPacket, TxChannels, TxGenerator, TxTelemetry,
};
pub use error::{Access, Error};
pub use lms7002m::Mac;
pub use lms7002m::ctrl::{Direction, Gain, RfPath, RxRoute, TxRoute};
pub use portable::hwid::Variant;
pub use sdr_frontend::FrontEndKind;
