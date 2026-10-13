//! The boot flash's handles: [`Flash`] reads, [`Programmer`] erases and writes.

use crate::fpga::flash;
use crate::lowlevel::Bus;

pub use crate::fpga::flash::{
    ADDRESS_SPACE, FlashError, FlashId, HEADER_BYTES, PAGE_BYTES, SECTOR_BYTES, SUBSECTOR_BYTES,
    WriteOptions,
};

/// Where the golden image starts, which libusdr's update code overwrites only on explicit
/// request (`json_controller/controller_ugen.c`).
pub const GOLDEN_IMAGE_OFFSET: u32 = 0;

/// Where the master image starts: the uSDR's `/ll/qspi_flash/master_off`
/// (`device/m2_lm6_1/m2_lm6_1.c`).
pub const MASTER_IMAGE_OFFSET: u32 = 0x1c_0000;

/// The board's boot flash, for reading; [`Flash::programmer`] erases and writes it.
#[derive(Debug)]
pub struct Flash<B> {
    /// The hardware seam, owned until [`Flash::into_bus`].
    bus: B,
}

impl<B: Bus> Flash<B> {
    /// Reaches the flash through `bus`. Nothing is sent until an operation is called.
    pub const fn new(bus: B) -> Self {
        Self { bus }
    }

    /// Reads the flash's JEDEC ID. Its [`Display`](std::fmt::Display) names the part as
    /// libusdr does.
    ///
    /// # Errors
    ///
    /// [`FlashError::ControllerTimeout`]; any bus failure.
    pub fn id(&mut self) -> Result<FlashId, FlashError> {
        flash::read_id(&mut self.bus)
    }

    /// Fills `out` with the flash's contents from `offset`.
    ///
    /// # Errors
    ///
    /// [`FlashError::OutOfRange`] past 16 MiB, before anything is sent;
    /// [`FlashError::ControllerTimeout`]; any bus failure.
    pub fn read(&mut self, offset: u32, out: &mut [u8]) -> Result<(), FlashError> {
        flash::read(&mut self.bus, offset, out)
    }

    /// The erase and write operations. They are destructive: call this only to change what
    /// the board boots.
    pub fn programmer(&mut self) -> Programmer<'_, B> {
        Programmer { bus: &mut self.bus }
    }

    /// Gives the bus back, for example to open a [`Device`](crate::Device) on it.
    pub fn into_bus(self) -> B {
        self.bus
    }
}

/// Erases and writes the boot flash; from [`Flash::programmer`].
///
/// Every operation here destroys flash contents. Overwriting the golden image
/// ([`GOLDEN_IMAGE_OFFSET`]) or interrupting a write to the master image can leave the
/// board unable to load its gateware until it is recovered by other means. Each
/// operation checks its range first and sends nothing if the range is invalid; once
/// started, a failure leaves the range partly erased or written.
#[derive(Debug)]
pub struct Programmer<'a, B> {
    /// The flash's bus.
    bus: &'a mut B,
}

impl<B: Bus> Programmer<'_, B> {
    /// Erases `len` bytes from `offset` to `0xff`: 64 KiB sectors where aligned, 4 KiB
    /// subsectors elsewhere, waiting for each to finish.
    ///
    /// # Errors
    ///
    /// Before anything is sent: [`FlashError::EraseGranularity`] unless `offset` and a
    /// non-zero `len` are multiples of 4 KiB; [`FlashError::OutOfRange`] past 16 MiB.
    /// Then [`FlashError::FlashTimeout`], [`FlashError::ControllerTimeout`] or any bus
    /// failure, with the range partly erased.
    pub fn erase(&mut self, offset: u32, len: u32) -> Result<(), FlashError> {
        flash::erase(self.bus, offset, len)
    }

    /// Erases the whole flash, golden image included, and waits for it to finish.
    ///
    /// # Errors
    ///
    /// [`FlashError::FlashTimeout`] if the flash is still busy after libusdr's poll limit
    /// (a large part's whole-chip erase can outlast it); [`FlashError::ControllerTimeout`];
    /// any bus failure.
    pub fn erase_chip(&mut self) -> Result<(), FlashError> {
        flash::erase_chip(self.bus)
    }

    /// Writes `data` from `offset`, as `options` say: by default the range is erased first,
    /// which needs it to be whole 4 KiB subsectors. Data is programmed 128 bytes at a time,
    /// each chunk checked in the controller's buffer before it is programmed.
    ///
    /// # Errors
    ///
    /// Before anything is sent: [`FlashError::OutOfRange`] past 16 MiB;
    /// [`FlashError::EraseGranularity`] when erasing a range that is not whole subsectors;
    /// [`FlashError::NothingToWrite`] for no data, or none past a skipped header;
    /// [`FlashError::PageCrossing`] if a 128-byte chunk would cross a 256-byte flash page,
    /// which happens only from an offset that is not a multiple of 128.
    /// Then [`FlashError::StagingMismatch`], [`FlashError::FlashTimeout`],
    /// [`FlashError::ControllerTimeout`] or any bus failure, with the range partly
    /// erased or written.
    pub fn write(
        &mut self,
        offset: u32,
        data: &[u8],
        options: WriteOptions,
    ) -> Result<(), FlashError> {
        flash::write(self.bus, offset, data, options)
    }
}
