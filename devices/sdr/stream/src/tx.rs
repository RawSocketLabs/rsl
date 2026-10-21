//! The TX cores: how a packet the host wrote is announced to the original core
//! (`CORE_SFETX_DMA32_R0`, libusdr's "Simple TX FE") and to the extended one
//! (`CORE_EXFETX_DMA32_R0_2`), how their four status words read, and the checks a TX
//! stream create makes. Their control commands, and the extended core's front end, are
//! [`EngineSequence`](crate::EngineSequence)'s.
//!
//! Source: `ipblks/streams/sfe_tx_4.c` (`sfe_tx4_push_ring_buffer`,
//! `sfe_extx4_push_ring_buffer`, `MAX_TX_FIFO_SZ`),
//! `stream_sfetrx4_dma32.c` (`parse_txcore_stat`, `MINIM_FWID_COMPAT`, the drop counting
//! of `_sfetrx4_stream_send`, `extxcfg_cache_init`); addresses from
//! `device/generic_usdr/generic_regs.h` (`M2PCI_REG_WR_TXDMA_CNF_L`,
//! `M2PCI_REG_WR_TXDMA_CFG0`, `M2PCI_REG_RD_TXDMA_STAT`), which `m2_lm6_1.c` and
//! `m2_lm7_1.c` both pass to the TX stream create.

use bnb::{bitfield, u15};

/// The oldest gateware build libusdr streams TX from (`MINIM_FWID_COMPAT`), as the FPGA's
/// `USR_ACCESS2` stamps it.
const MIN_TX_GATEWARE: u32 = 0xd2b1_0c09;

/// The first of the two registers announcing a packet (`M2PCI_REG_WR_TXDMA_CNF_L`, then
/// `_CNF_T`): [`tx_push_words`] goes there in order.
pub const REG_TX_PUSH: u32 = 12;

/// The first of the core's four status words (`M2PCI_REG_RD_TXDMA_STAT`).
pub const REG_TX_STATUS: u32 = 28;

/// The most bytes one TX packet takes: the core's FIFO (`MAX_TX_FIFO_SZ`, which
/// `sfe_tx4_mtu_get` reports).
pub const MAX_TX_PACKET_BYTES: u32 = 126_976;

/// The largest timestamp the original core takes: 47 bits.
pub const MAX_TX_TIMESTAMP: u64 = (1 << 47) - 1;

/// Whether libusdr's TX stream create accepts gateware stamped `build` (its
/// `USR_ACCESS2` word): built no earlier than `MINIM_FWID_COMPAT`.
#[must_use]
pub fn tx_gateware_supported(build: u32) -> bool {
    build_time(build) >= build_time(MIN_TX_GATEWARE)
}

/// The transport block for `samples_per_packet` samples of `sample_bytes` each, every
/// channel's included, or `None` past the core's FIFO (`sfe_tx4_mtu_get`'s limit, either
/// core).
#[must_use]
pub fn tx_block_bytes(samples_per_packet: u32, sample_bytes: u32) -> Option<u32> {
    samples_per_packet
        .checked_mul(sample_bytes)
        .filter(|&bytes| bytes <= MAX_TX_PACKET_BYTES)
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

/// The two words announcing a packet of `samples` the host has written to the next
/// buffer, to play at `time`, or as soon as it arrives with `None`, written in order from
/// [`REG_TX_PUSH`] (`sfe_tx4_push_ring_buffer`, which libusdr's commit calls with a
/// timestamp of -1 for "now").
///
/// `samples` is 1 to 32768 and `time` at most [`MAX_TX_TIMESTAMP`]; callers check both.
///
/// # Panics
///
/// For `samples` outside 1 to 32768.
#[must_use]
pub fn tx_push_words(samples: u32, time: Option<u64>) -> [u32; 2] {
    // libusdr's -1: every timestamp bit set.
    let stamp = time.unwrap_or(u64::MAX);
    let less_one = samples
        .checked_sub(1)
        .and_then(|less_one| u16::try_from(less_one).ok())
        .expect("invariant: 1 to 32768 samples");
    let high = u16::try_from(stamp >> 32 & 0x7fff).expect("15 bits");
    let first = Push::new()
        .with_now(time.is_none())
        .with_samples(u15::new(less_one))
        .with_time_high(u15::from_raw(high));
    let low = u32::try_from(stamp & 0xffff_ffff).expect("32 bits");
    [first.to_raw(), low]
}

/// The first of the extended core's four announcing registers
/// (`M2PCI_REG_WR_TXDMA_CFG0`): bytes less one, then bursts and samples, then the
/// timestamp's high and low halves; the last write announces the packet. The same
/// addresses read back as the status words ([`REG_TX_STATUS`]).
pub const REG_EXTENDED_TX_PUSH: u32 = 28;

/// The largest timestamp the extended core is given: 63 bits, libusdr's non-negative
/// `dm_time_t`. Its announcing registers take 64, all ones meaning "now".
pub const MAX_EXTENDED_TX_TIMESTAMP: u64 = (1 << 63) - 1;

/// The most bytes, and samples, one extended-core announcement takes (`1 << 20`).
pub const MAX_EXTENDED_TX_ANNOUNCE: u32 = 1 << 20;

/// The extended core's announcing registers as libusdr last wrote them
/// (`extxcfg_cache`): it writes the first three only when they change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtendedTxPush {
    /// Bytes less one, bursts and samples, and the timestamp's high half, last written.
    written: [u32; 3],
}

impl Default for ExtendedTxPush {
    fn default() -> Self {
        Self::new()
    }
}

