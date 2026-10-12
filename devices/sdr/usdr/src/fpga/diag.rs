//! Diagnostic registers libusdr exposes raw: the ATCRBS DSP core's register and the TX DMA
//! engine's status words. Neither has documented fields in the pinned source.

use super::gpio::FPGA;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// The ATCRBS DSP core's register (`M2PCI_REG_WR_LBDSP` / `M2PCI_REG_RD_LBDSP`), one
/// address for both directions; libusdr's `/ll/dsp/atcrbs/0/base`.
const REG_ATCRBS: u32 = 52;

/// The first of the TX DMA engine's four status words (`M2PCI_REG_RD_TXDMA_STAT`, then
/// `_STATM`, `_STATTS` and `_STAT_CPL`).
const REG_TX_DMA_STATUS: u32 = 28;

/// Writes the ATCRBS core's register (`dev_m2_lm6_1_sdr_atcrbs_set`).
pub(crate) fn write_atcrbs(bus: &mut dyn Bus, value: u32) -> Result<(), Error> {
    bus.write_regs(REG_ATCRBS, &[value])
        .writing(FPGA, REG_ATCRBS)
}

/// Reads the ATCRBS core's register (`dev_m2_lm6_1_sdr_atcrbs_get`).
pub(crate) fn read_atcrbs(bus: &mut dyn Bus) -> Result<u32, Error> {
    let mut value = [0];
    bus.read_regs(REG_ATCRBS, &mut value)
        .reading(FPGA, REG_ATCRBS)?;
    Ok(value[0])
}

/// Reads the TX DMA engine's four status words one register at a time, as
/// `dev_m2_lm6_1_debug_all_get` does.
pub(crate) fn read_tx_dma_status(bus: &mut dyn Bus) -> Result<[u32; 4], Error> {
    let mut words = [0; 4];
    for (reg, word) in (REG_TX_DMA_STATUS..).zip(&mut words) {
        let mut value = [0];
        bus.read_regs(reg, &mut value).reading(FPGA, reg)?;
        *word = value[0];
    }
    Ok(words)
}
