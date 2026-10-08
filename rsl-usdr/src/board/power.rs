//! Power-up and power-down (`usdr_init` and `usdr_dtor` in `device/m2_lm6_1/usdr_ctrl.c`).

use std::time::Duration;

use crate::chips::lms6002d::{Lms6002d, RxPath, TxPath};
use crate::chips::lp8758::{Buck, Lp8758};
use crate::chips::si5332::{LvpeclOutput, Reference, Si5332};
use crate::chips::tmp114::Tmp114;
use crate::chips::tps6381x::{Tps6381x, Vout};
use crate::error::{BusContext, Error};
use crate::fpga::{Gpi, Gpo, Hwid};
use crate::lowlevel::{Bus, I2cAddr, SpiAddr};

/// The LP8758 PMIC.
const PMIC: Lp8758 = Lp8758::at(I2cAddr { bus: 0, addr: 0x60 });
/// The TMP114 temperature sensor (revision 3).
const TEMP: Tmp114 = Tmp114::at(I2cAddr { bus: 0, addr: 0x4e });
/// The Si5332 clock generator.
const CLOCK: Si5332 = Si5332::at(I2cAddr { bus: 0, addr: 0x6a });
/// The TPS63811 boost converter.
const BOOST: Tps6381x = Tps6381x::at(I2cAddr { bus: 0, addr: 0x75 });
/// The LMS6002D.
const SPI_LMS6: SpiAddr = SpiAddr(0);

/// The PMIC revision libusdr accepts.
const PMIC_REVISION: u16 = 0xe001;
/// GPIO bank voltage, compatible with xSDR.
const VGPIO_MV: u32 = 1800;
/// LMS6002D I/O rail at normal sample rates.
const LMS_VIO_NORMAL_MV: u32 = 1800;
/// Boost converter output, 3.45 V.
const BOOST_VOUT: Vout = Vout::from_millivolts(3450);

/// A powered uSDR board.
#[derive(Debug)]
pub(crate) struct Board {
    /// The RF transceiver.
    #[expect(
        dead_code,
        reason = "tuning and bandwidth, the next ported operations, use it"
    )]
    lms: Lms6002d,
}

impl Board {
    /// Brings the board up from reset: rails, clocks, then the RF transceiver.
    ///
    /// Failures after the clocks are programmed turn the LED, booster and RF chip back
    /// off, over the same span as libusdr's `fail:` path.
    pub(crate) fn power_up(bus: &mut dyn Bus) -> Result<Self, Error> {
        // Gateware build ID: libusdr only logs it.
        Gpi::UsrAccess2.read(bus)?;
        let revision = Hwid::from_raw(Gpi::Hwid.read(bus)?).revision();
        if !(1..=3).contains(&revision) {
            return Err(Error::UnsupportedRevision(revision));
        }
        let rev3 = revision == 3;

        Gpo::LmsReset.set(bus, 0)?;
        if rev3 {
            TEMP.check_id(bus)?;
        }
        Self::power_rails(bus)?;
        bus.sleep(Duration::from_millis(10));

        let clocks = Self::start_clocks(bus, rev3);
        bus.sleep(Duration::from_millis(10));
        let lms = clocks
            .and_then(|()| Self::release_rf(bus))
            .inspect_err(|_| {
                // Best effort: the original failure is the one to report.
                let _ = Gpo::Led.set(bus, 0);
                let _ = Gpo::LmsReset.set(bus, 0);
                let _ = Gpo::Booster.set(bus, 0);
            })?;
        Self::configure_rf(bus, lms)
    }

    /// Checks the PMIC and boost converter and brings up their rails.
    fn power_rails(bus: &mut dyn Bus) -> Result<(), Error> {
        let revision = PMIC.revision(bus)?;
        if revision != PMIC_REVISION {
            return Err(Error::ChipId {
                chip: "LP8758",
                expected: PMIC_REVISION.into(),
                found: revision.into(),
            });
        }
        PMIC.set_soft_start(bus, false)?;
        PMIC.set_vout(bus, Buck::B1, VGPIO_MV)?;
        PMIC.set_vout(bus, Buck::B3, LMS_VIO_NORMAL_MV)?;
        // 1.0 V, 2.5 V, 1.2 V and 1.8 V rails, forced PWM.
        for buck in Buck::ALL {
            PMIC.enable(bus, buck, true)?;
        }
        BOOST.init(bus, true, BOOST_VOUT)
    }

    /// Programs the clock generator; on revision 3, then starts the on-board oscillator.
    ///
    /// Revisions 1 and 2 take the reference from the clock input. On revision 3 the
    /// oscillator is enabled only after programming, so the Si5332 may report no input
    /// clock: libusdr discards `si5332_init`'s result there (it overwrites it with the
    /// oscillator GPO write's). We tolerate only that missing clock; a bus failure or an
    /// absent Si5332 still fails, a deliberate divergence.
    fn start_clocks(bus: &mut dyn Bus, rev3: bool) -> Result<(), Error> {
        if !rev3 {
            return CLOCK.init(bus, 1, Reference::Input2, LvpeclOutput::Out1);
        }
        let clock = CLOCK.init(bus, 1, Reference::Oscillator, LvpeclOutput::Out0);
        let oscillator = Gpo::EnableOscillator.set(bus, 1);
        bus.sleep(Duration::from_millis(1));
        match clock {
            Ok(()) | Err(Error::ClockInputMissing) => oscillator,
            Err(err) => Err(err),
        }
    }

    /// Powers the RF section and releases the LMS6002D from reset.
    fn release_rf(bus: &mut dyn Bus) -> Result<Lms6002d, Error> {
        Gpo::Booster.set(bus, 1)?;
        Gpo::Led.set(bus, 1)?;
        Gpo::LmsReset.set(bus, 1)?;
        // libusdr reads the RF chip ID once here for its log, then again in create.
        bus.spi32(SPI_LMS6, 0x0400).during("LMS6002D ID read")?;
        bus.sleep(Duration::from_millis(1));
        Lms6002d::create(bus, SPI_LMS6)
    }

    /// Puts the transceiver in its idle configuration: both chains off, wideband RX and
    /// TX paths, external mixer off, FPGA DC correction on.
    fn configure_rf(bus: &mut dyn Bus, mut lms: Lms6002d) -> Result<Self, Error> {
        lms.set_tx_enabled(bus, false)?;
        lms.set_rx_enabled(bus, false)?;
        Gpo::RxMixerEnable.set(bus, 0)?;
        // `usdr_set_rx_port_switch` for LNAW: switch 0, mixer off.
        Gpo::RxSwitch.set(bus, 0)?;
        Gpo::RxMixerEnable.set(bus, 0)?;
        Gpo::TxSwitch.set(bus, 1)?;
        lms.set_rx_path(bus, RxPath::Lna1)?;
        lms.set_tx_path(bus, TxPath::Pa1)?;
        Gpo::DcCorrection.set(bus, 1)?;
        Ok(Self { lms })
    }

    /// Holds the RF transceiver in reset and turns off the LED and booster. Every step is
    /// attempted; the first failure is returned.
    #[expect(
        clippy::unused_self,
        reason = "consumes the board: nothing may drive it once powered down"
    )]
    pub(crate) fn power_down(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let reset = Gpo::LmsReset.set(bus, 0);
        let led = Gpo::Led.set(bus, 0);
        let booster = Gpo::Booster.set(bus, 0);
        reset.and(led).and(booster)
    }
}
