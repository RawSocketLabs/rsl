//! Pure-Rust driver for the Wavelet Lab uSDR (`m2_lm6_1`).
//!
//! [`Device`] is the radio. It talks to the board through a [`lowlevel::Bus`]: the same
//! register/SPI/I2C seam libusdr's `ls_op` provides, so the board logic here is checked
//! against libusdr's on a simulated board (`rsl-usdr-oracle`).
//!
//! Status: board power-up and power-down, temperature, and the thermal policy. Transports, sample rate, tuning and
//! streaming follow.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod device;
mod error;
mod fpga;
pub mod lowlevel;
mod thermal;

pub use device::{Device, DeviceBuilder};
pub use error::Error;
pub use thermal::{HARD_STOP_CELSIUS, ThermalLimits, ThermalPolicy};
