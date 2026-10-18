//! The RX chain's DC estimator: the mean of a channel's samples after its NCO, which
//! libusdr's LO-leakage and IQ-imbalance calibration measures power with.
//!
//! It sits behind the RX configuration port ([`Phy::Rx`], `REG_CFG_PHY_0`) as bank 6
//! (`PHY_REG_RXBANK_DC_EST`), reached by words of its own layout ([`EstimatorAccess`]): a
//! write sets one of its registers; a read request selects the word the next read of the
//! port returns.
//!
//! Source: `ipblks/dc_estim.c` (`_phy_rx_reg`, `phy_dc_estim_*`, `phy_rfe_pwrdc_get`,
//! `phy_do_meas_nco_avg`).

use std::time::Duration;

use bnb::{bitfield, u7};

use super::phy::Phy;
use crate::portable::fpga::FPGA;
use crate::portable::sequence::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};

/// One access to the estimator through the RX port (`_phy_rx_reg`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EstimatorAccess {
    /// Select the word at `addr` for the next read rather than write it.
    #[bits(31..=31)]
    read: bool,

    /// The bank: always the estimator's.
    #[bits(24..=30)]
    bank: u7,

    /// The estimator register.
    #[bits(16..=23)]
    addr: u8,

    /// The value written; 0 for a read.
    #[bits(0..=15)]
    value: u16,
}

/// The estimator's bank on the RX port (`PHY_REG_RXBANK_DC_EST`).
const BANK: u8 = 6;

/// Write address: 1 holds the estimator, 0 runs it (`ADDR_DC_EST_RESET`).
const HOLD: u8 = 0;

/// Write address: the accumulation setting (`ADDR_DC_EST_ACCM`).
const ACCUMULATION: u8 = 1;

/// Read address: the generation, which moves when a measurement finishes
/// (`DC_ESTIM_GEN`).
const GENERATION: u8 = 0;

/// Read addresses of channel A's I and Q means (`DC_ESTIM_AI`, `DC_ESTIM_AQ`); channel B's
/// follow at 6 and 7.
const CHANNEL_A_I: u8 = 4;

/// libusdr's accumulation setting for calibration (`acc_idx`, "TODO: FIXME" in its
/// source), which also divides the means.
const ACCUMULATION_SETTING: u16 = 1;

/// The sample full scale the means are taken against (`range`).
const FULL_SCALE: f64 = 32_767.0;

/// How often libusdr polls for a finished measurement before giving up.
const POLLS: u32 = 8000;

/// Which estimator word a [`DcMeasure`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    /// The generation at the start.
    Start,
    /// The generation at a poll's start.
    Generation,
    /// The channel's I mean.
    I,
    /// The channel's Q mean.
    Q,
    /// The generation again, to confirm the means are of one measurement.
    Confirm,
}

/// Where a [`DcMeasure`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Setting the accumulation.
    Accumulate,
    /// Holding the estimator.
    Hold,
    /// Waiting a microsecond in hold.
    Settle,
    /// Running it.
    Release,
    /// Selecting a word for the next read.
    Select(Word),
    /// Reading the selected word.
    Read(Word),
    /// Waiting a millisecond between polls.
    Wait,
    /// Finished.
    Done,
}

/// One measurement of the power at DC after channel `channel`'s NCO (0 for A, 1 for B),
/// in thousandths of a dB of full scale, as libusdr's `phy_do_meas_nco_avg` does: set the
/// accumulation, hold and run the estimator, note its generation, then poll every
/// millisecond until it moves and the means read between two equal generations
/// (`phy_rfe_pwrdc_get`, retried at once when the generation moved during the reads). A
/// sans-I/O machine over the RX port: the calibration runs it on the blocking board and
/// on the native USB handle. No measurement in 8000 polls is libusdr's `-EAGAIN`
/// ([`Self::value`] then reports `Err(())`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct DcMeasure {
    /// The channel's I mean's read address.
    i_addr: u8,
    /// Where it is.
    step: Step,
    /// The generation at the start.
    start: u32,
    /// The generation the current poll read.
    generation: u32,
    /// The I mean read.
    i: u32,
    /// The Q mean read.
    q: u32,
    /// Polls that found no new measurement.
    polls: u32,
    /// The power, or `Err(())` for no measurement in time; `None` until finished, and after
    /// a bus failure.
    value: Option<Result<i32, ()>>,
}

