//! The RX stream engine: the front end that packs ADC samples into bursts, the DMA that
//! moves blocks of bursts to the host, and the sync unit that lets them run.
//!
//! The front end takes commands through one register, `cmd << 28 | value`; the DMA engine
//! has a control and a status register and two configuration registers; the sync unit one
//! register. libusdr's single-channel `ci16` RX stream sets them as this module does.
//!
//! Source: `ipblks/streams/sfe_rx_4.c`, `dma_rx_32.c` and `stream_sfetrx4_dma32.c`;
//! addresses from `device/m2_lm6_1/m2_lm6_1.c` (`M2PCI_REG_WR_RXDMA_CONFIRM`,
//! `VIRT_CFG_SFX_BASE`, `CSR_RFE4_BASE`, `SRF4_FIFOBSZ`) and its `/ll/sync/0/base`.

use bnb::{BitEnum, bitfield, u2, u3, u4, u10, u13, u28};

use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// RX DMA control: enable, reset, host ready (`M2PCI_REG_WR_RXDMA_CONFIRM + 1`).
const REG_RX_DMA_CONTROL: u32 = 5;
/// RX DMA status, read-only (`M2PCI_REG_WR_RXDMA_CONFIRM + 2`).
const REG_RX_DMA_STATUS: u32 = 6;
/// The stream sync unit (`/ll/sync/0/base`).
const REG_SYNC: u32 = 14;
/// RX DMA configuration: words per burst, less one (`VIRT_CFG_SFX_BASE + 32`).
const REG_RX_DMA_BURST_WORDS: u32 = 0x1_0020;
/// RX DMA configuration: bursts per block, less one (`VIRT_CFG_SFX_BASE + 64`).
const REG_RX_DMA_BURSTS: u32 = 0x1_0040;
/// The RX front end's command register (`CSR_RFE4_BASE`).
const REG_RX_FRONT_END: u32 = 0x1_0100;

/// The front end's FIFO RAM, in bytes (`SRF4_FIFOBSZ`).
const FIFO_BYTES: u32 = 0x1_0000;
/// Bytes per front-end word (`cfg_word_bytes` for `CORE_SFERX_DMA32_R0`).
const WORD_BYTES: u32 = 8;
/// Most bursts in a DMA block (`MAX_BURSTS_RX_IN_BUFF`).
const MAX_BURSTS: u32 = 32;
/// Most words in a burst, and most samples (`1 << IPBLK_PARAM_BWORDS`).
const MAX_BURST_WORDS: u32 = 1 << 13;
/// Most bursts the FIFO count field holds (`SFE_CMD_BF_BTOTAL_MASK`).
const MAX_FIFO_BURSTS: u32 = (1 << 10) - 1;
/// Bits per `ci16` sample on the single-channel front end: two raw lanes of 16 bits.
const SAMPLE_BITS: u32 = 32;

/// RX DMA status: the engine reports active.
const STATUS_ACTIVE: u32 = 1 << 30;
/// RX DMA status as an absent or dead engine reads.
const STATUS_DEAD: u32 = 0xffff_ffff;

/// How a stream's packets split into bursts and fit the front end's FIFO
/// (`burst_fe_calculate` for one `ci16` channel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BurstPlan {
    /// Bytes in one burst, padded to whole words.
    bytes_per_burst: u32,

    /// Bursts in one DMA block.
    bursts: u32,

    /// Samples in one burst.
    samples_per_burst: u32,

    /// Bursts the FIFO holds.
    fifo_bursts: u32,
}

impl BurstPlan {
    /// The plan for packets of `samples`, or `None` where libusdr returns `-EINVAL` (or
    /// would divide by zero, for packets too small to fill a word per burst).
    ///
    /// libusdr's search: the fewest bursts that divide the packet evenly, keep each burst
    /// within 8192 samples and words, leave room for two bursts in the FIFO and split the
    /// words evenly; failing an even split, the one with the least padding. Its FIFO count
    /// comes from the last burst count it tried, as here.
    pub(crate) fn for_packet(samples: u32) -> Option<Self> {
        // libusdr's 32-bit product would wrap here; no such packet fits a burst anyway.
        let words = SAMPLE_BITS.checked_mul(samples)?.div_ceil(WORD_BYTES * 8);
        if words == 0 {
            return None;
        }
        let mut fifo_bursts = FIFO_BYTES / (words * WORD_BYTES);
        let mut best: Option<(u32, u32)> = None;
        let mut even = None;
        for bursts in 1..=MAX_BURSTS {
            if samples % bursts != 0 {
                continue;
            }
            let burst_words = words / bursts;
            if burst_words == 0 {
                return None;
            }
            fifo_bursts = FIFO_BYTES / (burst_words * WORD_BYTES);
            if fifo_bursts <= 1
                || samples / bursts > MAX_BURST_WORDS
                || burst_words > MAX_BURST_WORDS
            {
                continue;
            }
            if words % bursts == 0 {
                even = Some(bursts);
                break;
            }
            let padding = words.div_ceil(bursts) * bursts - words;
            if best.is_none_or(|(_, least)| padding < least) {
                best = Some((bursts, padding));
            }
        }
        let bursts = even.or(best.map(|(bursts, _)| bursts))?;
        Some(Self {
            bytes_per_burst: words.div_ceil(bursts) * WORD_BYTES,
            bursts,
            samples_per_burst: samples / bursts,
            fifo_bursts: fifo_bursts.min(MAX_FIFO_BURSTS),
        })
    }

