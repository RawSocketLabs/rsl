//! Tuning a synthesizer (`lms6002d_tune_pll_stat`): the VCO and divider for the LO, the
//! fractional divider and its read-back check, then the search of the VCO capacitor bank
//! against the tuning-voltage comparator (`lms6002d_find_vcocap`), settling mid-window.

use core::time::Duration;

use bnb::{u3, u6};

use super::super::script::{Script, checked};
use super::super::sequence::{Sequence, SequenceAction, SequenceFailure};
use super::chip::{Lms6002d, NAME};
use super::pll::{
    self, ChargePump, Comparator, DownOffset, FractionalDivider, Pll, PllConfig, VcoCapacitor,
    VcoRegulator, VcoSelect,
};
use super::rx_fe::Lna;
use super::spi;

/// The capacitor codes a synthesizer's VCO locked over, inclusive. `low` can exceed `high`
/// when the search found no lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CapacitorWindow {
    /// The lowest code that locked.
    pub(crate) low: u8,

    /// The highest code that locked.
    pub(crate) high: u8,
}

/// Why a tuning failed, beyond a bus failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuneFault {
    /// The LO, in Hz, is below the synthesizer's 170 MHz floor; refused before any write.
    UnsupportedFrequency(u32),

    /// No capacitor code locks at this LO, in Hz; found after the writes, as libusdr.
    Unlocked(u32),

    /// The divider read back wrong, or the comparator reported both bits.
    Fault,
}

/// The capacitor search's state: libusdr's five-step binary search from code 32, then a
/// linear scan up from its result until the comparator reports the capacitor too high.
#[derive(Clone, Copy, Debug)]
struct Search {
    /// The binary phase's step exponent; below 0 the linear phase begins.
    step: i32,
    /// The code being probed.
    code: i32,
    /// The window's bottom.
    low: i32,
    /// The window's top, -1 until found.
    high: i32,
    /// Still in the binary phase.
    binary: bool,
}

impl Search {
    /// The search before its first probe.
    const START: Self = Self {
        step: 4,
        code: 32,
        low: 0,
        high: -1,
        binary: true,
    };

    /// Moves on to the next code to probe; `None` once the linear phase passes the top.
    fn settle(mut self) -> Option<Self> {
        if self.binary && self.step < 0 {
            self.binary = false;
            self.low = self.code;
        }
        if !self.binary && self.code >= 64 {
            return None;
        }
        Some(self)
    }

    /// Applies one comparator reading: the search's next state, or the window it ended
    /// with, or a fault for a reading with both bits set.
    ///
    /// In the binary phase an in-window reading counts as too high, as libusdr's switch
    /// falls through; code 0 is never probed.
    fn reading(mut self, comparator: Comparator) -> Result<Probe, TuneFault> {
        match (comparator.vtune_high(), comparator.vtune_low()) {
            (true, true) => return Err(TuneFault::Fault),
            // Capacitor too low: raise it, or move the window's bottom up.
            (true, false) if self.binary => self.code += 1 << self.step,
            (true, false) => self.low = self.code + 1,
            // In the window.
            (false, false) if !self.binary => self.high = self.code,
            // Too high, or in the window during the binary phase.
            _ if self.binary => self.code -= 1 << self.step,
            _ => {
                self.high = (self.code - 1).max(0);
                return Ok(Probe::Done(self.window()));
            }
        }
        if self.binary {
            self.step -= 1;
        } else {
            self.code += 1;
        }
        Ok(Probe::Next(self))
    }

    /// The window found: a missing top counts as 0.
    fn window(&self) -> CapacitorWindow {
        let byte = |value: i32| u8::try_from(value).expect("invariant: the search stays in 0..=64");
        CapacitorWindow {
            low: byte(self.low),
            high: byte(self.high.max(0)),
        }
    }
}

/// What a comparator reading leaves the search with.
#[derive(Clone, Copy, Debug)]
enum Probe {
    /// The search goes on.
    Next(Search),
    /// It ended with this window.
    Done(CapacitorWindow),
}

/// Where a tuning is.
#[derive(Clone, Copy, Debug)]
enum Stage {
    /// The divider and loop setup, then the comparator on and a wait.
    Setup(Script),
    /// Reading the divider back, byte `index` next.
    Readback {
        /// The byte read next.
        index: usize,
        /// The bytes read so far.
        bytes: [u8; 4],
    },
    /// Probing a capacitor code: write it, wait, read the comparator.
    Probe {
        /// The search.
        search: Search,
        /// Accesses of this probe done: 0 to 2.
        done: u8,
    },
    /// Settling mid-window and turning the comparator off.
    Finish {
        /// The window found.
        window: CapacitorWindow,
        /// The writes.
        script: Script,
    },
    /// Finished: the window, a fault, or `None` after a bus failure.
    Done(Option<Result<CapacitorWindow, TuneFault>>),
}

