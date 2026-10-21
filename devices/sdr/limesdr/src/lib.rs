//! Pure-Rust driver for libusdr's `LimeSDR` family (`u3_limesdr`): the LimeSDR-mini.
//!
//! [`Device`] talks to the board through a [`control::ControlPort`], which carries the
//! firmware's LMS64C control frames ([`lms64c`]), so its operations are checked against
//! libusdr's on a simulated board (`limesdr-oracle`).
//!
//! **Unstable; unpublished.** Ported: the open ([`Device::open`], libusdr's `limesdr_init`),
//! the power-up and the close, which on this family do nothing on the bus, the sample rate
//! ([`Device::set_sample_rate`]), RX/TX LO tuning, gains and baseband bandwidths, with the
//! antenna switches following the LOs, the LMS7002M's preparation for streaming
//! ([`Device::prepare_streaming`]) and a stream's setup in libusdr's formats
//! ([`Device::setup_stream`], [`StreamSetup`]: its converter, `sdr-formats`'s, and burst
//! geometry). Not yet ported: the FT601 USB transport (not in the pinned libusdr) and with
//! it the streams themselves. The LMS7002M's register values, chip
//! operations and control code are `lms7002m`'s. See `PORTING.md` beside this crate.
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
mod gain;
mod portable;
pub mod stream;

pub use device::Device;
pub use error::{Access, Error, ExchangeError};
pub use gain::Gain;
pub use lms7002m::ctrl::Direction;
pub use portable::lms64c;
pub use sdr_formats::SampleFormat;
pub use stream::{StreamSetup, WireFormat};