/// The register writes one extended-core announcement makes, in order: the changed
/// first registers, then the last, which announces the packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtendedTxWrites {
    /// `(register, word)` pairs, the first `len`.
    writes: [(u32, u32); 4],

    /// Writes made.
    len: usize,
}

impl ExtendedTxWrites {
    /// The writes, in order.
    #[must_use]
    pub fn as_slice(&self) -> &[(u32, u32)] {
        &self.writes[..self.len]
    }
}

impl ExtendedTxPush {
    /// The cache a new stream starts with (`extxcfg_cache_init`): the first two at 0, the
    /// third at `~1`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            written: [0, 0, !1],
        }
    }

    /// The writes announcing a one-burst packet of `samples` (per channel) in `bytes` the
    /// host has written, to play at `time` or, with `None`, as soon as it arrives
    /// (libusdr's -1, all ones): `sfe_extx4_push_ring_buffer` from
    /// `stream_soft_extx_commit`.
    ///
    /// The cache takes every change at once, as libusdr's does even when a write fails:
    /// write them in order and stop at the first failure, and the cache is libusdr's.
    ///
    /// # Panics
    ///
    /// For `bytes` or `samples` outside 1 to [`MAX_EXTENDED_TX_ANNOUNCE`], which callers
    /// check.
    #[must_use]
    pub fn announce(&mut self, bytes: u32, samples: u32, time: Option<u64>) -> ExtendedTxWrites {
        let valid = 1..=MAX_EXTENDED_TX_ANNOUNCE;
        assert!(
            valid.contains(&bytes) && valid.contains(&samples),
            "invariant: 1 to 2^20 bytes and samples"
        );
        let stamp = time.unwrap_or(u64::MAX);
        let [high, low] =
            [stamp >> 32, stamp & 0xffff_ffff].map(|half| u32::try_from(half).expect("32 bits"));
        // One burst: `(1 << 0) - 1` in bits 31:24.
        let words = [(bytes - 1) & 0x00ff_ffff, (samples - 1) & 0x00ff_ffff, high];
        let mut writes = ExtendedTxWrites {
            writes: [(0, 0); 4],
            len: 0,
        };
        for ((reg, word), written) in (REG_EXTENDED_TX_PUSH..).zip(words).zip(&mut self.written) {
            if *written != word {
                writes.writes[writes.len] = (reg, word);
                writes.len += 1;
                *written = word;
            }
        }
        writes.writes[writes.len] = (REG_EXTENDED_TX_PUSH + 3, low);
        writes.len += 1;
        writes
    }
}

/// What the core's status words say about drops and its FIFO, as libusdr reads them
/// (`parse_txcore_stat`, the fields `_sfetrx4_stream_send` uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxCoreStatus {
    /// Bursts the front end dropped: an 8-bit counter.
    pub front_end_drops: u32,

    /// Bursts the DMA dropped: an 8-bit counter, or 16 bits in the USB layout.
    pub dma_drops: u32,

    /// How full the FIFO is, 0 to 255; 0 in the USB layout.
    pub fifo_used: u8,
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
    ///
    /// # Panics
    ///
    /// Never: the FIFO fill is one byte of the word.
    #[must_use]
    pub fn from_words(words: [u32; 4]) -> Self {
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
pub fn extend_drop_count(total: &mut u64, reading: u32) {
    let low = *total & 0xff;
    if low == u64::from(reading) {
        return;
    }
    if low > u64::from(reading) {
        *total += 0x100;
    }
    *total = (*total & !0xff) | u64::from(reading);
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
        assert!(tx_gateware_supported(next_year_day_one));
        assert!(!tx_gateware_supported(MIN_TX_GATEWARE - 1));
    }

    #[test]
    fn a_tx_block_holds_at_most_the_cores_fifo() {
        // `ci16`: four bytes a sample.
        assert_eq!(tx_block_bytes(31_744, 4), Some(MAX_TX_PACKET_BYTES));
        assert_eq!(tx_block_bytes(31_745, 4), None);
        assert_eq!(tx_block_bytes(u32::MAX, 4), None);
    }

    #[test]
    fn a_push_without_a_time_sets_every_timestamp_bit_and_the_now_flag() {
        // libusdr's -1: bits 46:32 all set, bit 30, 1024 samples less one in bits 29:15.
        assert_eq!(
            tx_push_words(1024, None),
            [0x4000_7fff | 1023 << 15, u32::MAX]
        );
    }

    #[test]
    fn a_timed_push_splits_the_timestamp() {
        assert_eq!(
            tx_push_words(2, Some(0x1_0000_0005)),
            [1 << 15 | 1, 5],
            "bits 46:32 in the first word, 31:0 in the second"
        );
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
    fn an_extended_announcement_writes_only_what_changed_then_the_last_register() {
        let mut push = ExtendedTxPush::new();
        // A fresh cache: bytes and samples change, the time's high half (0) differs from
        // `~1`.
        assert_eq!(
            push.announce(4096, 1024, Some(0x1_0000_0005)).as_slice(),
            [(28, 4095), (29, 1023), (30, 1), (31, 5)]
        );
        // The same packet again: only the announcing write.
        assert_eq!(
            push.announce(4096, 1024, Some(0x1_0000_0006)).as_slice(),
            [(31, 6)]
        );
        // "Now": every timestamp bit set.
        assert_eq!(
            push.announce(4096, 1024, None).as_slice(),
            [(30, u32::MAX), (31, u32::MAX)]
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
    fn a_wide_reading_spills_into_the_next_byte_as_in_libusdr() {
        let mut total = 0x200;
        extend_drop_count(&mut total, 0x0123);
        // Low byte 0 differs from 0x123 and is not above it: (0x200 & !0xff) | 0x123.
        assert_eq!(total, 0x323);
    }
}
