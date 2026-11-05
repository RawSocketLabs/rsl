// Rust binding to libairspy
// Copyright 2015 Adam Greig <adam@adamgreig.com>
// Licensed under the MIT License.

//! Receive floating-point IQ samples from an Airspy through libairspy.
//!
//! Requires libairspy 1.0.6 or later. Each received element is one complex
//! sample. [`Airspy::start_rx`] takes the device and returns a [`Receiver`];
//! [`Receiver::stop`] joins the callback threads and hands the device back,
//! while dropping the Receiver closes it instead.
//!
//! # Capture to a file
//!
//! This example requires an attached Airspy and writes interleaved I/Q values
//! as little-endian `f32` pairs to `airspy.iq`.
//!
//! ```no_run
//! use airspy::{Airspy, DEFAULT_RX_CAPACITY, Error, SampleRateSps};
//! use num_complex::Complex;
//! use std::fs::File;
//! use std::io::{BufWriter, Write};
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut dev = Airspy::open()?;
//! dev.set_sample_rate(SampleRateSps::new(10_000_000))?;
//! dev.set_frequency("434 MHz".parse()?)?;
//! dev.set_lna_agc(false)?;
//! dev.set_mixer_agc(false)?;
//! dev.set_lna_gain(0)?;
//! dev.set_mixer_gain(0)?;
//! dev.set_vga_gain(0)?;
//!
//! let mut output = BufWriter::new(File::create("airspy.iq")?);
//! let mut samples = vec![Complex::default(); 65_536];
//! let mut rx = dev.start_rx(DEFAULT_RX_CAPACITY)?;
//! let mut count = 0;
//! while count < 50_000_000 {
//!     let received = match rx.receive(&mut samples, Duration::from_secs(5)) {
//!         Ok(received) => received,
//!         // Recoverable: the stream resumes after the gap.
//!         Err(Error::Overflow { dropped }) => {
//!             eprintln!("dropped {dropped} samples");
//!             continue;
//!         }
//!         Err(error) => return Err(error.into()),
//!     };
//!     count += received;
//!     for sample in &samples[..received] {
//!         output.write_all(&sample.re.to_le_bytes())?;
//!         output.write_all(&sample.im.to_le_bytes())?;
//!     }
//! }
//! // The device comes back, ready for another session; drop it to close.
//! let _dev = rx.stop()?;
//! output.flush()?;
//! # Ok(())
//! # }
//! ```

// Beyond the workspace lints: every FFI call states the invariant it relies on.
#![warn(clippy::undocumented_unsafe_blocks)]

mod device;
mod error;
mod ffi;
mod library;
mod rx;

pub use device::{Airspy, Identity};
pub use error::{Error, Result};
pub use library::LibVersion;
pub use rfus::{FrequencyHz, SampleRateSps};
pub use rx::{DEFAULT_RX_CAPACITY, Receiver, StartError};
