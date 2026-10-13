//! Maintenance of the uSDR's boot flash: the QSPI NOR flash the FPGA loads its gateware
//! from, reached through the gateware's flash controller.
//!
//! [`Flash`] takes a bus of its own, so no [`Device`](crate::Device) is open while it
//! works, and it powers nothing up. That the controller needs only the FPGA to be
//! answering is an assumption: libusdr reaches the flash only on a fully opened device
//! (`json_controller/controller_ugen.c`), and the register sequence does not depend on
//! it. It reads the ID
//! and contents. Erasing and writing go through a [`Programmer`], which [`Flash::programmer`]
//! hands out only when asked: they change what the board boots, and a wrong range can leave
//! it unable to load its gateware.
//!
//! libusdr's layout, by its board description and update code: a golden image at
//! [`GOLDEN_IMAGE_OFFSET`], which that code rewrites only on explicit request, and the
//! master image at [`MASTER_IMAGE_OFFSET`]. Each is a Xilinx bitstream whose header
//! ([`ImageHeader`]) names the FPGA it is for, its build and where it warm-boots: the
//! golden image the FPGA loads at power-up points at the master image and reloads from
//! it (IPROG).
//!
//! The update is libusdr's flash tool (`tools/usdr_flash.c`), step for step:
//! [`Flash::boot_images`] reads what runs and what the flash holds, [`Image`] checks a
//! new image file (its CRCs by default), [`BootImages::check`] says whether it belongs in
//! a [`Slot`], and [`Programmer::update_master`], [`Programmer::update_golden`] and
//! [`Programmer::erase_master`] change the flash, refusing whatever the tool refuses.
//!
//! Recovery, by the order libusdr writes in:
//! - A master update erases the whole range first and writes the image's first 256 bytes,
//!   its header and sync word, last. Interrupted before that, the master slot holds no
//!   loadable image and the golden image stays as it was. Whether the FPGA then falls
//!   back to the golden image is the FPGA's `MultiBoot` behaviour, which libusdr does not
//!   state; run the update again, or [`Programmer::erase_master`] to blank the master
//!   header so only the golden image remains.
//! - A golden update writes the boot sector (4 KiB, with the warm-boot trampoline) first,
//!   then the rest. Interrupted, the board has no complete golden image and may load no
//!   gateware until the flash is rewritten by other means (JTAG): nothing here recovers
//!   it. Only [`Programmer::update_golden`] writes there, and only with an image that
//!   warm-boots the master slot and issues IPROG, as libusdr requires.
//! - A master update never writes below [`MASTER_IMAGE_OFFSET`], and is refused unless the
//!   golden image on the flash warm-boots that offset.
//!
//! Operations follow libusdr's `ipblks/espi_flash.c` register for register, including its
//! waits and poll limits, so they are checked against it on the simulated board. Where
//! libusdr would act on a range it cannot handle correctly, these reject it before touching
//! the bus; [`FlashError`] lists the cases.
//!
//! ```no_run
//! # fn demo(bus: impl rsl_usdr::lowlevel::Bus) -> Result<(), rsl_usdr::flash::FlashError> {
//! use rsl_usdr::flash::{Flash, MASTER_IMAGE_OFFSET};
//!
//! let mut flash = Flash::new(bus);
//! println!("{}", flash.id()?);
//! let mut header = [0; 256];
//! flash.read(MASTER_IMAGE_OFFSET, &mut header)?;
//! # Ok(())
//! # }
//! ```

mod flash;
mod image;
mod update;

pub use image::{
    HeaderError, HeaderFault, Image, ImageError, ImageHeader, ImageMismatch, ParseOptions, Slot,
};
pub use update::{BootImages, UpdateError, UpdateOptions};

pub use flash::{
    ADDRESS_SPACE, Flash, FlashError, FlashId, GOLDEN_IMAGE_OFFSET, HEADER_BYTES,
    MASTER_IMAGE_OFFSET, PAGE_BYTES, Programmer, SECTOR_BYTES, SUBSECTOR_BYTES, WriteOptions,
};