    /// Samples in one burst.
    pub(crate) const fn samples_per_burst(&self) -> u32 {
        self.samples_per_burst
    }

    /// Bytes in one burst, padding included: a burst whose samples do not fill whole words
    /// ends in a stub.
    pub(crate) const fn bytes_per_burst(&self) -> u32 {
        self.bytes_per_burst
    }

    /// Bytes in one DMA block: what the transport delivers per packet.
    pub(crate) const fn block_bytes(&self) -> u32 {
        self.bytes_per_burst * self.bursts
    }
}

/// A front-end command (`FE_CMD_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4, closed)]
#[repr(u8)]
enum Command {
    /// Samples per burst, less one.
    BurstSamples = 0,

    /// The burst format; see [`BurstFormat`].
    BurstFormat = 1,

    /// Burst throttling (unused here).
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
    command: Command,

    /// Its value. Bits 27:0.
    #[bits(0..=27)]
    value: u28,
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

/// The sample format the front end packs (`IFMT_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum SampleFormat {
    /// DSP output.
    Dsp,

    /// 8 bits.
    Bits8,

    /// 12 bits.
    Bits12,

    /// 16 bits.
    Bits16,
}

/// Which raw lanes a burst carries (`IFMT_CH_*`); lane 0 is I and lane 1 Q of channel 0.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
enum Lanes {
    /// All four.
    Lanes3210,

    /// Lanes 1 and 0: channel 0's I and Q.
    Lanes10,

    /// Lane 0.
    Lane0,

    /// Lane 1.
    Lane1,

    /// Lanes 2 and 0.
    Lanes20,

    /// Lanes 3 and 2.
    Lanes32,

    /// Lane 2.
    Lane2,

    /// Lane 3.
    Lane3,
}

/// The burst format (`FE_CMD_BURST_FORMAT`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BurstFormat {
    /// Bursts the FIFO holds. `BTOTAL`, bits 27:18.
    #[bits(18..=27)]
    fifo_bursts: u10,

    /// Words per burst, less one. `BWORDS`, bits 17:5.
    #[bits(5..=17)]
    words: u13,

    /// The lanes. `CHFMT`, bits 4:2.
    #[bits(2..=4)]
    lanes: Lanes,

    /// The sample format. `IFMT`, bits 1:0.
    #[bits(0..=1)]
    format: SampleFormat,
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

/// The sync unit's modes (`ST_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncMode {
    /// Streams held stopped.
    Stop = 0,

    /// Streams run as soon as started.
    FreeRun = 7,
}

/// Checks the RX DMA engine is alive, then stops it and resets the front end
/// (`dma_rx32_reset`).
///
/// # Errors
///
/// [`Error::DmaEngineFault`] if its status reads all ones; any bus failure.
pub(crate) fn reset_rx_dma(bus: &mut dyn Bus) -> Result<(), Error> {
    let mut status = [0];
    bus.read_regs(REG_RX_DMA_STATUS, &mut status)
        .reading(FPGA, REG_RX_DMA_STATUS)?;
    // libusdr only logs an engine that reads active but not dead.
    if status[0] & STATUS_ACTIVE != 0 && status[0] == STATUS_DEAD {
        return Err(Error::DmaEngineFault);
    }
    write_dma_control(bus, DmaControl::new().with_reset(true))?;
    write_dma_control(bus, DmaControl::new())
}

/// Puts the front end in reset, sets the burst size and format for `plan`, and releases it
/// (`_configure_simple_fe_generic` for one `ci16` channel), then sizes the DMA blocks
/// (`dma_rx32_configure`).
pub(crate) fn configure_rx(bus: &mut dyn Bus, plan: &BurstPlan) -> Result<(), Error> {
    let all_reset = FrontEndControl::new()
        .with_reset_burster(true)
        .with_reset_assembler(true)
        .with_reset_ddr(true)
        .with_reset_dsp(true);
    front_end(bus, Command::Control, all_reset.to_raw())?;
    front_end(bus, Command::BurstSamples, plan.samples_per_burst - 1)?;
    front_end(bus, Command::Control, FrontEndControl::new().to_raw())?;
    let words = plan.bytes_per_burst / WORD_BYTES;
    let format = BurstFormat::new()
        .with_fifo_bursts(u10::new(
            u16::try_from(plan.fifo_bursts).expect("invariant: at most 1023"),
        ))
        .with_words(u13::new(
            u16::try_from(words - 1).expect("invariant: at most 8191"),
        ))
        .with_lanes(Lanes::Lanes10)
        .with_format(SampleFormat::Bits16);
    front_end(bus, Command::BurstFormat, format.to_raw())?;
    write(bus, REG_RX_DMA_BURST_WORDS, words - 1)?;
    write(bus, REG_RX_DMA_BURSTS, plan.bursts - 1)
}

