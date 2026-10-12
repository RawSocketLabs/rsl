//! The uSDR USB control plane and RX streaming over the owned `rsl-usb` host backend.
//!
//! ```no_run
//! # fn main() -> Result<(), rsl_usdr::Error> {
//! use rsl_usdr::{Device, transport::usb::UsbBus};
//! let info = UsbBus::discover()?.into_iter().next()
//!     .ok_or(rsl_usdr::Error::Open(rsl_usdr::lowlevel::BusError::Unsupported("no uSDR attached")))?;
//! let mut radio = Device::with_bus(UsbBus::open(&info)?)?;
//! println!("{} °C", radio.temperature()?);
//! radio.set_rx_sample_rate(20_000_000)?;
//! radio.set_rx_frequency(320_000_000)?;
//! radio.start_rx_stream(1024)?;
//! let mut samples = vec![Default::default(); 1024];
//! let packet = radio.receive(&mut samples, std::time::Duration::from_secs(1))?;
//! println!("{} samples from {}", packet.samples, packet.first_sample);
//! radio.stop_rx_stream()?;
//! radio.close()?;
//! # Ok(()) }
//! ```
//!
//! Register/SPI/I²C access, board power-up and RX streaming are supported. RX uses eight
//! reusable bulk transfers on endpoint 0x83 and validates the complete sample-block/trailer
//! length before delivery. Ordinary receive timeouts leave transfers queued; malformed or
//! failed transfers require stopping the stream before restarting. Stream cleanup remains
//! available after control-plane failure. No TX or full-speed USB streaming is supported.
//! If stream setup or stop fails to drain cancelled transfers, close/drop the device and
//! reopen it: the retained endpoint cannot be reused by another start attempt.
//! Requires a high-speed readback endpoint (packet size over 256 bytes); full-speed USB
//! is rejected because packet-aligned replies need a device-side zero-length terminator.
//! A failed transaction makes the bus unusable: reopen rather than attributing a late
//! readback or notification to a later request. USB+PCIe mode is refused while `PCIe` owns
//! interrupt routing. Hardware operation remains unqualified until tested on a board.
//!
//! Feature `mio` adds `NativeUsb` through `UsbBus::into_native`: an unstable pilot that
//! reads the TMP114 from a caller-owned Mio reactor, with every USB call on a worker thread.
//! It takes over the whole device and does nothing else yet.

mod bus;
mod i2c;
#[cfg(feature = "mio")]
mod native;
mod protocol;
mod rx;

pub use bus::UsbBus;
#[cfg(feature = "mio")]
pub use native::{NativeEvent, NativeUsb};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod i2c_tests;
