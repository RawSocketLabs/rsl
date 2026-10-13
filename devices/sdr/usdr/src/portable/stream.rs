//! The stream engine's register sequences: the RX DMA, the RX front end, the sync unit and
//! the original TX core, set up, started and stopped as libusdr's stream group sets them.
//! The blocking board and the native USB handle run the same machines, so both write
//! exactly these registers in this order.
//!
//! The front end takes commands through one register, `cmd << 28 | value`; the RX DMA has
//! a control and a status register and two configuration registers in the engine's
//! configuration space (from 0x10000, behind each transport's index window); the sync unit
//! one register, which is also the TX core's control register: libusdr passes register 14
//! to the TX stream as its "sync base" and writes the core's commands there with bit 31
//! clear, where every sync-unit write sets it.
//!
//! Source: `ipblks/streams/sfe_rx_4.c`, `dma_rx_32.c`, `sfe_tx_4.c` and
//! `stream_sfetrx4_dma32.c`; addresses from `device/m2_lm6_1/m2_lm6_1.c`
//! (`M2PCI_REG_WR_RXDMA_CONFIRM`, `VIRT_CFG_SFX_BASE`, `CSR_RFE4_BASE`,
//! `M2PCI_REG_WR_SYNC_CTRL`) and its `/ll/sync/0/base`. The burst arithmetic and the
//! burst-format word are the driver's (`fpga/stream.rs`); these machines write what it
//! planned.

use bnb::{BitEnum, bitfield, u2, u3, u4, u28};

use super::sequence::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};

/// The gateware's name in failures, as the driver's other FPGA accesses name it.
const FPGA: &str = "FPGA";

/// The RX DMA engine's name when its status reads dead.
pub(crate) const RX_DMA: &str = "RX DMA engine";

/// RX DMA control: enable, reset, host ready (`M2PCI_REG_WR_RXDMA_CONFIRM + 1`).
const REG_RX_DMA_CONTROL: u32 = 5;
/// RX DMA status, read-only (`M2PCI_REG_WR_RXDMA_CONFIRM + 2`).
const REG_RX_DMA_STATUS: u32 = 6;
/// The stream sync unit (`/ll/sync/0/base`), also the TX core's control register
/// (`M2PCI_REG_WR_SYNC_CTRL`).
const REG_SYNC: u32 = 14;
/// RX DMA configuration: words per burst, less one (`VIRT_CFG_SFX_BASE + 32`).
const REG_RX_DMA_BURST_WORDS: u32 = 0x1_0020;
/// RX DMA configuration: bursts per block, less one (`VIRT_CFG_SFX_BASE + 64`).
const REG_RX_DMA_BURSTS: u32 = 0x1_0040;
/// The RX front end's command register (`CSR_RFE4_BASE`).
const REG_RX_FRONT_END: u32 = 0x1_0100;

/// RX DMA status as an absent or dead engine reads it: libusdr fails only a status that
/// reads active (bit 30) and all ones, which is all ones.
const RX_DMA_DEAD: u32 = 0xffff_ffff;

/// A front-end command (`FE_CMD_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4, closed)]
#[repr(u8)]
enum FrontEndCommand {
    /// Samples per burst, less one.
    BurstSamples = 0,

    /// The burst format, the driver's `BurstFormat` word.
    BurstFormat = 1,

    /// Burst throttling (`sfe_rx4_throttle`).
    Throttle = 2,

    /// Reset and run control; see [`FrontEndControl`].
    Control = 3,
}

/// One write to the front end's command register.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrontEndWrite {
    /// The command. Bits 31:28.
    #[bits(28..=31)]
    command: FrontEndCommand,

    /// Its value. Bits 27:0.
    #[bits(0..=27)]
    value: u28,
}

impl FrontEndWrite {
    /// The register word carrying `command` with `value`, which fits 28 bits.
    fn word(command: FrontEndCommand, value: u32) -> u32 {
        Self::new()
            .with_command(command)
            .with_value(u28::new(value))
            .to_raw()
    }
}

