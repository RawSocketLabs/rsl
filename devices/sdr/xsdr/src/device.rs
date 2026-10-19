//! The public device: identification, supply bring-up, RFIC power-up, sample rate, LO
//! tuning, gains, bandwidths and close; the other RF and baseband controls are
//! `controls`'.

use std::fmt;

mod controls;

pub use controls::{IqCorrection, LmlRxMode, TxGenerator};

use hal::{Sequence, run_sequence};
use lms7002m::ctrl::{Control, ControlError, Direction, Frontend, Gain, Operation};
use lms7002m::{Lms7002m, Synthesizer, TuneError};
use lp8758::{self, Revision};
use sdr_frontend::{FrontEnd, FrontEndKind};

use crate::board::{REFERENCE_HZ, XSDR_LMS, XSDR_SUPPLIES};
use crate::error::Error;
use crate::fe_probe;
use crate::lowlevel::Bus;
use crate::portable::close::Close;
use crate::portable::frontend::{RX_BANDS, TX_BANDS, XsdrFrontend};
use crate::portable::hwid::{IdentityRead, Variant};
use crate::portable::power::PowerUp;
use crate::portable::rate::{Gateware, RateError, RateState, SetSampleRate};
use crate::portable::supplies::Supplies;

/// The lowest sample rate libusdr's `/dm/rate` accepts (`RATE_MIN`).
const RATE_MIN: u32 = 90_000;
/// The highest (`RATE_MAX`).
const RATE_MAX: u32 = 195_000_000;

/// HWID PHY bit: a TX MMCM (`PHY_CFG_TX_MMCM`).
const PHY_TX_MMCM: u8 = 0x20;
/// HWID PHY bit: an RX MMCM (`PHY_CFG_RX_MMCM`).
const PHY_RX_MMCM: u8 = 0x10;
/// HWID PHY bit: one MMCM shared by RX and TX (`PHY_CFG_SINGLE_MMCM`).
const PHY_SINGLE_MMCM: u8 = 0x01;
/// HWID PHY bit: an FPGA DUC/DDC (`PHY_CFG_HAS_DUC_DDC`).
const PHY_DUC_DDC: u8 = 0x04;
/// HWID PHY bit: `LimeLight` port 2 carries RX (`PHY_CFG_LML2_IS_RX`).
const PHY_LML2_IS_RX: u8 = 0x40;
/// The PHY bits whose rate paths are not ported (see [`Error::UnsupportedGateware`]).
const PHY_UNPORTED: u8 = PHY_RX_MMCM | PHY_SINGLE_MMCM | PHY_DUC_DDC;

/// An open LMS7 family board. Only the XSDR opens so far.
pub struct Device {
    /// The board's bus.
    bus: Box<dyn Bus>,

    /// The hardware ID read at open.
    hwid: u32,

    /// The PMIC's revision, read at open.
    pmic: Revision,

    /// The LMS7002M control state, its chip cache zeroed until a power-up creates it, as
    /// in libusdr.
    control: Control,

    /// The RFIC is powered (`pwr_en`).
    powered: bool,

    /// The sample-rate state.
    rate: RateState,

    /// The RX and TX LOs last tuned, or 0 after a failure (`lms7_rxlo_last`,
    /// `lms7_txlo_last`).
    lo_last: [u32; 2],

    /// The front end the open's probe brought up, if any, with the state libusdr keeps
    /// for it.
    front_end: Option<FrontEnd>,
}

impl Device {
    /// Opens the board on `bus`, as libusdr's open does without an `fe=` hint: reads the
    /// hardware ID, pulses the LMS7002M power control, checks the LP8758's revision, sets
    /// the rails, turns on all four converters and selects the internal reference clock,
    /// then probes the front-end connector ([`Self::front_end`]). The RFIC stays unpowered
    /// ([`Self::power_up`]).
    ///
    /// # Errors
    ///
    /// [`Error::UnknownDevice`] or [`Error::UnsupportedVariant`] after the identification
    /// reads, before anything is driven; [`Error::Bus`] when an access fails. A front end
    /// that fails to come up is logged and skipped, as libusdr does.
    pub fn open(bus: impl Bus + 'static) -> Result<Self, Error> {
        let mut bus: Box<dyn Bus> = Box::new(bus);
        let mut identity = IdentityRead::new();
        run(&mut *bus, &mut identity)?;
        let hwid = identity.hwid();
        match Variant::from_hwid(hwid).map_err(Error::UnknownDevice)? {
            Variant::Xsdr => {}
            other => return Err(Error::UnsupportedVariant(other)),
        }

        let mut supplies = Supplies::new(XSDR_SUPPLIES);
        run(&mut *bus, &mut supplies)?;
        let front_end = fe_probe::probe(&mut *bus)?;
        Ok(Self {
            bus,
            hwid,
            pmic: supplies.pmic_revision(),
            control: Control::new(
                Lms7002m::new(XSDR_LMS.target),
                REFERENCE_HZ,
                RX_BANDS,
                TX_BANDS,
            ),
            powered: false,
            rate: RateState::default(),
            lo_last: [0; 2],
            front_end,
        })
    }

