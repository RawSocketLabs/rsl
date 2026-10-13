//! Power-up and power-down (`usdr_init` and `usdr_dtor` in `device/m2_lm6_1/usdr_ctrl.c`):
//! `Identified` before power, then `impl Board`.

use std::fmt;
use std::time::Duration;

use rsl_deps::tracing::{debug, warn};

use super::Board;
use super::board::Corrections;
use crate::chips::register::run_sequence;
use crate::error::Error;
use crate::fpga::{self, Gpi, Gpo, IdentityRead};
use crate::identity::Identity;
use crate::lowlevel::Bus;
use crate::portable::radio::{Chains, PowerUp, PowerUpPlan, Run, power_down};
use crate::portable::si5332::ClockSequence;
use crate::reference::ReferenceClock;
use crate::thermal::Thermometer;

/// A board identified and held in reset, before anything is powered: where the thermal
/// policy decides whether to continue. Owns the bus until power-up hands it to [`Board`].
pub(crate) struct Identified {
    /// The hardware seam.
    bus: Box<dyn Bus>,

    /// Revision 3 (on-board oscillator, TMP114 ID check).
    rev3: bool,

    /// Which DSP chains the gateware has.
    chains: Chains,
}

impl Board {
    /// Reads the hardware ID and holds the RF transceiver in reset; on revision 3, also
    /// checks the TMP114. Powers nothing.
    pub(crate) fn identify(mut bus: Box<dyn Bus>) -> Result<Identified, Error> {
        // Gateware build ID: libusdr only logs it.
        let identity = Self::read_identity(bus.as_mut())?;
        let revision = identity.revision;
        debug!(
            build = format_args!("{:#010x}", identity.gateware_build),
            revision,
            rx = identity.has_rx,
            tx = identity.has_tx,
            "gateware identified"
        );
        if !(1..=3).contains(&revision) {
            return Err(Error::UnsupportedRevision(revision));
        }
        let rev3 = revision == 3;
        Gpo::LmsReset.set(bus.as_mut(), 0)?;
        if rev3 {
            Self::TEMP.check_id(bus.as_mut())?;
        }
        let chains = Chains {
            rx: identity.has_rx,
            tx: identity.has_tx,
            extended_tx_core: identity.has_extended_tx_core(),
        };
        Ok(Identified { bus, rev3, chains })
    }

    /// The board temperature in °C.
    pub(crate) fn temperature(&mut self) -> Result<f32, Error> {
        Self::TEMP.celsius(self.bus.as_mut())
    }

    /// Moves the clocks' reference to `reference` (`usdr_set_extref`). Like libusdr, the
    /// sample clocks and synthesizers keep their settings until next set, which then use
    /// the new frequency; the on-board oscillator's enable is left as power-up set it.
    pub(crate) fn set_reference_clock(&mut self, reference: ReferenceClock) -> Result<(), Error> {
        let input = Self::si5332_reference(reference, self.radio.rev3);
        let mut switch = ClockSequence::reference_switch(Self::CLOCK.dev(), input);
        let switched = run_sequence(self.bus.as_mut(), &mut switch);
        // A missing input clock is reported only after every write, ACTIVE included: the
        // chip is on the new input, so later plans must use its frequency.
        if matches!(switched, Ok(()) | Err(Error::ClockInputMissing)) {
            self.reference = reference;
            self.radio.reference_hz = reference.hz();
        }
        switched
    }

    /// Writes the clock-measurement control (`dev_m2_lm6_1_sdr_clkmeas_set`).
    pub(crate) fn set_clock_measurement(&mut self, control: u8) -> Result<(), Error> {
        Gpo::ClockMeasurement.set(self.bus.as_mut(), control)
    }

    /// Reads the clock-measurement word (`dev_m2_lm6_1_sdr_clkmeas_get`).
    pub(crate) fn clock_measurement(&mut self) -> Result<u32, Error> {
        Gpi::ClockMeasurement.read(self.bus.as_mut())
    }

    /// Writes the ATCRBS DSP core's register.
    pub(crate) fn set_atcrbs_register(&mut self, value: u32) -> Result<(), Error> {
        fpga::write_atcrbs(self.bus.as_mut(), value)
    }

