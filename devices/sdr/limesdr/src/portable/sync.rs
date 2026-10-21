//! Starting and stopping the LimeSDR-mini's streams: libusdr's `limesdr_setup_stream` and
//! `limesdr_disable_stream` (`device/u3_limesdr/limesdr_ctrl.c`), which its stream sync
//! (`u3_limesdr.c::limesdr_device_stream_sync`) runs for any sync type but `"off"` and
//! for `"off"`.

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use lms7002m::DcDac;
use lms7002m::ctrl::Control;
use lms7002m::{BATCH_CAPACITY, Batch};

use super::fpga;

/// What the setup writes to [`fpga::STREAM_MODE`] as libusdr's stream sync calls it
/// (`limesdr_setup_stream(d, false, true, false)`): SISO SDR (0x0040), 16-bit samples
/// (bit 1 clear). The trigger pulse mode (0x0180), the default 0x0100 and 12-bit samples
/// are not reached from libusdr's API.
const SISO_SDR: u16 = 0x0040;

/// The TX DC correction both channel-A DACs get (`lms7002m_dc_corr(.., P_TXA_I, 127)`
/// and `P_TXA_Q`).
const TX_DC: i16 = 127;

/// One of the two values a step reads and a later one writes back modified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// Stream control (0x000A).
    Control,
    /// The timestamp and packet-loss clears (0x0009).
    Clears,
}

/// One step.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// A gateware register written with a value.
    Write(u16, u16),
    /// A gateware register read into a slot.
    Read(u16, Slot),
    /// A gateware register written with a slot's value, `(value & !clear) | set`.
    WriteBack {
        /// The register.
        reg: u16,
        /// The slot.
        slot: Slot,
        /// Bits cleared.
        clear: u16,
        /// Bits set.
        set: u16,
    },
    /// Refuses, issuing nothing more, when a slot has any of these bits set.
    Refuse(Slot, u16),
    /// LMS7002M writes.
    Lms(Batch),
}

/// The most steps a sync takes: the setup's.
const MAX_STEPS: usize = 19;

/// `limesdr_setup_stream` or `limesdr_disable_stream`, as a sequence.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StreamSync {
    /// The steps; the first `len` are used.
    steps: [Option<Step>; MAX_STEPS],
    /// How many there are.
    len: usize,
    /// The step in progress.
    at: usize,
    /// The values read: stream control, then the clears.
    saved: [u16; 2],
    /// A refusal step refused.
    refused: bool,
    /// An access failed, which ends the sync.
    failed: bool,
}

impl StreamSync {
    /// `limesdr_setup_stream(d, false, true, false)`: chip 0 selected, streaming stopped,
    /// the timestamp reset (after the check libusdr's debug build makes, `#ifndef NDEBUG`,
    /// that streaming is stopped), the channel-A TX DC DACs set and the DC correctors
    /// enabled for the running paths, SISO SDR on channel 0, then streaming started and
    /// bits 1 and 3 of the clears pulsed (`5 << 1`: `TXPCT_LOSS_CLR` and a bit the source
    /// does not name).
    pub(crate) fn setup(control: &Control) -> Self {
        let chip = control.chip();
        let reset = fpga::CLEAR_TX_LOSS | fpga::CLEAR_SAMPLES;
        let pulse = 5 << 1;
        Self::new(&[
            Step::Write(fpga::CHIP_SELECT, 1),
            Step::Read(fpga::STREAM_CONTROL, Slot::Control),
            Step::WriteBack {
                reg: fpga::STREAM_CONTROL,
                slot: Slot::Control,
                clear: fpga::RX_ENABLE | fpga::TX_ENABLE,
                set: 0,
            },
            Step::Read(fpga::STREAM_CONTROL, Slot::Control),
            Step::Refuse(Slot::Control, fpga::RX_ENABLE),
            Step::Read(fpga::STREAM_CLEARS, Slot::Clears),
            clears(reset, 0),
            clears(0, reset),
            clears(reset, 0),
            Step::Lms(chip.dc_dac(DcDac::TxAI, TX_DC)),
            Step::Lms(chip.dc_dac(DcDac::TxAQ, TX_DC)),
            Step::Lms(chip.dc_corrector_enable(control.rx_running(), control.tx_running())),
            Step::Write(fpga::STREAM_MODE, SISO_SDR),
            Step::Write(fpga::STREAM_CHANNELS, 1),
            Step::Read(fpga::STREAM_CLEARS, Slot::Clears),
            Step::Read(fpga::STREAM_CONTROL, Slot::Control),
            Step::WriteBack {
                reg: fpga::STREAM_CONTROL,
                slot: Slot::Control,
                clear: 0,
                set: fpga::RX_ENABLE,
            },
            clears(0, pulse),
            clears(pulse, 0),
        ])
    }

