//! The uSDR's TX stream engine: the original TX core (`CORE_SFETX_DMA32_R0`, libusdr's
//! "Simple TX FE"), which takes `ci16` packets from the host's DMA buffers, and the
//! extended one (`CORE_EXFETX_DMA32_R0_2`).
//!
//! The core takes three things: control commands (the portable machines in
//! `portable/stream.rs` write them), two words announcing each packet the host has written,
//! and reads of its four status words. The original core's announcement words, status
//! decoding and the create's checks are the shared `sdr-stream`'s (the LMS7 family's
//! gateware has the same core at the same registers); this module runs them over the bus
//! and holds the extended core's cached announcement.
//!
//! Source: `ipblks/streams/sfe_tx_4.c` (`sfe_tx4_push_ring_buffer`,
//! `sfe_extx4_push_ring_buffer`), `stream_sfetrx4_dma32.c`; addresses from
//! `device/m2_lm6_1/m2_lm6_1.c`'s TX stream create (`M2PCI_REG_WR_TXDMA_CNF_L`,
//! `M2PCI_REG_WR_TXDMA_CFG0`, `M2PCI_REG_RD_TXDMA_STAT`).

use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

pub(crate) use sdr_stream::{MAX_TX_TIMESTAMP, TxCoreStatus, extend_drop_count};
use sdr_stream::{
    REG_TX_PUSH, REG_TX_STATUS, tx_block_bytes, tx_gateware_supported, tx_push_words,
};

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
    if !tx_gateware_supported(build) {
        return Err(Error::OutdatedGateware(build));
    }
    Ok(())
}

/// The transport block for `samples_per_packet` samples of `sample_bytes` each, every
/// channel's included (`sfe_tx4_mtu_get`'s limit, either core).
///
/// # Errors
///
/// [`Error::UnsupportedPacketSize`] for a packet over the core's FIFO.
pub(crate) fn stream_block_bytes(samples_per_packet: u32, sample_bytes: u32) -> Result<u32, Error> {
    tx_block_bytes(samples_per_packet, sample_bytes)
        .ok_or(Error::UnsupportedPacketSize(samples_per_packet))
}

/// Announces a packet of `samples` the host has written to the next buffer, to play at
/// `time`, or as soon as it arrives with `None` (`sfe_tx4_push_ring_buffer`).
///
/// `samples` is 1 to 32768 and `time` at most [`MAX_TX_TIMESTAMP`]; callers check both.
pub(crate) fn push_tx(bus: &mut dyn Bus, samples: u32, time: Option<u64>) -> Result<(), Error> {
    bus.write_regs(REG_TX_PUSH, &tx_push_words(samples, time))
        .writing(FPGA, REG_TX_PUSH)
}

/// The first of the extended core's four announcing registers
/// (`M2PCI_REG_WR_TXDMA_CFG0`): bytes less one, then bursts and samples, then the
/// timestamp's high and low halves; the last write announces the packet. The same
/// addresses read back as the status words.
const REG_EXTENDED_PUSH: u32 = 28;

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
        // As libusdr: after a failed write the later changed registers are cached without
        // being written, and the announcing write is skipped; the first failure is returned.
        let mut failed = Ok(());
        for ((reg, word), written) in (REG_EXTENDED_PUSH..).zip(words).zip(&mut self.written) {
            if *written != word {
                if failed.is_ok() {
                    failed = bus.write_regs(reg, &[word]).writing(FPGA, reg);
                }
                *written = word;
            }
        }
        failed?;
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
