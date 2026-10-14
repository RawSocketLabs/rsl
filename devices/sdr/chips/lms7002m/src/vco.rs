//! The VCO capacitor-bank search both synthesizer tunings run: libusdr's
//! `_lms7002m_vco_range` over `lms7002m_{cgen,sxx}_trim_vco{,_slow}`.

use core::time::Duration;

use bnb::u5;
use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::reg::Reg;
use super::register::Register;
use super::regs::{CgenVco, SxxVco, SxxVcoSelect, VcoComparators, VcoReading};
use super::spi;
use super::step::checked;

/// The settling wait of the slow trims (`usleep(1000)`).
const SLOW_SETTLE: Duration = Duration::from_micros(1000);

/// Which synthesizer's VCO a probe trims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trim {
    /// The clock generator's (`CGEN_0x008B`, read back at `0x008C`).
    Cgen,

    /// The selected bank's synthesizer, with this VCO (`SXX_0x0121`, read back at
    /// `0x0123`).
    Sxx(SxxVcoSelect),
}

/// One trim: write a cap code, read the comparators. The slow variants wait 1 ms, after
/// the read for CGEN and before it for SXX, as libusdr's do.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Probe {
    /// The actions, in order; `None` past the end.
    actions: [Option<SequenceAction>; 3],

    /// Which of them is next.
    at: usize,

    /// The comparators' reading, once read.
    reading: Option<VcoReading>,

    /// An access failed.
    failed: bool,
}

impl Probe {
    /// A probe of `cap` on `trim`'s VCO, on the chip at `target`.
    pub(crate) fn new(target: SpiAddr, trim: Trim, cap: u8, slow: bool) -> Self {
        let (write, read) = match trim {
            Trim::Cgen => (
                CgenVco::new().with_bias(u5::new(15)).with_cap(cap).entry(),
                Reg::CgenComparators,
            ),
            Trim::Sxx(select) => (
                SxxVco::new()
                    .with_regulator(u5::new(16))
                    .with_cap(cap)
                    .with_select(select)
                    .entry(),
                Reg::SxxComparators,
            ),
        };
        let write = Some(SequenceAction::Fpga(spi::write(target, write)));
        let read = Some(SequenceAction::Fpga(spi::read(target, read)));
        let settle = slow.then_some(SequenceAction::Sleep(SLOW_SETTLE));
        let actions = match trim {
            Trim::Cgen => [write, read, settle],
            Trim::Sxx(_) if slow => [write, settle, read],
            Trim::Sxx(_) => [write, read, None],
        };
        Self {
            actions,
            at: 0,
            reading: None,
            failed: false,
        }
    }

    /// The comparators' reading, once the probe has finished.
    pub(crate) fn reading(&self) -> Option<VcoReading> {
        self.next().is_none().then_some(self.reading).flatten()
    }
}

impl Sequence for Probe {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.actions.get(self.at).copied().flatten()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = checked(action, result).inspect_err(|_| self.failed = true)?;
        if let SequenceAction::Fpga(FpgaAccess::Spi { write: false, .. }) = action {
            self.reading = Some(VcoComparators::from_raw(spi::value(word)).reading());
        }
        self.at += 1;
        Ok(())
    }
}

/// How a search ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Search {
    /// The widest closed range of OK codes found, `hi` below `lo` when none was
    /// (`*phi`, `*plo`: 0 and 255).
    Range {
        /// Its highest code.
        hi: u8,
        /// Its lowest code.
        lo: u8,
    },

    /// A probe read `LMS7002M_VCO_FAIL`: libusdr's `-EIO`.
    Failed,
}

/// Where the search is.
#[derive(Clone, Copy, Debug)]
enum Phase {
    /// The binary search for the lowest code not reading low: probing `cap`, with step
    /// `2^step` next.
    Binary {
        /// The code probed.
        cap: i32,
        /// The step's exponent, 6 down to 0.
        step: i32,
    },

    /// The linear scan from just below it, collecting up to four ranges of OK codes.
    Linear {
        /// The code probed.
        cap: i32,
        /// The ranges, `(lo, hi)`; open ones have `lo > hi`.
        ranges: [(i32, i32); 4],
        /// The range being filled.
        range: usize,
        /// High readings in a row.
        highs: u32,
    },

    /// Finished.
    Done(Search),
}

/// `_lms7002m_vco_range` with `start` > 255 (every caller's): a binary search from code
/// 128 for where the readings stop being low, then a scan upwards from `extend + 1` codes
/// below it, until four ranges are closed, the codes run out or more than `extend` high
/// readings come in a row. The widest closed range wins, the first on a tie; a range still
/// open when the scan ends does not count.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VcoSearch {
    /// The chip.
    target: SpiAddr,

    /// The VCO trimmed.
    trim: Trim,

    /// The slow trims.
    slow: bool,

    /// `extend_range`.
    extend: u32,

    /// Where the search is.
    phase: Phase,

    /// The probe in progress.
    probe: Probe,

    /// A probe's access failed.
    failed: bool,
}

impl VcoSearch {
    /// Starts a search; its first probe is code 128.
    pub(crate) fn new(target: SpiAddr, trim: Trim, slow: bool, extend: u32) -> Self {
        let cap = 128;
        Self {
            target,
            trim,
            slow,
            extend,
            phase: Phase::Binary { cap, step: 6 },
            probe: Probe::new(target, trim, code(cap), slow),
            failed: false,
        }
    }

    /// The outcome, once finished.
    pub(crate) const fn result(&self) -> Option<Search> {
        match self.phase {
            Phase::Done(search) => Some(search),
            Phase::Binary { .. } | Phase::Linear { .. } => None,
        }
    }

