//! The RF and baseband controls beyond rate, tuning, gain and bandwidth: explicit paths and
//! the RF loopback, TDD tuning, the baseband NCOs, the DC and I/Q corrections, the TX test
//! generator and mute, the baseband loopback and the `LimeLight` receive source. Each is
//! one libusdr device-manager path (`m2_lm7_1.c`'s handlers, `xsdr_ctrl.c`).

use std::ops::RangeInclusive;

use bnb::u3;
use lms7002m::ctrl::{ChipCall, ChipCalls, Direction, RfPath, RxRoute, TxRoute};
use lms7002m::{DcDac, Mac, RbbMode, RbbPath, TbbMode, TbbPath, Tsp, TspGenerator, TxMode, TxPath};

use super::{Device, run};
use crate::error::Error;
use crate::portable::frontend::XsdrFrontend;

/// The DC offsets taken: those whose low 11 bits, all libusdr writes (`v & 0x7ff`), read
/// back as the same value in two's complement. The DACs' own encoding is not in the pinned
/// source.
const DC_RANGE: RangeInclusive<i16> = -1024..=1023;
/// The gain corrections' largest code (11 bits).
const GAIN_MAX: u16 = 2047;
/// The phase correction's range: 12-bit two's complement.
const PHASE_RANGE: RangeInclusive<i16> = -2048..=2047;

/// An I/Q imbalance correction (`/dm/sdr/0/{rx,tx}/phgaincorr`): gain codes for I and Q
/// (0 to 2047; both 0 bypasses the gain stage) and a phase code (−2048 to 2047; 0
/// bypasses the phase stage).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IqCorrection {
    /// I's gain code.
    pub gain_i: u16,

    /// Q's gain code.
    pub gain_q: u16,

    /// The phase code.
    pub phase: i16,
}

/// What the TX processor transmits (`/dm/sdr/0/tfe/generator/*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxGenerator {
    /// The host's samples (`generator/enable`).
    Off,

    /// A constant I/Q level from the test generator (`generator/const`); a zero I selects
    /// its full scale, a zero Q its /4 frequency, as libusdr writes them.
    Dc {
        /// The I level.
        i: i16,

        /// The Q level.
        q: i16,
    },
}

/// What the `LimeLight` receive FIFO carries (`/debug/hw/lms7002m/0/rxlml`,
/// `XSDR_LMLRX_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LmlRxMode {
    /// The RX processor's samples (`XSDR_LMLRX_NORMAL`).
    Normal,

    /// The TX FIFO's samples, a digital loopback (`XSDR_LMLRX_DIGLOOPBACK`).
    DigitalLoopback,

    /// The LFSR test pattern (`XSDR_LMLRX_LFSR`).
    Lfsr,
}

impl Device {
    /// Selects the receive path (libusdr's `/dm/sdr/0/rx/path`, `lms7002m_rfe_set_path`):
    /// an LNA, an LNA in RF loopback from the TX band that feeds it, or the automatic band
    /// that follows the RX LO. The antenna switch follows; entering or leaving the
    /// loopback also sets the TX pad's loss and front end and rewrites the RX gains.
    ///
    /// # Errors
    ///
    /// [`Error::NoChannel`] before a power-up; [`Error::Bus`].
    pub fn set_rx_path(&mut self, route: RxRoute) -> Result<(), Error> {
        let op = self.control.set_rx_path(XsdrFrontend, route);
        self.run_control(op)
    }

    /// Selects the transmit path (libusdr's `/dm/sdr/0/tx/path`, `lms7002m_tfe_set_path`):
    /// band 1 (also named W) or band 2 (H), or the automatic band, which also ends the RF
    /// loopback. The antenna switch follows.
    ///
    /// # Errors
    ///
    /// [`Error::NoChannel`] before a power-up; [`Error::Bus`].
    pub fn set_tx_path(&mut self, route: TxRoute) -> Result<(), Error> {
        let op = self.control.set_tx_path(XsdrFrontend, route);
        self.run_control(op)
    }

    /// Selects a path on both channels (libusdr's `/dm/sdr/0/{rx,tx}/rfic_path`,
    /// `lms7002m_fe_set_lna`): both banks selected first, then as [`Self::set_tx_path`] for
    /// TX bands 1 and 2 and the automatic TX path and [`Self::set_rx_path`] for RX paths.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidPath`] for the W and H TX paths, which libusdr hands to the RX
    /// selection, after the bank selection; otherwise as those.
    pub fn set_rf_path(&mut self, path: RfPath) -> Result<(), Error> {
        let op = self.control.set_rf_path(XsdrFrontend, Mac::Both, path);
        self.run_control(op)
    }

