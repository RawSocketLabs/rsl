//! The synthesizers: the clock generator (`lms7002m_cgen_tune`, `lms7002m_cgen_disable`)
//! and the RX and TX LO synthesizers (`lms7002m_sxx_tune`, `lms7002m_sxx_disable`),
//! whose tunings search the VCO capacitor bank against the chip's comparators.

use core::num::NonZeroU32;

use bnb::{u2, u3, u4, u5, u6, u10};
use hal::{Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::Lms7002m;
use crate::batch::Batch;
use crate::register::Register;
use crate::regs::{
    CgenDivider, CgenEnable, CgenFracLow, CgenIntFrac, Mac, SxxChargePump, SxxControl, SxxFracLow,
    SxxIntFrac, SxxLoDivider, SxxVco, SxxVcoBias, SxxVcoSelect, VcoReading,
};
use crate::step::Wait;
use crate::vco::{Probe, Search, Trim, VcoSearch};

/// An LO synthesizer: libusdr's `lms7002m_sxx_path_t`. Each lives in one bank: RX in
/// channel A's, TX in channel B's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Synthesizer {
    /// `SXR` (`SXX_RX`).
    Rx,

    /// `SXT` (`SXX_TX`).
    Tx,
}

impl Synthesizer {
    /// The bank holding it, and that channel's index.
    const fn bank(self) -> (Mac, usize) {
        match self {
            Self::Rx => (Mac::A, 0),
            Self::Tx => (Mac::B, 1),
        }
    }
}

/// Why a synthesizer did not tune.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuneError {
    /// No capacitor code locks the target, or it is outside every VCO's reach: libusdr's
    /// `-ERANGE`.
    OutOfRange,

    /// A comparator read "fail": libusdr's `-EIO`.
    ComparatorFailed,
}

/// The middle of libusdr's CGEN VCO range, which the output divider aims for
/// (`CGEN_VCO_MID`).
const CGEN_VCO_MID: u32 = 2_350_000_000;
/// The lowest of the SXX VCOs' ranges (`SXX_VCOL_MIN`).
const VCOL_MIN: u32 = 1_900_000_000;
/// libusdr's SXX VCO ranges, low, middle and high (`SXX_VCO*_MIN`, `_MAX`).
const SXX_RANGES: [(u32, u32); 3] = [
    (VCOL_MIN, 2_611_000_000),
    (2_481_000_000, 3_377_000_000),
    (3_153_000_000, 3_999_000_000),
];
/// The VCO each of libusdr's four candidates uses: the fourth is the high VCO run at
/// twice the frequency from half the reference (`vcono`).
const CANDIDATE_VCOS: [SxxVcoSelect; 4] = [
    SxxVcoSelect::Low,
    SxxVcoSelect::Middle,
    SxxVcoSelect::High,
    SxxVcoSelect::High,
];
/// libusdr's tuning passes before it gives up.
const SXX_PASSES: u32 = 8;

/// A PLL's feedback divider: integer part and 20-bit fraction (`struct pll_cfg`).
fn pll(reference: NonZeroU32, vco: u32) -> (u32, u32) {
    let integer = vco / reference;
    let remainder = u64::from(vco - integer * reference.get());
    let fraction = (remainder << 20) / u64::from(reference.get());
    (
        integer,
        u32::try_from(fraction).expect("invariant: below 2^20"),
    )
}

/// The low 16 bits and bits 19:16 of a 20-bit fraction.
fn fraction_halves(fraction: u32) -> (u16, u4) {
    let [b0, b1, b2, _] = fraction.to_le_bytes();
    (u16::from_le_bytes([b0, b1]), u4::from_raw(b2))
}

/// The low bits of an integer divider field.
fn field10(value: u32) -> u10 {
    let [b0, b1, ..] = value.to_le_bytes();
    u10::from_raw(u16::from_le_bytes([b0, b1]))
}

