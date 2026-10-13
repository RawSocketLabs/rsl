//! The XMASS board's `PCIe` switch (an `ASMedia` ASM2806) as the `usdr_pcie_uram` driver
//! exposes it: `/dev/xmassN`, four ioctls on the switch's firmware flash.
//!
//! Sources: libusdr's `tools/xmass_fwupd.c` (MIT), the only userspace caller, and the
//! request codes in `pcie_uram_driver_if.h`. What the driver does with each request is
//! read from its `usdr_pcie_uram.c` (GPL-2.0, not vendored) at the pinned commit.
//!
//! No `unsafe` here: the requests are `device::xmass_requests`, beside the crate's other
//! ioctls.
#![forbid(unsafe_code)]

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rustix::fs::{Mode, OFlags};

use crate::device::xmass_requests;
pub use crate::uapi::{XMASS_FLASH_BYTES, XMASS_IDS_BYTES};

/// Serialises this process's flash reads and writes on every XMASS device: the driver
/// stages them in one buffer shared by all its XMASS devices, without a lock, so two at
/// once would mix their bytes. Other processes are not held back.
static DRIVER_BUFFER: Mutex<()> = Mutex::new(());

/// An open `/dev/xmassN`.
///
/// The driver lets one open file hold the device at a time, and runs each request through
/// the switch's own SPI master. When the switch loads the firmware its flash holds, and
/// what it does with an erased or partial flash, neither libusdr nor the driver says.
#[derive(Debug)]
pub struct XmassDevice {
    /// The device node.
    fd: OwnedFd,
}

impl XmassDevice {
    /// Opens the XMASS device at `path`, such as `/dev/xmass0`, read-write as
    /// `xmass_fwupd.c` does.
    ///
    /// # Errors
    ///
    /// The driver's refusal: `EBUSY` ([`io::ErrorKind::ResourceBusy`]) while another file
    /// holds the device; or any failure to open the node.
    pub fn open(path: &Path) -> io::Result<Self> {
        let fd = rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())?;
        Ok(Self { fd })
    }

    /// The switch's firmware ID (6 bytes, from its configuration space) and the flash's
    /// JEDEC ID reply (4 bytes), as the driver reads them (`PCIE_GETIDS`).
    ///
    /// # Errors
    ///
    /// The driver's: `EIO` once the switch is gone or in a PCI error state, `ETIMEDOUT`
    /// when its SPI master does not answer.
    pub fn ids(&mut self) -> io::Result<[u8; XMASS_IDS_BYTES]> {
        let mut ids = [0; XMASS_IDS_BYTES];
        xmass_requests::ids(self.fd.as_fd(), &mut ids)?;
        Ok(ids)
    }

    /// Erases the flash's first [`XMASS_FLASH_BYTES`] (`PCIE_FLASH_ERASE`): the driver
    /// clears the flash's write protection, then erases 4 KiB sectors from address 0 up.
    ///
    /// # Errors
    ///
    /// As [`Self::ids`]. A failure leaves the sectors before the one that failed erased.
    pub fn erase(&mut self) -> io::Result<()> {
        Ok(xmass_requests::erase(self.fd.as_fd())?)
    }

    /// Programs `image` into the flash's first [`XMASS_FLASH_BYTES`] (`PCIE_FLASH_WRITE`),
    /// 256-byte pages from address 0 up, without erasing first. The driver writes its own
    /// signature over parts of bytes `0xe00..0xf00` before programming.
    ///
    /// # Errors
    ///
    /// As [`Self::ids`]. A failure leaves the pages before the one that failed programmed.
    pub fn write(&mut self, image: &[u8; XMASS_FLASH_BYTES]) -> io::Result<()> {
        // The request takes a writable pointer; the driver only reads through it.
        let mut staged = Box::new(*image);
        let _buffer = DRIVER_BUFFER.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(xmass_requests::write(self.fd.as_fd(), &mut staged)?)
    }

    /// Reads the flash's first [`XMASS_FLASH_BYTES`] into `out` (`PCIE_FLASH_READ`).
    ///
    /// # Errors
    ///
    /// As [`Self::ids`]; `out` then holds whatever the driver copied, which is not the
    /// flash.
    pub fn read(&mut self, out: &mut [u8; XMASS_FLASH_BYTES]) -> io::Result<()> {
        let _buffer = DRIVER_BUFFER.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(xmass_requests::read(self.fd.as_fd(), out)?)
    }
}
