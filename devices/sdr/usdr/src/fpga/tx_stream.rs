//! The uSDR's TX stream engine: the original TX core (`CORE_SFETX_DMA32_R0`, libusdr's
//! "Simple TX FE"), which takes `ci16` packets from the host's DMA buffers.
//!
//! The core takes three things: control commands (the portable machines in
//! `portable/stream.rs` write them), two words announcing each packet the host has written,
//! and reads of its four status words. This module holds the announcements, the status
//! decoding and the checks a TX stream create makes.
//!
//! Source: `ipblks/streams/sfe_tx_4.c` (`sfe_tx4_push_ring_buffer`, `MAX_TX_FIFO_SZ`),
//! `stream_sfetrx4_dma32.c` (`parse_txcore_stat`, `MINIM_FWID_COMPAT`); addresses from
//! `device/m2_lm6_1/m2_lm6_1.c`'s TX stream create (`M2PCI_REG_WR_TXDMA_CNF_L`,
//! `M2PCI_REG_RD_TXDMA_STAT`).

use bnb::{bitfield, u15};

use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;
use crate::stream::{TxChannels, WireFormat};

/// The oldest gateware build libusdr streams TX from (`MINIM_FWID_COMPAT`), as the FPGA's
/// `USR_ACCESS2` stamps it.
const MIN_TX_GATEWARE: u32 = 0xd2b1_0c09;

/// The first of the two words announcing a packet (`M2PCI_REG_WR_TXDMA_CNF_L`, then
/// `_CNF_T`).
const REG_TX_PUSH: u32 = 12;
/// The first of the core's four status words (`M2PCI_REG_RD_TXDMA_STAT`).
const REG_TX_STATUS: u32 = 28;

/// The most bytes one TX packet takes: the core's FIFO (`MAX_TX_FIFO_SZ`, which
/// `sfe_tx4_mtu_get` reports).
pub(crate) const MAX_TX_PACKET_BYTES: u32 = 126_976;

/// Bytes one `ci16` sample takes on the wire.
#[cfg_attr(
    not(all(feature = "mio", target_os = "linux")),
    allow(
        dead_code,
        reason = "only NativeUsb's stream group (feature `mio`) uses it"
    )
)]
pub(crate) const TX_SAMPLE_BYTES: u32 = 4;

/// The largest timestamp the original core takes: 47 bits.
pub(crate) const MAX_TX_TIMESTAMP: u64 = (1 << 47) - 1;

/// The largest timestamp the extended core is given: 63 bits, libusdr's non-negative
/// `dm_time_t`. Its announcing registers take 64, all ones meaning "now"; USB's header
/// carries 63.
pub(crate) const MAX_EXTENDED_TX_TIMESTAMP: u64 = (1 << 63) - 1;

/// Refuses TX on gateware built before libusdr's minimum, as its TX stream create does.
///
/// # Errors
///
/// [`Error::OutdatedGateware`] for a `build` stamped earlier than `MINIM_FWID_COMPAT`.
pub(crate) fn check_tx_gateware(build: u32) -> Result<(), Error> {
    if build_time(build) < build_time(MIN_TX_GATEWARE) {
        return Err(Error::OutdatedGateware(build));
    }
    Ok(())
}

/// The transport block for `samples_per_packet` `ci16` samples; libusdr fails a packet over
/// the core's FIFO at create (`sfe_tx4_mtu_get`).
///
/// # Errors
///
/// [`Error::UnsupportedPacketSize`] for a packet over [`MAX_TX_PACKET_BYTES`].
#[cfg_attr(
    not(all(feature = "mio", target_os = "linux")),
    allow(
        dead_code,
        reason = "only NativeUsb's stream group (feature `mio`) uses it"
    )
)]
pub(crate) fn tx_block_bytes(samples_per_packet: u32) -> Result<u32, Error> {
    stream_block_bytes(samples_per_packet, TX_SAMPLE_BYTES)
}

