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

/// One part of a stream group's start: `USDR_DMS_START` of one stream, or `usdr_dms_sync`.
/// A device runs a group's parts as one sequence ([`EngineSequence::start`]); libusdr's
/// multi-device aggregate runs each part on every device before the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartPart {
    /// Run the RX DMA and front end.
    RunRx,

    /// Start the TX core on `lanes`: their mute, swap and the stream's channel count.
    StartTx {
        /// How the stream uses the core's lanes.
        lanes: TxLanes,
    },

    /// Set the sync unit's trigger.
    Sync(SyncTrigger),
}

impl StartPart {
    /// The parts that start a group with an RX stream if `rx` and a TX stream on `tx`
    /// lanes, if any, on `trigger`, in order: RX alone syncs, then runs (`usdr_dms_sync`,
    /// then `USDR_DMS_START`, the order `dm_stream.h` prescribes); with TX, RX runs, then
    /// TX starts, then the group syncs, the order every libusdr client uses, because the
    /// TX core's start writes the sync unit's register.
    pub(crate) const fn order(
        rx: bool,
        tx: Option<TxLanes>,
        trigger: SyncTrigger,
    ) -> [Option<Self>; 3] {
        let run_rx = if rx { Some(Self::RunRx) } else { None };
        match tx {
            None => [Some(Self::Sync(trigger)), run_rx, None],
            Some(lanes) => [
                run_rx,
                Some(Self::StartTx { lanes }),
                Some(Self::Sync(trigger)),
            ],
        }
    }
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
    /// The control word sending `command` with `lanes`' mute, swap and channel count.
    fn lanes_word(command: TxCommand, lanes: TxLanes) -> u32 {
        Self::new()
            .with_swap_lanes(lanes.swap)
            .with_mute_a(lanes.mute & 1 != 0)
            .with_mute_b(lanes.mute & 2 != 0)
            .with_channels_log2(u2::new(lanes.channels_log2 & 0b11))
            .with_command(command)
            .to_raw()
    }
}

/// How a TX stream uses the TX front end's two lanes: what `sfe_tx4_ctl` and `sfe_tx4_upd`
/// write besides their command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TxLanes {
    /// The lanes muted: bit 0 lane A, bit 1 lane B.
    pub(crate) mute: u8,

    /// Lanes A and B swapped, so a stream's first channel plays on lane B.
    pub(crate) swap: bool,

    /// The stream's channel count as a power of two (`MODE_FMT`): 0 for one, 1 for two;
    /// only starting the core writes it.
    pub(crate) channels_log2: u8,
}

/// The extended TX core's front-end command register (`CSR_TFE4_BASE`,
/// `VIRT_CFG_SFX_BASE + 3 * 256`): a command in bits 31:24, its value below
/// (`MAKE_EXFE_REG`).
const REG_EXTENDED_TX_FRONT_END: u32 = 0x1_0300;

/// An extended TX front-end command (`exfe_rx_cmd_regs` in `sfe_rx_4.c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtendedTxCommand {
    /// The lanes the stream's channels expand to, as a power of two (`EXFE_CMD_EXPAND`).
    Expand = 0x0d,

    /// The sample width: 0 for 16 bits, 1 for 12 (`EXFE_CMD_TXFMT12`).
    Format12 = 0x0e,

    /// The mute mask, as libusdr's `"mute"` option passes it (`EXFE_CMD_MUTE`).
    Mute = 0x0f,

    /// The first of four lane-shuffle stages (`EXFE_CMD_SHUFFLE_0`).
    Shuffle0 = 0x10,
}

