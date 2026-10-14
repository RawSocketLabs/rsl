//! Firmware updates for the XMASS board's `PCIe` switch, an `ASMedia` ASM2806: libusdr's
//! `tools/xmass_fwupd.c` (`xmass_fwupd.c`:N below), step for step.
//!
//! This is neither the uSDR's boot flash ([`crate::flash`]) nor libusdr's library: the
//! switch keeps its firmware in a flash of its own, which the `usdr_pcie_uram` Linux
//! driver erases, programs and reads on request through its XMASS device, `/dev/xmassN`.
//! The tool makes four such requests; [`SwitchFlash`] is them, implemented over the device
//! by `transport::pcie` (feature `pcie`, Linux) for `usdr_pcie::XmassDevice`.
//!
//! [`update`] is the tool's update: read the IDs, erase, write the image, read it all back
//! and compare it, except bytes `0xe00..0xf00` ([`SIGNATURE`]), which the tool does not
//! compare. [`read_ids`] is the tool's `info`. The tool checks nothing of an image but its
//! size, and neither does [`Firmware`]: the bytes are the switch's, in a format neither
//! libusdr nor the driver's interface describes.
//!
//! Sources: the tool (MIT) and the driver's interface header, `pcie_uram_driver_if.h`
//! (taken under MIT). The driver itself is GPL-2.0 and nothing is taken from it; what is
//! said below of its behaviour beyond those sources is observed interface behaviour, listed
//! in PORTING.md's provenance note.
//!
//! Recovery:
//! - A failure reading the IDs leaves the flash as it was.
//! - The driver is observed to clear the flash's write protection when erasing (and not to
//!   set it again), to erase 4 KiB sectors from address 0 up, and to program 256-byte pages
//!   from address 0 up. A failed erase or write leaves the flash part erased or part
//!   programmed, and a failed read-back or [`UpdateError::VerifyFailed`] leaves it holding
//!   the write.
//! - The IDs the tool reports as the firmware in use come from the switch, not its flash.
//!   When the switch loads its flash again, and what it does with an erased or partial one,
//!   the sources do not say, so recover before it restarts (a reset or power cycle): run
//!   [`update`] again. It erases first, so it can be repeated after a failure at any step.
//! - The driver is observed to offer `/dev/xmassN` only for a switch it finds, with a
//!   readable firmware ID, when it loads. A switch that does not come up after a restart
//!   gets no node, and only programming its flash by other means recovers it.
//! - The driver is observed to stage writes and reads in one buffer shared by all XMASS
//!   devices, with no lock. `XmassDevice` serialises its own process's requests; update one
//!   XMASS board at a time across processes.

use std::fmt;
use std::io;
use std::ops::Range;

/// The bytes of a firmware image, and of the flash the driver erases, writes and reads: the
/// tool's `BUFFER_SIZE` (`xmass_fwupd.c`:19).
pub const FIRMWARE_BYTES: usize = 65_536;

/// The bytes the driver returns for the IDs: the tool's `info[10]` (`xmass_fwupd.c`:27).
pub const IDS_BYTES: usize = 10;

/// The bytes a read-back is not compared over (`xmass_fwupd.c`:101): the driver may change
/// them when it programs the flash.
pub const SIGNATURE: Range<usize> = 0xe00..0xf00;

/// What `PCIE_FLASH_READ` is observed to return on success; the other requests return 0.
pub const READ_SUCCESS: i32 = 1;

/// The four requests the tool makes of the XMASS device, in the driver's terms. Each is one
/// ioctl: it fails with the driver's errno, or returns the driver's return value as it is.
pub trait SwitchFlash {
    /// Reads the IDs into `out` (`PCIE_GETIDS`); [`read_ids`] decodes them.
    ///
    /// # Errors
    ///
    /// The driver's.
    fn ids(&mut self, out: &mut [u8; IDS_BYTES]) -> io::Result<i32>;

    /// Erases the flash's first [`FIRMWARE_BYTES`] (`PCIE_FLASH_ERASE`).
    ///
    /// # Errors
    ///
    /// The driver's; the flash may then be part erased.
    fn erase(&mut self) -> io::Result<i32>;

    /// Programs `image` into the flash's first [`FIRMWARE_BYTES`], without erasing
    /// (`PCIE_FLASH_WRITE`).
    ///
    /// # Errors
    ///
    /// The driver's; the flash may then be part programmed.
    fn write(&mut self, image: &[u8; FIRMWARE_BYTES]) -> io::Result<i32>;

    /// Reads the flash's first [`FIRMWARE_BYTES`] into `out` (`PCIE_FLASH_READ`).
    ///
    /// # Errors
    ///
    /// The driver's; `out` then holds nothing meaningful.
    fn read(&mut self, out: &mut [u8; FIRMWARE_BYTES]) -> io::Result<i32>;
}

