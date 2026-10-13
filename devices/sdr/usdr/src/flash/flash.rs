//! The boot flash's handles: [`Flash`] reads, [`Programmer`] erases and writes.

use super::image::{Image, Slot};
use super::update::{self, BootImages, UpdateError, UpdateOptions};
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

    /// Reads the running gateware's build, the flash ID and both images' headers, as
    /// libusdr's flash tool does before anything else; what [`BootImages::check`] and the
    /// updates compare a new image with.
    ///
    /// # Errors
    ///
    /// [`UpdateError::UnrecognizedFlash`] for a part libusdr does not know;
    /// [`UpdateError::GoldenHeader`] if the golden image's header is unreadable;
    /// [`UpdateError::Flash`] if a read fails.
    pub fn boot_images(&mut self) -> Result<BootImages, UpdateError> {
        BootImages::read(&mut self.bus)
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

    /// Replaces the master image with `image`, as libusdr's flash tool does (`-w`): reads
    /// the [`BootImages`], refuses an image that does not belong in the master slot or is
    /// already there, erases the image's whole range and writes all of it but the first 256
    /// bytes, then those, and reads everything back. Nothing below
    /// [`MASTER_IMAGE_OFFSET`] is written.
    ///
    /// # Errors
    ///
    /// Before anything is written: those of [`Flash::boot_images`];
    /// [`UpdateError::Mismatch`] where [`BootImages::check`] fails;
    /// [`UpdateError::AlreadyRunning`] or [`UpdateError::AlreadyFlashed`] unless
    /// `options` say [`reflash`](UpdateOptions::reflash); [`UpdateError::Flash`] with
    /// [`FlashError::OutOfRange`] for an image past the flash's 16 MiB. Then
    /// [`UpdateError::Flash`] with the master slot partly erased or written but its header
    /// still blank, unless the failure was writing the header; or
    /// [`UpdateError::VerifyFailed`] once all of it was written.
    pub fn update_master(
        &mut self,
        image: &Image,
        options: UpdateOptions,
    ) -> Result<(), UpdateError> {
        update::update(self.bus, image, Slot::Master, options)
    }

    /// Replaces the golden image, the one the FPGA loads at power-up, with `image`, as
    /// libusdr's flash tool does when asked to (`-w` with `-G`): reads the [`BootImages`],
    /// refuses an image that is not a golden image for this FPGA or is already there, writes
    /// the 4 KiB boot sector, then the rest, and reads everything back.
    ///
    /// This is the one operation here that can leave the board unable to load any
    /// gateware: interrupted, the golden image is incomplete, and only rewriting the flash
    /// by other means (JTAG) recovers the board. Update the master image instead unless the
    /// golden image itself must change.
    ///
    /// # Errors
    ///
    /// As [`Programmer::update_master`], except that a failure while writing leaves the
    /// golden image partly erased or written.
    pub fn update_golden(
        &mut self,
        image: &Image,
        options: UpdateOptions,
    ) -> Result<(), UpdateError> {
        update::update(self.bus, image, Slot::Golden, options)
    }

    /// Blanks the master image's first 64 KiB, header included, so that only the golden
    /// image remains loadable, as libusdr's flash tool does (`-E`, after its typed
    /// confirmation, which calling this stands for). Reads the [`BootImages`] first and
    /// erases only if the golden image warm-boots [`MASTER_IMAGE_OFFSET`].
    ///
    /// # Errors
    ///
    /// Before anything is erased: those of [`Flash::boot_images`];
    /// [`UpdateError::NoMasterSupport`] if the golden image warm-boots nothing;
    /// [`UpdateError::TrampolineMismatch`] if it warm-boots elsewhere. Then
    /// [`UpdateError::Flash`] with the sector partly erased.
    pub fn erase_master(&mut self) -> Result<(), UpdateError> {
        update::erase_master(self.bus)
    }
}
