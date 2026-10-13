//! The public device: open with its probes, ADC power, the receive PGA, and close.

use std::fmt;

use rsl_lowlevel::{Sequence, SequenceAction, run_sequence};

use crate::board::{LSDR_SUPPLIES, LSDR_WIRING};
use crate::error::Error;
use crate::lowlevel::Bus;
use crate::portable::adc_power::AdcPower;
use crate::portable::close::close;
use crate::portable::fpga::{self, DEAD, LIVENESS};
use crate::portable::pga::{PgaGain, SetPga};
use crate::portable::probes::{Fitted, Probes};
use crate::portable::single::Single;
use crate::portable::supplies::Supplies;

/// How to open a board: libusdr's connection parameters that change what open does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenOptions {
    /// Test for an LMK04832 clock generator (libusdr's `lmk=1`); off by default, as in
    /// libusdr.
    probe_clock_generator: bool,
}

impl OpenOptions {
    /// libusdr's defaults: no clock-generator test.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            probe_clock_generator: false,
        }
    }

    /// Whether open tests for an LMK04832 on SPI bus 2 (libusdr's `lmk=1`).
    #[must_use]
    pub const fn probe_clock_generator(mut self, probe: bool) -> Self {
        self.probe_clock_generator = probe;
        self
    }
}

/// An open `LSDr`: a receive-only board with one or two `ADS42LBx9` ADCs.
///
/// Dropping a `Device` leaves the board as it is; [`Self::close`] powers it down as
/// libusdr's close does.
pub struct Device {
    /// The board's bus.
    bus: Box<dyn Bus>,

    /// What open found.
    fitted: Fitted,
}

impl Device {
    /// Opens the board on `bus`, as libusdr's open does up to its front-end probe: checks
    /// that the link is alive, reads the PMIC's identification, sets four rails, gates the
    /// Si549 sample clock, supplies the ADCs while it probes for them (and, when asked, for
    /// an LMK04832), then cuts their supply again. The ADCs stay unpowered
    /// ([`Self::set_power`]).
    ///
    /// Not yet ported: the front-end probe libusdr runs next, so no front end is detected
    /// or brought up.
    ///
    /// # Errors
    ///
    /// [`Error::DeadLink`] when the status register reads all ones, before anything is
    /// driven; [`Error::Bus`] when an access fails. A missing chip is not an error: see
    /// [`Self::fitted`].
    pub fn open(bus: impl Bus + 'static, options: OpenOptions) -> Result<Self, Error> {
        let mut bus: Box<dyn Bus> = Box::new(bus);
        let mut liveness = Single::new(SequenceAction::Fpga(LIVENESS), fpga::NAME);
        run(&mut *bus, &mut liveness)?;
        if liveness.value() == DEAD {
            return Err(Error::DeadLink);
        }
        run(&mut *bus, &mut Supplies::new(LSDR_SUPPLIES))?;
        let mut probes = Probes::new(LSDR_WIRING, options.probe_clock_generator);
        run(&mut *bus, &mut probes)?;
        Ok(Self {
            bus,
            fitted: probes.fitted(),
        })
    }

    /// What open found on the board.
    #[must_use]
    pub fn fitted(&self) -> Fitted {
        self.fitted
    }

    /// Powers both ADCs up or down (libusdr's `/dm/power/en`): switches their supply, then
    /// pulses their resets and the receive FIFOs' and leaves the ADCs released (on) or held
    /// in reset (off). The sample clock stays gated.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] when an access fails; the rest of the sequence is not run.
    pub fn set_power(&mut self, on: bool) -> Result<(), Error> {
        run(&mut *self.bus, &mut AdcPower::new(LSDR_WIRING, on))
    }

    /// Sets the receive PGA's gain (libusdr's `/dm/sdr/0/rx/gain/pga`, and
    /// `/dm/sdr/0/rx/gain` with no front end).
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] when an access fails; the rest of the sequence is not run.
    pub fn set_rx_pga_gain(&mut self, gain: PgaGain) -> Result<(), Error> {
        run(&mut *self.bus, &mut SetPga::new(gain))
    }

    /// Closes the board as libusdr does: gates the sample clock, cuts the ADC supply, and
    /// tells both ADC sockets to power down. Every write is attempted whatever the others
    /// did, as in libusdr.
    ///
    /// # Errors
    ///
    /// The first write's failure, [`Error::Bus`], after all four were attempted.
    pub fn close(mut self) -> Result<(), Error> {
        close(LSDR_WIRING)
            .into_iter()
            .map(|mut write| run(&mut *self.bus, &mut write))
            .fold(Ok(()), Result::and)
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("fitted", &self.fitted)
            .finish_non_exhaustive()
    }
}

/// Runs a portable sequence to its end over `bus`, with the failure in the driver's terms.
fn run(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_sequence(bus, sequence).map_err(Error::from)
}