    /// The phase after a probe of the current code read `reading`.
    fn advance(&self, reading: VcoReading) -> Phase {
        if reading == VcoReading::Fail {
            return Phase::Done(Search::Failed);
        }
        match self.phase {
            Phase::Binary { cap, step } => {
                let cap = if reading == VcoReading::Low {
                    cap + (1 << step)
                } else {
                    cap - (1 << step)
                };
                if step > 0 {
                    return Phase::Binary {
                        cap,
                        step: step - 1,
                    };
                }
                let extend = i32::try_from(self.extend).unwrap_or(i32::MAX);
                Self::scan((cap - 1).saturating_sub(extend).max(0), [(255, 0); 4], 0, 0)
            }
            Phase::Linear {
                cap,
                mut ranges,
                mut range,
                mut highs,
            } => {
                if reading != VcoReading::High {
                    highs = 0;
                }
                let (lo, hi) = &mut ranges[range];
                let closes = *lo <= *hi;
                match reading {
                    VcoReading::Ok => {
                        *lo = (*lo).min(cap);
                        *hi = cap;
                    }
                    VcoReading::High => {
                        if closes {
                            range += 1;
                        }
                        if highs > self.extend {
                            return Phase::Done(widest(&ranges[..range]));
                        }
                        highs += 1;
                    }
                    VcoReading::Low | VcoReading::Fail => {
                        if closes {
                            range += 1;
                        }
                    }
                }
                Self::scan(cap + 1, ranges, range, highs)
            }
            Phase::Done(_) => self.phase,
        }
    }

    /// The scan's next step at `cap`, or its end.
    fn scan(cap: i32, ranges: [(i32, i32); 4], range: usize, highs: u32) -> Phase {
        if cap < 256 && range < 4 {
            Phase::Linear {
                cap,
                ranges,
                range,
                highs,
            }
        } else {
            Phase::Done(widest(&ranges[..range]))
        }
    }
}

/// The widest of `closed`, the first on a tie, as `(hi, lo)`; `(0, 255)` when empty.
fn widest(closed: &[(i32, i32)]) -> Search {
    let mut best: Option<(i32, i32)> = None;
    for &(lo, hi) in closed {
        if best.is_none_or(|(best_lo, best_hi)| hi - lo > best_hi - best_lo) {
            best = Some((lo, hi));
        }
    }
    let (lo, hi) = best.unwrap_or((255, 0));
    Search::Range {
        hi: code(hi),
        lo: code(lo),
    }
}

/// A search position as the cap code libusdr passes (`(unsigned)vco_cap`, masked to the
/// 8-bit field).
fn code(cap: i32) -> u8 {
    cap.to_le_bytes()[0]
}

impl Sequence for VcoSearch {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed || self.result().is_some() {
            return None;
        }
        self.probe.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        self.probe
            .complete(result)
            .inspect_err(|_| self.failed = true)?;
        let Some(reading) = self.probe.reading() else {
            return Ok(());
        };
        self.phase = self.advance(reading);
        let cap = match self.phase {
            Phase::Binary { cap, .. } | Phase::Linear { cap, .. } => cap,
            Phase::Done(_) => return Ok(()),
        };
        self.probe = Probe::new(self.target, self.trim, code(cap), self.slow);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `search` against comparators that read OK on `ok` codes, low below and high
    /// above; returns the outcome and the codes probed.
    fn run(mut search: VcoSearch, ok: core::ops::RangeInclusive<u8>) -> (Search, usize) {
        let mut probes = 0;
        let mut cap = 0;
        while let Some(action) = search.next() {
            let word = match action {
                SequenceAction::Fpga(FpgaAccess::Spi {
                    word, write: true, ..
                }) => {
                    probes += 1;
                    cap = u8::try_from(word >> 1 & 0xff).unwrap();
                    0
                }
                SequenceAction::Fpga(_) => {
                    if ok.contains(&cap) {
                        0x2000
                    } else if cap < *ok.start() {
                        0
                    } else {
                        0x3000
                    }
                }
                SequenceAction::Sleep(_) | SequenceAction::Access(_) => 0,
            };
            search.complete::<()>(Ok(word)).unwrap();
        }
        (search.result().unwrap(), probes)
    }

    #[test]
    fn the_band_of_ok_codes_is_found_and_its_ends_reported() {
        let search = VcoSearch::new(SpiAddr(0), Trim::Cgen, false, 0);
        let (outcome, probes) = run(search, 40..=47);
        assert_eq!(outcome, Search::Range { hi: 47, lo: 40 });
        // The binary search ends on 39, one below the band; the scan starts one lower.
        assert_eq!(
            probes,
            7 + 2 + 8 + 2,
            "binary, two lows, the band, two highs"
        );
    }

    #[test]
    fn a_band_reaching_the_top_code_stays_open_and_does_not_count() {
        let search = VcoSearch::new(SpiAddr(0), Trim::Cgen, false, 0);
        let (outcome, _) = run(search, 250..=255);
        assert_eq!(
            outcome,
            Search::Range { hi: 0, lo: 255 },
            "libusdr's 'none'"
        );
    }

    #[test]
    fn a_failing_comparator_ends_the_search() {
        let mut search = VcoSearch::new(SpiAddr(0), Trim::Cgen, false, 0);
        search.complete::<()>(Ok(0)).unwrap();
        search.complete::<()>(Ok(0x1000)).unwrap();
        assert_eq!(search.result(), Some(Search::Failed));
        assert_eq!(search.next(), None);
    }
}
