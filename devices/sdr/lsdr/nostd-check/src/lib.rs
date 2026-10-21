//! Compile witness for the exact private lsdr portable sequences, not another driver.
//! A bare-metal check proves core (+ `bnb`'s alloc) compilation, not linking or hardware
//! operation.
#![no_std]
#![forbid(unsafe_code)]

#[path = "../../src/portable/mod.rs"]
mod portable;

use portable::adc_power::AdcPower;

use hal::{FpgaAccess, I2cAddr, Sequence, SequenceAction, SequenceFailure, SpiAddr};
use portable::fpga::{self, DEAD, LIVENESS};
use portable::lmk04832::ClockPlan;
use portable::nco::{phase_increment, select_channels, set_nco};
use portable::pga::{PgaGain, SetPga};
use portable::probes::Probes;
use portable::rate::{RateFinish, RatePlan, RatePrelude};
use portable::si549::{FrequencyPlan, SetFrequency};
use portable::single::Single;
use portable::supplies::{Supplies, SupplyPlan};
use portable::tps6594::{Buck, BuckVoltage};
use portable::wiring::Wiring;

/// The LSDr's addresses (`enum BUSIDX_m2_da09_4_ad45_2_rev000`).
const WIRING: Wiring = Wiring {
    pmic: I2cAddr::new(0, 0x48),
    oscillator: I2cAddr::new(0, 0x67),
    adcs: [SpiAddr(0), SpiAddr(1)],
    clock_generator: SpiAddr(2),
};

/// The LSDr's rails, as open sets them.
const PLAN: SupplyPlan = SupplyPlan {
    wiring: WIRING,
    rails: [
        (Buck::B1, BuckVoltage::from_millivolts(920)),
        (Buck::B2, BuckVoltage::from_millivolts(920)),
        (Buck::B3, BuckVoltage::from_millivolts(2200)),
        (Buck::B4, BuckVoltage::from_millivolts(1200)),
    ],
};

/// Whether open's liveness read of `status` finds a live board (libusdr's "Device is
/// dead!" on all ones).
pub fn alive(status: u32) -> Result<bool, &'static str> {
    let mut liveness = Single::new(SequenceAction::Fpga(LIVENESS), fpga::NAME);
    drive(&mut liveness, |_, _| Ok(status))?;
    Ok(liveness.value() != DEAD)
}

/// Runs open's supplies and probes, answering an SPI exchange on bus `n` with
/// `answers[n]`: the actions issued, and which ADC sockets and clock generator were found
/// (bits 0 and 1, bit 2).
pub fn open(clock_generator: bool, answers: [u32; 3]) -> Result<(usize, u8), &'static str> {
    let answer = |_, action| match action {
        SequenceAction::Fpga(FpgaAccess::Spi { target, .. }) => {
            Ok(answers[usize::from(target.bus())])
        }
        _ => Ok(0),
    };
    let supplies = drive(&mut Supplies::new(PLAN), answer)?;
    let mut probes = Probes::new(WIRING, clock_generator);
    let found = drive(&mut probes, answer)?;
    let fitted = probes.fitted();
    let bits = u8::from(fitted.adcs[0]) | u8::from(fitted.adcs[1]) << 1;
    Ok((
        supplies + found,
        bits | u8::from(fitted.clock_generator) << 2,
    ))
}

/// Switches both ADCs `on` or off: the actions issued and the ADC reset bits written last.
pub fn power(on: bool) -> Result<(usize, u32), &'static str> {
    let mut last = 0;
    let actions = drive(&mut AdcPower::new(WIRING, on), |_, action| {
        if let SequenceAction::Fpga(FpgaAccess::Write { value, .. }) = action {
            last = value & 0xff;
        }
        Ok(0)
    })?;
    Ok((actions, last))
}

/// Sets the PGA to `steps`, failing the write at position `failing`: the setting and the
/// writes issued, or `None` for a setting `PgaGain` cannot hold.
pub fn pga(steps: u8, failing: Option<usize>) -> Option<(u8, Result<usize, &'static str>)> {
    let gain = PgaGain::new(steps)?;
    let written = drive(&mut SetPga::new(gain), |at, _| {
        if failing == Some(at) { Err(()) } else { Ok(0) }
    });
    Some((gain.steps(), written))
}

/// Runs the close's writes, failing those at the positions `failing` marks: how many
/// succeeded. Each is attempted whatever the others did.
pub fn close_writes(failing: [bool; 4]) -> usize {
    WIRING
        .close()
        .into_iter()
        .zip(failing)
        .filter_map(|(mut write, fails)| {
            drive(&mut write, |_, _| if fails { Err(()) } else { Ok(0) }).ok()
        })
        .count()
}

