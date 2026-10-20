//! Compile witness for the exact private xsdr portable sequences, not another driver.
//! A bare-metal check proves core (+ `bnb`'s alloc) compilation, not linking or hardware
//! operation.
#![no_std]
#![forbid(unsafe_code)]

use core::num::NonZeroU32;

#[path = "../../src/portable/mod.rs"]
mod portable;

use hal::{FpgaAccess, I2cAddr, Sequence, SequenceAction, SequenceFailure, SpiAddr};
use lms7002m::ctrl::{Control, ControlError};
use lms7002m::{Lms7002m, ResetPlan, Synthesizer};
use lp8758::{Buck, BuckControl, BuckVoltage};
use portable::close::{Close, Down, streaming_down};
use portable::frontend::{RX_BANDS, TX_BANDS, XsdrFrontend};
use portable::hwid::{IdentityRead, Variant};
use portable::power::PowerUp;
use portable::rate::{Gateware, RateError, RateState, SetSampleRate};
use portable::supplies::{Supplies, SupplyPlan};

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

/// Brings RX alone (`rx_only`) or both directions of a created chip down: the SPI words
/// issued, the first `.1` of `.0` used.
pub fn streaming_down_words(rx_only: bool) -> Result<([u32; 32], usize), &'static str> {
    let mut chip = power_up_chip(0x3840)?.1.ok_or("not created")?;
    let mut words = [0; 32];
    let mut len = 0;
    let down = if rx_only { Down::Rx } else { Down::Both };
    for mut batch in streaming_down(&mut chip, down, ([false; 2], [false; 2]))
        .into_iter()
        .flatten()
    {
        drive(&mut batch, |_, action| {
            if let (SequenceAction::Fpga(FpgaAccess::Spi { word, .. }), Some(slot)) =
                (action, words.get_mut(len))
            {
                *slot = word;
                len += 1;
            }
            Ok(0)
        })?;
    }
    Ok((words, len))
}

/// A created chip's control state on the XSDR's 26 MHz reference and bands.
fn control() -> Result<Control, &'static str> {
    let chip = power_up_chip(0x3840)?.1.ok_or("not created")?;
    let reference = NonZeroU32::new(26_000_000).ok_or("zero reference")?;
    Ok(Control::new(chip, reference, RX_BANDS, TX_BANDS))
}

