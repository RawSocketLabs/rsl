//! An open device and its claimed interfaces.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rustix::io::Errno;

use crate::dispatch::Dispatcher;
use crate::enumerate::DeviceInfo;
use crate::error::Error;
use crate::queue::{InQueue, OutQueue, Pipe};
use crate::usbfs::{URB_BULK, URB_INTERRUPT, Usbfs};

/// How long a reset device may take to come back when it re-enumerates.
const REENUMERATE: Duration = Duration::from_secs(2);
/// How often to look for it meanwhile.
const REENUMERATE_POLL: Duration = Duration::from_millis(50);

/// An open device.
#[derive(Debug)]
pub struct Device {
    /// What it was found as.
    info: DeviceInfo,

    /// Its node.
    usbfs: Arc<Usbfs>,

    /// Its completions; shared with every interface and queue.
    dispatcher: Arc<Dispatcher>,
}

impl Device {
    /// Opens the device `info` describes.
    pub(crate) fn open(info: DeviceInfo) -> Result<Self, Error> {
        let usbfs = Arc::new(Usbfs::open(info.node())?);
        let dispatcher = Arc::new(Dispatcher::new(Box::new(Arc::clone(&usbfs))));
        Ok(Self {
            info,
            usbfs,
            dispatcher,
        })
    }

    /// What the device was found as.
    #[must_use]
    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Resets the device and returns it ready for use. If the reset makes it re-enumerate,
    /// as it does when its descriptors change, this finds it again at the same port and
    /// reopens it.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] while any interface or queue of it is alive; [`Error::Disconnected`]
    /// if it does not come back.
    pub fn reset(self) -> Result<Self, Error> {
        if Arc::strong_count(&self.dispatcher) > 1 {
            return Err(Error::Busy);
        }
        match self.usbfs.reset() {
            Ok(()) => Ok(self),
            Err(Errno::NODEV) => {
                let info = self.info.clone();
                drop(self);
                reopen(&info)
            }
            Err(errno) => Err(errno.into()),
        }
    }

    /// Claims interface `number`, detaching any kernel driver bound to it.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if another program has claimed it.
    pub fn claim_interface(&self, number: u8) -> Result<Interface, Error> {
        self.usbfs.claim(number)?;
        let config = fs::read_to_string(self.info.sysfs().join("bConfigurationValue"))?;
        let name = self
            .info
            .sysfs()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let sysfs = self
            .info
            .sysfs()
            .join(format!("{name}:{}.{number}", config.trim()));
        Ok(Interface(Arc::new(Claimed {
            usbfs: Arc::clone(&self.usbfs),
            dispatcher: Arc::clone(&self.dispatcher),
            number,
            sysfs,
        })))
    }
}

/// Finds a re-enumerated device at `info`'s port and opens it.
fn reopen(info: &DeviceInfo) -> Result<Device, Error> {
    let deadline = Instant::now() + REENUMERATE;
    loop {
        // The old entry lingers briefly; the new one has a new device number.
        if let Some(found) = info
            .find_again()
            .filter(|found| found.node() != info.node())
        {
            return found.open();
        }
        if Instant::now() >= deadline {
            return Err(Error::Disconnected);
        }
        thread::sleep(REENUMERATE_POLL);
    }
}

/// A control request's setup: everything but the data stage, whose length is the buffer's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setup {
    /// `bmRequestType`: direction, type and recipient. Its direction bit must match the call.
    pub request_type: u8,

    /// `bRequest`.
    pub request: u8,

    /// `wValue`.
    pub value: u16,

    /// `wIndex`.
    pub index: u16,
}

/// A claimed interface; released when it and every queue on it are dropped.
#[derive(Clone, Debug)]
pub struct Interface(Arc<Claimed>);

/// A claim on an interface, shared by its queues.
#[derive(Debug)]
struct Claimed {
    /// The device's node.
    usbfs: Arc<Usbfs>,

    /// The device's completions.
    dispatcher: Arc<Dispatcher>,

    /// The interface number.
    number: u8,

    /// Its sysfs directory, which lists its endpoints.
    sysfs: PathBuf,
}

impl Drop for Claimed {
    fn drop(&mut self) {
        self.usbfs.release(self.number);
    }
}

impl Interface {
    /// The interface number.
    #[must_use]
    pub fn number(&self) -> u8 {
        self.0.number
    }

    /// A control request reading into `data`, waiting up to `timeout`; returns the bytes
    /// read.
    ///
    /// # Errors
    ///
    /// [`Error::TimedOut`], [`Error::Disconnected`], or the device's refusal (a stall).
    pub fn control_in(
        &self,
        setup: Setup,
        data: &mut [u8],
        timeout: Duration,
    ) -> Result<usize, Error> {
        let request_type = setup.request_type | 0x80;
        self.0.usbfs.control(
            [request_type, setup.request],
            setup.value,
            setup.index,
            data,
            timeout,
        )
    }

    /// A control request sending `data`, waiting up to `timeout`.
    ///
    /// # Errors
    ///
    /// [`Error::TimedOut`], [`Error::Disconnected`], or the device's refusal (a stall).
    pub fn control_out(&self, setup: Setup, data: &[u8], timeout: Duration) -> Result<(), Error> {
        let request_type = setup.request_type & !0x80;
        // The kernel only reads an OUT request's data; the copy keeps that out of the
        // signature's way.
        let mut data = data.to_vec();
        self.0.usbfs.control(
            [request_type, setup.request],
            setup.value,
            setup.index,
            &mut data,
            timeout,
        )?;
        Ok(())
    }

    /// A queue of IN transfers on `endpoint` (direction bit set, such as `0x81`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidEndpoint`] if the interface has no such bulk or interrupt IN
    /// endpoint; [`Error::Busy`] if it already has a queue.
    pub fn in_queue(&self, endpoint: u8) -> Result<InQueue, Error> {
        if endpoint & 0x80 == 0 {
            return Err(Error::InvalidEndpoint(endpoint));
        }
        self.pipe(endpoint).map(InQueue::new)
    }

    /// A queue of OUT transfers on `endpoint` (direction bit clear, such as `0x01`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidEndpoint`] if the interface has no such bulk or interrupt OUT
    /// endpoint; [`Error::Busy`] if it already has a queue.
    pub fn out_queue(&self, endpoint: u8) -> Result<OutQueue, Error> {
        if endpoint & 0x80 != 0 {
            return Err(Error::InvalidEndpoint(endpoint));
        }
        self.pipe(endpoint).map(OutQueue::new)
    }

    /// A pipe on one of this interface's endpoints.
    fn pipe(&self, endpoint: u8) -> Result<Pipe, Error> {
        let dir = self.0.sysfs.join(format!("ep_{endpoint:02x}"));
        let attribute = |file: &str| {
            fs::read_to_string(dir.join(file)).map_err(|_| Error::InvalidEndpoint(endpoint))
        };
        let kind = match attribute("type")?.trim() {
            "Bulk" => URB_BULK,
            "Interrupt" => URB_INTERRUPT,
            _ => return Err(Error::InvalidEndpoint(endpoint)),
        };
        let size = u16::from_str_radix(attribute("wMaxPacketSize")?.trim(), 16)
            .map_err(|_| Error::InvalidEndpoint(endpoint))?;
        // Bits 10:0 are the packet size; 12:11 add packets per microframe, not bytes.
        let max_packet = usize::from(size & 0x7ff);
        self.0.dispatcher.open(endpoint)?;
        Ok(Pipe::new(
            Arc::clone(&self.0.dispatcher),
            Arc::clone(&self.0) as _,
            endpoint,
            kind,
            max_packet,
        ))
    }
}