/// The front end's run state (`RX_SCMD_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3, closed)]
#[repr(u8)]
enum RunCommand {
    /// Idle.
    Idle = 0,

    /// Start at a time.
    StartAt = 1,

    /// Start now.
    StartNow = 2,

    /// Stop at a time.
    StopAt = 3,

    /// Stop now.
    StopNow = 4,
}

/// The front end's run state and block resets (`FE_CMD_RESET`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrontEndControl {
    /// Hold the burster in reset. Bit 15.
    #[bits(15..=15)]
    reset_burster: bool,

    /// Hold the RX sample assembler in reset. Bit 14.
    #[bits(14..=14)]
    reset_assembler: bool,

    /// Hold the DDR input in reset. Bit 13.
    #[bits(13..=13)]
    reset_ddr: bool,

    /// Hold the DSP in reset. Bit 8.
    #[bits(8..=8)]
    reset_dsp: bool,

    /// The run state. Bits 2:0.
    #[bits(0..=2)]
    run: RunCommand,
}

/// The RX DMA engine's control register.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DmaControl {
    /// The host is ready for the first block. Bit 2.
    #[bits(2..=2)]
    ready: bool,

    /// Stop the DMA and reset the front end. Bit 1.
    #[bits(1..=1)]
    reset: bool,

    /// Run the DMA. Bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

/// The sync unit's modes (`ST_*`), those the stream group uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncTrigger {
    /// Streams held stopped (`ST_STOP`, libusdr's `"off"`).
    Stop = 0,

    /// Streams start on a 1PPS pulse (`ST_ONEPPS`: `"1pps"` or `"sysref"`).
    OnePps = 1,

    /// Streams start on the RX front end's activity (`ST_ADCACT`: `"all"` with RX alone,
    /// or `"rx"`).
    RxActivity = 2,

    /// Streams start on the TX front end's activity (`ST_DACACT`: `"all"` with TX alone,
    /// or `"tx"`).
    TxActivity = 3,

    /// Streams start on either front end's activity (`ST_ADCORDACACT`, `"any"`).
    EitherActivity = 5,

    /// RX and TX start together (`ST_DELAYADCDAC`: `"all"` with both).
    Duplex = 6,

    /// Streams run as soon as started (`ST_FREERUN`, `"none"`).
    FreeRun = 7,

    /// Streams start on a SYSREF the sync unit generates (`ST_ONEPPS | ST_SYSREF_GEN`,
    /// `"sysref+gen"`).
    GeneratedSysref = 9,
}

/// What a control write tells the TX core (`TX_CMD_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
#[repr(u8)]
enum TxCommand {
    /// Stop.
    Stop = 0,

    /// Take the mute and swap flags, nothing else.
    UpdateFlags = 1,

    /// Start, 12-bit samples; libusdr never sends it to this core.
    Start12 = 2,

    /// Start, 16-bit samples.
    Start16 = 3,
}

/// One write to the TX core's control register (`GP_PORT_TXDMA_CTRL_*`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TxControl {
    /// Swap the lanes. `MODE_SWAP_AB`, bit 10.
    #[bits(10..=10)]
    swap_lanes: bool,

    /// Mute lane A. `MODE_MUTEA`, bit 9.
    #[bits(9..=9)]
    mute_a: bool,

    /// Mute lane B. `MODE_MUTEB`, bit 8.
    #[bits(8..=8)]
    mute_b: bool,

    /// Reset the core's buffer pointers. `RESET_BUFS`, bit 7.
    #[bits(7..=7)]
    reset_buffers: bool,

    /// The front end's channel count, as a power of two. `MODE_FMT`, bits 4:3.
    #[bits(3..=4)]
    channels_log2: u2,

    /// Replay the buffers. `MODE_REP`, bit 2.
    #[bits(2..=2)]
    repeat: bool,

    /// The command. Bits 1:0.
    #[bits(0..=1)]
    command: TxCommand,
}

impl TxControl {
    /// The control word sending `command` with `mute` (bit 0 lane A, bit 1 lane B), for one
    /// unswapped channel.
    fn word(command: TxCommand, mute: u8) -> u32 {
        Self::new()
            .with_mute_a(mute & 1 != 0)
            .with_mute_b(mute & 2 != 0)
            .with_command(command)
            .to_raw()
    }
}