    /// Shifts the baseband on both channels by `hertz` with the LMS7002M's NCO (libusdr's
    /// `/dm/sdr/0/{rx,tx}/frequency/bb`, `lms7002m_bb_set_freq`); a running direction's
    /// mixer is set at once, and streaming up restores it. 0 bypasses the mixer.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::NoSampleRate`] before a sample rate (libusdr divides
    /// by zero); [`Error::NcoOutOfRange`] beyond half the converter clock. Then
    /// [`Error::Bus`].
    pub fn set_baseband_frequency(
        &mut self,
        direction: Direction,
        hertz: i32,
    ) -> Result<(), Error> {
        let op = self.control.set_nco(direction, Mac::Both, hertz);
        self.run_control(op).map_err(|error| match error {
            Error::NcoOutOfRange(_) => Error::NcoOutOfRange(hertz),
            other => other,
        })
    }

    /// Tunes RX and TX to one LO for TDD (libusdr's `/dm/sdr/0/tdd/frequency`): the RX
    /// synthesizer stopped and the TX one locked to `hertz` and shared, then each running
    /// direction's automatic band following it; 0 stops the TX synthesizer.
    ///
    /// # Errors
    ///
    /// As [`Self::set_rx_frequency`].
    pub fn set_tdd_frequency(&mut self, hertz: u32) -> Result<(), Error> {
        let op = self.control.tune_tdd(XsdrFrontend, hertz);
        self.run_control(op).map_err(|error| match error {
            Error::TuneOutOfRange(_) => Error::TuneOutOfRange(hertz),
            other => other,
        })
    }

    /// Sets the TX processor's mixer on `channels` to the raw 32-bit frequency word `word`
    /// (libusdr's `/dm/sdr/0/tfe/nco/frequency`): a fraction of the DAC clock, negative
    /// down-converting, 0 bypassing it. Not stored: streaming up restores
    /// [`Self::set_baseband_frequency`]'s setting instead.
    ///
    /// # Errors
    ///
    /// [`Error::NoChannel`] for [`Mac::None`], after selecting no bank as libusdr does;
    /// [`Error::Bus`].
    pub fn set_tx_nco_word(&mut self, channels: Mac, word: i32) -> Result<(), Error> {
        self.run_chip(&[ChipCall::MacSet(channels), ChipCall::TspCmix(Tsp::Tx, word)])
    }

    /// Sets the analog DC-offset DACs of `direction` on `channels` (libusdr's
    /// `/dm/sdr/0/{rx,tx}/dccorr`): I then Q per channel, A first. Not banked.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::NoChannel`] for [`Mac::None`];
    /// [`Error::InvalidCorrection`] outside −1024 to 1023 (libusdr masks to 11 bits). Then
    /// [`Error::Bus`].
    pub fn set_dc_offset(
        &mut self,
        direction: Direction,
        channels: Mac,
        i: i16,
        q: i16,
    ) -> Result<(), Error> {
        if channels == Mac::None {
            return Err(Error::NoChannel);
        }
        if !DC_RANGE.contains(&i) || !DC_RANGE.contains(&q) {
            return Err(Error::InvalidCorrection);
        }
        let dacs = match direction {
            Direction::Rx => [DcDac::RxAI, DcDac::RxAQ, DcDac::RxBI, DcDac::RxBQ],
            Direction::Tx => [DcDac::TxAI, DcDac::TxAQ, DcDac::TxBI, DcDac::TxBQ],
        };
        let [a_i, a_q, b_i, b_q] = dacs;
        let mut calls = Vec::with_capacity(4);
        if matches!(channels, Mac::A | Mac::Both) {
            calls.extend([ChipCall::DcDac(a_i, i), ChipCall::DcDac(a_q, q)]);
        }
        if matches!(channels, Mac::B | Mac::Both) {
            calls.extend([ChipCall::DcDac(b_i, i), ChipCall::DcDac(b_q, q)]);
        }
        self.run_chip(&calls)
    }

    /// Bypasses the RX processor's DC corrector on `channels` (libusdr's
    /// `/dm/sdr/0/rx/dccorrmode`, which does so for any value).
    ///
    /// # Errors
    ///
    /// [`Error::NoChannel`] for [`Mac::None`], after selecting no bank as libusdr does;
    /// [`Error::Bus`].
    pub fn disable_rx_dc_corrector(&mut self, channels: Mac) -> Result<(), Error> {
        self.run_chip(&[
            ChipCall::MacSet(channels),
            ChipCall::RxtspDcCorr(true, u3::new(0)),
        ])
    }

