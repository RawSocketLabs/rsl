//! The uSDR's TX stream engine: the original TX core (`CORE_SFETX_DMA32_R0`, libusdr's
//! "Simple TX FE"), which takes `ci16` packets from the host's DMA buffers, and the
//! extended one (`CORE_EXFETX_DMA32_R0_2`).
//!
//! The core takes three things: control commands (the portable machines in
//! `portable/stream.rs` write them), two words announcing each packet the host has written,
//! and reads of its four status words. The original core's announcement words, status
//! decoding and the create's checks, and the extended core's cached announcement, are the
//! shared `sdr-stream`'s (the LMS7 family's gateware has the same cores at the same
//! registers); this module runs them over the bus.
//!
//! Source: `ipblks/streams/sfe_tx_4.c` (`sfe_tx4_push_ring_buffer`,
//! `sfe_extx4_push_ring_buffer`), `stream_sfetrx4_dma32.c`; addresses from
//! `device/m2_lm6_1/m2_lm6_1.c`'s TX stream create (`M2PCI_REG_WR_TXDMA_CNF_L`,
//! `M2PCI_REG_WR_TXDMA_CFG0`, `M2PCI_REG_RD_TXDMA_STAT`).

use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// The largest timestamp the extended core is given (`sdr-stream`'s); USB's header
/// carries 63 bits too.
pub(crate) use sdr_stream::MAX_EXTENDED_TX_TIMESTAMP;
pub(crate) use sdr_stream::{MAX_TX_TIMESTAMP, TxCoreStatus, extend_drop_count};
use sdr_stream::{
    REG_TX_PUSH, REG_TX_STATUS, tx_block_bytes, tx_gateware_supported, tx_push_words,
};

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

/// The extended core's announcing registers as libusdr last wrote them (`extxcfg_cache`,
/// `sdr-stream`'s), run over the bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExtendedTxPush {
    /// The cache.
    cache: sdr_stream::ExtendedTxPush,
}

impl ExtendedTxPush {
    /// The cache a new stream starts with (`extxcfg_cache_init`).
    pub(crate) const fn new() -> Self {
        Self {
            cache: sdr_stream::ExtendedTxPush::new(),
        }
    }

    /// Announces a one-burst packet of `samples` (per channel) in `bytes` the host has
    /// written, to play at `time` or, with `None`, as soon as it arrives:
    /// `sfe_extx4_push_ring_buffer` from `stream_soft_extx_commit`.
    ///
    /// `bytes` and `samples` are 1 to 2^20; callers check both. As libusdr: after a failed
    /// write the later changed registers are cached without being written, and the
    /// announcing write is skipped; the first failure is returned.
    pub(crate) fn push(
        &mut self,
        bus: &mut dyn Bus,
        bytes: u32,
        samples: u32,
        time: Option<u64>,
    ) -> Result<(), Error> {
        for &(reg, word) in self.cache.announce(bytes, samples, time).as_slice() {
            bus.write_regs(reg, &[word]).writing(FPGA, reg)?;
        }
        Ok(())
    }
}

/// Reads the core's four status words in one transaction (`lowlevel_reg_rdndw`).
pub(crate) fn read_tx_status(bus: &mut dyn Bus) -> Result<[u32; 4], Error> {
    let mut words = [0; 4];
    bus.read_regs(REG_TX_STATUS, &mut words)
        .reading(FPGA, REG_TX_STATUS)?;
    Ok(words)
}