/// What the RX front end and DMA take for one stream's packets: the driver's burst plan
/// and its burst-format word (`BTOTAL`, `BWORDS`, `CHFMT`, `IFMT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RxFormat {
    /// Samples in one burst, at least one.
    samples_per_burst: u32,

    /// Front-end words in one burst, at least one.
    words_per_burst: u32,

    /// Bursts in one DMA block, at least one.
    bursts: u32,

    /// The `FE_CMD_BURST_FORMAT` value, within its 28 bits.
    burst_format: u32,
}

impl RxFormat {
    /// The format for bursts of `samples_per_burst` samples in `words_per_burst` words,
    /// `bursts` to a block, announced by `burst_format`; `None` for a zero count, whose
    /// less-one register value would wrap, or a format word past 28 bits.
    pub(crate) const fn new(
        samples_per_burst: u32,
        words_per_burst: u32,
        bursts: u32,
        burst_format: u32,
    ) -> Option<Self> {
        if samples_per_burst == 0 || words_per_burst == 0 || bursts == 0 {
            return None;
        }
        if burst_format >= 1 << 28 {
            return None;
        }
        Some(Self {
            samples_per_burst,
            words_per_burst,
            bursts,
            burst_format,
        })
    }
}

/// The most steps one engine sequence takes: an RX configuration's four front-end
/// commands, two DMA sizes and the sync write; a duplex start takes six (the RX run's two,
/// the TX core's three and the sync write).
const MAX_STEPS: usize = 7;

/// One step of an engine sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Read the RX DMA status: an engine reading dead fails the sequence (`dma_rx32_reset`).
    CheckRxDma,
    /// Write one register.
    Write {
        /// The register.
        reg: u32,
        /// The word written.
        value: u32,
    },
}

impl Step {
    /// The FPGA access this step is.
    const fn access(self) -> FpgaAccess {
        match self {
            Self::CheckRxDma => FpgaAccess::Read {
                reg: REG_RX_DMA_STATUS,
            },
            Self::Write { reg, value } => FpgaAccess::Write { reg, value },
        }
    }
}

/// A run of stream-engine register accesses, built from the engine's operations in the
/// order the stream group performs them. Fixed size; it does not allocate.
#[derive(Debug)]
pub(crate) struct EngineSequence {
    /// The steps, the first `len` in use.
    steps: [Step; MAX_STEPS],
    /// Steps in use.
    len: usize,
    /// Steps completed.
    done: usize,
    /// A step failed; nothing follows.
    failed: bool,
}

impl EngineSequence {
    /// No steps yet.
    pub(crate) const fn new() -> Self {
        Self {
            steps: [Step::CheckRxDma; MAX_STEPS],
            len: 0,
            done: 0,
            failed: false,
        }
    }

    /// The first register part of libusdr's RX stream create: check the DMA engine is
    /// alive, stop it and reset the front end. libusdr checks the packet size next, before
    /// [`Self::rx_configure`].
    pub(crate) fn rx_reset() -> Self {
        Self::new().reset_rx_dma()
    }

    /// The second register part of libusdr's RX stream create: configure the front end
    /// and DMA for `format` and hold the sync unit stopped.
    pub(crate) fn rx_configure(format: &RxFormat) -> Self {
        Self::new().configure_rx(format).sync(SyncTrigger::Stop)
    }

    /// Starts a group with an RX stream if `rx` and a TX stream muted as `tx_mute` says,
    /// if any, on `trigger`: RX alone syncs, then runs (`usdr_dms_sync`, then
    /// `USDR_DMS_START`, the order `dm_stream.h` prescribes); with TX, RX runs, then TX
    /// starts, then the group syncs, the order every libusdr client uses, because the TX
    /// core's start writes the sync unit's register.
    pub(crate) fn start(rx: bool, tx_mute: Option<u8>, trigger: SyncTrigger) -> Self {
        let rx_run = |sequence: Self| if rx { sequence.run_rx(true) } else { sequence };
        match tx_mute {
            None => rx_run(Self::new().sync(trigger)),
            Some(mute) => rx_run(Self::new()).start_tx(mute).sync(trigger),
        }
    }

