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
    /// Revisions 1 and 2 do not carry one, so their Si5332 then reports no input clock.
    Oscillator,
    /// Clock input 2, received differentially, at `hz` (`extclk=1` with `extref`;
    /// "external"). libusdr suggests 23 to 41 MHz for the LMS6002D.
    External {
        /// The reference frequency, in Hz.
        hz: u32,
    },
}

impl ReferenceClock {
    /// The on-board oscillator's and libusdr's default frequency (`USDR_INT_REFCLK`).
    pub const INTERNAL_HZ: u32 = 26_000_000;

    /// Rejects an external reference of 0 Hz, which libusdr would silently treat as 26 MHz.
    pub(crate) fn check(self) -> Result<(), Error> {
        match self {
            Self::External { hz: 0 } => Err(Error::UnsupportedFrequency(0)),
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
    fn an_external_reference_needs_a_frequency() {
        assert!(matches!(
            ReferenceClock::External { hz: 0 }.check(),
            Err(Error::UnsupportedFrequency(0))
        ));
        assert_eq!(ReferenceClock::External { hz: 40_000_000 }.hz(), 40_000_000);
        assert_eq!(ReferenceClock::Board.hz(), ReferenceClock::INTERNAL_HZ);
    }
}