    /// Sets the I/Q imbalance correction of `direction`'s processor on `channels`
    /// (libusdr's `/dm/sdr/0/{rx,tx}/phgaincorr`): the gains, then the phase.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidCorrection`] for codes out of range (libusdr masks them), before any
    /// bus access; [`Error::NoChannel`] for [`Mac::None`], after selecting no bank as
    /// libusdr does; [`Error::Bus`].
    pub fn set_iq_correction(
        &mut self,
        direction: Direction,
        channels: Mac,
        correction: IqCorrection,
    ) -> Result<(), Error> {
        let IqCorrection {
            gain_i,
            gain_q,
            phase,
        } = correction;
        if gain_i > GAIN_MAX || gain_q > GAIN_MAX || !PHASE_RANGE.contains(&phase) {
            return Err(Error::InvalidCorrection);
        }
        let tsp = tsp(direction);
        self.run_chip(&[
            ChipCall::MacSet(channels),
            ChipCall::TspGainCorrection(tsp, gain_i, gain_q),
            ChipCall::TspPhaseCorrection(tsp, i32::from(phase)),
        ])
    }

    /// Selects what the TX processor transmits on `channels` (libusdr's
    /// `/dm/sdr/0/tfe/generator/{enable,const}`).
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_tx_generator(&mut self, channels: Mac, generator: TxGenerator) -> Result<(), Error> {
        let (kind, i, q) = match generator {
            TxGenerator::Off => (TspGenerator::Normal, 0, 0),
            TxGenerator::Dc { i, q } => (TspGenerator::Dc, i, q),
        };
        self.run_chip(&[
            ChipCall::MacSet(channels),
            ChipCall::TspGenerator(Tsp::Tx, kind, i, q),
        ])
    }

    /// Loops the TX baseband back into the RX baseband on both channels (libusdr's
    /// `/dm/sdr/0/bb/loopback`, `xsdr_bb_loopback`): the TX ladder filter's output fed to
    /// the RX PGA, its filters bypassed.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn loop_baseband(&mut self) -> Result<(), Error> {
        self.run_chip(&[
            ChipCall::MacSet(Mac::Both),
            ChipCall::TbbPath(TbbPath::Ladder, TbbMode::Loopback),
            ChipCall::RbbPath(RbbPath::Bypass, RbbMode::Loopback),
        ])
    }

    /// Mutes the TX outputs of `channels` (libusdr's `/dm/sdr/0/tfe/antcfg`,
    /// `xsdr_tx_antennat_port_cfg`: bit 1 mutes A, bit 0 B): each channel's front end on
    /// with no band selected, A first. A later path or LO change selects a band again.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn mute_tx(&mut self, channels: Mac) -> Result<(), Error> {
        let mut calls = Vec::with_capacity(4);
        for bank in [Mac::A, Mac::B] {
            if channels == bank || channels == Mac::Both {
                calls.extend([
                    ChipCall::MacSet(bank),
                    ChipCall::TrfPath(TxPath::Mute, TxMode::Normal),
                ]);
            }
        }
        self.run_chip(&calls)
    }

    /// Selects what the `LimeLight` receive FIFO carries (libusdr's
    /// `/debug/hw/lms7002m/0/rxlml`, `lms7002m_set_lmlrx_mode`): the RX samples, the TX
    /// samples looped back digitally, or the LFSR pattern.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_lml_rx_mode(&mut self, mode: LmlRxMode) -> Result<(), Error> {
        let op = self
            .control
            .set_lml_rx_mode(mode == LmlRxMode::Lfsr, mode == LmlRxMode::DigitalLoopback);
        self.run_control(op)
    }

    /// Runs chip operations on the control state's chip, keeping the cache they leave.
    fn run_chip(&mut self, calls: &[ChipCall]) -> Result<(), Error> {
        let mut chip = ChipCalls::new(*self.control.chip(), calls)
            .expect("invariant: every handler runs at most four chip calls");
        let ran = run(&mut *self.bus, &mut chip);
        *self.control.chip_mut() = *chip.chip();
        ran?;
        if chip.refused() {
            return Err(Error::NoChannel);
        }
        Ok(())
    }
}

/// The processor of `direction`.
const fn tsp(direction: Direction) -> Tsp {
    match direction {
        Direction::Rx => Tsp::Rx,
        Direction::Tx => Tsp::Tx,
    }
}