    /// Checks the RX DMA engine is alive, then stops it and resets the front end
    /// (`dma_rx32_reset`).
    fn reset_rx_dma(self) -> Self {
        let reset = DmaControl::new().with_reset(true).to_raw();
        self.push(Step::CheckRxDma)
            .write(REG_RX_DMA_CONTROL, reset)
            .write(REG_RX_DMA_CONTROL, DmaControl::new().to_raw())
    }

    /// Puts the front end in reset, sets the burst size and format and releases it
    /// (`_configure_simple_fe_generic` for one channel), then sizes the DMA blocks
    /// (`dma_rx32_configure`).
    fn configure_rx(self, format: &RxFormat) -> Self {
        let all_reset = FrontEndControl::new()
            .with_reset_burster(true)
            .with_reset_assembler(true)
            .with_reset_ddr(true)
            .with_reset_dsp(true)
            .to_raw();
        let released = FrontEndControl::new().to_raw();
        self.front_end(FrontEndCommand::Control, all_reset)
            .front_end(FrontEndCommand::BurstSamples, format.samples_per_burst - 1)
            .front_end(FrontEndCommand::Control, released)
            .front_end(FrontEndCommand::BurstFormat, format.burst_format)
            .write(REG_RX_DMA_BURST_WORDS, format.words_per_burst - 1)
            .write(REG_RX_DMA_BURSTS, format.bursts - 1)
    }

    /// Runs or stops the RX DMA and front end, DMA first (`_sfetrx4_op`).
    pub(crate) fn run_rx(self, run: bool) -> Self {
        let command = if run {
            RunCommand::StartNow
        } else {
            RunCommand::StopNow
        };
        let control = FrontEndControl::new().with_run(command).to_raw();
        self.write(
            REG_RX_DMA_CONTROL,
            DmaControl::new().with_enabled(run).to_raw(),
        )
        .front_end(FrontEndCommand::Control, control)
    }

    /// Sets the sync unit's mode (`sfetrx4_stream_sync`).
    pub(crate) fn sync(self, trigger: SyncTrigger) -> Self {
        self.write(REG_SYNC, 1 << 31 | (trigger as u32) << 16)
    }

    /// Starts the TX core for one channel of 16-bit samples with `mute` (bit 0 lane A,
    /// bit 1 lane B): stop, reset its buffer pointers, then start (`sfe_tx4_ctl`,
    /// unswapped, no replay).
    fn start_tx(self, mute: u8) -> Self {
        let reset = TxControl::new().with_reset_buffers(true).to_raw();
        self.stop_tx()
            .write(REG_SYNC, reset)
            .write(REG_SYNC, TxControl::word(TxCommand::Start16, mute))
    }

    /// Stops the TX core (`sfe_tx4_ctl` stopping, and the TX stream's destroy).
    pub(crate) fn stop_tx(self) -> Self {
        self.write(REG_SYNC, TxControl::new().to_raw())
    }

    /// Sets the TX core's mute flags, unswapped (`sfe_tx4_upd`).
    pub(crate) fn update_tx_flags(self, mute: u8) -> Self {
        self.write(REG_SYNC, TxControl::word(TxCommand::UpdateFlags, mute))
    }

    /// Throttles the front end's bursts (`sfe_rx4_throttle`): with `Some((send, skip))`
    /// it delivers `send + 1` of every `send + skip + 2` bursts and drops the rest; `None`
    /// delivers every burst. The command's run state stays idle.
    pub(crate) fn throttle_rx(self, throttle: Option<(u8, u8)>) -> Self {
        let value = throttle.map_or(0, |(send, skip)| {
            1 << 16 | u32::from(send) << 8 | u32::from(skip)
        });
        self.front_end(FrontEndCommand::Throttle, value)
    }

