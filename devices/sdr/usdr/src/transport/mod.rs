//! Transports: [`Bus`](crate::lowlevel::Bus) implementations that reach real hardware, or a
//! Verilator model of its gateware.

#[cfg(all(feature = "pcie", target_os = "linux"))]
pub mod pcie;
#[cfg(all(feature = "usb", target_os = "linux"))]
pub mod usb;
#[cfg(all(feature = "verilator", target_os = "linux"))]
pub mod verilator;
#[cfg(any(test, all(any(feature = "pcie", feature = "usb"), target_os = "linux")))]
mod window;
