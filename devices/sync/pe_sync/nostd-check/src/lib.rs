//! Compile witness for the exact private pe-sync portable sequences, not another driver.
//! A bare-metal check proves core (+ `bnb`'s alloc) compilation, not linking or hardware
//! operation. The LMK05318's bus traffic is blocking std code in the driver, outside this
//! tree; its plan comes from the `no_std` `lmk05318`, exercised here.
#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

#[path = "../../src/portable/mod.rs"]
mod portable;

use hal::{Sequence, SequenceFailure};
use lmk05318::plan::Apll2;
use portable::board::{
    LMK1D1208I_LCK, LMK1D1208I_SETUP, LMK05318, LMK05318_OUTPUTS, LMK05318_XO, LMX1204,
    LMX1204_SETUP, LMX1214, LMX1214_SETUP, LMX2820_0, LMX2820_0_TUNE, LMX2820_1_TUNE,
};
use portable::lmk1d1208i;
use portable::lmx1204::plan::Plan as Lmx1204Plan;
use portable::lmx1204::sequence::{self as lmx1204, WaitLock};
use portable::lmx1214::plan::Plan as Lmx1214Plan;
use portable::lmx1214::sequence as lmx1214;
use portable::lmx2820::plan::Plan as Lmx2820Plan;
use portable::lmx2820::sequence::{self as lmx2820, Tune};
use portable::power::Power;
use portable::status::status_check;

/// Runs `sequence` to its end, answering every action with `answer`: the actions issued,
/// or the chip a failure names.
fn drive(sequence: &mut impl Sequence, answer: u32) -> Result<usize, &'static str> {
    let mut actions = 0;
    while sequence.next().is_some() {
        actions += 1;
        sequence.complete::<()>(Ok(answer)).map_err(reason)?;
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

/// The power-on, status check and power-off: their action counts.
pub fn power() -> Result<[usize; 3], &'static str> {
    Ok([
        drive(&mut Power::on(), 0)?,
        drive(&mut status_check(), 0)?,
        drive(&mut Power::off(), 0)?,
    ])
}

/// The LMK05318: its I2C address and whether the board's plan runs APLL2.
pub fn lmk05318_plan() -> Option<(u8, bool)> {
    let path = LMK05318_XO.path().ok()?;
    LMK05318_XO.apll1(path).ok()?;
    let mut apll2 = Apll2::cascaded();
    apll2.plan(&LMK05318_OUTPUTS).ok()?;
    Some((LMK05318.addr(), apll2.vco_hz() != 0))
}

/// Both LMX2820 plans, then #0's create and tune, every read answering `status`: their action counts and
/// whether it locked.
pub fn synthesizer(status: u32) -> Result<(usize, usize, bool), &'static str> {
    Lmx2820Plan::solve(&LMX2820_1_TUNE).map_err(|error| error.reason())?;
    let plan = Lmx2820Plan::solve(&LMX2820_0_TUNE).map_err(|error| error.reason())?;
    let created = drive(&mut lmx2820::Create::new(LMX2820_0), status)?;
    let mut tune = Tune::new(LMX2820_0, &plan);
    let tuned = drive(&mut tune, status)?;
    drive(&mut lmx2820::status(LMX2820_0), status)?;
    Ok((created, tuned, tune.locked()))
}

/// The LMX1214's create, configuration and temperature read: their action counts.
pub fn lo_distribution() -> Result<[usize; 3], &'static str> {
    let plan = Lmx1214Plan::solve(&LMX1214_SETUP).map_err(|error| error.reason())?;
    Ok([
        drive(&mut lmx1214::create(LMX1214), 0)?,
        drive(&mut lmx1214::configure(LMX1214, &plan), 0)?,
        drive(&mut lmx1214::temperature(LMX1214), 0)?,
    ])
}

/// The LMX1204's create, configuration and lock wait, every read answering `status`: their
/// action counts and whether it locked.
pub fn clock_distribution(status: u32) -> Result<([usize; 3], bool), &'static str> {
    let plan = Lmx1204Plan::solve(&LMX1204_SETUP).map_err(|error| error.reason())?;
    let mut wait = WaitLock::new(LMX1204, &plan);
    let counts = [
        drive(&mut lmx1204::create(LMX1204), status)?,
        drive(&mut lmx1204::configure(LMX1204, &plan), status)?,
        drive(&mut wait, status)?,
    ];
    Ok((counts, wait.locked()))
}

/// The `LCK` fan-out buffer's create with its identity byte reading `identity`.
pub fn fan_out(identity: u8) -> Result<usize, &'static str> {
    drive(
        &mut lmk1d1208i::Create::new(LMK1D1208I_LCK, LMK1D1208I_SETUP),
        identity.into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exact_source_witness_runs_the_pinned_session() {
        // Golden session.trace: 1208 power-on ops (lines 9-1216), the 10 status reads
        // (1217-1226), 407 for the close (1497-1903).
        assert_eq!(power(), Ok([1208, 10, 407]));
        assert_eq!(lmk05318_plan(), Some((0x64, true)), "150 MHz needs APLL2");
        // Lines 1340-1348, 1349-1375 and 1376-1378 with lock (RB_LD = 2) on every read.
        assert_eq!(synthesizer(0x8000), Ok((9, 27, true)));
        assert_eq!(lo_distribution(), Ok([6, 16, 1]), "lines 1418-1440");
        assert_eq!(
            clock_distribution(0x0200),
            Ok(([8, 31, 2], true)),
            "lines 1441-1484"
        );
        assert_eq!(fan_out(0x20), Ok(4), "lines 1488-1491");
    }

    #[test]
    fn the_exact_source_witness_keeps_its_refusals_and_timeouts() {
        assert_eq!(fan_out(0x31), Err("LMK1D1208I"));
        let unlocked = synthesizer(0);
        assert_eq!(unlocked.map(|(_, _, locked)| locked), Ok(false));
        assert_eq!(clock_distribution(0).map(|(_, locked)| locked), Ok(false));
    }
}