    /// `limesdr_disable_stream`: chip 0 selected, then streaming stopped.
    pub(crate) fn disable() -> Self {
        Self::new(&[
            Step::Write(fpga::CHIP_SELECT, 1),
            Step::Read(fpga::STREAM_CONTROL, Slot::Control),
            Step::WriteBack {
                reg: fpga::STREAM_CONTROL,
                slot: Slot::Control,
                clear: fpga::RX_ENABLE | fpga::TX_ENABLE,
                set: 0,
            },
        ])
    }

    /// A sync of `steps`.
    fn new(steps: &[Step]) -> Self {
        let mut all = [None; MAX_STEPS];
        for (slot, &step) in all.iter_mut().zip(steps) {
            *slot = Some(step);
        }
        Self {
            steps: all,
            len: steps.len(),
            at: 0,
            saved: [0; 2],
            refused: false,
            failed: false,
        }
    }

    /// Whether the setup refused because streaming read as running after it was stopped
    /// (libusdr's `-EPERM`, "Streaming must be stopped to reset timestamp").
    pub(crate) const fn refused(&self) -> bool {
        self.refused
    }

    /// The step in progress, if the sync goes on.
    fn step(&self) -> Option<Step> {
        if self.refused || self.failed || self.at >= self.len {
            return None;
        }
        self.steps[self.at]
    }

    /// The gateware access a step makes.
    fn access(&self, step: Step) -> Option<FpgaAccess> {
        let wide = u32::from;
        match step {
            Step::Write(reg, value) => Some(FpgaAccess::Write {
                reg: wide(reg),
                value: wide(value),
            }),
            Step::Read(reg, _) => Some(FpgaAccess::Read { reg: wide(reg) }),
            Step::WriteBack {
                reg,
                slot,
                clear,
                set,
            } => Some(FpgaAccess::Write {
                reg: wide(reg),
                value: wide(self.saved[slot as usize] & !clear | set),
            }),
            Step::Refuse(..) | Step::Lms(_) => None,
        }
    }

    /// Moves past finished steps and decides refusal steps, which access nothing.
    fn settle(&mut self) {
        while let Some(step) = self.step() {
            match step {
                Step::Refuse(slot, mask) => {
                    self.refused = self.saved[slot as usize] & mask != 0;
                    self.at += 1;
                }
                Step::Lms(batch) if batch.next().is_none() => self.at += 1,
                _ => return,
            }
        }
    }
}

/// The clears register written back from its read value.
const fn clears(clear: u16, set: u16) -> Step {
    Step::WriteBack {
        reg: fpga::STREAM_CLEARS,
        slot: Slot::Clears,
        clear,
        set,
    }
}

const _: () = assert!(
    BATCH_CAPACITY >= 4,
    "the DC corrector enable is four writes"
);

