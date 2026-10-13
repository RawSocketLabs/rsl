//! The receive PGA's gain: `dev_m2_d09_4_ad45_2_pga` in `device/m2_lsdr/m2_da09_4_ad45_2.c`,
//! behind `/dm/sdr/0/rx/gain/pga` and, with no front end, `/dm/sdr/0/rx/gain`.
//!
//! The PGA sits behind gateware GPO banks; the pinned source names neither the part nor
//! the gain's unit. libusdr writes the code `63 - 2 × gain` to the gain bank, then pulses the
//! latch enable.

use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure};

use super::fpga::{self, Gpo, PGA_LATCH};
use super::outcome::outcome;

/// A receive PGA gain setting, libusdr's 0 to 31 (its unit is not in the source).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PgaGain(u8);

impl PgaGain {
    /// The highest setting libusdr writes; it clamps larger values to it.
    pub const MAX: Self = Self(31);

    /// The setting `steps`, or `None` above [`Self::MAX`].
    #[must_use]
    pub const fn new(steps: u8) -> Option<Self> {
        if steps <= Self::MAX.0 {
            Some(Self(steps))
        } else {
            None
        }
    }

    /// The setting, 0 to 31.
    #[must_use]
    pub const fn steps(self) -> u8 {
        self.0
    }

    /// The code the gain bank receives: `63 - (gain << 1)`.
    const fn code(self) -> u8 {
        63 - (self.0 << 1)
    }
}

/// `dev_m2_d09_4_ad45_2_pga`: the gain code, then the latch pulsed. Every failure ends it,
/// as libusdr returns on each.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SetPga {
    /// The writes in order.
    writes: [u8; 3],

    /// Writes completed.
    done: usize,

    /// A write failed; nothing more is issued.
    failed: bool,
}

impl SetPga {
    /// Starts setting `gain`; nothing has been accessed yet.
    pub(crate) const fn new(gain: PgaGain) -> Self {
        Self {
            writes: [gain.code(), PGA_LATCH, 0],
            done: 0,
            failed: false,
        }
    }
}

impl Sequence for SetPga {
    fn next(&self) -> Option<SequenceAction> {
        let bank = if self.done == 0 {
            Gpo::PgaGain
        } else {
            Gpo::PgaLatch
        };
        let data = self.writes.get(self.done).filter(|_| !self.failed)?;
        Some(SequenceAction::Fpga(bank.write(*data)))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        outcome(action, result, fpga::NAME).inspect_err(|_| self.failed = true)?;
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::FpgaAccess;

    use super::*;

    #[test]
    fn settings_past_libusdrs_clamp_are_unrepresentable() {
        assert_eq!(PgaGain::new(31), Some(PgaGain::MAX));
        assert_eq!(PgaGain::new(32), None);
    }

    #[test]
    fn the_gain_code_is_written_then_latched() {
        let mut set = SetPga::new(PgaGain::new(10).unwrap());
        let mut words = [0; 3];
        for word in &mut words {
            let Some(SequenceAction::Fpga(FpgaAccess::Write { reg: 0, value })) = set.next() else {
                panic!("every step is a GPO write");
            };
            *word = value;
            set.complete::<()>(Ok(0)).unwrap();
        }
        assert_eq!(words, [0x0400_002b, 0x0300_000f, 0x0300_0000]);
        assert_eq!(set.next(), None);
    }
}
