//! The M.2-to-`PCIe` breakout (libusdr's `exm2pe`, `device/ext_exm2pe/board_exm2pe.c`): a
//! carrier with a GPS module and a reference oscillator trimmed by an optional DAC.
//!
//! The board has no ID to read. libusdr's bring-up succeeds whether or not its DAC answers,
//! so on a device whose other probes fail, libusdr reports this board even when nothing is
//! attached: "M.2-to-`PCIe` breakout without a DAC" is indistinguishable from no front end.
//! Its bring-up is therefore all of its probe.

use std::time::Duration;

use rsl_lowlevel::{Bus, I2cAddr};

use crate::FRONT_END_BUS;
use crate::chips::dac80501::{Dac80501, Identity};
use crate::error::Error;
use crate::gpio::{FrontEndGpio, Pin, PinMode};

/// The oscillator-trim DAC (`I2C_ADDR_DAC`).
const DAC: Dac80501 = Dac80501::at(I2cAddr::new(FRONT_END_BUS, 0x48));

/// The connector's I2C data line (`GPIO_SDA`).
const SDA: Pin = 0;
/// The connector's I2C clock line (`GPIO_SCL`).
const SCL: Pin = 1;
/// The 1PPS input (`GPIO_1PPS`).
const PPS: Pin = 2;
/// The GPS UART's transmit line (`GPIO_UART_TX`).
const UART_TX: Pin = 3;
/// The GPS UART's receive line (`GPIO_UART_RX`).
const UART_RX: Pin = 4;
/// The reference oscillator's enable (`GPIO_EN_OSC`).
const EN_OSC: Pin = 5;
/// The GPS module's enable (`GPIO_EN_GPS`).
const EN_GPS: Pin = 6;

/// Bring-up options for the M.2-to-`PCIe` breakout: libusdr's `fe=exm2pe...` options,
/// typed. Unset options keep libusdr's defaults.
///
/// libusdr rejects `fe=exm2pe` with no options at all (its parser takes the empty remainder
/// for an unrecognised option); [`Exm2peOptions::default`] behaves as a recognised option
/// that changes nothing would. The `uart_` option, which only logs what the GPS sends, is
/// not ported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Exm2peOptions {
    /// GPS module power (`gps_`); off by default.
    pub gps: Option<bool>,

    /// Reference oscillator power (`osc_`); off by default.
    pub oscillator: Option<bool>,

    /// Oscillator DAC code (`dac_`); ignored when no DAC answers, as in libusdr.
    pub dac: Option<u16>,
}

/// An attached M.2-to-`PCIe` breakout (`board_exm2pe_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exm2pe {
    /// Its DAC identified itself (`dac_present`).
    dac: bool,
}

impl Exm2pe {
    /// Brings up the board (`board_exm2pe_init`). It powers the oscillator and GPS to look
    /// for the DAC, then leaves them as `options` say (off by default). It always succeeds
    /// unless the bus fails: libusdr ignores every other failure.
    pub(crate) fn init(bus: &mut dyn Bus, options: Option<&Exm2peOptions>) -> Result<Self, Error> {
        let options = options.copied().unwrap_or_default();
        FrontEndGpio::configure(bus, SDA, PinMode::Alternate0)?;
        FrontEndGpio::configure(bus, SCL, PinMode::Alternate0)?;
        FrontEndGpio::configure(bus, EN_OSC, PinMode::Output)?;
        FrontEndGpio::configure(bus, EN_GPS, PinMode::Output)?;
        let enables = 1 << EN_OSC | 1 << EN_GPS;
        FrontEndGpio::drive(bus, enables, enables)?;
        bus.sleep(Duration::from_millis(10));

        let board = Self {
            dac: DAC.init(bus)? == Identity::Known,
        };
        if let Some(code) = options.dac {
            // libusdr's `-EACCES` without a DAC is discarded here.
            let _ = board.set_dac(bus, code);
        }

        FrontEndGpio::configure(bus, PPS, PinMode::Alternate0)?;
        FrontEndGpio::configure(bus, UART_TX, PinMode::Alternate0)?;
        FrontEndGpio::configure(bus, UART_RX, PinMode::Alternate0)?;
        let levels = u16::from(options.oscillator.unwrap_or(false)) << EN_OSC
            | u16::from(options.gps.unwrap_or(false)) << EN_GPS;
        FrontEndGpio::drive(bus, enables, levels)?;
        Ok(board)
    }

    /// Sets the oscillator DAC's code (`board_exm2pe_set_dac`).
    ///
    /// # Errors
    ///
    /// [`Error::NoOscillatorDac`] if the board's DAC did not identify itself at bring-up
    /// (libusdr's `-EACCES`); any bus failure.
    pub fn set_dac(self, bus: &mut dyn Bus, code: u16) -> Result<(), Error> {
        if !self.dac {
            return Err(Error::NoOscillatorDac);
        }
        DAC.set_code(bus, code)
    }

    /// Whether the board's DAC identified itself.
    #[must_use]
    pub const fn has_dac(self) -> bool {
        self.dac
    }
}