/// The extended core's four shuffle stages for a complex TX stream whose channel map is
/// `map` (one or two channel numbers, each 0 or 1, in the stream's order): libusdr's
/// `exfe_trx4_update_chmap` with `mask` set, four raw lanes and no 3x16 packing.
///
/// libusdr inverts the map for TX into a table of all four lanes whose unmapped entries
/// are `0x7f` (its `memset` of `~CH_SWAP_IQ_FLAG`), not the `0xff` it skips, so lanes past
/// the stream's take channel `0x7f` masked to the stream's channels; that is kept.
fn extended_tx_shuffle(map: &[u8]) -> [u16; 4] {
    const LANES: usize = 4;
    const UNMAPPED: u8 = 0x7f;
    // Lanes in use: two per complex channel; the channel mask libusdr applies.
    let total = 2 * map.len();
    let mask = map.len().saturating_sub(1);
    let mut reverse = [UNMAPPED; LANES];
    for (lane, &channel) in map.iter().enumerate() {
        if let Some(entry) = reverse.get_mut(usize::from(channel)) {
            *entry = u8::try_from(lane).expect("invariant: at most two channels");
        }
    }
    let mut stages = [0_u16; 4];
    for group in (0..LANES).step_by(total.max(1)) {
        for offset in 0..total {
            let lane = group + offset;
            let channel = usize::from(reverse[lane / 2] & 0x7f);
            let source = 2 * (channel & mask) + offset % 2;
            let shuffle = source ^ lane;
            // Two stages for four lanes (`lg_chans`); the other two stay 0.
            for (bit, stage) in stages.iter_mut().take(2).enumerate() {
                if shuffle & 1 << bit != 0 {
                    *stage |= 1 << lane;
                }
            }
        }
    }
    stages
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

    /// Starts a group with an RX stream if `rx` and a TX stream on `tx` lanes, if any, on
    /// `trigger`: its [`StartPart::order`] in one sequence.
    pub(crate) fn start_lanes(rx: bool, tx: Option<TxLanes>, trigger: SyncTrigger) -> Self {
        StartPart::order(rx, tx, trigger)
            .into_iter()
            .flatten()
            .fold(Self::new(), Self::start_part)
    }

    /// One part of a group's start.
    pub(crate) fn start_part(self, part: StartPart) -> Self {
        match part {
            StartPart::RunRx => self.run_rx(true),
            StartPart::StartTx { lanes } => self.start_tx(lanes),
            StartPart::Sync(trigger) => self.sync(trigger),
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

    /// Starts the TX core on `lanes`: stop, reset its buffer pointers, then start
    /// (`sfe_tx4_ctl`, no replay). libusdr sends the 16-bit start whatever the stream's
    /// format, the extended core's 12-bit streams included.
    fn start_tx(self, lanes: TxLanes) -> Self {
        let reset = TxControl::new().with_reset_buffers(true).to_raw();
        self.stop_tx()
            .write(REG_SYNC, reset)
            .write(REG_SYNC, TxControl::lanes_word(TxCommand::Start16, lanes))
    }

    /// Stops the TX core (`sfe_tx4_ctl` stopping, and the TX stream's destroy).
    pub(crate) fn stop_tx(self) -> Self {
        self.write(REG_SYNC, TxControl::new().to_raw())
    }

    /// Sets the TX core's mute and swap flags from `lanes` (`sfe_tx4_upd`); the update
    /// command carries no channel count.
    pub(crate) fn update_tx_lanes(self, lanes: TxLanes) -> Self {
        let flags = TxLanes {
            channels_log2: 0,
            ..lanes
        };
        self.write(
            REG_SYNC,
            TxControl::lanes_word(TxCommand::UpdateFlags, flags),
        )
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

    /// Sets the extended TX core's front end up for a stream, as libusdr's TX create does
    /// (`initialize_stream_tx_32` with `CORE_EXFETX_DMA32_R0_2`): unmuted, then expanded to
    /// the stream's lanes and sample width (`twelve_bit`: `ci12`), then its channel `map`
    /// (one or two channel numbers) put onto the core's lanes (`exfe_tx4_mute`,
    /// `exfe_tx4_config`, `exfe_trx4_update_chmap`). The uSDR's core has four lanes; a
    /// stream of complex samples uses two per channel.
    pub(crate) fn extended_tx_configure(map: &[u8], twelve_bit: bool) -> Self {
        let expand = if map.len() > 1 { 2 } else { 1 };
        Self::new()
            .extended_tx_mute(0)
            .extended_tx(ExtendedTxCommand::Expand as u8, expand)
            .extended_tx(ExtendedTxCommand::Format12 as u8, u32::from(twelve_bit))
            .extended_tx_map(map)
    }

    /// Mutes the extended TX core's lanes in `mask`, written as libusdr's `"mute"` option
    /// writes it (`exfe_tx4_mute`).
    pub(crate) fn extended_tx_mute(self, mask: u32) -> Self {
        self.extended_tx(ExtendedTxCommand::Mute as u8, mask)
    }

    /// Puts a complex stream's channel `map` onto the extended TX core's four lanes: the
    /// four shuffle stages `exfe_trx4_update_chmap` computes for TX, with its quirks.
    pub(crate) fn extended_tx_map(self, map: &[u8]) -> Self {
        (ExtendedTxCommand::Shuffle0 as u8..)
            .zip(extended_tx_shuffle(map))
            .fold(self, |sequence, (command, bits)| {
                sequence.extended_tx(command, u32::from(bits))
            })
    }

    /// Appends extended TX front-end command `command` with `value`'s low 24 bits
    /// (`MAKE_EXFE_REG`).
    fn extended_tx(self, command: u8, value: u32) -> Self {
        let word = u32::from(command) << 24 | value & 0x00ff_ffff;
        self.write(REG_EXTENDED_TX_FRONT_END, word)
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

    /// One unswapped TX channel with `mute`.
    const fn one_channel(mute: u8) -> TxLanes {
        TxLanes {
            mute,
            swap: false,
            channels_log2: 0,
        }
    }

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
        let rx = run(
            EngineSequence::start_lanes(true, None, SyncTrigger::FreeRun),
            0,
        );
        assert_eq!(
            rx.accesses(),
            [
                write(14, 0x8007_0000),
                write(5, 1),
                write(0x1_0100, 0x3000_0002),
            ]
        );
        let muted_b = Some(one_channel(2));
        let duplex = run(
            EngineSequence::start_lanes(true, muted_b, SyncTrigger::Duplex),
            0,
        );
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
            EngineSequence::start_lanes(false, Some(one_channel(3)), SyncTrigger::TxActivity),
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
    fn lane_words_carry_swap_and_channel_count_and_updates_drop_the_count() {
        let two_swapped = TxLanes {
            mute: 1,
            swap: true,
            channels_log2: 1,
        };
        let start = run(
            EngineSequence::start_lanes(false, Some(two_swapped), SyncTrigger::FreeRun),
            0,
        );
        // `TX_CMD_START_16BIT` with `MODE_FMT` 1 (bit 3), `MODE_MUTEA` (bit 9) and
        // `MODE_SWAP_AB` (bit 10).
        assert_eq!(start.accesses()[2], write(14, 0x60b));
        let update = run(EngineSequence::new().update_tx_lanes(two_swapped), 0);
        assert_eq!(
            update.accesses(),
            [write(14, 0x601)],
            "sfe_tx4_upd has no FMT"
        );
    }

    #[test]
    fn libusdrs_control_words() {
        assert_eq!(
            TxControl::lanes_word(TxCommand::UpdateFlags, one_channel(3)),
            0x301
        );
        assert_eq!(
            FrontEndWrite::word(FrontEndCommand::Throttle, 1 << 16 | 2 << 8 | 5),
            0x2001_0205
        );
    }

    #[test]
    fn extended_shuffles_are_libusdrs_masked_lane_map() {
        // One channel: lanes 2 and 3 take channel 0x7f masked to 0, so their sources are
        // lanes 0 and 1, XOR 2: stage 1 set for lanes 2 and 3. libusdr masks the channel
        // number away, so channel 1 alone maps as channel 0.
        assert_eq!(extended_tx_shuffle(&[0]), [0, 0b1100, 0, 0]);
        assert_eq!(extended_tx_shuffle(&[1]), [0, 0b1100, 0, 0]);
        // Two channels fill the four lanes in order: no shuffle.
        assert_eq!(extended_tx_shuffle(&[0, 1]), [0; 4]);
    }

    #[test]
    fn an_extended_tx_setup_unmutes_expands_sets_the_width_then_maps_the_lanes() {
        // Two ci12 channels: mute 0, expand 2, 12-bit, then four zero shuffle stages, all
        // at `CSR_TFE4_BASE` as `cmd << 24 | value`.
        let setup = run(EngineSequence::extended_tx_configure(&[0, 1], true), 0);
        assert_eq!(setup.outcome, Ok(()));
        assert_eq!(
            setup.accesses(),
            [
                write(0x1_0300, 0x0f00_0000),
                write(0x1_0300, 0x0d00_0002),
                write(0x1_0300, 0x0e00_0001),
                write(0x1_0300, 0x1000_0000),
                write(0x1_0300, 0x1100_0000),
                write(0x1_0300, 0x1200_0000),
                write(0x1_0300, 0x1300_0000),
            ]
        );
        let mute = run(EngineSequence::new().extended_tx_mute(0x1ff_ffff), 0);
        assert_eq!(
            mute.accesses(),
            [write(0x1_0300, 0x0fff_ffff)],
            "the mask's low 24 bits"
        );
    }
}
