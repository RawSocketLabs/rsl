//! The public device: open, power-up, sample rate, LO tuning, gains, bandwidths, stream
//! preparation and close.

use std::fmt;

use hal::{Sequence, SequenceAction};
use lms7002m::ctrl::{Control, ControlError, Direction, Frontend, Operation};
use lms7002m::{Synthesizer, TuneError};

use crate::control::ControlPort;
use crate::error::{Error, ExchangeError};
use crate::gain::Gain;
use crate::lms64c::Frame;
use crate::portable::frontend::LimeFrontend;
use crate::portable::lsop::{BadReply, LsOp};
use crate::portable::open::{Action, Open};
use crate::portable::rate::SetSampleRate;
use crate::portable::stream::PrepareStreaming;

/// What libusdr reports at `/dm/revision` for this family: an emulated constant, the same
/// for every board (`dev_limesdr_sdr_revision_get`, which documents none of its fields).
const REVISION: u64 = (2 << 8) << 32 | (1 << 27 | 1 << 23 | 13 << 17 | 1 << 12 | 1 << 6 | 1);

/// The lowest sample rate libusdr accepts on this family (`RATE_MIN`).
const RATE_MIN: u32 = 90_000;
/// The highest (`RATE_MAX`, `2*61440000`).
const RATE_MAX: u32 = 122_880_000;

/// An open LimeSDR-mini.
pub struct Device {
    /// The board's control channel.
    port: Box<dyn ControlPort>,

    /// The nominal reference frequency measured at open, in hertz.
    reference_hz: u32,

    /// The LMS7002M control state `limesdr_init` leaves, then each operation's, kept also
    /// when one fails, as libusdr keeps its.
    control: Control,
}

