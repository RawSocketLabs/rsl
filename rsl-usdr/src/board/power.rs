//! Power-up and power-down (`usdr_init` and `usdr_dtor` in `device/m2_lm6_1/usdr_ctrl.c`):
//! `Identified` before power, then `impl Board`.

use std::fmt;
use std::time::Duration;

use super::Board;
use crate::chips::lms6002d::{Lms6002d, Lna, PowerAmp};
use crate::chips::lp8758::{Buck, BuckControl, Config};
use crate::chips::si5332::{LvpeclOutput, Reference};
use crate::error::Error;
use crate::fpga::{Gpi, Gpo, Hwid};
use crate::lowlevel::Bus;
use crate::thermal::Thermometer;

/// A board identified and held in reset, before anything is powered: where the thermal
/// policy decides whether to continue. Owns the bus until power-up hands it to [`Board`].
pub(crate) struct Identified {
    /// The hardware seam.
    bus: Box<dyn Bus>,

    /// Revision 3 (on-board oscillator, TMP114 ID check).
    rev3: bool,
}

impl Board {
    /// Reads the hardware ID and holds the RF transceiver in reset; on revision 3, also
    /// checks the TMP114. Powers nothing.
    pub(crate) fn identify(mut bus: Box<dyn Bus>) -> Result<Identified, Error> {
        // Gateware build ID: libusdr only logs it.
        Gpi::UsrAccess2.read(bus.as_mut())?;
        let revision = Hwid::from_raw(Gpi::Hwid.read(bus.as_mut())?).revision();
        if !(1..=3).contains(&revision) {
            return Err(Error::UnsupportedRevision(revision));
        }
        let rev3 = revision == 3;
        Gpo::LmsReset.set(bus.as_mut(), 0)?;
        if rev3 {
            Self::TEMP.check_id(bus.as_mut())?;
        }
        Ok(Identified { bus, rev3 })
    }

    /// The board temperature in °C.
    pub(crate) fn temperature(&mut self) -> Result<f32, Error> {
        Self::TEMP.celsius(self.bus.as_mut())
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
    pub(crate) fn power_up(mut self) -> Result<Board, Error> {
        self.power_rails()?;
        self.bus.sleep(Duration::from_millis(10));

        let clocks = self.start_clocks();
        self.bus.sleep(Duration::from_millis(10));
        let lms = clocks.and_then(|()| self.release_rf()).inspect_err(|_| {
            // Best effort: the original failure is the one to report.
            let bus = self.bus.as_mut();
            let _ = Gpo::Led.set(bus, 0);
            let _ = Gpo::LmsReset.set(bus, 0);
            let _ = Gpo::Booster.set(bus, 0);
        })?;
        self.configure_rf(lms)
    }

    /// Checks the PMIC and boost converter and brings up their rails.
    fn power_rails(&mut self) -> Result<(), Error> {
        let bus = self.bus.as_mut();
        Board::PMIC.check_revision(bus)?;
        // libusdr's `lp8758_ss(0)`: spread spectrum off; this byte also sets the 105 °C
        // die-temperature warning and drops the EN-pin pull-downs.
        Board::PMIC.configure(bus, Config::new().with_warn_at_105c(true))?;
        Board::PMIC.set_voltage(bus, Buck::B1, Board::VGPIO)?;
        Board::PMIC.set_voltage(bus, Buck::B3, Board::LMS_VIO_NORMAL)?;
        // All four channels on, forced PWM.
        for buck in Buck::ALL {
            let on = BuckControl::new()
                .with_enabled(true)
                .with_discharge_when_off(true);
            Board::PMIC.control(bus, buck, on.with_forced_pwm(true))?;
        }
        Board::BOOST.init(bus, true, Board::BOOST_VOLTAGE)
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
        if !self.rev3 {
            return Board::CLOCK.init(bus, 1, Reference::Input2, LvpeclOutput::Out1);
        }
        let clock = Board::CLOCK.init(bus, 1, Reference::Oscillator, LvpeclOutput::Out0);
        let oscillator = Gpo::EnableOscillator.set(bus, 1);
        bus.sleep(Duration::from_millis(1));
        match clock {
            Ok(()) | Err(Error::ClockInputMissing) => oscillator,
            Err(err) => Err(err),
        }
    }

    /// Powers the RF section and releases the LMS6002D from reset.
    fn release_rf(&mut self) -> Result<Lms6002d, Error> {
        let bus = self.bus.as_mut();
        Gpo::Booster.set(bus, 1)?;
        Gpo::Led.set(bus, 1)?;
        Gpo::LmsReset.set(bus, 1)?;
        // libusdr reads the RF chip ID once here for its log, then again in create.
        Lms6002d::read_id(bus, Lms6002d::USDR_TARGET)?;
        bus.sleep(Duration::from_millis(1));
        Lms6002d::create(bus, Lms6002d::USDR_TARGET)
    }

    /// Puts the transceiver in its idle configuration (both chains off, wideband RX and TX
    /// paths, external mixer off, FPGA DC correction on) and hands the bus to the board.
    fn configure_rf(self, mut lms: Lms6002d) -> Result<Board, Error> {
        let Self { mut bus, .. } = self;
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
        Ok(Board { bus, lms })
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