    /// Reads the ATCRBS DSP core's register.
    pub(crate) fn atcrbs_register(&mut self) -> Result<u32, Error> {
        fpga::read_atcrbs(self.bus.as_mut())
    }

    /// Reads the TX DMA engine's four status words.
    pub(crate) fn tx_dma_status(&mut self) -> Result<[u32; 4], Error> {
        fpga::read_tx_dma_status(self.bus.as_mut())
    }

    /// Reads the gateware build, then the hardware ID (`dev_m2_lm6_1_sdr_revision_get`).
    pub(crate) fn identity(&mut self) -> Result<Identity, Error> {
        Self::read_identity(self.bus.as_mut())
    }

    /// Runs the identity sequence over `bus`.
    fn read_identity(bus: &mut dyn Bus) -> Result<Identity, Error> {
        let mut read = IdentityRead::default();
        run_sequence(bus, &mut read)?;
        Ok(read
            .identity()
            .expect("invariant: a finished identity sequence read both words"))
    }

    /// Holds the RF transceiver in reset and turns off the LED and booster. Every step is
    /// attempted; the first failure is returned.
    pub(crate) fn power_down(mut self) -> Result<(), Error> {
        self.run_script(power_down())
    }
}

impl Identified {
    /// The board temperature in °C, before power-up.
    ///
    /// Works before power-up on revision 3: libusdr checks the TMP114's ID there before
    /// touching the PMIC, so the sensor is powered from reset. Revisions 1 and 2 get the same
    /// check from [`Identified::check_temperature_sensor`] before any thermal decision,
    /// failing closed if no sensor answers. Assumed, not verified: the sensor has completed a
    /// conversion by then (its result register reads 0 °C until the first one).
    pub(crate) fn temperature(&mut self) -> Result<f32, Error> {
        Board::TEMP.celsius(self.bus.as_mut())
    }

    /// Confirms a TMP114 answers before its readings are trusted. Revision 3 already
    /// checked it in [`Board::identify`]; libusdr never checks on revisions 1 and 2, so this
    /// is an addition there.
    pub(crate) fn check_temperature_sensor(&mut self) -> Result<(), Error> {
        if self.rev3 {
            return Ok(());
        }
        Board::TEMP.check_id(self.bus.as_mut())
    }

    /// Brings the board up from reset: rails, clocks, then the RF transceiver and its idle
    /// configuration ([`PowerUp`]).
    ///
    /// Failures after the clocks are programmed turn the LED, booster and RF chip back
    /// off, over the same span as libusdr's `fail:` path.
    pub(crate) fn power_up(self, reference: ReferenceClock) -> Result<Board, Error> {
        let Self {
            mut bus,
            rev3,
            chains,
        } = self;
        let plan = PowerUpPlan {
            rails: Board::RAILS,
            clock: Board::CLOCK.dev(),
            reference: Board::si5332_reference(reference, rev3),
            lvpecl: Board::lvpecl(rev3),
            rev3,
            // The on-board oscillator stays off only for an external reference.
            oscillator: !matches!(reference, ReferenceClock::External { .. }),
            chains,
            reference_hz: reference.hz(),
        };
        let mut power_up = Run::new(PowerUp::new(plan));
        run_sequence(bus.as_mut(), &mut power_up)
            .inspect_err(|error| warn!(%error, "power-up failed"))?;
        let radio = power_up
            .steps()
            .radio()
            .expect("invariant: a power-up without a failure configured the board");
        debug!(rev3, "board powered up");
        Ok(Board {
            bus,
            radio,
            corrections: Corrections::default(),
            reference,
            streams: None,
            front_end: None,
        })
    }
}

impl Thermometer for Identified {
    fn celsius(&mut self) -> Result<f32, Error> {
        self.temperature()
    }

    fn sleep(&mut self, duration: Duration) {
        self.bus.sleep(duration);
    }
}

impl Thermometer for Board {
    fn celsius(&mut self) -> Result<f32, Error> {
        self.temperature()
    }

    fn sleep(&mut self, duration: Duration) {
        self.bus.sleep(duration);
    }
}

impl fmt::Debug for Identified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identified")
            .field("rev3", &self.rev3)
            .finish_non_exhaustive()
    }
}
