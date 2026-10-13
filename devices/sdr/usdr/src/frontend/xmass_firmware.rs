//! Firmware updates for the XMASS board's `PCIe` switch, an `ASMedia` ASM2806: libusdr's
//! `tools/xmass_fwupd.c` (`xmass_fwupd.c`:N below), step for step.
//!
//! This is neither the uSDR's boot flash ([`crate::flash`]) nor libusdr's library: the
//! switch keeps its firmware in a flash of its own, which the `usdr_pcie_uram` Linux
//! driver erases, programs and reads on request through its XMASS device, `/dev/xmassN`.
//! The tool makes four such requests; [`BridgeFlash`] is them, implemented over the device
//! by `transport::pcie` (feature `pcie`, Linux) for `rsl_usdr_pcie::XmassDevice`.
//!
//! [`update`] is the tool's update: read the IDs, erase, write the image, read it all back
//! and compare it, except bytes `0xe00..0xf00` ([`SIGNATURE`]), where the driver writes
//! its own signature before programming. [`read_ids`] is the tool's `info`. The tool checks
//! nothing of an image but its size, and neither does [`Firmware`]: the bytes are the
//! switch's, in a format neither libusdr nor the driver describes.
//!
//! Recovery, from the driver's order (`usdr_pcie_uram.c` at libusdr's pinned commit,
//! GPL-2.0, not vendored):
//! - A failure reading the IDs changes nothing.
//! - Erasing first clears the flash's write protection (and never sets it again), then
//!   erases 4 KiB sectors from address 0 up; writing programs 256-byte pages from address 0
//!   up. A failed erase or write leaves the flash part erased or part programmed, and a
//!   failed read-back or [`UpdateError::VerifyFailed`] leaves it holding the write.
//! - The IDs the tool reports as the firmware in use come from the switch, not its flash.
//!   When the switch loads its flash again, and what it does with an erased or partial one,
//!   neither libusdr nor the driver says, so recover before it restarts (a reset or power
//!   cycle): run [`update`] again. It erases first, so it can be repeated after a failure
//!   at any step.
//! - The driver creates `/dev/xmassN` when it loads, for each ASM2806 upstream port whose
//!   firmware ID it can read. A switch that does not come up after a restart gets no node,
//!   and only programming its flash by other means recovers it.
//! - The driver stages writes and reads in one buffer shared by all XMASS devices, with no
//!   lock. `XmassDevice` serialises its own process's requests; update one XMASS board at
//!   a time across processes.

use std::fmt;
use std::io;
use std::ops::Range;

/// The bytes of a firmware image, and of the flash the driver erases, writes and reads: the
/// tool's `BUFFER_SIZE` (`xmass_fwupd.c`:19).
pub const FIRMWARE_BYTES: usize = 65_536;

/// The bytes the driver returns for the IDs: the tool's `info[10]` (`xmass_fwupd.c`:27).
pub const IDS_BYTES: usize = 10;

/// The bytes a read-back is not compared over, because the driver writes its own signature
/// there (`xmass_fwupd.c`:101).
pub const SIGNATURE: Range<usize> = 0xe00..0xf00;

/// The four requests the tool makes of the XMASS device, in the driver's terms. Each is one
/// ioctl, and fails with the driver's errno.
pub trait BridgeFlash {
    /// The switch's firmware ID and the flash's JEDEC ID reply (`PCIE_GETIDS`); see
    /// [`BridgeIds`].
    ///
    /// # Errors
    ///
    /// The driver's.
    fn ids(&mut self) -> io::Result<[u8; IDS_BYTES]>;

    /// Erases the flash's first [`FIRMWARE_BYTES`] (`PCIE_FLASH_ERASE`).
    ///
    /// # Errors
    ///
    /// The driver's; the flash may then be part erased.
    fn erase(&mut self) -> io::Result<()>;

    /// Programs `image` into the flash's first [`FIRMWARE_BYTES`], without erasing
    /// (`PCIE_FLASH_WRITE`). The driver writes its signature into [`SIGNATURE`] first.
    ///
    /// # Errors
    ///
    /// The driver's; the flash may then be part programmed.
    fn write(&mut self, image: &[u8; FIRMWARE_BYTES]) -> io::Result<()>;

    /// Reads the flash's first [`FIRMWARE_BYTES`] into `out` (`PCIE_FLASH_READ`).
    ///
    /// # Errors
    ///
    /// The driver's; `out` then holds nothing meaningful.
    fn read(&mut self, out: &mut [u8; FIRMWARE_BYTES]) -> io::Result<()>;
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
        let bytes = Box::<[u8]>::from(bytes)
            .try_into()
            .map_err(|bytes: Box<[u8]>| FirmwareSizeError { len: bytes.len() })?;
        Ok(Self { bytes })
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

/// What the driver reports before any change, and the tool prints (`xmass_fwupd.c`:69-71).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct BridgeIds {
    /// The firmware the switch runs, as six bytes from its configuration space; the tool
    /// prints them as one hex string, `ASM2806 Firmware in use`.
    pub firmware: [u8; 6],

