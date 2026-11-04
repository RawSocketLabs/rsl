// Rust binding to libairspy
// Copyright 2015 Adam Greig <adam@adamgreig.com>
// Licensed under the MIT License.

//! Receive floating-point IQ samples from an Airspy through libairspy.
//!
//! Requires libairspy 1.0.6 or later. Each received element is one complex
//! sample. Call [`Airspy::stop_rx`] to join the callback threads before restarting;
//! dropping the device also stops reception.
//!
//! # Capture to a file
//!
//! This example requires an attached Airspy and writes interleaved I/Q values
//! as little-endian `f32` pairs to `airspy.iq`.
//!
//! ```no_run
//! use airspy::Airspy;
//! use num_complex::Complex;
//! use std::fs::File;
//! use std::io::{BufWriter, Write};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut dev = Airspy::open()?;
//! dev.set_sample_rate(10_000_000)?;
//! dev.set_frequency(434_000_000)?;
//! dev.set_lna_agc(false)?;
//! dev.set_mixer_agc(false)?;
//! dev.set_lna_gain(0)?;
//! dev.set_mixer_gain(0)?;
//! dev.set_vga_gain(0)?;
//!
//! let mut output = BufWriter::new(File::create("airspy.iq")?);
//! let mut samples = vec![Complex::default(); 65_536];
//! dev.start_rx()?;
//! let mut count = 0;
//! while count < 50_000_000 {
//!     let received = dev.receive(&mut samples, 5_000)?;
//!     count += received;
//!     for sample in &samples[..received] {
//!         output.write_all(&sample.re.to_le_bytes())?;
//!         output.write_all(&sample.im.to_le_bytes())?;
//!     }
//! }
//! dev.stop_rx()?;
//! output.flush()?;
//! # Ok(())
//! # }
//! ```

mod device;
mod error;
mod ffi;
mod library;
mod rx;

pub use device::{Airspy, Identity};
pub use error::{Error, Result};
pub use library::LibVersion;