/// A firmware image for the switch: exactly [`FIRMWARE_BYTES`], the only check the tool
/// makes.
#[derive(Clone, PartialEq, Eq)]
pub struct Firmware {
    /// The image.
    bytes: Box<[u8; FIRMWARE_BYTES]>,
}

impl Firmware {
    /// Takes an image file's contents.
    ///
    /// The tool reads the first [`FIRMWARE_BYTES`] of its file and refuses a shorter one
    /// (`xmass_fwupd.c`:46-54), so it flashes the start of a longer file without a word;
    /// this refuses any other length.
    ///
    /// # Errors
    ///
    /// [`FirmwareSizeError`] unless `bytes` is exactly [`FIRMWARE_BYTES`] long.
    pub fn new(bytes: &[u8]) -> Result<Self, FirmwareSizeError> {
        let image: &[u8; FIRMWARE_BYTES] = bytes
            .try_into()
            .map_err(|_| FirmwareSizeError { len: bytes.len() })?;
        Ok(Self {
            bytes: Box::new(*image),
        })
    }

    /// The image.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; FIRMWARE_BYTES] {
        &self.bytes
    }
}

/// The length only: the image is 64 KiB of opaque bytes.
impl fmt::Debug for Firmware {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Firmware")
            .field("bytes", &FIRMWARE_BYTES)
            .finish_non_exhaustive()
    }
}

/// An image file is not [`FIRMWARE_BYTES`] long.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("an XMASS switch firmware image is {FIRMWARE_BYTES} bytes, not {len}")]
pub struct FirmwareSizeError {
    /// The file's length.
    pub len: usize,
}

/// The switch's IDs, as the tool prints them before any change (`xmass_fwupd.c`:69-71).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct SwitchIds {
    /// The firmware the switch runs, six bytes; the tool prints them as one hex string,
    /// `ASM2806 Firmware in use`.
    pub firmware: [u8; 6],

    /// The flash's ID, four bytes; the tool prints the first three, `FlashID` (observed: the
    /// JEDEC read-identification reply, manufacturer, memory type and capacity first).
    pub flash_id: [u8; 4],
}

/// A request returned a value that is neither an error nor its success value. The tool
/// takes any such value as success; the driver is observed to pass positive failure codes
/// through, after which a read's buffer holds no read-back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the XMASS driver returned {returned:#x}, not its success value {success}")]
pub struct UnexpectedReturn {
    /// What the driver returned.
    pub returned: i32,
    /// What it returns on success.
    pub success: i32,
}