    /// The flash's reply to the JEDEC read-identification command, as four bytes; the tool
    /// prints the first three, `FlashID` (manufacturer, memory type, capacity).
    pub flash_id: [u8; 4],
}

impl From<[u8; IDS_BYTES]> for BridgeIds {
    fn from(ids: [u8; IDS_BYTES]) -> Self {
        let [f0, f1, f2, f3, f4, f5, j0, j1, j2, j3] = ids;
        Self {
            firmware: [f0, f1, f2, f3, f4, f5],
            flash_id: [j0, j1, j2, j3],
        }
    }
}

/// A failed switch firmware update, by the step that failed. See the module's recovery
/// notes for what the flash holds after each.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpdateError {
    /// Reading the IDs failed, before anything changed ("Unable to get ids").
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
/// The driver's.
pub fn read_ids(bridge: &mut dyn BridgeFlash) -> io::Result<BridgeIds> {
    bridge.ids().map(BridgeIds::from)
}

/// Replaces the switch's firmware with `firmware`, as the tool does
/// (`xmass_fwupd.c`:63-104): reads the IDs, erases, writes, reads back and compares.
/// Returns the IDs read first, the firmware that was running.
///
/// A free function rather than a method: it is the tool's procedure over any
/// [`BridgeFlash`], and the image is only its input.
///
/// # Errors
///
/// [`UpdateError`], at the step that failed; the steps after it are not made.
pub fn update(bridge: &mut dyn BridgeFlash, firmware: &Firmware) -> Result<BridgeIds, UpdateError> {
    let ids = read_ids(bridge).map_err(UpdateError::Ids)?;
    bridge.erase().map_err(UpdateError::Erase)?;
    bridge
        .write(firmware.as_bytes())
        .map_err(UpdateError::Write)?;
    let mut back = filled(0);
    bridge.read(&mut back).map_err(UpdateError::ReadBack)?;
    verify(firmware.as_bytes(), &back)?;
    Ok(ids)
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

    /// A flash that programs faithfully, with the driver's signature stood in for by
    /// inverting [`SIGNATURE`]'s bytes, and one byte that reads back wrong if asked.
    struct FakeFlash {
        flash: Box<[u8; FIRMWARE_BYTES]>,
        misread: Option<usize>,
        steps: Vec<&'static str>,
    }

    impl FakeFlash {
        fn new() -> Self {
            Self {
                flash: filled(0x5a),
                misread: None,
                steps: Vec::new(),
            }
        }
    }

    impl BridgeFlash for FakeFlash {
        fn ids(&mut self) -> io::Result<[u8; IDS_BYTES]> {
            self.steps.push("ids");
            Ok([1, 2, 3, 4, 5, 6, 0xc2, 0x20, 0x11, 0xc2])
        }

        fn erase(&mut self) -> io::Result<()> {
            self.steps.push("erase");
            self.flash.fill(0xff);
            Ok(())
        }

        fn write(&mut self, image: &[u8; FIRMWARE_BYTES]) -> io::Result<()> {
            self.steps.push("write");
            for (at, (cell, byte)) in self.flash.iter_mut().zip(image).enumerate() {
                let byte = if SIGNATURE.contains(&at) {
                    !byte
                } else {
                    *byte
                };
                *cell &= byte;
            }
            Ok(())
        }

        fn read(&mut self, out: &mut [u8; FIRMWARE_BYTES]) -> io::Result<()> {
            self.steps.push("read");
            out.copy_from_slice(&*self.flash);
            if let Some(at) = self.misread {
                out[at] ^= 1;
            }
            Ok(())
        }
    }

    fn image() -> Firmware {
        let bytes: Vec<u8> = (0..FIRMWARE_BYTES)
            .map(|i| u8::try_from(i * 7 % 251).expect("below 251"))
            .collect();
        Firmware::new(&bytes).expect("64 KiB")
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
    fn the_ids_split_into_the_switch_firmware_and_the_flash_reply() {
        let ids = read_ids(&mut FakeFlash::new()).expect("the fake answers");
        assert_eq!(ids.firmware, [1, 2, 3, 4, 5, 6]);
        assert_eq!(ids.flash_id, [0xc2, 0x20, 0x11, 0xc2]);
    }

    #[test]
    fn the_drivers_signature_does_not_fail_the_read_back() {
        let mut flash = FakeFlash::new();
        let firmware = image();
        update(&mut flash, &firmware).expect("only the signature differs");
        assert_eq!(flash.steps, ["ids", "erase", "write", "read"]);
        assert_ne!(
            flash.flash[SIGNATURE],
            firmware.as_bytes()[SIGNATURE],
            "the fake changed the signature bytes, so the comparison skipped them"
        );
    }

    #[test]
    fn a_byte_just_outside_the_signature_that_reads_back_wrong_fails_the_update() {
        for at in [0, SIGNATURE.start - 1, SIGNATURE.end, FIRMWARE_BYTES - 1] {
            let mut flash = FakeFlash::new();
            flash.misread = Some(at);
            let error = update(&mut flash, &image()).expect_err("a byte differs");
            assert!(
                matches!(error, UpdateError::VerifyFailed { first, bytes: 1 } if first == at),
                "{error:?} names byte {at:#x}"
            );
        }
    }
}
