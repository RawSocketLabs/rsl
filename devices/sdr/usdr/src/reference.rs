//! Where the board's reference clock comes from.

use crate::error::Error;

/// The reference every clock on the board derives from: the Si5332's input, which also
/// sets the LMS6002D's PLL reference and the sample-clock plans.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReferenceClock {
    /// libusdr's default: the on-board 26 MHz oscillator on revision 3, clock input 2 at
    /// 26 MHz on revisions 1 and 2.
    #[default]
    Board,
    /// The on-board 26 MHz oscillator (`extclk=0`, `/dm/sdr/refclk/path` "internal").
    /// Inferred from libusdr, not checked on hardware: revisions 1 and 2 carry none, so their
    /// Si5332 would then report no input clock.
    Oscillator,
    /// Clock input 2, received differentially, at `hz` (`extclk=1` with `extref`;
    /// "external"): between [`Self::MIN_EXTERNAL_HZ`] and [`Self::MAX_EXTERNAL_HZ`]; libusdr
    /// suggests 23 to 41 MHz for the LMS6002D.
    External {
        /// The reference frequency, in Hz.
        hz: u32,
    },
}

impl ReferenceClock {
    /// The on-board oscillator's and libusdr's default frequency (`USDR_INT_REFCLK`).
    pub const INTERNAL_HZ: u32 = 26_000_000;

    /// The lowest external reference the Si5332 plans can use: below it, the PLL's 15-bit
    /// integer term (`128 * 2.625 GHz / reference`) overflows.
    pub const MIN_EXTERNAL_HZ: u32 = 10_253_907;

    /// The highest external reference: one /2 prescale must bring it within the PLL's 50 MHz
    /// input limit (`si5332_Fref_max`).
    pub const MAX_EXTERNAL_HZ: u32 = 100_000_000;

    /// Rejects an external reference the clock plans cannot use. libusdr accepts any value
    /// (and treats 0 as 26 MHz), then programs garbage clocks at the next rate change.
    pub(crate) fn check(self) -> Result<(), Error> {
        match self {
            Self::External { hz }
                if !(Self::MIN_EXTERNAL_HZ..=Self::MAX_EXTERNAL_HZ).contains(&hz) =>
            {
                Err(Error::UnsupportedReferenceClock(hz))
            }
            _ => Ok(()),
        }
    }

    /// Whether clock input 2 is used on a board of this revision.
    pub(crate) const fn uses_input2(self, rev3: bool) -> bool {
        match self {
            Self::Board => !rev3,
            Self::Oscillator => false,
            Self::External { .. } => true,
        }
    }

    /// The reference frequency, in Hz.
    pub(crate) const fn hz(self) -> u32 {
        match self {
            Self::External { hz } => hz,
            Self::Board | Self::Oscillator => Self::INTERNAL_HZ,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_default_uses_input2_only_before_revision_3() {
        assert!(ReferenceClock::Board.uses_input2(false));
        assert!(!ReferenceClock::Board.uses_input2(true));
        assert!(!ReferenceClock::Oscillator.uses_input2(false));
        assert!(ReferenceClock::External { hz: 26_000_000 }.uses_input2(true));
    }

    #[test]
    fn an_external_reference_must_suit_the_si5332_plans() {
        for hz in [0, 40_000, ReferenceClock::MIN_EXTERNAL_HZ - 1, 100_000_001] {
            assert!(
                matches!(
                    ReferenceClock::External { hz }.check(),
                    Err(Error::UnsupportedReferenceClock(found)) if found == hz
                ),
                "{hz} Hz"
            );
        }
        for hz in [ReferenceClock::MIN_EXTERNAL_HZ, 26_000_000, 100_000_000] {
            assert!(ReferenceClock::External { hz }.check().is_ok(), "{hz} Hz");
        }
        assert_eq!(ReferenceClock::External { hz: 40_000_000 }.hz(), 40_000_000);
        assert_eq!(ReferenceClock::Board.hz(), ReferenceClock::INTERNAL_HZ);
    }
}