/// Sets a sample rate on chips whose VCO comparators always read low, with or without a
/// TX MMCM: the actions issued and the outcome.
pub fn rate_unlocked(tx_mmcm: bool) -> Result<(usize, &'static str), &'static str> {
    let mut rate = SetSampleRate::new(
        &control()?,
        RateState::default(),
        Gateware {
            tx_mmcm,
            rx_port1: true,
            separate_clkdiv: false,
        },
        I2cAddr::new(0, 0x60),
        (1_000_000, 1_000_000),
    );
    let actions = drive(&mut rate, |_, _| Ok(0))?;
    // The AFE came on; no CGEN frequency was taken.
    if !rate.state().afe_active || rate.control().clocks().cgen != 0 {
        return Err("inconsistent state");
    }
    let outcome = match rate.outcome() {
        Some(Ok(())) => "set",
        Some(Err(RateError::Control(ControlError::Clock))) => "clock",
        Some(Err(_)) => "other",
        None => "unfinished",
    };
    Ok((actions, outcome))
}

/// Runs a stream create's link calibration alone on a chip with no rate, with or without
/// a TX MMCM: the actions issued and the outcome.
pub fn calibrate_link(tx_mmcm: bool) -> Result<(usize, &'static str), &'static str> {
    let mut link = SetSampleRate::calibrate_link(
        &control()?,
        RateState::default(),
        Gateware {
            tx_mmcm,
            rx_port1: true,
            separate_clkdiv: false,
        },
        I2cAddr::new(0, 0x60),
    );
    let actions = drive(&mut link, |_, _| Ok(0))?;
    let outcome = match link.outcome() {
        Some(Ok(())) => "calibrated",
        Some(Err(RateError::Control(ControlError::NoRate))) => "no rate",
        Some(Err(RateError::Calibration)) => "calibration",
        Some(Err(_)) => "other",
        None => "unfinished",
    };
    Ok((actions, outcome))
}

/// Whether setting `rate` both ways with both directions running leaves a link the TX
/// MMCM can be configured for: the refusal a TX stream's rate checks before any access.
pub fn tx_clock_fits(rate: u32) -> Result<bool, &'static str> {
    let mut control = control()?;
    control.set_running([true; 2], [true; 2]);
    Ok(SetSampleRate::tx_clock_fits(
        &control,
        RateState::default(),
        Gateware {
            tx_mmcm: true,
            rx_port1: true,
            separate_clkdiv: false,
        },
        (rate, rate),
    ))
}

/// Runs a link calibration with both directions running on a chip with no rate: the
/// actions issued and the outcome.
pub fn calibrate_tx_link() -> Result<(usize, &'static str), &'static str> {
    let mut control = control()?;
    control.set_running([true; 2], [true; 2]);
    let mut link = SetSampleRate::calibrate_link(
        &control,
        RateState::default(),
        Gateware {
            tx_mmcm: true,
            rx_port1: true,
            separate_clkdiv: false,
        },
        I2cAddr::new(0, 0x60),
    );
    let actions = drive(&mut link, |_, _| Ok(0))?;
    let outcome = match link.outcome() {
        Some(Err(RateError::Mmcm(_))) => "mmcm",
        Some(_) => "other",
        None => "unfinished",
    };
    Ok((actions, outcome))
}

/// Tunes the RX LO on the same chips: whether it tuned.
pub fn tune_unlocked() -> Result<bool, &'static str> {
    let mut tune = control()?.tune(XsdrFrontend, Synthesizer::Rx, 1_000_000_000);
    drive(&mut tune, |_, _| Ok(0))?;
    Ok(tune.outcome() == Some(Ok(())))
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
    fn the_exact_source_witness_runs_the_rate_and_tuning_machines() {
        // CGEN never locks: four tunings at each of two ceilings, then the refusal.
        let (actions, outcome) = rate_unlocked(true).expect("no bus failure");
        assert_eq!(outcome, "clock");
        assert_eq!(
            rate_unlocked(false).expect("no bus failure"),
            (actions, "clock")
        );
        assert_eq!(tune_unlocked(), Ok(false));
    }

    #[test]
    fn the_exact_source_witness_runs_the_link_calibration_alone() {
        // Without a TX MMCM: the fixed delay, then libusdr's unmeasured failure.
        assert_eq!(calibrate_link(false), Ok((1, "calibration")));
        // With one: the RX pins and their 1 ms, then streaming up finds no rate.
        assert_eq!(calibrate_link(true), Ok((2, "no rate")));
    }

    #[test]
    fn the_exact_source_witness_runs_the_tx_mmcm_checks() {
        // 10 MS/s: a 20 MHz link clock; 5 MS/s: 10 MHz, below the VCO's reach at 63.
        assert_eq!(tx_clock_fits(10_000_000), Ok(true));
        assert_eq!(tx_clock_fits(5_000_000), Ok(false));
        // With no rate the link clock is 0: the TX MMCM refuses before any access.
        assert_eq!(calibrate_tx_link(), Ok((0, "mmcm")));
    }

    #[test]
    fn the_exact_source_witness_brings_either_direction_down() {
        // Golden lines 99-124: the close's 26 streaming-down writes.
        let (both, both_len) = streaming_down_words(false).expect("no failure");
        assert_eq!(both_len, 26);
        // RX alone: the same RX writes first, no TX writes, and the AFE word last.
        let (rx, rx_len) = streaming_down_words(true).expect("no failure");
        assert!(rx_len < both_len, "the TX writes are left out");
        assert_eq!(
            rx[..rx_len - 1],
            both[..rx_len - 1],
            "the RX writes, as the close's"
        );
        let afe = |word: u32| word >> 16 & 0x7fff;
        assert_eq!(
            afe(rx[rx_len - 1]),
            afe(both[both_len - 1]),
            "the AFE word last"
        );
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
