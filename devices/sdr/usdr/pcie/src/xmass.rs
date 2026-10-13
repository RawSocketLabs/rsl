//! The XMASS board's `PCIe` switch (an `ASMedia` ASM2806) as the `usdr_pcie_uram` driver
//! exposes it: `/dev/xmassN`, four ioctls on the switch's firmware flash.
//!
//! Sources: libusdr's `tools/xmass_fwupd.c` (MIT), the only userspace caller, and the
//! request codes in `pcie_uram_driver_if.h` (taken under MIT). The driver itself is
//! GPL-2.0 and nothing is taken from it; what is said here of its behaviour beyond those
//! sources is observed interface behaviour, listed in PORTING.md's provenance note.
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

/// Serialises this process's flash reads and writes on every XMASS device: the driver is
/// observed to stage them in one buffer shared by all its XMASS devices, without a lock, so
/// two at once could mix their bytes. Other processes are not held back.
static DRIVER_BUFFER: Mutex<()> = Mutex::new(());

/// An open `/dev/xmassN`.
///
/// Each request returns the driver's return value unjudged. `xmass_fwupd.c` takes any
/// value that is not an error as success; the driver is observed to return 0, or 1 for
/// [`Self::read`], on success, and to pass other positive values through on some failures,
/// so callers should accept only those (`rsl-usdr`'s firmware update does).
///
/// The driver lets one open file hold the device at a time. When the switch loads the
/// firmware its flash holds, and what it does with an erased or partial flash, the sources
/// do not say.
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
    /// Any failure to open the node, among them `EBUSY` ([`io::ErrorKind::ResourceBusy`])
    /// while another file holds the device.
    pub fn open(path: &Path) -> io::Result<Self> {
        let fd = rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())?;
        Ok(Self { fd })
    }

    /// Reads the switch's firmware ID (6 bytes) and the flash's JEDEC ID reply (4 bytes)
    /// into `out` (`PCIE_GETIDS`).
    ///
    /// # Errors
    ///
    /// The driver's errno; among those observed, `EIO` once the switch is gone or in a PCI
    /// error state and `ETIMEDOUT` when its SPI master does not answer. Not a complete
    /// list.
    pub fn ids(&mut self, out: &mut [u8; XMASS_IDS_BYTES]) -> io::Result<i32> {
        Ok(xmass_requests::ids(self.fd.as_fd(), out)?)
    }

    /// Erases the flash's first [`XMASS_FLASH_BYTES`] (`PCIE_FLASH_ERASE`).
    ///
    /// # Errors
    ///
    /// As [`Self::ids`]; the flash may then be part erased.
    pub fn erase(&mut self) -> io::Result<i32> {
        Ok(xmass_requests::erase(self.fd.as_fd())?)
    }

    /// Programs `image` into the flash's first [`XMASS_FLASH_BYTES`] without erasing first
    /// (`PCIE_FLASH_WRITE`). Bytes `0xe00..0xf00` may read back changed.
    ///
    /// # Errors
    ///
    /// As [`Self::ids`]; the flash may then be part programmed.
    pub fn write(&mut self, image: &[u8; XMASS_FLASH_BYTES]) -> io::Result<i32> {
        // The request takes a writable pointer; the driver only reads through it.
        let mut staged = Box::new(*image);
        let _buffer = DRIVER_BUFFER.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(xmass_requests::write(self.fd.as_fd(), &mut staged)?)
    }

    /// Reads the flash's first [`XMASS_FLASH_BYTES`] into `out` (`PCIE_FLASH_READ`).
    ///
    /// # Errors
    ///
    /// As [`Self::ids`]. After a failure, or a return value other than 1, `out` holds
    /// nothing meaningful.
    pub fn read(&mut self, out: &mut [u8; XMASS_FLASH_BYTES]) -> io::Result<i32> {
        let _buffer = DRIVER_BUFFER.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(xmass_requests::read(self.fd.as_fd(), out)?)
    }
}