    /// The front end the open's probe brought up: on an empty connector, the M.2-to-`PCIe`
    /// breakout without a DAC, which libusdr cannot tell from no board.
    #[must_use]
    pub fn front_end(&self) -> Option<FrontEndKind> {
        self.front_end.as_ref().map(FrontEnd::kind)
    }

    /// Powers the LMS7002M up (libusdr's `/dm/power/en` = 1): its LDOs on, its reset
    /// released, its reset programme loaded and its version checked.
    ///
    /// # Errors
    ///
    /// [`Error::ChipId`] for an LP8758 whose revision is not 0xE001, the one libusdr
    /// accepts, before any bus access; [`Error::ChipId`] for an LMS7002M other than version 7,
    /// revision 1; [`Error::Bus`] when an access fails.
    pub fn power_up(&mut self) -> Result<(), Error> {
        if self.pmic != lp8758::REVISION {
            return Err(Error::ChipId {
                chip: lp8758::NAME,
                expected: lp8758::REVISION.to_raw().into(),
                found: self.pmic.to_raw().into(),
            });
        }
        let mut power = PowerUp::new(XSDR_LMS);
        run(&mut *self.bus, &mut power)?;
        if let Some(lms) = power.chip() {
            *self.control.chip_mut() = lms;
            self.powered = true;
        }
        Ok(())
    }

    /// Sets the RX and TX sample rates, in samples per second (libusdr's
    /// `/dm/rate/rxtxadcdac` with no converter floors; `/dm/rate/master` is both equal):
    /// the RFIC powered up first if it is not, the AFE enabled (first time), the
    /// LMS7002M's clock generator, dividers and `LimeLight` interface set, RX streamed up
    /// and the receive link's clock phase calibrated, then the processors' self-tests
    /// waited out. Above 72 MS/s on the first call the link is double-pumped; above
    /// 70 MS/s the I/O and core supplies are raised.
    ///
    /// Supported gateware has a TX MMCM and no RX MMCM, shared MMCM or DUC/DDC (HWID PHY
    /// bits). Without a TX MMCM libusdr, and so this, fails the calibration.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::InvalidRate`] for a rate outside 90 kS/s to
    /// 195 MS/s or none in either direction; [`Error::UnsupportedGateware`]. Then
    /// [`Error::UndeliverableRate`], [`Error::ClockUnlocked`],
    /// [`Error::LinkCalibration`], a power-up's errors, or [`Error::Bus`].
    pub fn set_sample_rate(&mut self, rx: u32, tx: u32) -> Result<(), Error> {
        let in_range = |rate: u32| rate == 0 || (RATE_MIN..=RATE_MAX).contains(&rate);
        if rx == 0 && tx == 0 {
            return Err(Error::InvalidRate(0));
        }
        if let Some(rate) = [rx, tx].into_iter().find(|&rate| !in_range(rate)) {
            return Err(Error::InvalidRate(rate));
        }
        let [.., phy] = self.hwid.to_be_bytes();
        if phy & PHY_UNPORTED != 0 {
            return Err(Error::UnsupportedGateware(phy));
        }
        if !self.powered {
            self.power_up()?;
        }
        let gateware = Gateware {
            tx_mmcm: phy & PHY_TX_MMCM != 0,
            rx_port1: phy & PHY_LML2_IS_RX == 0,
        };
        let mut rate = SetSampleRate::new(
            &self.control,
            self.rate,
            gateware,
            XSDR_SUPPLIES.pmic,
            (rx, tx),
        );
        let ran = run(&mut *self.bus, &mut rate);
        self.control = *rate.control();
        self.rate = rate.state();
        ran?;
        match rate.outcome() {
            Some(Err(RateError::Control(error))) => Err(control_error(error)),
            Some(Err(RateError::Calibration)) => Err(Error::LinkCalibration),
            Some(Ok(())) | None => Ok(()),
        }
    }

    /// Tunes the RX LO to `hertz` (libusdr's `/dm/sdr/0/rx/frequency`); 0 stops the
    /// synthesizer. While RX runs on the automatic path ([`Self::set_rx_path`]), the
    /// receive band and antenna switch follow the LO (below 2.2 GHz the wide-band LNA,
    /// above it the high-band one). Tuning to the LO last tuned does nothing, as in
    /// libusdr.
    ///
    /// # Errors
    ///
    /// [`Error::TuneOutOfRange`] when no VCO reaches `hertz` or none locks;
    /// [`Error::VcoComparatorFailed`]; [`Error::Bus`].
    pub fn set_rx_frequency(&mut self, hertz: u32) -> Result<(), Error> {
        self.tune(Synthesizer::Rx, hertz)
    }

