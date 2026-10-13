//! libusdr's gateware update procedure, as its flash tool runs it (`tools/usdr_flash.c`,
//! `usdr_flash.c`:N below): read what the board runs and what its flash holds, check the
//! new image against it, write it in libusdr's order and read it back.
//!
//! Every step is the tool's, in its order, so the register operations match it one for
//! one on the simulated board. The tool's command line, its prompts and messages, its
//! deliberate image corruption (`-C`) and its file I/O are not ported: the caller holds
//! the image's bytes, and calling [`Programmer::erase_master`](super::Programmer::erase_master)
//! is the confirmation the tool asks to have typed.

use std::time::Duration;

use super::image::{GOLDEN_BOOT_BYTES, HeaderError, Image, ImageHeader, ImageMismatch, Slot};
use super::{FlashError, FlashId, HEADER_BYTES, MASTER_IMAGE_OFFSET, SECTOR_BYTES, WriteOptions};
use crate::fpga::{Gpi, flash};
use crate::lowlevel::{Bus, BusError};

/// The tool's wait after reading the board's description, before the flash
/// (`usleep(1000)`, `usdr_flash.c`:157).
const SETTLE: Duration = Duration::from_millis(1);

/// The bytes the tool compares at a time when it reads an image back
/// (`usdr_flash.c`:377-386).
const VERIFY_BLOCK_BYTES: usize = 256;

/// What the board runs and what its flash holds, as libusdr's flash tool reads them before
/// any update (`usdr_flash.c`:142-193). From [`Flash::boot_images`](super::Flash::boot_images).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BootImages {
    /// The build stamp of the gateware running now (`USR_ACCESS2`, GPI word 0).
    pub running_build: u32,

    /// The flash part, one libusdr recognizes.
    pub flash_id: FlashId,

    /// The golden image's header, from the flash's first 256 bytes, cut-off packets
    /// accepted. The tool refuses to do anything when it is unreadable.
    pub golden: ImageHeader,

    /// The master image's header, read likewise from [`MASTER_IMAGE_OFFSET`]. An
    /// unreadable one (no master image) does not stop an update.
    pub master: Result<ImageHeader, HeaderError>,
}

impl BootImages {
    /// Reads them as the tool does, in its order: the running build, a 1 ms wait, the flash
    /// ID, then both headers (`usdr_flash.c`:142-193).
    pub(super) fn read(bus: &mut dyn Bus) -> Result<Self, UpdateError> {
        let running_build = read_running_build(bus)?;
        bus.sleep(SETTLE);
        let flash_id = flash::read_id(bus)?;
        if !flash_id.is_recognized() {
            return Err(UpdateError::UnrecognizedFlash(flash_id));
        }
        let mut golden = [0; HEADER_BYTES];
        flash::read(bus, Slot::Golden.offset(), &mut golden)?;
        let mut master = [0; HEADER_BYTES];
        flash::read(bus, Slot::Master.offset(), &mut master)?;
        let options = super::ParseOptions::new().allow_crop();
        let golden = ImageHeader::parse(&golden, options).map_err(UpdateError::GoldenHeader)?;
        let master = ImageHeader::parse(&master, options);
        Ok(Self {
            running_build,
            flash_id,
            golden,
            master,
        })
    }

    /// Whether libusdr would flash `image` into `slot` beside this golden image: the tool's
    /// information check (`-i`, `usdr_flash.c`:249-253).
    ///
    /// # Errors
    ///
    /// See [`ImageHeader::check_for`].
    pub const fn check(&self, image: &Image, slot: Slot) -> Result<(), ImageMismatch> {
        image.header().check_for(slot, &self.golden)
    }

    /// Refuses an image whose build is already where it would go, unless
    /// [`UpdateOptions::reflash`] (`usdr_flash.c`:272-283). An unreadable master header
    /// is compared as far as it was read, as libusdr does.
    fn refuse_reflash(&self, image: &Image, slot: Slot) -> Result<(), UpdateError> {
        let build = image.header().build;
        let master = self.master.unwrap_or_else(|error| error.partial());
        if build == self.running_build && master.build == build {
            return Err(UpdateError::AlreadyRunning { build });
        }
        let flashed = match slot {
            Slot::Golden => self.golden.build,
            Slot::Master => master.build,
        };
        if flashed == build {
            return Err(UpdateError::AlreadyFlashed { slot, build });
        }
        Ok(())
    }
}

/// How an update treats an image already on the flash. The default refuses one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateOptions {
    /// Write even an image whose build is already there (`-F`).
    reflash: bool,
}

impl UpdateOptions {
    /// Refuse an image whose build already runs from the master image, or already fills
    /// the slot.
    #[must_use]
    pub const fn new() -> Self {
        Self { reflash: false }
    }

    /// Write the image even if its build is already there (the tool's `-F`).
    #[must_use]
    pub const fn reflash(self) -> Self {
        Self { reflash: true }
    }
}

