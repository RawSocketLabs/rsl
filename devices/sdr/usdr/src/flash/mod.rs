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
//! master image at [`MASTER_IMAGE_OFFSET`]. Their headers and update procedure are not
//! ported; this module moves bytes only.
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

pub use flash::{
    ADDRESS_SPACE, Flash, FlashError, FlashId, GOLDEN_IMAGE_OFFSET, HEADER_BYTES,
    MASTER_IMAGE_OFFSET, PAGE_BYTES, Programmer, SECTOR_BYTES, SUBSECTOR_BYTES, WriteOptions,
};
