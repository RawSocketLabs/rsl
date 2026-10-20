//! The transport seam: what a USB backend, a simulator or a capture replay must provide.
//!
//! **Unstable.** The RTL2832U is driven by vendor control transfers on endpoint 0 and
//! streams samples on one bulk IN endpoint. A port moves one control transfer at a time and
//! keeps their order; which requests carry which register accesses is the driver's
//! business (and, until captures arrive, a GAP in `PROVENANCE.md`), not the port's.

use std::num::NonZeroUsize;
use std::time::Duration;

pub use hal::BusError;

/// A control transfer's setup: everything but the data stage, whose length is the buffer's.
/// The port sets `bmRequestType`'s direction bit from the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Setup {
    /// `bmRequestType`.
    pub request_type: u8,

    /// `bRequest`.
    pub request: u8,

    /// `wValue`.
    pub value: u16,

    /// `wIndex`.
    pub index: u16,
}

/// One dongle's control endpoint, sample stream and time.
pub trait ControlPort: Send {
    /// A control transfer reading into `data`, waiting up to `timeout`; returns the bytes
    /// the device sent.
    ///
    /// # Errors
    ///
    /// Any transport failure, including the device's refusal (a stall).
    fn control_in(
        &mut self,
        setup: Setup,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, BusError>;

    /// A control transfer sending `data`, waiting up to `timeout`.
    ///
    /// # Errors
    ///
    /// Any transport failure, including the device's refusal (a stall).
    fn control_out(&mut self, setup: Setup, data: &[u8], timeout: Duration)
    -> Result<(), BusError>;

    /// Starts the bulk RX stream: keeps `depth` transfers of `transfer_len` bytes in flight.
    ///
    /// # Errors
    ///
    /// Any transport failure; a stream already running.
    fn rx_start(&mut self, transfer_len: usize, depth: NonZeroUsize) -> Result<(), BusError>;

    /// Waits up to `timeout` for the oldest RX transfer, copies it into `data` and
    /// returns its length; the transfer goes back in flight.
    ///
    /// # Errors
    ///
    /// [`BusError::Timeout`]; `data` shorter than a transfer; no stream running; any
    /// transport failure.
    fn rx_read(&mut self, data: &mut [u8], timeout: Duration) -> Result<usize, BusError>;

    /// Stops the bulk RX stream, cancelling what is in flight.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn rx_stop(&mut self) -> Result<(), BusError>;

    /// Waits for `duration`. Driver code never reads the clock itself, so a simulator can
    /// make this virtual.
    fn sleep(&mut self, duration: Duration);
}