/// A refused or failed gateware update.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateError {
    /// Reading the running gateware's build failed, before anything else was done.
    #[error("reading the running gateware's build failed")]
    #[non_exhaustive]
    RunningBuild {
        /// The bus failure.
        #[source]
        source: BusError,
    },

    /// A flash access failed, or a write was refused before it started; see
    /// [`FlashError`] for what the flash holds then.
    #[error(transparent)]
    Flash(#[from] FlashError),

    /// The flash is not a part libusdr recognizes, so nothing more was done (the tool's
    /// exit 2).
    #[error("flash part not recognized: {0}")]
    UnrecognizedFlash(FlashId),

    /// The golden image's header is unreadable, so nothing more was done ("the FPGA G
    /// image is corrupted").
    #[error("the golden image's header is unreadable")]
    GoldenHeader(#[source] HeaderError),

    /// The image does not belong in the slot; nothing was written.
    #[error(transparent)]
    Mismatch(#[from] ImageMismatch),

    /// The image's build runs now and is the master image; nothing was written ("you're
    /// using latest firmware already").
    #[error("build {build:#010x} already runs from the master image")]
    #[non_exhaustive]
    AlreadyRunning {
        /// The build.
        build: u32,
    },

    /// The slot already holds the image's build; nothing was written.
    #[error("the {slot} image is already build {build:#010x}")]
    #[non_exhaustive]
    AlreadyFlashed {
        /// The slot.
        slot: Slot,
        /// The build.
        build: u32,
    },

    /// The golden image warm-boots nothing, so the board has no master image to erase;
    /// nothing was erased ("No Master support detected").
    #[error("the golden image sets no warm-boot address")]
    NoMasterSupport,

    /// The golden image warm-boots somewhere other than [`MASTER_IMAGE_OFFSET`]; nothing
    /// was erased ("Master trampoline mismatches").
    #[error("the golden image warm-boots {warm_boot_start:#x}, not the master image")]
    #[non_exhaustive]
    TrampolineMismatch {
        /// The golden image's [`ImageHeader::warm_boot_start`].
        warm_boot_start: u32,
    },

    /// The image was written, but reads back differently. libusdr reports this and
    /// succeeds. The slot's header now reads as the image's build, so writing the image
    /// again needs [`UpdateOptions::reflash`].
    #[error("{blocks} 256-byte blocks read back wrong, the first at {first:#x}")]
    #[non_exhaustive]
    VerifyFailed {
        /// Blocks of 256 bytes that differ.
        blocks: usize,
        /// The first differing block's flash address.
        first: u32,
    },
}

/// Writes `image` into `slot` as the tool does (`-w`, with `-G` for the golden slot),
/// then reads it back (`usdr_flash.c`:204-321, 366-391).
pub(super) fn update(
    bus: &mut dyn Bus,
    image: &Image,
    slot: Slot,
    options: UpdateOptions,
) -> Result<(), UpdateError> {
    let flash = BootImages::read(bus)?;
    flash.check(image, slot)?;
    if !options.reflash {
        flash.refuse_reflash(image, slot)?;
    }
    let offset = slot.offset();
    let data = image.as_bytes();
    match slot {
        Slot::Golden => {
            // The boot sector, with its warm-boot trampoline, then the rest.
            let (boot, body) = data
                .split_at_checked(GOLDEN_BOOT_BYTES as usize)
                .expect("invariant: an image is whole 64 KiB sectors");
            flash::write(bus, offset, boot, WriteOptions::new())?;
            flash::write(bus, offset + GOLDEN_BOOT_BYTES, body, WriteOptions::new())?;
        }
        Slot::Master => {
            // Everything but the header, so an interrupted write leaves no master image
            // the golden image could boot; then the header.
            flash::write(bus, offset, data, WriteOptions::new().skip_header())?;
            let header = &data[..HEADER_BYTES];
            flash::write(bus, offset, header, WriteOptions::new().without_erase())?;
        }
    }
    verify(bus, offset, data)
}

/// Blanks the master image's first 64 KiB sector, header included, as the tool does
/// (`-E`, `usdr_flash.c`:323-364), once the golden image is known to warm-boot it.
pub(super) fn erase_master(bus: &mut dyn Bus) -> Result<(), UpdateError> {
    let flash = BootImages::read(bus)?;
    match flash.golden.warm_boot_start {
        0 => Err(UpdateError::NoMasterSupport),
        MASTER_IMAGE_OFFSET => Ok(flash::erase(bus, MASTER_IMAGE_OFFSET, SECTOR_BYTES)?),
        warm_boot_start => Err(UpdateError::TrampolineMismatch { warm_boot_start }),
    }
}

/// Reads `data.len()` bytes back from `offset` and compares them 256 at a time.
fn verify(bus: &mut dyn Bus, offset: u32, data: &[u8]) -> Result<(), UpdateError> {
    let mut back = vec![0; data.len()];
    flash::read(bus, offset, &mut back)?;
    let mut differing = data
        .chunks(VERIFY_BLOCK_BYTES)
        .zip(back.chunks(VERIFY_BLOCK_BYTES))
        .enumerate()
        .filter(|(_, (written, read))| written != read)
        .map(|(block, _)| block);
    let Some(first) = differing.next() else {
        return Ok(());
    };
    let first_byte = u32::try_from(first * VERIFY_BLOCK_BYTES).expect("invariant: within 16 MiB");
    Err(UpdateError::VerifyFailed {
        blocks: 1 + differing.count(),
        first: offset + first_byte,
    })
}

/// Reads the running gateware's build stamp (`dev_gpi_get32(dev, 0)`, `usdr_flash.c`:29-32).
fn read_running_build(bus: &mut dyn Bus) -> Result<u32, UpdateError> {
    let mut word = [0];
    bus.read_regs(Gpi::UsrAccess2.register(), &mut word)
        .map_err(|source| UpdateError::RunningBuild { source })?;
    Ok(word[0])
}
