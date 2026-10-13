//! `pe_sync_check_status`: one readback from each chip right after power-on, which libusdr
//! logs. A failed access fails the open; the values themselves are not judged.

use rsl_lowlevel::{RegisterAccess, SequenceAction};

use super::board::{
    LMK1D1208I_LCK, LMK1D1208I_LRF, LMX1204, LMX1214, LMX2820_0, LMX2820_1, LP87524, LP87524_OTP,
};
use super::fpga::{self, STATUS_READ};
use super::lmk1d1208i::{self, Reg as Lmk1d1208iReg};
use super::lmx1204::{self, reg::MuxOut};
use super::lmx1214;
use super::lmx2820::{self, reg::Reg as Lmx2820Reg};
use super::script::Script;
use super::ti_spi::{self, Register};

/// The LP87524's name in errors.
const LP87524_NAME: &str = "LP87524";

/// The status reads, in libusdr's order: the FPGA's status word, the LP87524's OTP byte,
/// both LMX2820s' R28 (a reserved register holding 0x639), the LMX1204's R33 and the
/// LMX1214's R79 (each after routing SPI readback to MUXOUT), both LMK1D1208Is' identity.
pub(crate) fn status_check() -> Script {
    let read_lmx2820 = |target| ti_spi::read(target, Lmx2820Reg::Probe as u8);
    let lmx1214_readback = lmx1214::reg::MUXOUT_READBACK;
    Script::new()
        .then(fpga::NAME, SequenceAction::Fpga(STATUS_READ))
        .then(
            LP87524_NAME,
            SequenceAction::Access(RegisterAccess::read(LP87524, LP87524_OTP)),
        )
        .then(lmx2820::sequence::NAME, read_lmx2820(LMX2820_0))
        .then(lmx2820::sequence::NAME, read_lmx2820(LMX2820_1))
        .then(
            lmx1204::sequence::NAME,
            ti_spi::write(LMX1204, MuxOut::readback(false).entry()),
        )
        .then(
            lmx1204::sequence::NAME,
            ti_spi::read(LMX1204, lmx1204::reg::Reg::MultiplierTrim33 as u8),
        )
        .then(
            lmx1214::sequence::NAME,
            ti_spi::write(LMX1214, (lmx1214_readback.0.into(), lmx1214_readback.1)),
        )
        .then(
            lmx1214::sequence::NAME,
            ti_spi::read(LMX1214, lmx1214::reg::Reg::Undisclosed79 as u8),
        )
        .then(
            lmk1d1208i::NAME,
            SequenceAction::Access(RegisterAccess::read(
                LMK1D1208I_LCK,
                Lmk1d1208iReg::Identity,
            )),
        )
        .then(
            lmk1d1208i::NAME,
            SequenceAction::Access(RegisterAccess::read(
                LMK1D1208I_LRF,
                Lmk1d1208iReg::Identity,
            )),
        )
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::Sequence;

    use super::*;
    use crate::portable::testing::{Exchange, drive};

    #[test]
    fn the_status_check_reads_every_chip_in_libusdrs_order() {
        // Golden session.trace lines 1217-1226.
        let (done, outcome) = drive(&mut status_check(), |_| Ok(0));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done,
            [
                Exchange::Read(20),
                Exchange::I2cRead(0x60, 0x01),
                Exchange::Spi(0x009c_0000),
                Exchange::Spi(0x009c_0000),
                Exchange::Spi(0x0017_6040),
                Exchange::Spi(0x00a1_0000),
                Exchange::Spi(0x0017_6040),
                Exchange::Spi(0x00cf_0000),
                Exchange::I2cRead(0x68, 0x05),
                Exchange::I2cRead(0x69, 0x05),
            ]
        );
    }

    #[test]
    fn a_failed_read_ends_the_check() {
        let mut check = status_check();
        check.complete::<()>(Ok(0)).unwrap();
        let failure = check.complete(Err(())).unwrap_err();
        assert_eq!(
            failure.access().map(|(chip, _, reg)| (chip, reg)),
            Some((LP87524_NAME, 0x01))
        );
        assert_eq!(check.next(), None);
    }
}