/// The transport block for `samples_per_packet` samples of `sample_bytes` each, every
/// channel's included (`sfe_tx4_mtu_get`'s limit, either core).
///
/// # Errors
///
/// [`Error::UnsupportedPacketSize`] for a packet over [`MAX_TX_PACKET_BYTES`].
pub(crate) fn stream_block_bytes(samples_per_packet: u32, sample_bytes: u32) -> Result<u32, Error> {
    samples_per_packet
        .checked_mul(sample_bytes)
        .filter(|&bytes| bytes <= MAX_TX_PACKET_BYTES)
        .ok_or(Error::UnsupportedPacketSize(samples_per_packet))
}

/// libusdr's `get_xilinx_rev_h`: a Xilinx `USR_ACCESS` build stamp (day, month, year
/// from 2000, hour, minute, second in bits 31:27, 26:23, 22:17, 16:12, 11:6, 5:0) as a
/// number that sorts by build time.
fn build_time(stamp: u32) -> u64 {
    let field = |shift: u32, bits: u32| u64::from(stamp >> shift & ((1 << bits) - 1));
    let (day, month, year) = (field(27, 5), field(23, 4), field(17, 6));
    let (hour, minute, second) = (field(12, 5), field(6, 6), field(0, 6));
    second + (minute + (hour + (day + (month + (2000 + year) * 100) * 100) * 100) * 100) * 100
}

/// The first word announcing a packet; the second is the timestamp's low 32 bits.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Push {
    /// No timestamp: play the packet as soon as it arrives. Bit 30.
    #[bits(30..=30)]
    now: bool,

    /// Samples in the packet, less one. Bits 29:15.
    #[bits(15..=29)]
    samples: u15,

    /// The timestamp's bits 46:32. Bits 14:0.
    #[bits(0..=14)]
    time_high: u15,
}

/// What the core's status words say about drops and its FIFO, as libusdr reads them
/// (`parse_txcore_stat`, the fields `_sfetrx4_stream_send` uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TxCoreStatus {
    /// Bursts the front end dropped: an 8-bit counter.
    pub(crate) front_end_drops: u32,

    /// Bursts the DMA dropped: an 8-bit counter, or 16 bits in the USB layout.
    pub(crate) dma_drops: u32,

    /// How full the FIFO is, 0 to 255; 0 in the USB layout.
    pub(crate) fifo_used: u8,
}

impl TxCoreStatus {
    /// The USB layout's flag in the first status word, bit 3.
    const USB: u32 = 1 << 3;

    /// Decodes the four status words.
    ///
    /// In the `PCIe` layout the second word holds the FIFO fill in bits 23:16, the front
    /// end's drops in bits 15:8 and the DMA's in bits 7:0. In the USB layout (first word's
    /// bit 3 set) the front end's drops are the first word's bits 15:8 and the DMA's the
    /// second word's bits 31:16.
    pub(crate) fn from_words(words: [u32; 4]) -> Self {
        let [stat, stat_m, ..] = words;
        let byte = |word: u32, shift: u32| (word >> shift) & 0xff;
        if stat & Self::USB != 0 {
            Self {
                front_end_drops: byte(stat, 8),
                dma_drops: stat_m >> 16,
                fifo_used: 0,
            }
        } else {
            Self {
                front_end_drops: byte(stat_m, 8),
                dma_drops: byte(stat_m, 0),
                fifo_used: u8::try_from(byte(stat_m, 16)).expect("invariant: one byte"),
            }
        }
    }
}

/// Extends one of the core's 8-bit drop counters into `total`, as `_sfetrx4_stream_send`
/// does: a new low byte below the last one counts a wrap. The new reading replaces the
/// low byte by an OR over it cleared, so a 16-bit reading (the USB layout's DMA count)
/// lands across the next byte too, as in libusdr.
pub(crate) fn extend_drop_count(total: &mut u64, reading: u32) {
    let low = *total & 0xff;
    if low == u64::from(reading) {
        return;
    }
    if low > u64::from(reading) {
        *total += 0x100;
    }
    *total = (*total & !0xff) | u64::from(reading);
}