/// One part of a tuning: writes, a wait, a search or a single probe.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// A batch of writes.
    Writes(Batch),

    /// A wait.
    Wait(Wait),

    /// A capacitor-bank search.
    Search(VcoSearch),

    /// One probe of the chosen code.
    Probe(Probe),
}

impl Sequence for Part {
    fn next(&self) -> Option<SequenceAction> {
        match self {
            Self::Writes(batch) => batch.next(),
            Self::Wait(wait) => wait.next(),
            Self::Search(search) => search.next(),
            Self::Probe(probe) => probe.next(),
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        match self {
            Self::Writes(batch) => batch.complete(result),
            Self::Wait(wait) => wait.complete(result),
            Self::Search(search) => search.complete(result),
            Self::Probe(probe) => probe.complete(result),
        }
    }
}

/// Where a CGEN tuning is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CgenStage {
    /// Writing the dividers.
    Program,
    /// The 20 µs settle.
    Settle,
    /// Searching with the fast trims (0) or the slow ones (1).
    Search(u8),
    /// Probing the chosen code.
    Check,
    /// Disabling the generator after finding no code.
    Disable,
    /// Finished, with this outcome.
    Done(Result<(), TuneError>),
}

/// `lms7002m_cgen_tune`: the dividers for the output, a 20 µs settle, the capacitor
/// search (fast, then slow if the first finds nothing), and a check of the middle of the
/// widest range. Finding no range disables the generator; a middle that does not read OK
/// leaves it running. Either is [`TuneError::OutOfRange`].
#[derive(Clone, Copy, Debug)]
pub struct CgenTune {
    /// The chip.
    target: SpiAddr,

    /// Where the tuning is.
    stage: CgenStage,

    /// The current part.
    part: Part,

    /// An access failed (other than the disable's, which libusdr ignores).
    failed: bool,
}

impl CgenTune {
    /// The outcome, once finished: `Ok` when the generator locks.
    #[must_use]
    pub const fn outcome(&self) -> Option<Result<(), TuneError>> {
        match self.stage {
            CgenStage::Done(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// The stage after the current part finished.
    fn advance(&mut self) {
        let (stage, part) = match (self.stage, self.part) {
            (CgenStage::Program, _) => (CgenStage::Settle, Part::Wait(Wait::micros(20))),
            (CgenStage::Settle, _) => (CgenStage::Search(0), self.search(false)),
            (CgenStage::Search(pass), Part::Search(search)) => match search.result() {
                Some(Search::Failed) => {
                    (CgenStage::Done(Err(TuneError::ComparatorFailed)), self.part)
                }
                Some(Search::Range { hi, lo }) if hi >= lo => {
                    let middle = u8::midpoint(hi, lo);
                    (
                        CgenStage::Check,
                        Part::Probe(Probe::new(self.target, Trim::Cgen, middle, false)),
                    )
                }
                _ if pass == 0 => (CgenStage::Search(1), self.search(true)),
                _ => (CgenStage::Disable, Part::Writes(cgen_disable(self.target))),
            },
            (CgenStage::Check, Part::Probe(probe)) => {
                let outcome = match probe.reading() {
                    Some(VcoReading::Ok) => Ok(()),
                    _ => Err(TuneError::OutOfRange),
                };
                (CgenStage::Done(outcome), self.part)
            }
            (CgenStage::Disable, _) => (CgenStage::Done(Err(TuneError::OutOfRange)), self.part),
            (CgenStage::Done(outcome), part) => (CgenStage::Done(outcome), part),
            (stage, _) => unreachable!("invariant: {stage:?} runs its own part"),
        };
        self.stage = stage;
        self.part = part;
    }

    /// A search with the fast or slow trims.
    fn search(&self, slow: bool) -> Part {
        Part::Search(VcoSearch::new(self.target, Trim::Cgen, slow, 0))
    }
}

impl Sequence for CgenTune {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed || self.outcome().is_some() {
            return None;
        }
        self.part.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        match self.part.complete(result) {
            Ok(()) => {}
            // `lms7002m_cgen_disable(m)`'s result is ignored.
            Err(_) if self.stage == CgenStage::Disable => {
                self.stage = CgenStage::Done(Err(TuneError::OutOfRange));
                return Ok(());
            }
            Err(failure) => {
                self.failed = true;
                return Err(failure);
            }
        }
        while self.outcome().is_none() && self.part.next().is_none() {
            self.advance();
        }
        Ok(())
    }
}

/// `lms7002m_cgen_disable`'s write: the generator's dividers, SDM, VCO and comparator
/// powered down and its global enable off.
fn cgen_disable(target: SpiAddr) -> Batch {
    let mut batch = Batch::new(target);
    batch.push(
        CgenEnable::new()
            .with_feedback_divider_powered_down(true)
            .with_output_divider_powered_down(true)
            .with_sdm_powered_down(true)
            .with_vco_powered_down(true)
            .with_comparator_powered_down(true)
            .entry(),
    );
    batch
}

/// Where an SXX tuning is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SxxStage {
    /// Selecting the bank and, on a first start, powering the synthesizer up.
    Start,
    /// The 10 ms settle after a first start; for a running synthesizer, its bank
    /// selection.
    Settle,
    /// In pass `pass`, programming candidate `candidate`'s divider.
    Divider {
        /// The pass.
        pass: u32,
        /// The candidate VCO, 0–3.
        candidate: usize,
    },
    /// In pass `pass`, searching candidate `candidate`.
    Search {
        /// The pass.
        pass: u32,
        /// The candidate VCO, 0–3.
        candidate: usize,
    },
    /// Reprogramming the best candidate's divider, as another was tried last.
    Restore,
    /// The 1 ms between passes.
    Retry(u32),
    /// Writing the LO divider, the chosen VCO and code, and restoring the bank selection.
    Finish,
    /// Finished, with this outcome.
    Done(Result<(), TuneError>),
}

/// `lms7002m_sxx_tune`: up to eight passes over the VCOs that can reach the LO (the low,
/// middle and high VCOs, and the high one at twice the frequency from half the
/// reference), each a capacitor search; the candidate with the widest range wins and gets
/// the middle of it. Later passes use the slow trims and a wider scan.
#[derive(Clone, Copy, Debug)]
pub struct SxxTune {
    /// The chip.
    target: SpiAddr,