    /// Tunes the TX LO to `hertz` (libusdr's `/dm/sdr/0/tx/frequency`); 0 stops the
    /// synthesizer. Tuning to the LO last tuned does nothing, as in libusdr.
    ///
    /// # Errors
    ///
    /// As [`Self::set_rx_frequency`].
    pub fn set_tx_frequency(&mut self, hertz: u32) -> Result<(), Error> {
        self.tune(Synthesizer::Tx, hertz)
    }

    /// `xsdr_rfic_fe_set_freq` on an XSDR: skipped when unchanged, otherwise tuned, the
    /// last LO kept only on success.
    fn tune(&mut self, synthesizer: Synthesizer, hertz: u32) -> Result<(), Error> {
        let index = usize::from(synthesizer == Synthesizer::Tx);
        if self.lo_last[index] == hertz {
            return Ok(());
        }
        let op = self.control.tune(XsdrFrontend, synthesizer, hertz);
        let result = self.run_control(op);
        self.lo_last[index] = if result.is_ok() { hertz } else { 0 };
        result.map_err(|error| match error {
            Error::TuneOutOfRange(_) => Error::TuneOutOfRange(hertz),
            other => other,
        })
    }

    /// Sets a gain on both channels (libusdr's `/dm/sdr/0/{rx,tx}/gain/*`), returning the
    /// gain reached in dB as libusdr reports it. libusdr's `/dm/sdr/0/rx/gain` and
    /// `.../gain/auto` are [`Gain::RxLna`]; `/dm/sdr/0/tx/gain` and its `auto` are
    /// [`Gain::TxPad`].
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_gain(&mut self, gain: Gain, value: i32) -> Result<i32, Error> {
        let (op, actual) = self.control.set_gain(gain, value);
        self.run_control(op)?;
        Ok(actual)
    }

    /// Sets the RX or TX baseband filter bandwidth on both channels, in hertz (libusdr's
    /// `/dm/sdr/0/{rx,tx}/bandwidth`); 0 lets the sample rate choose it again.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_bandwidth(&mut self, direction: Direction, hertz: u32) -> Result<(), Error> {
        let op = self.control.set_bandwidth(direction, hertz);
        self.run_control(op)
    }

    /// Runs a control operation, keeping the state it leaves even when it fails.
    fn run_control<F: Frontend>(&mut self, mut op: Operation<F>) -> Result<(), Error> {
        let ran = run(&mut *self.bus, &mut op);
        self.control = *op.control();
        ran?;
        match op.outcome() {
            Some(Err(error)) => Err(control_error(error)),
            Some(Ok(())) | None => Ok(()),
        }
    }

    /// Closes the board as libusdr's `usdr_dmd_close` does on an XSDR without streams
    /// (`xsdr_dtor`): the LMS7002M's receive and transmit paths brought down and the chip
    /// destroyed, its power control, LDOs and the LED off, the TX MMCM powered down.
    ///
    /// The chip writes follow its cached state, so a board never powered up gets the words
    /// libusdr sends from its zeroed state. A device dropped without `close` leaves the
    /// board as it is.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] for the first failure that ends the close, as libusdr's: the AFE write
    /// that ends streaming-down, a `lms7002m_destroy` write or a board write. Failed writes
    /// earlier in streaming-down are skipped without an error, as libusdr skips them.
    pub fn close(mut self) -> Result<(), Error> {
        run(&mut *self.bus, &mut Close::new(*self.control.chip()))
    }

    /// The hardware ID word: device ID in bits 23:16, gateware revision in 15:8, PHY
    /// features in 7:0.
    #[must_use]
    pub fn hwid(&self) -> u32 {
        self.hwid
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("hwid", &format_args!("{:#010x}", self.hwid))
            .field("pmic", &self.pmic)
            .field("powered", &self.powered)
            .field("front_end", &self.front_end)
            .finish_non_exhaustive()
    }
}

/// A control operation's refusal in the driver's terms.
fn control_error(error: ControlError) -> Error {
    match error {
        ControlError::NoChannel => Error::NoChannel,
        ControlError::Decimation => Error::UndeliverableRate,
        ControlError::Clock => Error::ClockUnlocked,
        ControlError::Tune(TuneError::ComparatorFailed) => Error::VcoComparatorFailed,
        // The frequency is filled in by the tuning that failed.
        ControlError::Tune(TuneError::OutOfRange) => Error::TuneOutOfRange(0),
        ControlError::NoRate => Error::NoSampleRate,
        ControlError::InvalidRate => Error::InvalidRate(0),
        ControlError::InvalidPath => Error::InvalidPath,
        // The frequency is filled in by the caller.
        ControlError::NcoOutOfRange => Error::NcoOutOfRange(0),
    }
}

/// Runs a portable sequence to its end over `bus`, with the failure in the driver's terms.
fn run(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_sequence(bus, sequence).map_err(Error::from)
}