/// Announces a packet of `samples` the host has written to the next buffer, to play at
/// `time`, or as soon as it arrives with `None` (`sfe_tx4_push_ring_buffer`, which
/// libusdr's commit calls with a timestamp of -1 for "now").
///
/// `samples` is 1 to 32768 and `time` at most [`MAX_TX_TIMESTAMP`]; callers check both.
pub(crate) fn push_tx(bus: &mut dyn Bus, samples: u32, time: Option<u64>) -> Result<(), Error> {
    // libusdr's -1: every timestamp bit set.
    let stamp = time.unwrap_or(u64::MAX);
    let less_one = u16::try_from(samples - 1).expect("invariant: at most 32768 samples");
    let first = Push::new()
        .with_now(time.is_none())
        .with_samples(u15::new(less_one))
        .with_time_high(u15::from_raw(
            u16::try_from(stamp >> 32 & 0x7fff).expect("15 bits"),
        ));
    let low = u32::try_from(stamp & 0xffff_ffff).expect("32 bits");
    bus.write_regs(REG_TX_PUSH, &[first.to_raw(), low])
        .writing(FPGA, REG_TX_PUSH)
}

/// The extended TX core's front-end command register (`CSR_TFE4_BASE`,
/// `VIRT_CFG_SFX_BASE + 3 * 256`): a command in bits 31:24, its value below
/// (`MAKE_EXFE_REG`).
const REG_EXTENDED_FRONT_END: u32 = 0x1_0300;

/// The first of the extended core's four announcing registers
/// (`M2PCI_REG_WR_TXDMA_CFG0`): bytes less one, then bursts and samples, then the
/// timestamp's high and low halves; the last write announces the packet. The same
/// addresses read back as the status words.
const REG_EXTENDED_PUSH: u32 = 28;

/// An extended TX front-end command (`exfe_rx_cmd_regs` in `sfe_rx_4.c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtendedCommand {
    /// The lanes the stream's channels expand to, as a power of two (`EXFE_CMD_EXPAND`).
    Expand = 0x0d,

    /// The sample width: 0 for 16 bits, 1 for 12 (`EXFE_CMD_TXFMT12`).
    Format12 = 0x0e,

    /// The mute mask, as libusdr's `"mute"` option passes it (`EXFE_CMD_MUTE`).
    Mute = 0x0f,

    /// The first of four lane-shuffle stages (`EXFE_CMD_SHUFFLE_0`).
    Shuffle0 = 0x10,
}

/// The extended core's front end, set up for a stream as libusdr's TX create does
/// (`initialize_stream_tx_32` with `CORE_EXFETX_DMA32_R0_2`): unmuted, then expanded to
/// the stream's lanes and sample width, then its channels mapped onto the core's lanes
/// (`exfe_tx4_mute`, `exfe_tx4_config`, `exfe_trx4_update_chmap`). The uSDR's core has
/// four lanes; a stream of `channels` complex `wire` samples uses two per channel.
pub(crate) fn configure_extended_tx(
    bus: &mut dyn Bus,
    channels: TxChannels,
    wire: WireFormat,
) -> Result<(), Error> {
    let expand = match channels.count() {
        1 => 1,
        _ => 2,
    };
    let twelve_bit = u32::from(wire == WireFormat::Ci12);
    mute_extended_tx(bus, 0)?;
    extended_command(bus, ExtendedCommand::Expand, expand)?;
    extended_command(bus, ExtendedCommand::Format12, twelve_bit)?;
    map_extended_tx(bus, channels)
}

/// Mutes the extended core's lanes in `mask`, written as libusdr's `"mute"` option writes
/// it (`exfe_tx4_mute`).
pub(crate) fn mute_extended_tx(bus: &mut dyn Bus, mask: u32) -> Result<(), Error> {
    extended_command(bus, ExtendedCommand::Mute, mask)
}

/// Maps a complex stream's `channels` onto the extended core's four lanes: the four
/// shuffle stages `exfe_trx4_update_chmap` computes for TX, with its quirks.
pub(crate) fn map_extended_tx(bus: &mut dyn Bus, channels: TxChannels) -> Result<(), Error> {
    let stages = extended_tx_shuffle(channels);
    (0_u8..).zip(stages).try_for_each(|(stage, bits)| {
        let command = ExtendedCommand::Shuffle0 as u8 + stage;
        extended_word(bus, command, u32::from(bits))
    })
}