/// Runs or stops the RX DMA and front end, DMA first (`_sfetrx4_op`).
pub(crate) fn run_rx(bus: &mut dyn Bus, run: bool) -> Result<(), Error> {
    write_dma_control(bus, DmaControl::new().with_enabled(run))?;
    let command = if run {
        RunCommand::StartNow
    } else {
        RunCommand::StopNow
    };
    front_end(
        bus,
        Command::Control,
        FrontEndControl::new().with_run(command).to_raw(),
    )
}

/// Tells the RX DMA engine the host is ready, before the first block arrives.
pub(crate) fn signal_rx_ready(bus: &mut dyn Bus) -> Result<(), Error> {
    write_dma_control(bus, DmaControl::new().with_ready(true))
}

/// Sets the sync unit's mode (`sfetrx4_stream_sync`).
pub(crate) fn sync(bus: &mut dyn Bus, mode: SyncMode) -> Result<(), Error> {
    write(bus, REG_SYNC, 1 << 31 | (mode as u32) << 16)
}

/// Writes the DMA control register.
fn write_dma_control(bus: &mut dyn Bus, control: DmaControl) -> Result<(), Error> {
    write(bus, REG_RX_DMA_CONTROL, control.to_raw())
}

/// Sends a front-end command.
fn front_end(bus: &mut dyn Bus, command: Command, value: u32) -> Result<(), Error> {
    let word = FrontEndWrite::new()
        .with_command(command)
        .with_value(u28::new(value));
    write(bus, REG_RX_FRONT_END, word.to_raw())
}

/// Writes one FPGA register.
fn write(bus: &mut dyn Bus, reg: u32, value: u32) -> Result<(), Error> {
    bus.write_regs(reg, &[value]).writing(FPGA, reg)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values: libusdr's arithmetic worked by hand.

    #[test]
    fn a_1024_sample_packet_is_one_4096_byte_burst() {
        let plan = BurstPlan::for_packet(1024).expect("valid");
        assert_eq!(
            plan,
            BurstPlan {
                bytes_per_burst: 4096,
                bursts: 1,
                samples_per_burst: 1024,
                fifo_bursts: 16,
            }
        );
        assert_eq!(plan.block_bytes(), 4096);
    }

    #[test]
    fn a_packet_too_big_for_one_burst_splits_evenly() {
        // 16384 samples are 8192 words: one burst would leave no FIFO room for two.
        let plan = BurstPlan::for_packet(16384).expect("valid");
        assert_eq!(
            (plan.bursts, plan.samples_per_burst, plan.bytes_per_burst),
            (2, 8192, 32768)
        );
    }

    #[test]
    fn small_packets_get_a_deep_fifo() {
        // 256 samples are 128 words: 64 bursts fit the FIFO, past the bottom 24 bits.
        let plan = BurstPlan::for_packet(256).expect("valid");
        assert_eq!((plan.bursts, plan.fifo_bursts), (1, 64));
    }

    #[test]
    fn a_packet_that_cannot_split_evenly_pads_each_burst() {
        // 16383 = 3 × 43 × 127: three bursts of 5461 samples, each 2730.5 words, so 2731.
        let plan = BurstPlan::for_packet(16383).expect("valid");
        assert_eq!(
            (plan.bursts, plan.samples_per_burst, plan.bytes_per_burst),
            (3, 5461, 21848)
        );
    }

    #[test]
    fn a_packet_too_large_to_count_is_refused() {
        assert_eq!(BurstPlan::for_packet(1 << 27), None);
    }

    #[test]
    fn the_front_end_words_are_libusdrs() {
        let format = BurstFormat::new()
            .with_fifo_bursts(u10::new(16))
            .with_words(u13::new(511))
            .with_lanes(Lanes::Lanes10)
            .with_format(SampleFormat::Bits16);
        assert_eq!(format.to_raw(), 0x0040_3fe7);
        let all_reset = FrontEndControl::new()
            .with_reset_burster(true)
            .with_reset_assembler(true)
            .with_reset_ddr(true)
            .with_reset_dsp(true);
        assert_eq!(all_reset.to_raw(), 0xe100);
    }
}
