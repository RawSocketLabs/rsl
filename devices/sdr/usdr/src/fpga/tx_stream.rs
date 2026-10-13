//! The uSDR's TX stream engine: the original TX core (`CORE_SFETX_DMA32_R0`, libusdr's
//! "Simple TX FE"), which takes `ci16` packets from the host's DMA buffers.
//!
//! The core takes three things: control commands, two words announcing each packet the
//! host has written, and reads of its four status words. Its control register is the sync
//! unit's: libusdr passes register 14 to the TX stream as its "sync base" and writes the
//! core's commands there with bit 31 clear, where every sync-unit write sets it.
//!
//! Source: `ipblks/streams/sfe_tx_4.c` (`sfe_tx4_ctl`, `sfe_tx4_upd`,
//! `sfe_tx4_push_ring_buffer`, `MAX_TX_FIFO_SZ`) and `stream_sfetrx4_dma32.c`
//! (`parse_txcore_stat`); addresses from `device/m2_lm6_1/m2_lm6_1.c`'s TX stream create
//! (`M2PCI_REG_WR_TXDMA_CNF_L`, `M2PCI_REG_WR_SYNC_CTRL`, `M2PCI_REG_RD_TXDMA_STAT`).

use bnb::{BitEnum, bitfield, u2, u15};

use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// The first of the two words announcing a packet (`M2PCI_REG_WR_TXDMA_CNF_L`, then
/// `_CNF_T`).
const REG_TX_PUSH: u32 = 12;
/// The core's control register, shared with the sync unit (`M2PCI_REG_WR_SYNC_CTRL`).
const REG_TX_CONTROL: u32 = 14;
/// The first of the core's four status words (`M2PCI_REG_RD_TXDMA_STAT`).
const REG_TX_STATUS: u32 = 28;

/// The most bytes one TX packet takes: the core's FIFO (`MAX_TX_FIFO_SZ`, which
/// `sfe_tx4_mtu_get` reports).
pub(crate) const MAX_TX_PACKET_BYTES: u32 = 126_976;

/// Bytes one `ci16` sample takes on the wire.
pub(crate) const TX_SAMPLE_BYTES: u32 = 4;

/// The largest timestamp the core takes: 47 bits.
pub(crate) const MAX_TX_TIMESTAMP: u64 = (1 << 47) - 1;

/// What a control write tells the core (`TX_CMD_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
#[repr(u8)]
enum Command {
    /// Stop.
    Stop = 0,

    /// Take the mute and swap flags, nothing else.
    UpdateFlags = 1,

    /// Start, 12-bit samples; libusdr never sends it to this core.
    Start12 = 2,

    /// Start, 16-bit samples.
    Start16 = 3,
}

/// One write to the core's control register (`GP_PORT_TXDMA_CTRL_*`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Control {
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
    command: Command,
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

/// Stops the core (`sfe_tx4_ctl` stopping, and the TX stream's destroy).
pub(crate) fn stop_tx(bus: &mut dyn Bus) -> Result<(), Error> {
    write_control(bus, Control::new())
}

/// Starts the core for one channel of 16-bit samples with `mute` (bit 0 lane A, bit 1
/// lane B): stop, reset its buffer pointers, then start (`sfe_tx4_ctl`, unswapped, no
/// replay).
pub(crate) fn start_tx(bus: &mut dyn Bus, mute: u8) -> Result<(), Error> {
    stop_tx(bus)?;
    write_control(bus, Control::new().with_reset_buffers(true))?;
    write_control(bus, flags(mute).with_command(Command::Start16))
}

/// Sets the core's mute flags, unswapped (`sfe_tx4_upd`).
pub(crate) fn update_tx_flags(bus: &mut dyn Bus, mute: u8) -> Result<(), Error> {
    write_control(bus, flags(mute).with_command(Command::UpdateFlags))
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

/// The mute flags of a control write, for one unswapped channel.
fn flags(mute: u8) -> Control {
    Control::new()
        .with_mute_a(mute & 1 != 0)
        .with_mute_b(mute & 2 != 0)
}

/// Writes the control register.
fn write_control(bus: &mut dyn Bus, control: Control) -> Result<(), Error> {
    bus.write_regs(REG_TX_CONTROL, &[control.to_raw()])
        .writing(FPGA, REG_TX_CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values: libusdr's `sfe_tx_4.c` shifts worked by hand.

    #[test]
    fn a_one_channel_start_mutes_lane_b_in_libusdrs_word() {
        // `TX_CMD_START_16BIT` (3) with `MODE_MUTEB` (bit 8): 0x103.
        assert_eq!(flags(2).with_command(Command::Start16).to_raw(), 0x103);
        // `RESET_BUFS` alone: bit 7.
        assert_eq!(Control::new().with_reset_buffers(true).to_raw(), 0x80);
        // `TX_CMD_UPD_FLAGS` muting both lanes: bits 9, 8 and command 1.
        assert_eq!(flags(3).with_command(Command::UpdateFlags).to_raw(), 0x301);
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