impl DcMeasure {
    /// A measurement on `channel`, before its first write.
    pub(crate) const fn new(channel: u8) -> Self {
        Self {
            i_addr: CHANNEL_A_I + 2 * channel,
            step: Step::Accumulate,
            start: 0,
            generation: 0,
            i: 0,
            q: 0,
            polls: 0,
            value: None,
        }
    }

    /// The power once measured, in thousandths of a dB; `Err(())` when no measurement
    /// finished in 8000 polls.
    pub(crate) const fn value(&self) -> Option<Result<i32, ()>> {
        self.value
    }

    /// The register address `word` reads.
    const fn addr(&self, word: Word) -> u8 {
        match word {
            Word::Start | Word::Generation | Word::Confirm => GENERATION,
            Word::I => self.i_addr,
            Word::Q => self.i_addr + 1,
        }
    }

    /// Where it goes once `word`, reading `value`, has been read.
    fn after_read(&mut self, word: Word, value: u32) -> Step {
        match word {
            Word::Start => {
                self.start = value;
                Step::Select(Word::Generation)
            }
            Word::Generation if value == self.start => Step::Wait,
            Word::Generation => {
                self.generation = value;
                Step::Select(Word::I)
            }
            Word::I => {
                self.i = value;
                Step::Select(Word::Q)
            }
            Word::Q => {
                self.q = value;
                Step::Select(Word::Confirm)
            }
            Word::Confirm if value == self.generation => {
                let signed = |word: u32| i32::from_ne_bytes(word.to_ne_bytes());
                self.value = Some(Ok(millidecibels(signed(self.i), signed(self.q))));
                Step::Done
            }
            Word::Confirm => Step::Select(Word::Generation),
        }
    }
}

impl Sequence for DcMeasure {
    fn next(&self) -> Option<SequenceAction> {
        let write = |access: EstimatorAccess| {
            let value = access.with_bank(u7::new(BANK)).to_raw();
            SequenceAction::Fpga(FpgaAccess::Write {
                reg: Phy::Rx as u32,
                value,
            })
        };
        let set = |addr, value| write(EstimatorAccess::new().with_addr(addr).with_value(value));
        Some(match self.step {
            Step::Accumulate => set(ACCUMULATION, ACCUMULATION_SETTING),
            Step::Hold => set(HOLD, 1),
            Step::Settle => SequenceAction::Sleep(Duration::from_micros(1)),
            Step::Release => set(HOLD, 0),
            Step::Select(word) => write(
                EstimatorAccess::new()
                    .with_read(true)
                    .with_addr(self.addr(word)),
            ),
            Step::Read(_) => SequenceAction::Fpga(FpgaAccess::Read {
                reg: Phy::Rx as u32,
            }),
            Step::Wait => SequenceAction::Sleep(Duration::from_millis(1)),
            Step::Done => return None,
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let value = match (action, result) {
            (SequenceAction::Fpga(access), Err(source)) => {
                self.step = Step::Done;
                return Err(SequenceFailure::FpgaTransport {
                    chip: FPGA,
                    access,
                    source,
                });
            }
            (_, Ok(value)) => value,
            // A wait cannot fail.
            (_, Err(_)) => 0,
        };
        self.step = match self.step {
            Step::Accumulate => Step::Hold,
            Step::Hold => Step::Settle,
            Step::Settle => Step::Release,
            Step::Release => Step::Select(Word::Start),
            Step::Select(word) => Step::Read(word),
            Step::Read(word) => self.after_read(word, value),
            Step::Wait => {
                self.polls += 1;
                if self.polls >= POLLS {
                    self.value = Some(Err(()));
                    Step::Done
                } else {
                    Step::Select(Word::Generation)
                }
            }
            Step::Done => Step::Done,
        };
        Ok(())
    }
}

/// The power of 16.16 means `i` and `q` in thousandths of a dB of full scale, in libusdr's
/// double arithmetic, floored at -180 dB and truncated to an integer.
fn millidecibels(i: i32, q: i32) -> i32 {
    let scale =
        |mean: i32| f64::from(mean) / f64::from(ACCUMULATION_SETTING) / 65_536.0 / FULL_SCALE;
    let (i, q) = (scale(i), scale(q));
    let power = (i * i + q * q).max(1e-18);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "libusdr truncates the double to int; it lies within -180000..=+3011"
    )]
    let millidecibels = (1000.0 * 10.0 * power.log10()) as i32;
    millidecibels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_words_are_libusdrs() {
        // `_phy_rx_reg(.., true, 6, 1, 1)` and `(.., false, 6, 4, 0)`.
        let accumulate = EstimatorAccess::new()
            .with_bank(u7::new(BANK))
            .with_addr(ACCUMULATION)
            .with_value(1);
        assert_eq!(accumulate.to_raw(), 0x0601_0001);
        let select = EstimatorAccess::new()
            .with_read(true)
            .with_bank(u7::new(BANK))
            .with_addr(CHANNEL_A_I);
        assert_eq!(select.to_raw(), 0x8604_0000);
    }

