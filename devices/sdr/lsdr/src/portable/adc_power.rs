//! Powering the ADCs up or down: `_lsdr_adc_activate` in `device/m2_lsdr/m2_da09_4_ad45_2.c`,
//! which `/dm/power/en` runs with mask 3 (on) or 0 (off).
//!
//! The ADC supply (TPS6594 LDO4) is switched, then both ADCs are held in reset, the receive
//! FIFOs reset with the ADCs released, and the ADCs left released (on) or held in reset
//! (off). The Si549 sample clock is not touched: only libusdr's sample-rate setting enables
//! it.

use core::time::Duration;

use hal::{Register, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

use super::fpga::{self, ADC_RESET, FIFO_RESET, Gpo};
use super::outcome::outcome;
use super::tps6594::{self, Ldo4Control};
use super::wiring::Wiring;

/// libusdr's wait after switching the supply (`usleep(10000)`).
const SUPPLY_SETTLE: Duration = Duration::from_millis(10);

/// libusdr's wait after each reset step (`usleep(1000)`).
const RESET_SETTLE: Duration = Duration::from_millis(1);

/// `_lsdr_adc_activate` with both ADCs on or both off. Every failure ends it, as libusdr
/// returns on each.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AdcPower {
    /// Where the PMIC answers.
    wiring: Wiring,

    /// Both ADCs on (libusdr's mask 3) or off (mask 0).
    on: bool,

    /// Actions completed.
    done: usize,

    /// An action failed; nothing more is issued.
    failed: bool,
}

impl AdcPower {
    /// Starts switching both ADCs `on` or off; nothing has been accessed yet.
    pub(crate) const fn new(wiring: Wiring, on: bool) -> Self {
        Self {
            wiring,
            on,
            done: 0,
            failed: false,
        }
    }

    /// Action `step`, and the chip it reaches.
    fn action(&self, step: usize) -> Option<(SequenceAction, &'static str)> {
        let gpo = |data| Some((SequenceAction::Fpga(Gpo::AdcConfig.write(data)), fpga::NAME));
        let wait = |duration| Some((SequenceAction::Sleep(duration), fpga::NAME));
        // libusdr's `0x03 ^ (msk & 3)`: an active ADC's reset bit is cleared.
        let active = if self.on { ADC_RESET } else { 0 };
        match step {
            0 => Some((
                SequenceAction::Access(RegisterAccess::write(
                    self.wiring.pmic,
                    Ldo4Control::switched(self.on).entry(),
                )),
                tps6594::NAME,
            )),
            1 => wait(SUPPLY_SETTLE),
            2 => gpo(ADC_RESET),
            3 | 5 => wait(RESET_SETTLE),
            4 => gpo(FIFO_RESET),
            6 => gpo(ADC_RESET ^ active),
            _ => None,
        }
    }
}

impl Sequence for AdcPower {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.action(self.done).map(|(action, _)| action)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some((action, chip)) = self.action(self.done).filter(|_| !self.failed) else {
            return Ok(());
        };
        outcome(action, result, chip).inspect_err(|_| self.failed = true)?;
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::{FpgaAccess, I2cAddr, SpiAddr};

    use super::*;

    /// The `LSDr`'s addresses.
    const WIRING: Wiring = Wiring {
        pmic: I2cAddr::new(0, 0x48),
        oscillator: I2cAddr::new(0, 0x67),
        adcs: [SpiAddr(0), SpiAddr(1)],
        clock_generator: SpiAddr(2),
    };

    /// The data of the last GPO write `power` issues, run to its end.
    fn last_reset_bits(mut power: AdcPower) -> Option<u32> {
        let mut last = None;
        while let Some(action) = power.next() {
            if let SequenceAction::Fpga(FpgaAccess::Write { value, .. }) = action {
                last = Some(value & 0xff);
            }
            power.complete::<()>(Ok(0)).unwrap();
        }
        last
    }

    #[test]
    fn powering_up_ends_with_both_adcs_out_of_reset_and_down_with_both_held() {
        assert_eq!(last_reset_bits(AdcPower::new(WIRING, true)), Some(0x00));
        assert_eq!(last_reset_bits(AdcPower::new(WIRING, false)), Some(0x03));
    }

    #[test]
    fn the_supply_is_switched_before_any_reset_line_moves() {
        let power = AdcPower::new(WIRING, true);
        assert!(matches!(
            power.next(),
            Some(SequenceAction::Access(RegisterAccess::Write {
                reg: 0x20,
                value: 0x33,
                ..
            }))
        ));
    }
}