/// The extended core's four shuffle stages for a complex TX stream of `channels`: libusdr's
/// `exfe_trx4_update_chmap` with `mask` set, four raw lanes and no 3x16 packing.
///
/// libusdr inverts the map for TX into a table of all four lanes whose unmapped entries
/// are `0x7f` (its `memset` of `~CH_SWAP_IQ_FLAG`), not the `0xff` it skips, so lanes past
/// the stream's take channel `0x7f` masked to the stream's channels; that is kept.
fn extended_tx_shuffle(channels: TxChannels) -> [u16; 4] {
    const LANES: usize = 4;
    const UNMAPPED: u8 = 0x7f;
    let map: &[u8] = match channels {
        TxChannels::Channel0 => &[0],
        TxChannels::Channel1 => &[1],
        TxChannels::Both => &[0, 1],
    };
    // Lanes in use: two per complex channel; the channel mask libusdr applies.
    let total = 2 * map.len();
    let mask = map.len() - 1;
    let mut reverse = [UNMAPPED; LANES];
    for (lane, &channel) in map.iter().enumerate() {
        reverse[usize::from(channel)] = u8::try_from(lane).expect("a lane index");
    }
    let mut stages = [0_u16; 4];
    for group in (0..LANES).step_by(total) {
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

/// Writes one extended front-end command.
fn extended_command(bus: &mut dyn Bus, command: ExtendedCommand, value: u32) -> Result<(), Error> {
    extended_word(bus, command as u8, value)
}

/// Writes command `command` with `value`'s low 24 bits (`MAKE_EXFE_REG`).
fn extended_word(bus: &mut dyn Bus, command: u8, value: u32) -> Result<(), Error> {
    let word = u32::from(command) << 24 | value & 0x00ff_ffff;
    bus.write_regs(REG_EXTENDED_FRONT_END, &[word])
        .writing(FPGA, REG_EXTENDED_FRONT_END)
}

/// The extended core's announcing registers as libusdr last wrote them
/// (`extxcfg_cache`): it writes the first three only when they change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExtendedTxPush {
    /// Bytes less one, bursts and samples, and the timestamp's high half, last written.
    written: [u32; 3],
}

impl ExtendedTxPush {
    /// The cache a new stream starts with (`extxcfg_cache_init`): the first two at 0, the
    /// third at `~1`.
    pub(crate) const fn new() -> Self {
        Self {
            written: [0, 0, !1],
        }
    }

    /// Announces a one-burst packet of `samples` (per channel) in `bytes` the host has
    /// written, to play at `time` or, with `None`, as soon as it arrives (libusdr's -1, all
    /// ones): `sfe_extx4_push_ring_buffer` from `stream_soft_extx_commit`.
    ///
    /// `bytes` and `samples` are 1 to 2^20; callers check both.
    pub(crate) fn push(
        &mut self,
        bus: &mut dyn Bus,
        bytes: u32,
        samples: u32,
        time: Option<u64>,
    ) -> Result<(), Error> {
        let stamp = time.unwrap_or(u64::MAX);
        let [high, low] =
            [stamp >> 32, stamp & 0xffff_ffff].map(|half| u32::try_from(half).expect("32 bits"));
        // One burst: `(1 << 0) - 1` in bits 31:24.
        let words = [(bytes - 1) & 0x00ff_ffff, (samples - 1) & 0x00ff_ffff, high];
        for ((reg, word), written) in (REG_EXTENDED_PUSH..).zip(words).zip(&mut self.written) {
            if *written != word {
                // Cached before the write's result, as libusdr does.
                *written = word;
                bus.write_regs(reg, &[word]).writing(FPGA, reg)?;
            }
        }
        let last = REG_EXTENDED_PUSH + 3;
        bus.write_regs(last, &[low]).writing(FPGA, last)
    }
}

/// Reads the core's four status words in one transaction (`lowlevel_reg_rdndw`).
pub(crate) fn read_tx_status(bus: &mut dyn Bus) -> Result<[u32; 4], Error> {
    let mut words = [0; 4];
    bus.read_regs(REG_TX_STATUS, &mut words)
        .reading(FPGA, REG_TX_STATUS)?;
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values: libusdr's `sfe_tx_4.c` shifts worked by hand.

    #[test]
    fn builds_sort_by_date_not_by_their_raw_bits() {
        let minimum = build_time(MIN_TX_GATEWARE);
        assert!(
            build_time(MIN_TX_GATEWARE - 1) < minimum,
            "a second earlier"
        );
        // Day 1 of a later year: a smaller word, a later build.
        let next_year_day_one = 1 << 27 | ((MIN_TX_GATEWARE >> 17 & 0x3f) + 1) << 17;
        assert!(next_year_day_one < MIN_TX_GATEWARE);
        assert!(build_time(next_year_day_one) > minimum);
        assert!(check_tx_gateware(next_year_day_one).is_ok());
        assert!(matches!(
            check_tx_gateware(MIN_TX_GATEWARE - 1),
            Err(Error::OutdatedGateware(_))
        ));
    }

    #[test]
    fn a_tx_block_holds_at_most_the_cores_fifo() {
        assert_eq!(tx_block_bytes(31_744).ok(), Some(MAX_TX_PACKET_BYTES));
        assert!(matches!(
            tx_block_bytes(31_745),
            Err(Error::UnsupportedPacketSize(31_745))
        ));
        assert!(matches!(
            tx_block_bytes(u32::MAX),
            Err(Error::UnsupportedPacketSize(_))
        ));
    }

    #[test]
    fn a_push_without_a_time_sets_every_timestamp_bit_and_the_now_flag() {
        // libusdr's -1: bits 46:32 all set, bit 30, 1024 samples less one in bits 29:15.
        let first = Push::new()
            .with_now(true)
            .with_samples(u15::new(1023))
            .with_time_high(u15::MAX);
        assert_eq!(first.to_raw(), 0x4000_7fff | 1023 << 15);
    }

    #[test]
    fn pcie_status_words_carry_the_fifo_and_both_drop_counters() {
        let status = TxCoreStatus::from_words([0, 0xaa_40_03_07, 0, 0]);
        assert_eq!(
            status,
            TxCoreStatus {
                front_end_drops: 3,
                dma_drops: 7,
                fifo_used: 0x40,
            }
        );
    }

    #[test]
    fn the_usb_layout_moves_the_counters_and_reports_no_fifo() {
        let status = TxCoreStatus::from_words([0x0000_0508, 0x0123_ffff, 0, 0]);
        assert_eq!(
            status,
            TxCoreStatus {
                front_end_drops: 5,
                dma_drops: 0x0123,
                fifo_used: 0,
            }
        );
    }

    #[test]
    fn a_counter_that_wraps_adds_a_turn() {
        let mut total = 0;
        for reading in [5, 250, 3, 3, 4] {
            extend_drop_count(&mut total, reading);
        }
        // 0 → 5 → 250, then 3 is below 250: one wrap, 256 + 3, then 4.
        assert_eq!(total, 0x104);
    }

    #[test]
    fn extended_shuffles_are_libusdrs_masked_lane_map() {
        // One channel: lanes 2 and 3 take channel 0x7f masked to 0, so their sources are
        // lanes 0 and 1, XOR 2: stage 1 set for lanes 2 and 3. libusdr masks the channel
        // number away, so channel 1 alone maps as channel 0.
        assert_eq!(extended_tx_shuffle(TxChannels::Channel0), [0, 0b1100, 0, 0]);
        assert_eq!(extended_tx_shuffle(TxChannels::Channel1), [0, 0b1100, 0, 0]);
        // Two channels fill the four lanes in order: no shuffle.
        assert_eq!(extended_tx_shuffle(TxChannels::Both), [0; 4]);
    }

    #[test]
    fn a_wide_reading_spills_into_the_next_byte_as_in_libusdr() {
        let mut total = 0x200;
        extend_drop_count(&mut total, 0x0123);
        // Low byte 0 differs from 0x123 and is not above it: (0x200 & !0xff) | 0x123.
        assert_eq!(total, 0x323);
    }
}