impl Device {
    /// Opens the board on `port`, as libusdr's `limesdr_init` does: measures the reference
    /// clock, pulses the LMS7002M's reset, checks its version, loads its reset programme
    /// and stops streaming.
    ///
    /// # Errors
    ///
    /// [`Error::DetectorStalled`] or [`Error::UnknownReference`] for a reference clock that
    /// cannot be measured or recognised, before the LMS7002M is touched;
    /// [`Error::ChipId`] for an LMS7002M version word other than 0x3841, before anything is
    /// written to it; [`Error::Bus`] or [`Error::Reset`] when an exchange fails.
    #[expect(
        clippy::missing_panics_doc,
        reason = "invariant: an open finishes only after recognising its reference and \
                  creating the LMS7002M"
    )]
    pub fn open(port: impl ControlPort + 'static) -> Result<Self, Error> {
        let mut port: Box<dyn ControlPort> = Box::new(port);
        let mut open = Open::new();
        while let Some(action) = open.next() {
            let result = match action {
                Action::Op(op) => exchange(&mut *port, op),
                Action::Sleep(duration) => {
                    port.sleep(duration);
                    Ok(0)
                }
            };
            open.complete(result)?;
        }
        let reference_hz = open
            .reference_hz()
            .expect("invariant: a finished open has recognised its reference");
        let control = open
            .control()
            .expect("invariant: a finished open has created the LMS7002M");
        Ok(Self {
            port,
            reference_hz,
            control,
        })
    }

    /// Powers the board up as libusdr's `/dm/power/en` = 1 does on this family: not at all
    /// (`dev_limesdr_pwren_set` returns at once). The open has already brought the LMS7002M
    /// up.
    ///
    /// # Errors
    ///
    /// None yet; fallible for the power control a later port may add.
    pub fn power_up(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// Sets the RX and TX sample rates in samples per second, 0 for none (libusdr's
    /// `/dm/rate/rxtxadcdac` with no converter floors; `/dm/rate/master` is both equal):
    /// the LMS7002M's clock generator, processor dividers and `LimeLight` interface in
    /// SISO DDR both ways, the converters as fast as the processors allow, then both
    /// `LimeLight` clocks direct. Once streaming is prepared, each running direction's
    /// processor divider and, where none was set, bandwidth follow; TX also gets libusdr's
    /// amplifier and pad gains.
    ///
    /// # Errors
    ///
    /// Before any exchange: [`Error::InvalidRate`] for a rate outside libusdr's 90 kS/s to
    /// 122.88 MS/s, or none in either direction. Then [`Error::UndeliverableRate`],
    /// [`Error::ClockUnlocked`], or [`Error::Bus`].
    pub fn set_sample_rate(&mut self, rx: u32, tx: u32) -> Result<(), Error> {
        if rx == 0 && tx == 0 {
            return Err(Error::InvalidRate(0));
        }
        let in_range = |rate: u32| rate == 0 || (RATE_MIN..=RATE_MAX).contains(&rate);
        if let Some(rate) = [rx, tx].into_iter().find(|&rate| !in_range(rate)) {
            return Err(Error::InvalidRate(rate));
        }
        let mut rate = SetSampleRate::new(self.control, rx, tx);
        let ran = self.run(&mut rate);
        self.control = *rate.control();
        ran?;
        rate.refusal()
            .map_or(Ok(()), |refusal| Err(rate_refusal(refusal)))
    }

    /// Tunes the RX LO to `hertz` (libusdr's `/dm/sdr/0/rx/frequency`); 0 stops the
    /// synthesizer. Once streaming is prepared, the receive band and antenna switch follow
    /// the LO: below 1.7 GHz the wide-band LNA, below 4 GHz the high-band one, above it
    /// the low-band one.
    ///
    /// # Errors
    ///
    /// [`Error::TuneOutOfRange`] when no VCO reaches `hertz` or none locks;
    /// [`Error::VcoComparatorFailed`]; [`Error::Bus`].
    pub fn set_rx_frequency(&mut self, hertz: u32) -> Result<(), Error> {
        self.tune(Synthesizer::Rx, hertz)
    }

    /// Tunes the TX LO to `hertz` (libusdr's `/dm/sdr/0/tx/frequency`); 0 stops the
    /// synthesizer. Once streaming is prepared, the transmit band and antenna switch follow
    /// the LO: band 2 below 2 GHz, band 1 above.
    ///
    /// # Errors
    ///
    /// As [`Self::set_rx_frequency`].
    pub fn set_tx_frequency(&mut self, hertz: u32) -> Result<(), Error> {
        self.tune(Synthesizer::Tx, hertz)
    }

    /// `lms7002m_fe_set_freq` (FDD) as this family's handlers call it: every time, with no
    /// check against the LO last tuned.
    fn tune(&mut self, synthesizer: Synthesizer, hertz: u32) -> Result<(), Error> {
        let op = self.control.tune(LimeFrontend, synthesizer, hertz);
        self.run_control(op, |refusal| match refusal {
            ControlError::Tune(TuneError::OutOfRange) => Error::TuneOutOfRange(hertz),
            ControlError::Tune(TuneError::ComparatorFailed) => Error::VcoComparatorFailed,
            other => unrefused(other),
        })
    }

    /// Sets a gain on both channels (libusdr's `/dm/sdr/0/{rx,tx}/gain*`), returning the
    /// gain reached in dB as libusdr reports it. libusdr's `/dm/sdr/0/rx/gain` is
    /// [`Gain::RxLna`], `.../rx/gain/vga` [`Gain::RxTia`] and `/dm/sdr/0/tx/gain`
    /// [`Gain::TxPad`].
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn set_gain(&mut self, gain: Gain, value: i32) -> Result<i32, Error> {
        let (op, actual) = self.control.set_gain(gain.control(), value);
        self.run_control(op, unrefused)?;
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
        self.run_control(op, unrefused)
    }

    /// Prepares the LMS7002M to stream, as libusdr does when it creates an RX or TX stream
    /// (`limesdr_prepare_streaming`): a 1 MS/s rate both ways if none is set, then channel
    /// A streamed up in both directions, each band and antenna switch following its LO.
    /// Later rates and tunings then update the running paths. The streams themselves need
    /// the FT601 transport, which is not ported.
    ///
    /// # Errors
    ///
    /// [`Error::NoSampleRate`] when the default rate could not be set, before the
    /// streaming up writes anything (libusdr ignores that rate's failure, then divides by
    /// zero); [`Error::Bus`].
    pub fn prepare_streaming(&mut self) -> Result<(), Error> {
        let mut prepare = PrepareStreaming::new(self.control);
        let ran = self.run(&mut prepare);
        self.control = *prepare.control();
        ran?;
        prepare
            .refusal()
            .map_or(Ok(()), |refusal| Err(rate_refusal(refusal)))
    }

    /// Closes the board as libusdr's `usdr_dmd_close` does on this family: without a bus
    /// operation (`limesdr_dtor` is a "TODO"), leaving the LMS7002M as the open and later
    /// operations left it and releasing the control channel.
    ///
    /// # Errors
    ///
    /// None yet; fallible for the stream shutdown a later port adds.
    pub fn close(self) -> Result<(), Error> {
        drop(self.port);
        Ok(())
    }

    /// The nominal reference-clock frequency measured at open, in hertz: 10, 30.72, 38.4,
    /// 40 or 52 MHz (the LimeSDR-mini's own is 40 MHz).
    #[must_use]
    pub const fn reference_hz(&self) -> u32 {
        self.reference_hz
    }

    /// The board revision libusdr reports (`/dm/revision`): a constant it emulates rather
    /// than reads, the same for every LimeSDR-mini.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        REVISION
    }

    /// Runs a control operation, keeping the state it leaves even when it fails; a
    /// refusal is reported as `refused` names it.
    fn run_control<F: Frontend>(
        &mut self,
        mut op: Operation<F>,
        refused: impl FnOnce(ControlError) -> Error,
    ) -> Result<(), Error> {
        let ran = self.run(&mut op);
        self.control = *op.control();
        ran?;
        match op.outcome() {
            Some(Err(refusal)) => Err(refused(refusal)),
            Some(Ok(())) | None => Ok(()),
        }
    }

    /// Runs a portable sequence to its end, one LMS64C exchange per access.
    fn run(&mut self, sequence: &mut impl Sequence) -> Result<(), Error> {
        while let Some(action) = sequence.next() {
            let result = match action {
                SequenceAction::Fpga(access) => {
                    exchange(&mut *self.port, LsOp::from_access(access))
                }
                SequenceAction::Sleep(duration) => {
                    self.port.sleep(duration);
                    Ok(0)
                }
                SequenceAction::Access(_) => {
                    unreachable!("invariant: this board's sequences use no I2C")
                }
            };
            sequence.complete(result)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("reference_hz", &self.reference_hz)
            .finish_non_exhaustive()
    }
}

/// A sample rate's refusal, or a stream preparation's (its default rate unset).
fn rate_refusal(refusal: ControlError) -> Error {
    match refusal {
        ControlError::Decimation => Error::UndeliverableRate,
        ControlError::Clock => Error::ClockUnlocked,
        ControlError::NoRate => Error::NoSampleRate,
        other => unrefused(other),
    }
}

/// A refusal this board's operations cannot meet: they always select a channel (the open
/// leaves both selected, and the board streams channel A), and `Device` passes the
/// control code only rates within libusdr's range.
fn unrefused(refusal: ControlError) -> Error {
    unreachable!("invariant: this board's operations are never refused with {refusal:?}")
}

/// Performs `op` as one LMS64C exchange on `port`: what it returned.
fn exchange(port: &mut dyn ControlPort, op: LsOp) -> Result<u32, ExchangeError> {
    let reply = port.exchange(&op.request().to_bytes())?;
    op.reply(&Frame::from_bytes(reply))
        .map_err(|bad| match bad {
            BadReply::Refused(status) => ExchangeError::Refused(status),
            BadReply::Mismatched(command) => ExchangeError::Mismatched(command),
        })
}