/// A failed switch firmware update, by the step that failed. See the module's recovery
/// notes for what the flash holds after each. An [`UnexpectedReturn`] arrives as the
/// `io::Error`'s inner error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateError {
    /// Reading the IDs failed, before the flash changed ("Unable to get ids").
    #[error("reading the XMASS switch's IDs failed")]
    Ids(#[source] io::Error),

    /// Erasing the flash failed ("Erasing flash error").
    #[error("erasing the XMASS switch's flash failed")]
    Erase(#[source] io::Error),

    /// Writing the image failed ("Writing flash error").
    #[error("writing the XMASS switch's firmware failed")]
    Write(#[source] io::Error),

    /// Reading the image back failed ("Readback flash error").
    #[error("reading the XMASS switch's firmware back failed")]
    ReadBack(#[source] io::Error),

    /// The image reads back differently outside [`SIGNATURE`] ("Verification failed!").
    #[error(
        "the XMASS switch's firmware reads back differently: {bytes} bytes, the first at {first:#x}"
    )]
    #[non_exhaustive]
    VerifyFailed {
        /// The first differing byte's address.
        first: usize,
        /// How many bytes differ.
        bytes: usize,
    },
}

/// Reads the IDs, as the tool's `info` does (`xmass_fwupd.c`:63-76).
///
/// # Errors
///
/// The driver's, or an [`UnexpectedReturn`].
pub fn read_ids(switch: &mut dyn SwitchFlash) -> io::Result<SwitchIds> {
    let mut ids = [0; IDS_BYTES];
    succeeded(switch.ids(&mut ids), 0)?;
    let [f0, f1, f2, f3, f4, f5, j0, j1, j2, j3] = ids;
    Ok(SwitchIds {
        firmware: [f0, f1, f2, f3, f4, f5],
        flash_id: [j0, j1, j2, j3],
    })
}

/// Replaces the switch's firmware with `firmware`, as the tool does
/// (`xmass_fwupd.c`:63-104): reads the IDs, erases, writes, reads back and compares.
/// Returns the IDs read first, the firmware that was running.
///
/// Unlike the tool, a request counts as done only if it returns its success value (0, or
/// [`READ_SUCCESS`] for the read); see [`UnexpectedReturn`].
///
/// A free function rather than a method: it is the tool's procedure over any
/// [`SwitchFlash`], and the image is only its input.
///
/// # Errors
///
/// [`UpdateError`], at the step that failed; the steps after it are not made.
pub fn update(switch: &mut dyn SwitchFlash, firmware: &Firmware) -> Result<SwitchIds, UpdateError> {
    let ids = read_ids(switch).map_err(UpdateError::Ids)?;
    succeeded(switch.erase(), 0).map_err(UpdateError::Erase)?;
    succeeded(switch.write(firmware.as_bytes()), 0).map_err(UpdateError::Write)?;
    let mut back = filled(0);
    succeeded(switch.read(&mut back), READ_SUCCESS).map_err(UpdateError::ReadBack)?;
    verify(firmware.as_bytes(), &back)?;
    Ok(ids)
}

/// A request's outcome: its error, or an [`UnexpectedReturn`] unless it returned `success`.
fn succeeded(returned: io::Result<i32>, success: i32) -> io::Result<()> {
    match returned? {
        value if value == success => Ok(()),
        returned => Err(io::Error::other(UnexpectedReturn { returned, success })),
    }
}

/// A flash-sized buffer of `byte`, on the heap.
fn filled(byte: u8) -> Box<[u8; FIRMWARE_BYTES]> {
    vec![byte; FIRMWARE_BYTES]
        .into_boxed_slice()
        .try_into()
        .expect("invariant: FIRMWARE_BYTES long")
}

/// Compares what was written with what reads back, outside [`SIGNATURE`]
/// (`xmass_fwupd.c`:101).
fn verify(written: &[u8; FIRMWARE_BYTES], back: &[u8; FIRMWARE_BYTES]) -> Result<(), UpdateError> {
    let mut differing = written
        .iter()
        .zip(back.iter())
        .enumerate()
        .filter(|(at, (w, b))| !SIGNATURE.contains(at) && w != b)
        .map(|(at, _)| at);
    let Some(first) = differing.next() else {
        return Ok(());
    };
    Err(UpdateError::VerifyFailed {
        first,
        bytes: 1 + differing.count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A switch whose requests all succeed, and whose read hands back what was last
    /// written (as a stale staging buffer would), returning `read_returns`.
    struct EchoingSwitch {
        written: Box<[u8; FIRMWARE_BYTES]>,
        read_returns: i32,
    }

    impl SwitchFlash for EchoingSwitch {
        fn ids(&mut self, out: &mut [u8; IDS_BYTES]) -> io::Result<i32> {
            *out = [1, 2, 3, 4, 5, 6, 0xc2, 0x20, 0x11, 0xc2];
            Ok(0)
        }

        fn erase(&mut self) -> io::Result<i32> {
            Ok(0)
        }

        fn write(&mut self, image: &[u8; FIRMWARE_BYTES]) -> io::Result<i32> {
            *self.written = *image;
            Ok(0)
        }

        fn read(&mut self, out: &mut [u8; FIRMWARE_BYTES]) -> io::Result<i32> {
            *out = *self.written;
            Ok(self.read_returns)
        }
    }

    fn switch(read_returns: i32) -> EchoingSwitch {
        EchoingSwitch {
            written: filled(0),
            read_returns,
        }
    }

    #[test]
    fn images_of_any_other_length_are_refused() {
        for len in [0, FIRMWARE_BYTES - 1, FIRMWARE_BYTES + 1] {
            assert_eq!(
                Firmware::new(&vec![0; len]),
                Err(FirmwareSizeError { len }),
                "only a {FIRMWARE_BYTES}-byte image is accepted"
            );
        }
    }

    #[test]
    fn the_ids_split_into_the_switch_firmware_and_the_flash_id() {
        let ids = read_ids(&mut switch(READ_SUCCESS)).expect("the fake answers");
        assert_eq!(ids.firmware, [1, 2, 3, 4, 5, 6]);
        assert_eq!(ids.flash_id, [0xc2, 0x20, 0x11, 0xc2]);
    }

    #[test]
    fn a_read_returning_a_positive_failure_code_is_not_verified_against_its_stale_buffer() {
        let firmware = Firmware::new(&vec![0x5a; FIRMWARE_BYTES]).expect("64 KiB");
        update(&mut switch(READ_SUCCESS), &firmware).expect("the echo verifies");
        let error = update(&mut switch(0x87), &firmware).expect_err("0x87 is not success");
        let UpdateError::ReadBack(source) = error else {
            panic!("the read-back failed, not {error:?}");
        };
        let inner = source
            .get_ref()
            .and_then(|e| e.downcast_ref::<UnexpectedReturn>());
        assert_eq!(
            inner,
            Some(&UnexpectedReturn {
                returned: 0x87,
                success: READ_SUCCESS
            }),
            "the driver's value is reported"
        );
    }
}
