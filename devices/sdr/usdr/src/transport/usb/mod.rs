//! The uSDR USB control plane over the owned `rsl-usb` host backend.
//!
//! ```no_run
//! # fn main() -> Result<(), rsl_usdr::Error> {
//! use rsl_usdr::{Device, transport::usb::UsbBus};
//! let info = UsbBus::discover()?.into_iter().next()
//!     .ok_or(rsl_usdr::Error::Open(rsl_usdr::lowlevel::BusError::Unsupported("no uSDR attached")))?;
//! let mut radio = Device::with_bus(UsbBus::open(&info)?)?;
//! println!("{} °C", radio.temperature()?);
//! radio.close()?;
//! # Ok(()) }
//! ```
//!
//! Register/SPI/I²C access and board power-up are supported. RX streaming is not.
//! Requires a high-speed readback endpoint (packet size over 256 bytes); full-speed USB
//! is rejected because packet-aligned replies need a device-side zero-length terminator.
//! A failed transaction makes the bus unusable: reopen rather than attributing a late
//! readback or notification to a later request. USB+PCIe mode is refused while `PCIe` owns
//! interrupt routing. Hardware operation remains unqualified until tested on a board.

mod bus;
mod protocol;

pub use bus::UsbBus;

#[cfg(test)]
mod tests;
