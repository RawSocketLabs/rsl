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
//! use std::fs::File;
//! use std::io::{BufWriter, Write};
//! use std::sync::mpsc;
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! airspy::init()?;
//! let mut dev = Airspy::new()?;
//! dev.set_sample_rate(10_000_000)?;
//! dev.set_freq(434_000_000)?;
//! dev.set_lna_agc(false)?;
//! dev.set_mixer_agc(false)?;
//! dev.set_lna_gain(0)?;
//! dev.set_mixer_gain(0)?;
//! dev.set_vga_gain(0)?;
//!
//! let mut output = BufWriter::new(File::create("airspy.iq")?);
//! let (tx, rx) = mpsc::channel();
//! dev.start_rx(tx)?;
//! let mut count = 0;
//! while count < 50_000_000 {
//!     let samples = rx.recv_timeout(Duration::from_secs(5))?;
//!     count += samples.len();
//!     for sample in samples {
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

pub use device::{Airspy, PartID};
pub use error::{Error, Result};
pub use library::{exit, init, LibVersion};
pub use rx::IQ;