    #[test]
    fn power_is_floored_at_minus_180_db_and_full_scale_is_0_db() {
        assert_eq!(millidecibels(0, 0), -180_000);
        // 32767 · 65536 is full scale on I alone.
        assert_eq!(millidecibels(32_767 * 65_536, 0), 0);
        // Half scale: -6.0206 dB, truncated towards zero.
        assert_eq!(millidecibels(32_767 * 32_768, 0), -6020);
    }

    /// Runs `measure`, answering each read of the RX port with the next of `reads` (the
    /// last repeated); the reads made, the waits taken and the value.
    fn run(mut measure: DcMeasure, reads: &[u32]) -> (usize, usize, Option<Result<i32, ()>>) {
        let (mut taken, mut waits) = (0, 0);
        while let Some(action) = measure.next() {
            let word = match action {
                SequenceAction::Fpga(FpgaAccess::Read { reg: 56 }) => {
                    taken += 1;
                    reads[(taken - 1).min(reads.len() - 1)]
                }
                SequenceAction::Sleep(wait) if wait == Duration::from_millis(1) => {
                    waits += 1;
                    0
                }
                _ => 0,
            };
            measure.complete::<()>(Ok(word)).unwrap();
        }
        (taken, waits, measure.value())
    }

    #[test]
    fn a_generation_that_never_moves_times_out_after_8000_polls() {
        let (reads, waits, value) = run(DcMeasure::new(0), &[7]);
        assert_eq!(value, Some(Err(())));
        assert_eq!(waits, 8000, "a millisecond after every poll, as libusdr");
        assert_eq!(
            reads,
            1 + 8000,
            "the start, then one generation read a poll"
        );
    }

    #[test]
    fn means_read_across_a_moving_generation_are_read_again() {
        // Start 1; the first poll finds 2 with means, but 3 after them; the retry reads 3,
        // the means and 3 again.
        let full_scale = 32_767 * 65_536;
        let reads = [1, 2, 0, 0, 3, 3, full_scale, 0, 3];
        let (taken, waits, value) = run(DcMeasure::new(0), &reads);
        assert_eq!(value, Some(Ok(0)), "the confirmed means, full scale on I");
        assert_eq!((taken, waits), (9, 0), "no wait between the retries");
    }
}
