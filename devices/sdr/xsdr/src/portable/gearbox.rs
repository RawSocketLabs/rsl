//! The FPGA DSP chains' update after a sample rate on gateware with a DUC/DDC:
//! `_xsdr_update_fe_dsp` (`device/m2_lm7_1/xsdr_ctrl.c`), which loads a chain's FIR
//! microcode (`fgearbox_load_fir_ex`, `ipblks/fgearbox.c`; the tables are `sdr-gearbox`'s)
//! when its factor changed, and otherwise pulses the chain's reset.
//!
//! For each direction with a rate: when the LMS7002M's rate left another factor than the
//! chain holds, the chain is held in reset, its microcode loaded a byte per write 1 µs
//! apart and the reset released; with the same factor its reset is only pulsed. RX goes
//! first.

use core::time::Duration;

use hal::{Sequence, SequenceAction, SequenceFailure};
use sdr_gearbox::{Factor, UCODE_BYTES};

use super::fpga::{self, Gpo};
use super::outcome::outcome;

/// The wait between a load's writes (`fgearbox_load_fir_ex(.., 1)`).
const LOAD_GAP: Duration = Duration::from_micros(1);

/// One board action of an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    /// A GPO bank write.
    Gpo(Gpo, u8),
    /// A wait, in microseconds.
    Wait(u64),
}

/// The most actions around a load: the TX reload's five before it.
const ACTS: usize = 5;

/// One chain's update: the actions before its load, the load if any, the actions after.
#[derive(Clone, Copy, Debug)]
struct ChainUpdate {
    /// Actions before the load, the first `before.1`.
    before: ([Act; ACTS], usize),
    /// The microcode and the bank that takes it, when reloading.
    load: Option<(&'static [u8; UCODE_BYTES], Gpo)>,
    /// Actions after the load, the first `after.1`.
    after: ([Act; ACTS], usize),
}

impl ChainUpdate {
    /// An update of `count` actions only.
    const fn acts(acts: [Act; ACTS], count: usize) -> Self {
        Self {
            before: (acts, count),
            load: None,
            after: ([Act::Wait(0); ACTS], 0),
        }
    }

    /// The RX chain's reload of `factor`'s microcode.
    const fn rx_reload(factor: Factor) -> Self {
        let pad = Act::Wait(0);
        Self {
            before: (
                [
                    Act::Gpo(Gpo::RxChainReset, 0xf),
                    Act::Wait(10),
                    Act::Gpo(Gpo::RxChainReset, 0x2),
                    Act::Wait(10),
                    pad,
                ],
                4,
            ),
            load: Some((factor.decimation_ucode(), Gpo::RxChainProgram)),
            after: (
                [Act::Wait(10), Act::Gpo(Gpo::RxChainReset, 0), pad, pad, pad],
                2,
            ),
        }
    }

    /// The RX chain's reset pulse, its factor unchanged.
    const fn rx_pulse() -> Self {
        let pad = Act::Wait(0);
        Self::acts(
            [
                Act::Gpo(Gpo::RxChainReset, 0x2),
                Act::Wait(10),
                Act::Gpo(Gpo::RxChainReset, 0),
                pad,
                pad,
            ],
            3,
        )
    }

    /// The TX chain's reload of `factor`'s microcode.
    const fn tx_reload(factor: Factor) -> Self {
        let pad = Act::Wait(0);
        Self {
            before: (
                [
                    Act::Wait(10),
                    Act::Gpo(Gpo::TxChainReset, 0x3),
                    Act::Wait(10),
                    Act::Gpo(Gpo::TxChainReset, 0x2),
                    Act::Wait(10),
                ],
                5,
            ),
            load: Some((factor.interpolation_ucode(), Gpo::TxChainProgram)),
            after: (
                [Act::Wait(10), Act::Gpo(Gpo::TxChainReset, 0), pad, pad, pad],
                2,
            ),
        }
    }

    /// The TX chain's reset pulse.
    const fn tx_pulse() -> Self {
        let pad = Act::Wait(0);
        Self::acts(
            [
                Act::Gpo(Gpo::TxChainReset, 0x2),
                Act::Wait(10),
                Act::Gpo(Gpo::TxChainReset, 0),
                pad,
                pad,
            ],
            3,
        )
    }

    /// Actions in the whole update: before, a write and a wait per byte, after.
    const fn len(&self) -> usize {
        let load = if self.load.is_some() {
            2 * UCODE_BYTES
        } else {
            0
        };
        self.before.1 + load + self.after.1
    }