/// One synthesizer tuning. It owns a copy of the chip's cache, which the tuning updates
/// (the modulator clock on; on RX, the VCO selection once settled); take it back with
/// [`Self::chip`], also after a failure.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PllTune {
    /// The chip.
    lms: Lms6002d,
    /// Which synthesizer.
    pll: Pll,
    /// The LO asked, in Hz.
    lo_hz: u32,
    /// The divider written, most significant byte first.
    divider: [u8; 4],
    /// The VCO selection the tuning ends with.
    tuned: VcoSelect,
    /// Where it is.
    stage: Stage,
}

/// A [`VcoCapacitor`] value for `code`, keeping its low six bits as libusdr's macro does.
fn capacitor(code: i32) -> VcoCapacitor {
    let code = u8::try_from(code & 0x3f).expect("invariant: masked to six bits");
    VcoCapacitor::new().with_capacitor(u6::new(code))
}

impl PllTune {
    /// Tunes `pll`'s LO to `lo_hz` from a `reference_hz` reference.
    pub(crate) fn new(mut lms: Lms6002d, pll: Pll, lo_hz: u32, reference_hz: u32) -> Self {
        let mut tune = Self {
            lms,
            pll,
            lo_hz,
            divider: [0; 4],
            tuned: VcoSelect::new(),
            stage: Stage::Done(Some(Err(TuneFault::UnsupportedFrequency(lo_hz)))),
        };
        if lo_hz < pll::LOWEST_LO_HZ {
            return tune;
        }
        let (vco, exponent) = pll::vco_for(lo_hz);
        let divider = FractionalDivider::for_vco(reference_hz, u64::from(lo_hz) << (exponent + 1));
        tune.divider = divider.to_raw().to_be_bytes();
        let lo_buffer = match pll {
            Pll::Tx => Lna::None,
            Pll::Rx => lms.rx_vco.lo_buffer(),
        };
        tune.tuned = VcoSelect::new()
            .with_vco(vco)
            .with_divider_range(u3::new(exponent | 0b100))
            .with_lo_buffer(lo_buffer);
        lms.set_pll_modulator_clock(pll, true);
        tune.lms = lms;
        let mut setup = Script::new().fpga(NAME, lms.write(lms.clock_enables));
        for (reg, byte) in pll.divider().into_iter().zip(tune.divider) {
            setup = setup.fpga(NAME, lms.write_raw(reg, byte));
        }
        // The VCO first with no divider range or LO buffer, then all three.
        let setup = setup
            .fpga(NAME, lms.write_at(pll, PllConfig::TUNING))
            .fpga(NAME, lms.write_at(pll, VcoSelect::new().with_vco(vco)))
            .fpga(NAME, lms.write_at(pll, tune.tuned))
            .fpga(NAME, lms.write_at(pll, ChargePump::TUNING))
            .fpga(NAME, lms.write_at(pll, VcoRegulator::TUNING))
            .fpga(NAME, lms.write_at(pll, DownOffset::TUNING))
            .fpga(NAME, lms.write_at(pll, capacitor(32)))
            .fpga(
                NAME,
                lms.write_raw(pll.comparator_power(), pll::COMPARATOR_ON),
            )
            .sleep(Duration::from_micros(100));
        tune.stage = Stage::Setup(setup);
        tune
    }

    /// The chip's cache as the tuning has left it so far.
    pub(crate) const fn chip(&self) -> &Lms6002d {
        &self.lms
    }

    /// The outcome once finished: the window locked over, or a fault; `None` while running
    /// or after a bus failure, which [`Sequence::complete`] reports instead.
    pub(crate) const fn outcome(&self) -> Option<Result<CapacitorWindow, TuneFault>> {
        match self.stage {
            Stage::Done(outcome) => outcome,
            _ => None,
        }
    }

    /// The probe's action `done` accesses in.
    fn probe_action(&self, search: Search, done: u8) -> SequenceAction {
        let access = match done {
            0 => self.lms.write_at(self.pll, capacitor(search.code)),
            1 => return SequenceAction::Sleep(Duration::from_micros(150)),
            _ => self.lms.read_at::<Comparator>(self.pll),
        };
        SequenceAction::Fpga(access)
    }

    /// The writes that settle mid-window (rounding down, as libusdr's integer division
    /// does) and turn the comparator off.
    fn finish(&self, window: CapacitorWindow) -> Stage {
        let mid = u8::midpoint(window.low, window.high);
        let lms = self.lms;
        let script = Script::new()
            .fpga(NAME, lms.write_at(self.pll, capacitor(i32::from(mid))))
            .fpga(NAME, lms.write_at(self.pll, self.tuned))
            .fpga(
                NAME,
                lms.write_raw(self.pll.comparator_power(), pll::COMPARATOR_OFF),
            );
        Stage::Finish { window, script }
    }

    /// The stage after the search moved to `search`.
    fn probing(&self, search: Search) -> Stage {
        match search.settle() {
            Some(search) => Stage::Probe { search, done: 0 },
            None => self.finish(
                Search {
                    high: if search.high == -1 { 0 } else { search.high },
                    ..search
                }
                .window(),
            ),
        }
    }
}

