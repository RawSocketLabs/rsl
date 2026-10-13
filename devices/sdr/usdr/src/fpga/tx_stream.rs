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
pub(crate) const TX_SAMPLE_BYTES: u32 = 4;

/// The largest timestamp the core takes: 47 bits.
pub(crate) const MAX_TX_TIMESTAMP: u64 = (1 << 47) - 1;

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
pub(crate) fn tx_block_bytes(samples_per_packet: u32) -> Result<u32, Error> {
    samples_per_packet
        .checked_mul(TX_SAMPLE_BYTES)
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
    fn a_wide_reading_spills_into_the_next_byte_as_in_libusdr() {
        let mut total = 0x200;
        extend_drop_count(&mut total, 0x0123);
        // Low byte 0 differs from 0x123 and is not above it: (0x200 & !0xff) | 0x123.
        assert_eq!(total, 0x323);
    }
}