/// What a rate ran: its decimation, and the actions of its two parts.
pub type RateRun = (u32, usize, usize);

/// Sets `rate` with the Si549 answering `device_type`, laying out a clock generator if
/// `clock`; `None` where libusdr refuses the rate before an access (the Si549's range or
/// the clock's plan).
pub fn rate(rate: u32, clock: bool, device_type: u32) -> Option<Result<RateRun, &'static str>> {
    let plan = RatePlan::new(rate);
    let frequency = FrequencyPlan::new(plan.adc_rate)?;
    let clock = if clock {
        Some(ClockPlan::new(rate, rate).ok()?)
    } else {
        None
    };
    let answer = |_, _| Ok(device_type);
    let mut prelude = RatePrelude::new(plan, SetFrequency::new(WIRING.oscillator, frequency));
    Some(drive(&mut prelude, answer).and_then(|prelude| {
        let finish = drive(&mut RateFinish::new(WIRING, plan, clock), answer)?;
        Ok((plan.decimation.factor(), prelude, finish))
    }))
}

/// Sets the NCO to `offset` on an ADC at `adc_rate`: the phase increment and the writes
/// issued, the channel selection first, or `None` without a rate.
pub fn nco(adc_rate: u32, offset: u32) -> Option<(u32, usize)> {
    let select = SequenceAction::Fpga(select_channels());
    drive(&mut Single::new(select, fpga::NAME), |_, _| Ok(0)).ok()?;
    let increment = phase_increment(adc_rate, offset)?;
    let writes = drive(&mut set_nco(increment), |_, _| Ok(0)).ok()?;
    Some((increment, 1 + writes))
}

/// Runs `sequence` to its end, answering action `n` with `answer(n, action)`: the actions
/// issued, or the chip the failure names.
fn drive(
    sequence: &mut impl Sequence,
    mut answer: impl FnMut(usize, SequenceAction) -> Result<u32, ()>,
) -> Result<usize, &'static str> {
    let mut actions = 0;
    let mut first = None;
    while let Some(action) = sequence.next() {
        let result = answer(actions, action);
        actions += 1;
        if let Err(failure) = sequence.complete(result) {
            if !sequence.attempts_after_failure() {
                return Err(reason(failure));
            }
            first.get_or_insert(reason(failure));
        }
    }
    first.map_or(Ok(actions), Err)
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
    fn the_exact_source_witness_runs_the_pinned_open() {
        // Golden line 10, and the dead link.
        assert_eq!(alive(0), Ok(true));
        assert_eq!(alive(u32::MAX), Ok(false));
        // Single-ADC golden lines 11-26: three identification reads, four rails, the LDO4
        // and Si549 gate with its waits, two ADC probes and LDO4 off.
        assert_eq!(open(false, [0x6c, u32::MAX, u32::MAX]), Ok((13 + 3, 0b001)));
        // Dual-ADC lmk=1 lines 11-33: seven LMK04832 exchanges more. The floating readback
        // finds no clock generator.
        assert_eq!(open(true, [0x6c, 0x6d, u32::MAX]), Ok((13 + 10, 0b011)));
    }

    #[test]
    fn the_exact_source_witness_runs_power_gain_and_close() {
        // Golden lines 57-63: LDO4, a wait, three reset writes and two waits.
        assert_eq!(power(true), Ok((7, 0x00)));
        assert_eq!(power(false), Ok((7, 0x03)));
        assert_eq!(pga(31, None), Some((31, Ok(3))));
        assert_eq!(pga(32, None), None, "libusdr clamps; PgaGain refuses");
        assert_eq!(pga(10, Some(1)), Some((10, Err("FPGA"))), "the latch write");
        // Golden lines 65-68, every write attempted.
        assert_eq!(close_writes([false; 4]), 4);
        assert_eq!(close_writes([true, false, true, false]), 2);
    }

    #[test]
    fn the_exact_source_witness_runs_the_rate_and_the_nco() {
        // 50 MS/s: x5; the Si549's check and 13 writes, the channel mask and 1280 microcode
        // bytes; then the wait and the 18-step tail, with the LMK04832's 38 writes and
        // three status reads before it on a clocked board.
        assert_eq!(rate(50_000_000, false, 0x31), Some(Ok((5, 1295, 19))));
        assert_eq!(rate(50_000_000, true, 0x31), Some(Ok((5, 1295, 60))));
        assert_eq!(rate(50_000_000, false, 0xff), Some(Err("Si549")));
        assert_eq!(
            rate(400_000_001, false, 0x31),
            None,
            "past the Si549's range"
        );
        assert_eq!(nco(250_000_000, 10_000_000), Some((0xca3d_70a4, 6)));
        assert_eq!(nco(0, 10_000_000), None, "no rate yet");
    }
}