impl Sequence for StreamSync {
    fn next(&self) -> Option<SequenceAction> {
        let mut this = *self;
        this.settle();
        match this.step()? {
            Step::Lms(batch) => batch.next(),
            step => this.access(step).map(SequenceAction::Fpga),
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        self.settle();
        let Some(step) = self.step() else {
            return Ok(());
        };
        if let Step::Lms(mut batch) = step {
            let completed = batch.complete(result);
            self.failed = completed.is_err();
            self.steps[self.at] = Some(Step::Lms(batch));
            self.settle();
            return completed;
        }
        let access = self
            .access(step)
            .expect("invariant: settled steps other than LMS7002M writes access the gateware");
        let value = result.map_err(|source| {
            self.failed = true;
            SequenceFailure::FpgaTransport {
                chip: fpga::NAME,
                access,
                source,
            }
        })?;
        if let Step::Read(_, slot) = step {
            // A 16-bit register, zero-extended.
            let [low, high, ..] = value.to_le_bytes();
            self.saved[slot as usize] = u16::from_le_bytes([low, high]);
        }
        self.at += 1;
        self.settle();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::num::NonZeroU32;
    use std::vec::Vec;

    use hal::SpiAddr;
    use lms7002m::Lms7002m;

    use super::super::frontend::{RX_BANDS, TX_BANDS};
    use super::*;

    /// The control state of a freshly opened board: nothing running.
    fn control() -> Control {
        let reference = NonZeroU32::new(40_000_000).expect("non-zero");
        Control::new(Lms7002m::new(SpiAddr(0)), reference, RX_BANDS, TX_BANDS)
    }

    /// Runs `sync` against gateware registers that read `control` (0x000A) and `clears`
    /// (0x0009) and keep what is written, the LMS7002M writes answered 0; every gateware
    /// access, as `(write, reg, value)`, and the count of LMS7002M writes.
    fn run(
        mut sync: StreamSync,
        mut control: u16,
        mut clears: u16,
    ) -> (Vec<(bool, u16, u16)>, usize) {
        let mut fpga = Vec::new();
        let mut lms = 0;
        while let Some(action) = sync.next() {
            let value = match action {
                SequenceAction::Fpga(FpgaAccess::Read { reg }) => {
                    let reg = u16::try_from(reg).expect("16-bit");
                    let value = if reg == fpga::STREAM_CONTROL {
                        control
                    } else {
                        clears
                    };
                    fpga.push((false, reg, value));
                    u32::from(value)
                }
                SequenceAction::Fpga(FpgaAccess::Write { reg, value }) => {
                    let (reg, value) = (
                        u16::try_from(reg).expect("16-bit"),
                        u16::try_from(value).expect("16-bit"),
                    );
                    match reg {
                        fpga::STREAM_CONTROL => control = value,
                        fpga::STREAM_CLEARS => clears = value,
                        _ => {}
                    }
                    fpga.push((true, reg, value));
                    0
                }
                SequenceAction::Fpga(FpgaAccess::Spi { .. }) => {
                    lms += 1;
                    0
                }
                other => panic!("unexpected {other:?}"),
            };
            sync.complete::<()>(Ok(value)).expect("no failure");
        }
        (fpga, lms)
    }

    #[test]
    fn the_setup_stops_resets_the_timestamp_configures_then_starts_and_pulses_the_clears() {
        let (fpga, lms) = run(StreamSync::setup(&control()), 0x0003, 0x0100);
        assert_eq!(
            fpga,
            [
                (true, 0xffff, 1),
                (false, 0x000a, 0x0003),
                (true, 0x000a, 0x0000),
                (false, 0x000a, 0x0000),
                (false, 0x0009, 0x0100),
                (true, 0x0009, 0x0100),
                (true, 0x0009, 0x0103),
                (true, 0x0009, 0x0100),
                (true, 0x0008, 0x0040),
                (true, 0x0007, 0x0001),
                (false, 0x0009, 0x0100),
                (false, 0x000a, 0x0000),
                (true, 0x000a, 0x0001),
                (true, 0x0009, 0x010a),
                (true, 0x0009, 0x0100),
            ]
        );
        assert_eq!(
            lms,
            2 + 2 + 4,
            "two DC DACs, each twice, then the four enables"
        );
    }

    #[test]
    fn a_setup_whose_stop_did_not_take_refuses_before_the_timestamp_reset() {
        let mut sync = StreamSync::setup(&control());
        let mut accesses = 0;
        while let Some(action) = sync.next() {
            accesses += 1;
            let read = matches!(action, SequenceAction::Fpga(FpgaAccess::Read { .. }));
            // Stream control keeps reading RX_EN set, whatever is written.
            sync.complete::<()>(Ok(u32::from(read)))
                .expect("no failure");
        }
        assert!(sync.refused());
        assert_eq!(
            accesses, 4,
            "select, stop's read and write, the check's read"
        );
    }

    #[test]
    fn the_disable_selects_the_chip_and_stops_streaming() {
        let (fpga, lms) = run(StreamSync::disable(), 0x0103, 0);
        assert_eq!(
            fpga,
            [
                (true, 0xffff, 1),
                (false, 0x000a, 0x0103),
                (true, 0x000a, 0x0100)
            ]
        );
        assert_eq!(lms, 0);
    }
}
