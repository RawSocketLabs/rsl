//! The LMX1214's bus operations as `hw/lmx1214/lmx1214.c` runs them: `lmx1214_create`, the
//! register part of `lmx1214_solver` and `lmx1214_get_temperature`.

use core::time::Duration;

use rsl_lowlevel::{SequenceAction, SpiAddr};

use super::plan::Plan;
use super::reg::{CLOCK_DIVIDER_RESET, CREATE, Reg};
use crate::portable::chain::Then;
use crate::portable::script::Script;
use crate::portable::ti_spi::{self, Modify};

/// The chip's name in errors.
pub(crate) const NAME: &str = "LMX1214";

/// The pause after `lmx1214_create`'s writes, before the temperature read.
const CREATE_SETTLE: Duration = Duration::from_millis(1);

/// The pause after the solver's registers, before the main divider's release.
const CONFIGURE_SETTLE: Duration = Duration::from_millis(10);

/// `lmx1214_get_temperature`: the sensor read, which libusdr only logs.
pub(crate) const fn temperature(target: SpiAddr) -> Script {
    Script::new().then(NAME, ti_spi::read(target, Reg::TemperatureCount as u8))
}

/// `lmx1214_create`: MUXOUT and the temperature sensor set up, 1 ms, then the temperature
/// read, whose failure fails the create.
pub(crate) fn create(target: SpiAddr) -> Script {
    let mut script = Script::new();
    for (reg, value) in CREATE {
        script = script.then(NAME, ti_spi::write(target, (reg.into(), value)));
    }
    script
        .then(NAME, SequenceAction::Sleep(CREATE_SETTLE))
        .append(temperature(target))
}

/// `lmx1214_solver`'s bus part: the main divider held in reset (R25 read and written back
/// with `CLK_DIV_RST`), the plan's registers, 10 ms, then the divider released (R25 read
/// again and written back without it).
pub(crate) type Configure = Then<Modify, Then<Script, Modify>>;

/// Starts configuring the chip at `target` for `plan`.
pub(crate) fn configure(target: SpiAddr, plan: &Plan) -> Configure {
    let reset = |set, clear| Modify::new(NAME, target, Reg::ClockMux as u8, set, clear);
    let mut writes = Script::new();
    for entry in plan.registers() {
        writes = writes.then(NAME, ti_spi::write(target, entry));
    }
    let writes = writes.then(NAME, SequenceAction::Sleep(CONFIGURE_SETTLE));
    Then::new(
        reset(CLOCK_DIVIDER_RESET, 0),
        Then::new(writes, reset(0, CLOCK_DIVIDER_RESET)),
    )
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::{FpgaAccess, Sequence};

    use super::super::plan::{Aux, Request};
    use super::super::reg::AuxFormat;
    use super::*;
    use crate::portable::testing::{Exchange, drive};

    /// The board's LMX1214 bus.
    const TARGET: SpiAddr = SpiAddr(5);

    /// Answers R25 reads with its reset value 0x0211, everything else with 0.
    #[expect(clippy::unnecessary_wraps, reason = "the test executor's answer type")]
    fn chip(action: &SequenceAction) -> Result<u32, ()> {
        Ok(match action {
            SequenceAction::Fpga(FpgaAccess::Spi {
                word: 0x0099_0000, ..
            }) => 0x0211,
            _ => 0,
        })
    }

    #[test]
    fn create_writes_its_setup_and_reads_the_temperature() {
        // Golden session.trace lines 1418-1423.
        let (done, outcome) = drive(&mut create(TARGET), chip);
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done,
            [
                Exchange::Spi(0x0056_0000),
                Exchange::Spi(0x004f_0005),
                Exchange::Spi(0x0018_0001),
                Exchange::Spi(0x0017_e040),
                Exchange::Sleep(1000),
                Exchange::Spi(0x0098_0000),
            ]
        );
    }

    #[test]
    fn configure_holds_the_divider_in_reset_around_the_registers() {
        let plan = Plan::solve(&Request {
            input_hz: 550_000_000,
            output_hz: 550_000_000,
            outputs: [true; 4],
            aux: Aux {
                enabled: true,
                hz: 550_000_000,
                format: AuxFormat::Lvds,
            },
        })
        .expect("libusdr's plan");
        let mut sequence = configure(TARGET, &plan);
        let (done, outcome) = drive(&mut sequence, chip);
        assert_eq!(outcome, Ok(()));
        assert_eq!(done.len(), 2 + 11 + 1 + 2);
        assert_eq!(
            done[..2],
            [Exchange::Spi(0x0099_0000), Exchange::Spi(0x0019_0251)]
        );
        assert_eq!(done[13], Exchange::Sleep(10_000));
        assert_eq!(
            done[14..],
            [Exchange::Spi(0x0099_0000), Exchange::Spi(0x0019_0211)],
            "released from what it reads then"
        );
        assert_eq!(sequence.next(), None);
    }
}