    /// Action `index` of the update.
    fn action(&self, index: usize) -> Option<SequenceAction> {
        let act = |act: Act| match act {
            Act::Gpo(bank, value) => SequenceAction::Fpga(bank.write(value)),
            Act::Wait(micros) => SequenceAction::Sleep(Duration::from_micros(micros)),
        };
        if index < self.before.1 {
            return Some(act(self.before.0[index]));
        }
        let mut index = index - self.before.1;
        if let Some((ucode, bank)) = self.load {
            if index < 2 * UCODE_BYTES {
                return Some(if index % 2 == 0 {
                    SequenceAction::Fpga(bank.write(ucode[index / 2]))
                } else {
                    SequenceAction::Sleep(LOAD_GAP)
                });
            }
            index -= 2 * UCODE_BYTES;
        }
        (index < self.after.1).then(|| act(self.after.0[index]))
    }
}

/// What a chain's update depends on: its rate (`s_rxrate`, `s_txrate`), the factor it
/// holds (`s_rx_dec`, `s_tx_int`, 0 before any load) and the factor the rate left
/// (`rx_dsp_decim`, `tx_dsp_inter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChainState {
    /// The chain's sample rate; 0 leaves it alone.
    pub(crate) rate: u32,
    /// The factor it holds.
    pub(crate) held: u32,
    /// The factor the rate asks.
    pub(crate) wanted: u32,
}

impl ChainState {
    /// The update, if the chain has a rate: a reload when the factor changed (and a
    /// table covers it; the rate path asks no other), else a reset pulse.
    fn update(self, reload: fn(Factor) -> ChainUpdate, pulse: ChainUpdate) -> Option<ChainUpdate> {
        if self.rate == 0 {
            return None;
        }
        if self.held == self.wanted {
            return Some(pulse);
        }
        Some(Factor::from_factor(self.wanted).map_or(pulse, reload))
    }
}

/// `_xsdr_update_fe_dsp` on gateware with a DUC/DDC.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UpdateFeDsp {
    /// The RX chain's update, then the TX chain's.
    updates: [Option<ChainUpdate>; 2],
    /// The update running.
    chain: usize,
    /// Its next action.
    at: usize,
    /// A write failed.
    failed: bool,
}

impl UpdateFeDsp {
    /// The update for the RX and TX chains as `rx` and `tx` stand.
    pub(crate) fn new(rx: ChainState, tx: ChainState) -> Self {
        let mut update = Self {
            updates: [
                rx.update(ChainUpdate::rx_reload, ChainUpdate::rx_pulse()),
                tx.update(ChainUpdate::tx_reload, ChainUpdate::tx_pulse()),
            ],
            chain: 0,
            at: 0,
            failed: false,
        };
        update.skip_finished();
        update
    }

    /// The factors the chains hold afterwards: a reloaded chain the factor it was asked.
    pub(crate) fn held(rx: ChainState, tx: ChainState) -> (u32, u32) {
        let after = |chain: ChainState| {
            let reloaded = chain.rate != 0
                && chain.held != chain.wanted
                && Factor::from_factor(chain.wanted).is_some();
            if reloaded { chain.wanted } else { chain.held }
        };
        (after(rx), after(tx))
    }

    /// Moves past chains with nothing (more) to do.
    fn skip_finished(&mut self) {
        while let Some(update) = self.updates.get(self.chain) {
            match update {
                Some(update) if self.at < update.len() => return,
                _ => {
                    self.chain += 1;
                    self.at = 0;
                }
            }
        }
    }
}

impl Sequence for UpdateFeDsp {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.updates.get(self.chain)?.as_ref()?.action(self.at)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        outcome(action, result, fpga::NAME).inspect_err(|_| self.failed = true)?;
        self.at += 1;
        self.skip_finished();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::FpgaAccess;

    use super::*;

    /// Runs `update`, returning its GPO writes' words and its waits' count.
    fn run(mut update: UpdateFeDsp) -> ([u32; 16], usize, usize) {
        let (mut words, mut writes, mut waits) = ([0; 16], 0, 0);
        while let Some(action) = update.next() {
            match action {
                SequenceAction::Fpga(FpgaAccess::Write { value, .. }) => {
                    if let Some(slot) = words.get_mut(writes) {
                        *slot = value;
                    }
                    writes += 1;
                }
                _ => waits += 1,
            }
            update.complete::<()>(Ok(0)).expect("no failure");
        }
        (words, writes, waits)
    }

    #[test]
    fn a_changed_factor_reloads_its_chain_and_an_unchanged_one_pulses_its_reset() {
        let rx = ChainState {
            rate: 4_000_000,
            held: 0,
            wanted: 8,
        };
        let tx = ChainState {
            rate: 4_000_000,
            held: 8,
            wanted: 8,
        };
        let (words, writes, waits) = run(UpdateFeDsp::new(rx, tx));
        // RX: reset 0xf, release to 2, 1280 bytes, released; TX: 2, then 0.
        assert_eq!(writes, 2 + 1280 + 1 + 2);
        assert_eq!(waits, 2 + 1280 + 1 + 1);
        assert_eq!(&words[..2], [21 << 24 | 0xf, 21 << 24 | 2]);
        assert_eq!(words[2] >> 24, 20, "the microcode's bank");
        assert_eq!(UpdateFeDsp::held(rx, tx), (8, 8));
    }

    #[test]
    fn a_chain_without_a_rate_is_left_alone() {
        let idle = ChainState {
            rate: 0,
            held: 0,
            wanted: 4,
        };
        let (_, writes, waits) = run(UpdateFeDsp::new(idle, idle));
        assert_eq!((writes, waits), (0, 0));
        assert_eq!(UpdateFeDsp::held(idle, idle), (0, 0));
    }
}
