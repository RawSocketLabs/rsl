//! Which family member the board is: the hardware ID libusdr reads twice at open.

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};

use super::fpga::{self, REG_HWID};
use super::outcome::outcome;

/// An LMS7 family member, as libusdr's `xsdr_init` tells them apart by HWID bits 23:16
/// (`enum xsdr_devices`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Variant {
    /// XSDR (`XSDR_DEV`, 0x30): one PMIC, a front-end connector. Supported.
    Xsdr,

    /// XTRX (`XTRX_DEV`, 0x2e): two PMICs and a reference-trim DAC. Identified only.
    Xtrx,

    /// sSDR (`SSDR_DEV`, 0x31): adds an LMS8001 converter. Identified only.
    Ssdr,

    /// sSDR Pro (`SSDRPRO_DEV`, 0x33): as sSDR, plus a secure EEPROM. Identified only.
    SsdrPro,
}

impl Variant {
    /// The member a hardware ID names, or its unknown device ID (bits 23:16).
    pub(crate) const fn from_hwid(hwid: u32) -> Result<Self, u8> {
        let device = hwid.to_le_bytes()[2];
        match device {
            0x30 => Ok(Self::Xsdr),
            0x2e => Ok(Self::Xtrx),
            0x31 => Ok(Self::Ssdr),
            0x33 => Ok(Self::SsdrPro),
            unknown => Err(unknown),
        }
    }
}

/// The two HWID reads libusdr makes before it drives anything: the device factory's
/// (`usdr_device_m2_lm7_1_create`, choosing its parameter tree) and `xsdr_init`'s, whose
/// value it then uses. Both are kept, so the trace matches; the second is authoritative.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IdentityRead {
    /// Reads completed so far.
    reads: u8,

    /// The last word read.
    hwid: u32,

    /// A read failed; nothing more is issued.
    failed: bool,
}

impl IdentityRead {
    /// How many times libusdr reads the HWID.
    const READS: u8 = 2;

    /// Starts the reads.
    pub(crate) const fn new() -> Self {
        Self {
            reads: 0,
            hwid: 0,
            failed: false,
        }
    }

    /// The word the last read returned: the hardware ID once the sequence finished.
    pub(crate) const fn hwid(self) -> u32 {
        self.hwid
    }
}

impl Sequence for IdentityRead {
    fn next(&self) -> Option<SequenceAction> {
        (self.reads < Self::READS && !self.failed)
            .then_some(SequenceAction::Fpga(FpgaAccess::Read { reg: REG_HWID }))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        self.hwid = outcome(action, result, fpga::NAME).inspect_err(|_| self.failed = true)?;
        self.reads += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_device_id_byte_selects_the_member() {
        assert_eq!(Variant::from_hwid(0x0030_0000), Ok(Variant::Xsdr));
        assert_eq!(
            Variant::from_hwid(0xff30_ffff),
            Ok(Variant::Xsdr),
            "gateware revision and PHY bits do not change the member"
        );
        assert_eq!(
            Variant::from_hwid(0x0032_0000),
            Err(0x32),
            "SSDR2_DEV is commented out"
        );
    }

    #[test]
    fn the_second_read_is_the_one_kept_and_a_failure_ends_the_reads() {
        let mut read = IdentityRead::new();
        read.complete::<()>(Ok(1)).unwrap();
        assert!(read.next().is_some(), "one read is not an identity");
        read.complete::<()>(Ok(0x0030_0000)).unwrap();
        assert_eq!(read.hwid(), 0x0030_0000);
        assert_eq!(read.next(), None);

        let mut failed = IdentityRead::new();
        assert!(failed.complete(Err(())).is_err());
        assert_eq!(failed.next(), None, "nothing follows a failed read");
    }
}