    /// The cached selection, restored at the end.
    mac: crate::regs::LmlControl,

    /// The reference clock in hertz, at least 2 (the fourth candidate halves it).
    reference: NonZeroU32,

    /// The VCO frequency before the high-at-half candidate's doubling.
    vco: u32,

    /// How many times the LO was doubled to reach it.
    divider: u8,

    /// Which candidates can reach it.
    candidates: [bool; 4],

    /// The current pass's ranges, widths and middles, per candidate.
    widths: [(u32, u8); 4],

    /// The last candidate searched in the current pass (`lidx`).
    last: usize,

    /// The winning candidate and its middle code.
    chosen: Option<(usize, u8)>,

    /// Where the tuning is.
    stage: SxxStage,

    /// The current part.
    part: Part,

    /// An access failed.
    failed: bool,
}

impl SxxTune {
    /// The outcome, once finished: `Ok` when the synthesizer locks.
    #[must_use]
    pub const fn outcome(&self) -> Option<Result<(), TuneError>> {
        match self.stage {
            SxxStage::Done(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// Candidate `candidate`'s divider writes (`_lms7002m_sxx_pll_vco_set`).
    fn divider(&self, candidate: usize) -> Part {
        let reference = if candidate == 3 {
            NonZeroU32::new(self.reference.get() / 2)
                .expect("invariant: sxx_tune refuses references below 2")
        } else {
            self.reference
        };
        let (integer, fraction) = pll(reference, self.vco);
        let (low, high) = fraction_halves(fraction);
        let mut batch = Batch::new(self.target);
        batch.push(SxxFracLow::new().with_fraction(low).entry());
        batch.push(
            SxxIntFrac::new()
                .with_integer(field10(integer.wrapping_sub(4)))
                .with_fraction_high(high)
                .entry(),
        );
        Part::Writes(batch)
    }

    /// The next candidate at or after `candidate` in `pass`, or the pass's verdict.
    fn next_candidate(&mut self, pass: u32, candidate: usize) -> (SxxStage, Part) {
        if let Some(candidate) = (candidate..4).find(|&i| self.candidates[i]) {
            return (
                SxxStage::Divider { pass, candidate },
                self.divider(candidate),
            );
        }
        let mut best = (0, 0);
        for (index, &(width, _)) in self.widths.iter().enumerate() {
            if best.1 < width {
                best = (index, width);
            }
        }
        if best.1 > 0 {
            let (index, _) = best;
            self.chosen = Some((index, self.widths[index].1));
            if index != self.last {
                return (SxxStage::Restore, self.divider(index));
            }
            return (SxxStage::Finish, self.finish());
        }
        (SxxStage::Retry(pass), Part::Wait(Wait::micros(1000)))
    }

    /// The final writes.
    fn finish(&self) -> Part {
        let (candidate, cap) = self.chosen.expect("invariant: a candidate was chosen");
        let divider = self.divider + u8::from(candidate == 3);
        let mut batch = Batch::new(self.target);
        batch.push(sxx_lo_divider(divider).entry());
        batch.push(
            SxxVco::new()
                .with_regulator(u5::new(16))
                .with_cap(cap)
                .with_select(CANDIDATE_VCOS[candidate])
                .entry(),
        );
        batch.push(self.mac.entry());
        Part::Writes(batch)
    }

    /// The stage after the current part finished.
    fn advance(&mut self) {
        let (stage, part) = match self.stage {
            SxxStage::Start => (SxxStage::Settle, Part::Wait(Wait::micros(10_000))),
            SxxStage::Settle => {
                self.widths = [(0, 0); 4];
                self.next_candidate(0, 0)
            }
            SxxStage::Divider { pass, candidate } => {
                self.last = candidate;
                (
                    SxxStage::Search { pass, candidate },
                    Part::Search(VcoSearch::new(
                        self.target,
                        Trim::Sxx(CANDIDATE_VCOS[candidate]),
                        pass > 1,
                        2 * pass,
                    )),
                )
            }
            SxxStage::Search { pass, candidate } => {
                let Part::Search(search) = self.part else {
                    unreachable!("invariant: a search stage runs a search");
                };
                match search.result() {
                    Some(Search::Failed) | None => {
                        (SxxStage::Done(Err(TuneError::ComparatorFailed)), self.part)
                    }
                    Some(Search::Range { hi, lo }) => {
                        if hi >= lo {
                            let middle = u8::midpoint(lo, hi);
                            self.widths[candidate] = (u32::from(hi - lo) + 1, middle);
                        }
                        self.next_candidate(pass, candidate + 1)
                    }
                }
            }
            SxxStage::Restore => (SxxStage::Finish, self.finish()),
            SxxStage::Retry(pass) if pass + 1 < SXX_PASSES => {
                self.widths = [(0, 0); 4];
                self.next_candidate(pass + 1, 0)
            }
            SxxStage::Retry(_) => (SxxStage::Done(Err(TuneError::OutOfRange)), self.part),
            SxxStage::Finish => (SxxStage::Done(Ok(())), self.part),
            SxxStage::Done(_) => (self.stage, self.part),
        };
        self.stage = stage;
        self.part = part;
    }
}

impl Sequence for SxxTune {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed || self.outcome().is_some() {
            return None;
        }
        self.part.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        self.part
            .complete(result)
            .inspect_err(|_| self.failed = true)?;
        while self.outcome().is_none() && self.part.next().is_none() {
            self.advance();
        }
        Ok(())
    }
}

/// The LO divider word libusdr writes: divider powers 3 and 3, `divider` halvings.
fn sxx_lo_divider(divider: u8) -> SxxLoDivider {
    SxxLoDivider::new()
        .with_div2_power(u3::new(3))
        .with_div4_power(u3::new(3))
        .with_divider(u3::from_raw(divider))
}

impl Lms7002m {
    /// Tunes the clock generator to `output` hertz from a `reference` hertz clock, the DAC
    /// clock divided by `2^dac_order` from the ADC's (`lms7002m_cgen_tune`).
    ///
    /// An output of 0, or one whose VCO frequency overflows 32 bits, is
    /// [`TuneError::OutOfRange`] at once with nothing written; libusdr divides by zero or
    /// wraps.
    #[must_use]
    pub fn cgen_tune(&self, reference: NonZeroU32, output: u32, dac_order: u2) -> CgenTune {
        let refused = CgenTune {
            target: self.target,
            stage: CgenStage::Done(Err(TuneError::OutOfRange)),
            part: Part::Writes(Batch::new(self.target)),
            failed: false,
        };
        // The output divider closest to centring the VCO, in libusdr's 2..=512 range.
        let Some(quotient) = CGEN_VCO_MID.checked_div(output) else {
            return refused;
        };
        let half_divider = quotient.div_ceil(2).clamp(1, 256);
        let Some(vco) = half_divider
            .checked_mul(output)
            .and_then(|vco| vco.checked_mul(2))
        else {
            return refused;
        };
        let (integer, fraction) = pll(reference, vco);
        let (low, high) = fraction_halves(fraction);
        let [divider, ..] = (half_divider - 1).to_le_bytes();
        let mut batch = Batch::new(self.target);
        batch.push(
            CgenEnable::new()
                .with_running(true)
                .with_sdm_clock(true)
                .with_enabled(true)
                .entry(),
        );
        batch.push(CgenFracLow::new().with_fraction(low).entry());
        batch.push(
            CgenIntFrac::new()
                .with_integer(field10(integer.wrapping_sub(1)))
                .with_fraction_high(high)
                .entry(),
        );
        batch.push(
            CgenDivider::new()
                .with_clock_h_over_l(dac_order)
                .with_output_divider(divider)
                .entry(),
        );
        CgenTune {
            target: self.target,
            stage: CgenStage::Program,
            part: Part::Writes(batch),
            failed: false,
        }
    }

    /// Powers the clock generator down (`lms7002m_cgen_disable`).
    pub fn cgen_disable(&self) -> Batch {
        cgen_disable(self.target)
    }

    /// Tunes `synthesizer` to `lo` hertz from a `reference` hertz clock
    /// (`lms7002m_sxx_tune`), its TX-to-RX LO buffer on for TDD (`lo_to_rx`).
    ///
    /// An LO above the high VCO's range, or too low for six halvings to reach the low
    /// VCO's, is [`TuneError::OutOfRange`] at once, with nothing changed or written; so is
    /// a reference below 2 Hz, whose half (the fourth candidate's) libusdr divides by.
    /// Otherwise the synthesizer's `EN_DIR_SXX` is cached set first; a synthesizer
    /// already running gets its bank selected only, a stopped one its bias, charge pump,
    /// controls and LO divider too and 10 ms to settle. The bank selection is restored
    /// only when it locks.
    #[must_use]
    pub fn sxx_tune(
        &mut self,
        synthesizer: Synthesizer,
        reference: NonZeroU32,
        lo: u32,
        lo_to_rx: bool,
    ) -> SxxTune {
        let (bank, index) = synthesizer.bank();
        let mut tune = SxxTune {
            target: self.target,
            mac: self.mac,
            reference,
            vco: lo,
            divider: 0,
            candidates: [false; 4],
            widths: [(0, 0); 4],
            last: 0,
            chosen: None,
            stage: SxxStage::Done(Err(TuneError::OutOfRange)),
            part: Part::Writes(Batch::new(self.target)),
            failed: false,
        };
        if lo > SXX_RANGES[2].1 || reference.get() < 2 {
            return tune;
        }
        while tune.vco < VCOL_MIN {
            if tune.divider > 5 {
                return tune;
            }
            tune.divider += 1;
            tune.vco <<= 1;
        }
        let vco = tune.vco;
        let within = |(min, max): (u32, u32)| min < vco && vco < max;
        let (high_min, high_max) = SXX_RANGES[2];
        tune.candidates = [
            within(SXX_RANGES[0]),
            within(SXX_RANGES[1]),
            within(SXX_RANGES[2]),
            within((high_min / 2, high_max / 2)) && tune.divider < 6,
        ];

        let running = self.en_dir[index].sxx();
        self.en_dir[index].set_sxx(true);
        let mut batch = Batch::new(self.target);
        batch.push(self.mac.with_mac(bank).entry());
        if !running {
            batch.push(self.en_dir[index].entry());
            batch.push(
                SxxVcoBias::new()
                    .with_regulator_divider(204)
                    .with_bias(192)
                    .entry(),
            );
            batch.push(
                SxxChargePump::new()
                    .with_undocumented_13(true)
                    .with_offset_current(u6::new(20))
                    .with_pulse_current(u6::new(20))
                    .entry(),
            );
            batch.push(
                SxxControl::new()
                    .with_running(true)
                    .with_vco_current_limit(true)
                    .with_divprog_div2(true)
                    .with_sdm_clock(true)
                    .with_lo_to_rx_buffer_powered_down(!lo_to_rx)
                    .with_enabled(true)
                    .entry(),
            );
            batch.push(sxx_lo_divider(6).entry());
        }
        // A running synthesizer needs no settle: its first pass follows the selection.
        tune.stage = if running {
            SxxStage::Settle
        } else {
            SxxStage::Start
        };
        tune.part = Part::Writes(batch);
        tune
    }

    /// Stops `synthesizer` (`lms7002m_sxx_disable`): its bank's `EN_DIR_SXX` cleared, with
    /// the bank selection restored.
    pub fn sxx_disable(&mut self, synthesizer: Synthesizer) -> Batch {
        let (bank, index) = synthesizer.bank();
        self.en_dir[index].set_sxx(false);
        let mut batch = Batch::new(self.target);
        batch.push(self.mac.with_mac(bank).entry());
        batch.push(self.en_dir[index].entry());
        batch.push(self.mac.entry());
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The XSDR's 26 MHz reference.
    const FREF: NonZeroU32 = NonZeroU32::new(26_000_000).unwrap();

    #[test]
    fn an_output_libusdr_would_divide_by_or_overflow_is_refused_before_any_write() {
        let chip = Lms7002m::created(SpiAddr(0));
        for output in [0, 3_000_000_000] {
            let tune = chip.cgen_tune(FREF, output, u2::new(0));
            assert_eq!(tune.next(), None, "{output} Hz: nothing written");
            assert_eq!(tune.outcome(), Some(Err(TuneError::OutOfRange)));
        }
    }

    #[test]
    fn a_reference_too_small_to_halve_is_refused_before_any_write() {
        let mut chip = Lms7002m::created(SpiAddr(0));
        let tune = chip.sxx_tune(Synthesizer::Tx, NonZeroU32::MIN, 1_000_000_000, false);
        assert_eq!(tune.next(), None);
        assert_eq!(tune.outcome(), Some(Err(TuneError::OutOfRange)));
    }

    #[test]
    fn the_divider_is_the_integer_part_and_a_twenty_bit_fraction() {
        assert_eq!(pll(FREF, 2_340_000_000), (90, 0));
        assert_eq!(pll(FREF, 2_353_000_000), (90, 1 << 19), "one half");
    }

    #[test]
    fn an_lo_too_high_or_too_low_is_refused_before_any_write() {
        let mut chip = Lms7002m::created(SpiAddr(0));
        for lo in [4_000_000_000, 1_000_000] {
            let tune = chip.sxx_tune(Synthesizer::Rx, FREF, lo, false);
            assert_eq!(tune.next(), None);
            assert_eq!(tune.outcome(), Some(Err(TuneError::OutOfRange)));
        }
        assert_eq!(chip, Lms7002m::created(SpiAddr(0)), "nothing cached");
    }
}