impl Sequence for PllTune {
    fn next(&self) -> Option<SequenceAction> {
        match self.stage {
            Stage::Setup(script) | Stage::Finish { script, .. } => script.next(),
            Stage::Readback { index, .. } => Some(SequenceAction::Fpga(
                self.lms.read_raw(self.pll.divider()[index]),
            )),
            Stage::Probe { search, done } => Some(self.probe_action(search, done)),
            Stage::Done(_) => None,
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        match &mut self.stage {
            Stage::Setup(script) => {
                if let Err(failure) = script.complete(result) {
                    self.stage = Stage::Done(None);
                    return Err(failure);
                }
                if script.next().is_none() {
                    self.stage = Stage::Readback {
                        index: 0,
                        bytes: [0; 4],
                    };
                }
            }
            Stage::Readback { index, bytes } => {
                // libusdr ignores read failures here; its zeroed buffer then fails the
                // comparison.
                bytes[*index] = result.map_or(0, spi::value);
                *index += 1;
                if *index == bytes.len() {
                    self.stage = if *bytes == self.divider {
                        self.probing(Search::START)
                    } else {
                        Stage::Done(Some(Err(TuneFault::Fault)))
                    };
                }
            }
            Stage::Probe { search, done } => {
                let (search, step) = (*search, *done);
                let word = match checked(NAME, action, result) {
                    Ok(word) => word,
                    Err(failure) => {
                        self.stage = Stage::Done(None);
                        return Err(failure);
                    }
                };
                if step < 2 {
                    self.stage = Stage::Probe {
                        search,
                        done: step + 1,
                    };
                    return Ok(());
                }
                self.stage = match search.reading(Comparator::from(spi::value(word))) {
                    Err(fault) => Stage::Done(Some(Err(fault))),
                    Ok(Probe::Next(next)) => self.probing(next),
                    Ok(Probe::Done(window)) => self.finish(window),
                };
            }
            Stage::Finish { window, script } => {
                let window = *window;
                if let Err(failure) = script.complete(result) {
                    self.stage = Stage::Done(None);
                    return Err(failure);
                }
                if script.next().is_some() {
                    return Ok(());
                }
                if self.pll == Pll::Rx {
                    self.lms.rx_vco = self.tuned;
                }
                // libusdr's lock test: the search ended with an empty window at the bottom.
                let unlocked = window.low > window.high && window.high == 0;
                self.stage = Stage::Done(Some(if unlocked {
                    Err(TuneFault::Unlocked(self.lo_hz))
                } else {
                    Ok(window)
                }));
            }
            Stage::Done(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::sequence::FpgaAccess;
    use super::*;

    /// The comparator reading the capacitor too low (`VTUNE_H`).
    const TOO_LOW: u32 = 0x80;
    /// The comparator reading the capacitor too high (`VTUNE_L`).
    const TOO_HIGH: u32 = 0x40;

    #[test]
    fn a_window_ended_in_the_linear_phase_keeps_its_top() {
        // libusdr's `lms6002d_find_vcocap`, worked by hand: the binary phase probes 32
        // (too low), 48 (too high), 40 (low), 44 (high) and 42 (low), leaving 43; the
        // linear phase starts there with the window's bottom at 43 and finds 43 too high,
        // so the top is 42. The window {43, 42} is no lock failure (its top is not 0), and
        // the capacitor settles at the midpoint, 42.
        let (lms, _) = Lms6002d::create(Lms6002d::USDR_TARGET);
        let mut tune = PllTune::new(lms, Pll::Rx, 1_000_000_000, 26_000_000);
        let mut divider = tune.divider.into_iter();
        let mut readings = [TOO_LOW, TOO_HIGH, TOO_LOW, TOO_HIGH, TOO_LOW, TOO_HIGH].into_iter();
        let comparator = u32::from(u8::from(pll::Reg::RxComparator));
        let capacitor = u32::from(u8::from(pll::Reg::RxVcoCapacitor));
        let mut settled = None;
        while let Some(action) = tune.next() {
            let reply = match action {
                SequenceAction::Fpga(FpgaAccess::Spi {
                    write: false, reg, ..
                }) if reg == comparator => readings.next().expect("a scripted comparator reading"),
                SequenceAction::Fpga(FpgaAccess::Spi { write: false, .. }) => {
                    divider.next().map_or(0, u32::from)
                }
                SequenceAction::Fpga(FpgaAccess::Spi {
                    write: true,
                    reg,
                    word,
                    ..
                }) if reg == capacitor => {
                    settled = Some(word.to_le_bytes()[0] & 0x3f);
                    0
                }
                _ => 0,
            };
            tune.complete::<()>(Ok(reply)).unwrap();
        }
        assert_eq!(
            tune.outcome(),
            Some(Ok(CapacitorWindow { low: 43, high: 42 }))
        );
        assert_eq!(
            settled,
            Some(42),
            "the last capacitor written is mid-window"
        );
    }
}
