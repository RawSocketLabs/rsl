//! Power-up and power-down (`usdr_init` and `usdr_dtor` in `device/m2_lm6_1/usdr_ctrl.c`):
//! `Identified` before power, then `impl Board`.

use std::fmt;
use std::time::Duration;

use rsl_deps::tracing::{debug, warn};

use super::Board;
use super::board::{Chains, RxState, TxState};
use crate::chips::lms6002d::{Lms6002d, Lna, PowerAmp};
use crate::chips::register::run_sequence;
use crate::chips::si5332::{LvpeclOutput, Reference};
use crate::error::Error;
use crate::fpga::{self, Gpi, Gpo, IdentityRead};
use crate::identity::Identity;
use crate::lowlevel::Bus;
use crate::portable::power::PowerRails;
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

    /// Where the clocks' reference comes from; chosen at power-up.
    reference: ReferenceClock,
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
        };
        Ok(Identified {
            bus,
            rev3,
            chains,
            reference: ReferenceClock::Board,
        })
    }

    /// The board temperature in °C.
    pub(crate) fn temperature(&mut self) -> Result<f32, Error> {
        Self::TEMP.celsius(self.bus.as_mut())
    }

    /// Moves the clocks' reference to `reference` (`usdr_set_extref`). Like libusdr, the
    /// sample clocks and synthesizers keep their settings until next set, which then use
    /// the new frequency; the on-board oscillator's enable is left as power-up set it.
    pub(crate) fn set_reference_clock(&mut self, reference: ReferenceClock) -> Result<(), Error> {
        let input = Self::si5332_reference(reference, self.rev3);
        let mut switch = ClockSequence::reference_switch(Self::CLOCK.dev(), input);
        let switched = run_sequence(self.bus.as_mut(), &mut switch);
        // A missing input clock is reported only after every write, ACTIVE included: the
        // chip is on the new input, so later plans must use its frequency.
        if matches!(switched, Ok(()) | Err(Error::ClockInputMissing)) {
            self.reference = reference;
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
        let bus = self.bus.as_mut();
        let reset = Gpo::LmsReset.set(bus, 0);
        let led = Gpo::Led.set(bus, 0);
        let booster = Gpo::Booster.set(bus, 0);
        reset.and(led).and(booster)
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

    /// Brings the board up from reset: rails, clocks, then the RF transceiver.
    ///
    /// Failures after the clocks are programmed turn the LED, booster and RF chip back
    /// off, over the same span as libusdr's `fail:` path.
    pub(crate) fn power_up(mut self, reference: ReferenceClock) -> Result<Board, Error> {
        self.reference = reference;
        self.power_rails()?;
        self.bus.sleep(Duration::from_millis(10));

        let clocks = self.start_clocks();
        self.bus.sleep(Duration::from_millis(10));
        let lms = clocks
            .and_then(|()| self.release_rf())
            .inspect_err(|error| {
                warn!(%error, "power-up failed; turning the RF section back off");
                // Best effort: the original failure is the one to report.
                let bus = self.bus.as_mut();
                let _ = Gpo::Led.set(bus, 0);
                let _ = Gpo::LmsReset.set(bus, 0);
                let _ = Gpo::Booster.set(bus, 0);
            })?;
        self.configure_rf(lms)
    }

    /// Checks the PMIC and boost converter and brings up their rails ([`Board::RAILS`]),
    /// running the portable machine.
    fn power_rails(&mut self) -> Result<(), Error> {
        run_sequence(self.bus.as_mut(), &mut PowerRails::new(Board::RAILS))
    }

    /// Programs the clock generator; on revision 3, then starts the on-board oscillator.
    ///
    /// Revisions 1 and 2 take the reference from the clock input. On revision 3 the
    /// oscillator is enabled only after programming, so the Si5332 may report no input
    /// clock: libusdr discards `si5332_init`'s result there (it overwrites it with the
    /// oscillator GPO write's). We tolerate only that missing clock; a bus failure or an
    /// absent Si5332 still fails, a deliberate divergence.
    fn start_clocks(&mut self) -> Result<(), Error> {
        let bus = self.bus.as_mut();
        let lvpecl = Board::lvpecl(self.rev3);
        let input = Board::si5332_reference(self.reference, self.rev3);
        if !self.rev3 {
            return Self::init_clock(bus, input, lvpecl);
        }
        let clock = Self::init_clock(bus, input, lvpecl);
        // The on-board oscillator stays off only for an external reference.
        let external = matches!(self.reference, ReferenceClock::External { .. });
        let oscillator = Gpo::EnableOscillator.set(bus, u8::from(!external));
        bus.sleep(Duration::from_millis(1));
        match clock {
            Ok(()) => oscillator,
            Err(Error::ClockInputMissing) => {
                debug!("Si5332 reported no input before the oscillator started; tolerated");
                oscillator
            }
            Err(err) => Err(err),
        }
    }

    /// `si5332_init` with output 0 undivided: programs the clock generator, then waits for
    /// it to run.
    fn init_clock(
        bus: &mut dyn Bus,
        reference: Reference,
        lvpecl: LvpeclOutput,
    ) -> Result<(), Error> {
        let clock = Board::CLOCK.dev();
        run_sequence(
            bus,
            &mut ClockSequence::program(clock, 1, reference, lvpecl),
        )?;
        // libusdr reads FPGA register 0xC here and ignores the result; kept so traces line up.
        let _ = bus.read_regs(0xc, &mut [0]);
        run_sequence(bus, &mut ClockSequence::wait_active(clock))
    }

    /// Powers the RF section and releases the LMS6002D from reset.
    fn release_rf(&mut self) -> Result<Lms6002d, Error> {
        let bus = self.bus.as_mut();
        Gpo::Booster.set(bus, 1)?;
        Gpo::Led.set(bus, 1)?;
        Gpo::LmsReset.set(bus, 1)?;
        // libusdr reads the RF chip ID once here for its log, then again in create.
        let id = Lms6002d::read_id(bus, Lms6002d::USDR_TARGET)?;
        debug!(
            chip = Lms6002d::NAME,
            id = format_args!("{id:#04x}"),
            "RF transceiver out of reset"
        );
        bus.sleep(Duration::from_millis(1));
        Lms6002d::create(bus, Lms6002d::USDR_TARGET)
    }

    /// Puts the transceiver in its idle configuration (both chains off, wideband RX and TX
    /// paths, external mixer off, FPGA DC correction on) and hands the bus to the board.
    fn configure_rf(self, mut lms: Lms6002d) -> Result<Board, Error> {
        let Self {
            mut bus,
            rev3,
            chains,
            reference,
        } = self;
        let io = bus.as_mut();
        lms.set_tx_enabled(io, false)?;
        lms.set_rx_enabled(io, false)?;
        Gpo::RxMixerEnable.set(io, 0)?;
        // `usdr_set_rx_port_switch` for LNAW: switch 0, mixer off.
        Gpo::RxSwitch.set(io, 0)?;
        Gpo::RxMixerEnable.set(io, 0)?;
        Gpo::TxSwitch.set(io, 1)?;
        lms.select_lna(io, Lna::Lna1)?;
        lms.select_pa(io, PowerAmp::Pa1)?;
        Gpo::DcCorrection.set(io, 1)?;
        debug!(rev3, "board powered up");
        Ok(Board {
            bus,
            lms,
            rev3,
            chains,
            reference,
            vio_boosted: false,
            adc_rate_hz: 0,
            mixer_lo_hz: 0,
            rx: RxState::default(),
            tx: TxState::default(),
            rx_running: false,
            rx_stream: None,
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
