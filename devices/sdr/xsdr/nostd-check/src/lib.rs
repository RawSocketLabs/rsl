//! Compile witness for the exact private rsl-xsdr portable sequences, not another driver.
//! A bare-metal check proves core (+ `bnb`'s alloc) compilation, not linking or hardware
//! operation.
#![no_std]
#![forbid(unsafe_code)]

#[path = "../../src/portable/mod.rs"]
mod portable;

use portable::close::Close;
use portable::hwid::{IdentityRead, Variant};
use portable::power::PowerUp;
use portable::supplies::{Supplies, SupplyPlan};
use rsl_lms7002m::{Lms7002m, ResetPlan};
use rsl_lowlevel::{I2cAddr, Sequence, SequenceAction, SequenceFailure, SpiAddr};
use rsl_lp8758::{Buck, BuckControl, BuckVoltage};

/// An LMS7002M on SPI bus 0 with libusdr's XSDR arguments (`xsdr_pwren`).
const LMS: ResetPlan = ResetPlan {
    target: SpiAddr(0),
    ldo_mask: 0,
    shared_reference: true,
};

/// A supply plan shaped like the XSDR's: three rails, every channel on in forced PWM.
const PLAN: SupplyPlan = SupplyPlan {
    pmic: I2cAddr::new(0, 0x60),
    rails: [
        (Buck::B1, BuckVoltage::from_millivolts(1480)),
        (Buck::B2, BuckVoltage::from_millivolts(1285)),
        (Buck::B3, BuckVoltage::from_millivolts(1840)),
    ],
    control: BuckControl::new()
        .with_enabled(true)
        .with_discharge_when_off(true)
        .with_forced_pwm(true),
};

/// The family member a hardware ID names, as its device ID, through the real identity
/// sequence; an unknown device ID is the error.
pub fn identify(hwid: u32) -> Result<u8, u8> {
    let mut read = IdentityRead::new();
    drive(&mut read, |_, _| Ok(hwid)).map_err(|_| 0)?;
    Variant::from_hwid(read.hwid()).map(|variant| match variant {
        Variant::Xsdr => 0x30,
        Variant::Xtrx => 0x2e,
        Variant::Ssdr => 0x31,
        Variant::SsdrPro => 0x33,
    })
}

/// Runs the supply bring-up, answering every PMIC read with `byte`: the actions issued
/// and the PMIC revision word.
pub fn supplies(byte: u8) -> Result<(usize, u16), &'static str> {
    let mut supplies = Supplies::new(PLAN);
    let actions = drive(&mut supplies, |_, _| Ok(byte.into()))?;
    Ok((actions, supplies.pmic_revision().to_raw()))
}

/// Runs the RFIC power-up, answering the LMS7002M's version read with `version`: the
/// actions issued and the chip state it leaves.
fn power_up_chip(version: u16) -> Result<(usize, Option<Lms7002m>), &'static str> {
    let mut power = PowerUp::new(LMS);
    let actions = drive(&mut power, |_, _| Ok(version.into()))?;
    Ok((actions, power.chip()))
}

/// Runs the RFIC power-up: the actions issued and whether the chip was created.
pub fn power_up(version: u16) -> Result<(usize, bool), &'static str> {
    power_up_chip(version).map(|(actions, chip)| (actions, chip.is_some()))
}

/// Runs the close from a powered-up chip or a never-created one, failing the exchange at
/// position `failing`: the actions issued.
pub fn close(powered: bool, failing: Option<usize>) -> Result<usize, &'static str> {
    let chip = if powered {
        power_up_chip(0x3840)?.1.ok_or("not created")?
    } else {
        Lms7002m::new(LMS.target)
    };
    drive(&mut Close::new(chip), |at, _| {
        if failing == Some(at) { Err(()) } else { Ok(0) }
    })
}

/// Runs `sequence` to its end, answering action `n` with `answer(n, action)`: the actions
/// issued, or the chip the failure names.
fn drive(
    sequence: &mut impl Sequence,
    mut answer: impl FnMut(usize, SequenceAction) -> Result<u32, ()>,
) -> Result<usize, &'static str> {
    let mut actions = 0;
    while let Some(action) = sequence.next() {
        let result = answer(actions, action);
        actions += 1;
        sequence.complete(result).map_err(reason)?;
    }
    Ok(actions)
}

/// The chip a failure names.
fn reason(failure: SequenceFailure<()>) -> &'static str {
    match failure {
        SequenceFailure::Transport { chip, .. }
        | SequenceFailure::FpgaTransport { chip, .. }
        | SequenceFailure::ChipId { chip, .. }
        | SequenceFailure::ChipMissing { chip }
        | SequenceFailure::InputMissing { chip } => chip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exact_source_witness_identifies_by_the_device_id_byte() {
        assert_eq!(identify(0x0030_0000), Ok(0x30));
        assert_eq!(identify(0x002e_ffff), Ok(0x2e));
        assert_eq!(identify(0x0032_0000), Err(0x32));
    }

    #[test]
    fn the_exact_source_witness_runs_the_pinned_session() {
        // Golden lines 12-26: the LMS pulse, two revision reads, three rails, four
        // controls, a wait, two status reads and the clock select.
        assert_eq!(supplies(0xe0), Ok((15, 0xe0e0)));
        // Golden lines 78-97: GPO writes and waits, then lms7002m_create's ten writes and
        // version read.
        assert_eq!(power_up(0x3840), Ok((20, true)));
        assert_eq!(power_up(0x3880), Err("LMS7002M"));
        // Golden lines 99-133: 26 streaming-down writes, five destroy writes, four board
        // writes. A never-created chip also selects both banks first.
        assert_eq!(close(true, None), Ok(35));
        assert_eq!(close(false, None), Ok(36));
    }

    #[test]
    fn the_exact_source_witness_keeps_libusdrs_close_failure_rules() {
        assert_eq!(
            close(true, Some(0)),
            Ok(35 - 1),
            "a skipped RX processor write"
        );
        assert_eq!(close(true, Some(25)), Err("LMS7002M"), "the AFE write");
        assert_eq!(close(true, Some(31)), Err("FPGA"), "the power control");
    }
}