    /// Tells the RX DMA engine the host is ready, before the first block arrives.
    pub(crate) fn signal_rx_ready(self) -> Self {
        self.write(
            REG_RX_DMA_CONTROL,
            DmaControl::new().with_ready(true).to_raw(),
        )
    }

    /// Writes `words` to `reg` in order: a register port that takes a run of words, such as
    /// the DSP chains' configuration ports a stream create writes. At most
    /// [`MAX_STEPS`] words.
    pub(crate) fn port_writes(reg: u32, words: &[u32]) -> Self {
        words
            .iter()
            .fold(Self::new(), |sequence, &word| sequence.write(reg, word))
    }

    /// Appends a front-end command.
    fn front_end(self, command: FrontEndCommand, value: u32) -> Self {
        self.write(REG_RX_FRONT_END, FrontEndWrite::word(command, value))
    }

    /// Appends a register write.
    fn write(self, reg: u32, value: u32) -> Self {
        self.push(Step::Write { reg, value })
    }

    /// Appends `step`.
    fn push(mut self, step: Step) -> Self {
        *self
            .steps
            .get_mut(self.len)
            .expect("invariant: no engine operation composes past MAX_STEPS") = step;
        self.len += 1;
        self
    }

    /// The step in progress, if any.
    fn step(&self) -> Option<Step> {
        if self.failed {
            return None;
        }
        self.steps[..self.len].get(self.done).copied()
    }
}

