//! [`ControlPort`] over the owned Linux usbfs backend (`usb`).
//!
//! The caller names the dongle, its interface and its bulk IN endpoint: none of them is
//! evidenced yet (`RTL-GAP-01` to `RTL-GAP-03`), so nothing here hard-codes one. Claiming
//! the interface detaches any kernel driver bound to it; `CAPTURES.md` still requires
//! `dvb_usb_rtl28xxu` to be blacklisted before hardware work.

use std::io;
use std::num::NonZeroUsize;
use std::thread;
use std::time::Duration;

use usb::{InQueue, Interface};

use crate::control::{BusError, ControlPort, Setup};
use crate::portable::rtl2832u::{UsbId, UsbInterface};

/// One claimed dongle.
#[derive(Debug)]
pub struct UsbPort {
    /// The claimed interface; control transfers go through it.
    interface: Interface,

    /// The bulk IN endpoint the stream uses.
    rx_endpoint: u8,

    /// The running stream and its transfer length, if any.
    rx: Option<(InQueue, usize)>,
}

impl UsbPort {
    /// Opens the first attached device with identity `id` and claims `interface.number`,
    /// detaching any kernel driver bound to it.
    ///
    /// # Errors
    ///
    /// [`usb::Error::NotFound`] if no such device is attached; any open or claim failure.
    pub fn open(id: UsbId, interface: UsbInterface) -> Result<Self, usb::Error> {
        let info = usb::devices()?
            .into_iter()
            .find(|info| (info.vendor_id(), info.product_id()) == (id.vendor, id.product))
            .ok_or(usb::Error::NotFound)?;
        let claimed = info.open()?.claim_interface(interface.number)?;
        Ok(Self {
            interface: claimed,
            rx_endpoint: interface.rx_endpoint,
            rx: None,
        })
    }
}

impl ControlPort for UsbPort {
    fn control_in(
        &mut self,
        setup: Setup,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, BusError> {
        self.interface
            .control_in(usb_setup(setup), data, timeout)
            .map_err(bus_error)
    }

    fn control_out(
        &mut self,
        setup: Setup,
        data: &[u8],
        timeout: Duration,
    ) -> Result<(), BusError> {
        self.interface
            .control_out(usb_setup(setup), data, timeout)
            .map_err(bus_error)
    }

    fn rx_start(&mut self, transfer_len: usize, depth: NonZeroUsize) -> Result<(), BusError> {
        if self.rx.is_some() {
            return Err(BusError::Unsupported("RX stream already running"));
        }
        let mut queue = self
            .interface
            .in_queue(self.rx_endpoint)
            .map_err(bus_error)?;
        for _ in 0..depth.get() {
            let buffer = queue.alloc(transfer_len).map_err(bus_error)?;
            queue
                .submit(buffer)
                .map_err(|rejected| bus_error(rejected.error))?;
        }
        self.rx = Some((queue, transfer_len));
        Ok(())
    }

    fn rx_read(&mut self, data: &mut [u8], timeout: Duration) -> Result<usize, BusError> {
        let Some((queue, transfer_len)) = &mut self.rx else {
            return Err(BusError::Unsupported("RX stream not running"));
        };
        if data.len() < *transfer_len {
            return Err(BusError::Unsupported("RX buffer shorter than a transfer"));
        }
        let done = queue
            .wait(timeout)
            .map_err(bus_error)?
            .ok_or(BusError::Timeout)?;
        done.status.map_err(usb::Error::from).map_err(bus_error)?;
        let len = done.buffer.len();
        data[..len].copy_from_slice(&done.buffer);
        queue
            .submit(done.buffer)
            .map_err(|rejected| bus_error(rejected.error))?;
        Ok(len)
    }

    fn rx_stop(&mut self) -> Result<(), BusError> {
        let Some((mut queue, _)) = self.rx.take() else {
            return Ok(());
        };
        queue.cancel_all().map(drop).map_err(bus_error)
    }

    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

/// The usb crate's form of a setup.
fn usb_setup(setup: Setup) -> usb::Setup {
    usb::Setup {
        request_type: setup.request_type,
        request: setup.request,
        value: setup.value,
        index: setup.index,
    }
}

/// A usb failure as the seam reports it.
fn bus_error(error: usb::Error) -> BusError {
    match error {
        usb::Error::TimedOut => BusError::Timeout,
        usb::Error::Disconnected => BusError::Disconnected,
        usb::Error::Io(error) => BusError::Io(error),
        other => BusError::Io(io::Error::other(other)),
    }
}