impl Sequence for EngineSequence {
    fn next(&self) -> Option<SequenceAction> {
        Some(SequenceAction::Fpga(self.step()?.access()))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(step) = self.step() else {
            return Ok(());
        };
        let word = match result {
            Ok(word) => word,
            Err(source) => {
                self.failed = true;
                return Err(SequenceFailure::FpgaTransport {
                    chip: FPGA,
                    access: step.access(),
                    source,
                });
            }
        };
        if step == Step::CheckRxDma && word == RX_DMA_DEAD {
            self.failed = true;
            return Err(SequenceFailure::ChipMissing { chip: RX_DMA });
        }
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values: libusdr's shifts worked by hand.

    /// The accesses a sequence made, in order, and how it ended; no allocation, as the
    /// `no_std` witness compiles these tests too.
    struct Run {
        /// The accesses, the first `len` made.
        accesses: [FpgaAccess; MAX_STEPS],
        /// Accesses made.
        len: usize,
        /// The sequence's end.
        outcome: Result<(), SequenceFailure<()>>,
    }

    impl Run {
        /// The accesses made.
        fn accesses(&self) -> &[FpgaAccess] {
            &self.accesses[..self.len]
        }
    }

    /// Runs `sequence` to its end, every read returning `status`.
    fn run(mut sequence: EngineSequence, status: u32) -> Run {
        let mut run = Run {
            accesses: [FpgaAccess::Read { reg: 0 }; MAX_STEPS],
            len: 0,
            outcome: Ok(()),
        };
        while let Some(SequenceAction::Fpga(access)) = sequence.next() {
            run.accesses[run.len] = access;
            run.len += 1;
            let word = match access {
                FpgaAccess::Read { .. } => status,
                _ => 0,
            };
            if let Err(failure) = sequence.complete::<()>(Ok(word)) {
                run.outcome = Err(failure);
                break;
            }
        }
        run
    }

    /// A write of `value` to `reg`.
    const fn write(reg: u32, value: u32) -> FpgaAccess {
        FpgaAccess::Write { reg, value }
    }

    #[test]
    fn an_rx_setup_writes_libusdrs_words_in_its_order() {
        // One 1024-sample ci16 burst: 512 words, 16 bursts in the FIFO, lanes 1 and 0.
        let format = RxFormat::new(1024, 512, 1, 0x0040_3fe7).expect("valid");
        let reset = run(EngineSequence::rx_reset(), 0);
        assert_eq!(reset.outcome, Ok(()));
        assert_eq!(
            reset.accesses(),
            [FpgaAccess::Read { reg: 6 }, write(5, 0b010), write(5, 0)],
            "the status, then the DMA reset and release"
        );
        let configure = run(EngineSequence::rx_configure(&format), 0);
        assert_eq!(configure.outcome, Ok(()));
        assert_eq!(
            configure.accesses(),
            [
                write(0x1_0100, 0x3000_e100),
                write(0x1_0100, 1023),
                write(0x1_0100, 0x3000_0000),
                write(0x1_0100, 0x1040_3fe7),
                write(0x1_0020, 511),
                write(0x1_0040, 0),
                write(14, 0x8000_0000),
            ]
        );
    }

    #[test]
    fn a_format_whose_less_one_registers_would_wrap_is_refused() {
        assert_eq!(RxFormat::new(0, 1, 1, 0), None);
        assert_eq!(RxFormat::new(1, 0, 1, 0), None);
        assert_eq!(RxFormat::new(1, 1, 0, 0), None);
        assert_eq!(
            RxFormat::new(1, 1, 1, 1 << 28),
            None,
            "past BTOTAL's 28 bits"
        );
    }

    #[test]
    fn a_dead_rx_dma_engine_stops_the_reset_after_its_status() {
        let dead = run(EngineSequence::rx_reset(), 0xffff_ffff);
        assert_eq!(
            dead.outcome,
            Err(SequenceFailure::ChipMissing { chip: RX_DMA })
        );
        assert_eq!(dead.accesses(), [FpgaAccess::Read { reg: 6 }]);
        // Active but not all ones is only logged by libusdr.
        let active = run(EngineSequence::rx_reset(), 0x4000_0000);
        assert_eq!(active.outcome, Ok(()));
        assert_eq!(active.accesses().len(), 3);
    }

    #[test]
    fn a_failed_write_names_its_register_and_ends_the_sequence() {
        let mut reset = EngineSequence::rx_reset();
        reset.complete::<()>(Ok(0)).unwrap();
        assert_eq!(
            reset.complete(Err(())),
            Err(SequenceFailure::FpgaTransport {
                chip: FPGA,
                access: write(5, 0b010),
                source: (),
            })
        );
        assert_eq!(reset.next(), None);
    }

    #[test]
    fn rx_alone_syncs_before_it_runs_and_tx_starts_before_the_sync() {
        let rx = run(EngineSequence::start(true, None, SyncTrigger::FreeRun), 0);
        assert_eq!(
            rx.accesses(),
            [
                write(14, 0x8007_0000),
                write(5, 1),
                write(0x1_0100, 0x3000_0002),
            ]
        );
        let duplex = run(EngineSequence::start(true, Some(2), SyncTrigger::Duplex), 0);
        assert_eq!(
            duplex.accesses(),
            [
                write(5, 1),
                write(0x1_0100, 0x3000_0002),
                write(14, 0),
                write(14, 0x80),
                // `TX_CMD_START_16BIT` (3) with `MODE_MUTEB` (bit 8).
                write(14, 0x103),
                write(14, 0x8006_0000),
            ]
        );
        let tx = run(
            EngineSequence::start(false, Some(3), SyncTrigger::TxActivity),
            0,
        );
        assert_eq!(
            tx.accesses(),
            [
                write(14, 0),
                write(14, 0x80),
                write(14, 0x303),
                write(14, 0x8003_0000)
            ]
        );
    }

    #[test]
    fn stopping_rx_halts_the_dma_then_the_front_end() {
        let halt = run(EngineSequence::new().run_rx(false), 0);
        assert_eq!(halt.accesses(), [write(5, 0), write(0x1_0100, 0x3000_0004)]);
    }

    #[test]
    fn the_largest_composition_fits() {
        let format = RxFormat::new(1, 1, 1, 0).expect("valid");
        assert_eq!(EngineSequence::rx_configure(&format).len, MAX_STEPS);
    }

    #[test]
    fn libusdrs_control_words() {
        assert_eq!(TxControl::word(TxCommand::UpdateFlags, 3), 0x301);
        assert_eq!(
            FrontEndWrite::word(FrontEndCommand::Throttle, 1 << 16 | 2 << 8 | 5),
            0x2001_0205
        );
    }
}
