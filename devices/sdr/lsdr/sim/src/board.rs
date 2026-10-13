//! The simulated board: FPGA register space plus the chips behind its SPI and I2C cores.
//!
//! Register and bus facts below cite the pinned libusdr
//! (`device/m2_lsdr/m2_da09_4_ad45_2.c`, `device/generic_usdr/generic_regs.h`).

use std::collections::BTreeMap;
use std::fmt;

use crate::chips::{Ads42lbx9, I2cChip, Lmk04832, Si549, Tps6594};
use crate::trace::Op;

/// FPGA register 0: the general-purpose outputs on write (`dev_gpo_set` writes
/// `(bank & 0x7f) << 24 | (data & 0xff)`, `M2PCI_REG_STAT_CTRL`, an 8-bit GPO core), and
/// the word open reads first to tell a live board from a dead link (all ones).
const REG_GPO: u32 = 0;

/// The SPI cores (`DNLL_SPI_COUNT` 5): `SPI_ADS42LBX9_0`, `SPI_ADS42LBX9_1`,
/// `SPI_LMK04832`, `SPI_DAC` and `SPI_EXTERNAL` (the front-end connector).
const SPI_BUSES: u32 = 5;
/// `SPI_ADS42LBX9_0`.
const SPI_ADC0: u32 = 0;
/// `SPI_ADS42LBX9_1`.
const SPI_ADC1: u32 = 1;
/// `SPI_LMK04832`.
const SPI_LMK04832: u32 = 2;

/// `I2C_BUS_TPS6594`: bus 0, 0x48.
const I2C_TPS6594: I2cAddress = I2cAddress::new(0, 0x48);
/// `I2C_BUS_SI549`: bus 0, 0x67.
const I2C_SI549: I2cAddress = I2cAddress::new(0, 0x67);

/// `LDO4_CTRL`, the TPS6594 register switching the ADC supply (`_lsdr_adc_activate`).
const TPS6594_LDO4_CTRL: u8 = 0x20;
/// `LDOX_EN`, bit 0 of an LDO control register.
const TPS6594_LDO_EN: u8 = 1;
/// The Si549's `ODC` register; bit 0 `OE` enables its output clock.
const SI549_ODC: u8 = 17;

/// The board fits libusdr's one `LSDr` factory tells apart by probing, not by an ID.
///
/// The factory has one device ID (`s_uuid_000`, commented "single ADC, no DAC") and reads
/// no hardware ID; open looks for an ADC on SPI 0 and 1 and, only when asked with the
/// `lmk=1` parameter, an LMK04832 on SPI 2. The source also names "dual ADC + `clk_gen`"
/// boards, without registering an ID for them; both fits here use the one ID. No fit has a
/// DAC: libusdr declares no TX channel and never sets `dac38rf_present`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// One `ADS42LBx9` (on SPI 0), clocked by the Si549; the registered board.
    SingleAdc,
    /// Two `ADS42LBx9` (SPI 0 and 1) and an LMK04832 clock generator (SPI 2).
    DualAdcClockGen,
}

/// An I2C device address: FPGA I2C core (libusdr's instance), bus number and 7-bit device
/// address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct I2cAddress {
    /// The FPGA I2C core (controller instance). The factory declares one
    /// (`DNLL_I2C_COUNT` 1), but the front-end probe's fe4ch4007200 addresses core 1, and the
    /// oracle shim passes that through to the board, so the trace keeps it.
    pub core: u8,
    /// The FPGA I2C bus the device sits on.
    pub bus: u8,
    /// The 7-bit device address.
    pub addr: u16,
}

impl I2cAddress {
    /// The device at `addr` on bus `bus` of core 0, where every chip of the board sits.
    #[must_use]
    pub const fn new(bus: u8, addr: u16) -> Self {
        Self { core: 0, bus, addr }
    }

    /// Decodes libusdr's `ls_op` I2C address (`MAKE_LSOP_I2C_ADDR`):
    /// `[31:24] instance, [23:16] bus, [15:0] device`.
    #[must_use]
    pub fn from_lsop(lsop: u32) -> Self {
        let [core, bus, hi, lo] = lsop.to_be_bytes();
        Self {
            core,
            bus,
            addr: u16::from_be_bytes([hi, lo]),
        }
    }
}

/// `BUS:ADDR` on core 0, `CORE:BUS:ADDR` on any other core.
impl fmt::Display for I2cAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.core != 0 {
            write!(f, "{}:", self.core)?;
        }
        write!(f, "{}:{:#04x}", self.bus, self.addr)
    }
}

/// A transfer the board cannot complete.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SimError {
    /// The FPGA has no SPI core with this number.
    #[error("no SPI core {0}")]
    NoSpiBus(u32),
}

/// A healthy `LSDr` board whose chips answer as the pinned libusdr expects.
#[derive(Debug)]
pub struct SimBoard {
    /// FPGA registers, by address; unwritten registers read zero (assumption: the
    /// gateware's reset values are not in the source).
    regs: BTreeMap<u32, u32>,
    /// Last data written to each general-purpose output bank.
    gpo: BTreeMap<u8, u32>,
    /// The ADCs on SPI 0 and 1, where fitted.
    adcs: [Option<Ads42lbx9>; 2],
    /// The clock generator on SPI 2, where fitted.
    lmk: Option<Lmk04832>,
    /// The PMIC.
    pmic: Tps6594,
    /// The ADC clock oscillator.
    oscillator: Si549,
    /// Virtual time, advanced only by [`SimBoard::sleep_us`].
    now_us: u64,
    /// Every operation performed, in order.
    trace: Vec<Op>,
}

impl SimBoard {
    /// A board with `variant`'s chips fitted, all at power-on.
    #[must_use]
    pub fn new(variant: Variant) -> Self {
        let dual = variant == Variant::DualAdcClockGen;
        Self {
            regs: BTreeMap::new(),
            gpo: BTreeMap::new(),
            adcs: [Some(Ads42lbx9::new()), dual.then(Ads42lbx9::new)],
            lmk: dual.then(Lmk04832::new),
            pmic: Tps6594::new(),
            oscillator: Si549::new(),
            now_us: 0,
            trace: Vec::new(),
        }
    }

    /// The same board behind a dead link: register 0 reads all ones, the value libusdr's
    /// open takes for "Device is dead!".
    #[must_use]
    pub fn dead(mut self) -> Self {
        self.regs.insert(REG_GPO, u32::MAX);
        self
    }

    /// Writes a 32-bit FPGA register.
    pub fn write_reg(&mut self, addr: u32, value: u32) {
        self.trace.push(Op::RegWrite { addr, value });
        if addr == REG_GPO {
            let [bank, ..] = value.to_be_bytes();
            self.gpo.insert(bank & 0x7f, value & 0xff);
        }
        self.regs.insert(addr, value);
    }

    /// Reads a 32-bit FPGA register.
    pub fn read_reg(&mut self, addr: u32) -> u32 {
        let value = self.regs.get(&addr).copied().unwrap_or(0);
        self.trace.push(Op::RegRead { addr, value });
        value
    }

    /// Shifts one 32-bit word through the SPI device on `bus` and returns the word read back.
    ///
    /// A bus with nothing fitted (SPI 1 and 2 on a single-ADC board, the DAC bus, the
    /// empty front-end connector) reads all ones, as a sim choice: a floating MISO. That
    /// fails `ads42lbx9_reset_and_check` and `lmk04832_reset_and_test`, which libusdr takes
    /// for an absent chip.
    ///
    /// # Errors
    ///
    /// [`SimError::NoSpiBus`] for a bus beyond the FPGA's five SPI cores.
    pub fn spi(&mut self, bus: u32, out: u32) -> Result<u32, SimError> {
        if bus >= SPI_BUSES {
            return Err(SimError::NoSpiBus(bus));
        }
        let read = match bus {
            SPI_ADC0 | SPI_ADC1 => self.adcs[bus as usize]
                .as_mut()
                .map(|adc| adc.transact(out)),
            SPI_LMK04832 => self.lmk.as_mut().map(|lmk| lmk.transact(out)),
            _ => None,
        }
        .unwrap_or(u32::MAX);
        self.trace.push(Op::Spi { bus, out, read });
        Ok(read)
    }

    /// Writes `write` to the device at `addr`, then reads `read_len` bytes in wire order.
    ///
    /// Neither libusdr transport reports a missing acknowledge, so an absent device is not
    /// an error: as a sim choice (the same as `rsl-usdr-sim`'s), its reads return the
    /// idle-high bus, `0xff` per byte.
    pub fn i2c(&mut self, addr: I2cAddress, write: &[u8], read_len: usize) -> Vec<u8> {
        let chip: Option<&mut dyn I2cChip> = match addr {
            I2C_TPS6594 => Some(&mut self.pmic),
            I2C_SI549 => Some(&mut self.oscillator),
            _ => None,
        };
        let read = chip.map_or_else(
            || vec![0xff; read_len],
            |chip| chip.transfer(write, read_len),
        );
        self.trace.push(Op::I2c {
            addr,
            write: write.to_vec(),
            read: read.clone(),
        });
        read
    }

    /// Advances virtual time.
    pub fn sleep_us(&mut self, us: u64) {
        self.now_us += us;
        self.trace.push(Op::Sleep { us });
    }

    /// Virtual microseconds elapsed since the board was created.
    #[must_use]
    pub fn now_us(&self) -> u64 {
        self.now_us
    }

    /// Every operation performed so far, in order.
    #[must_use]
    pub fn trace(&self) -> &[Op] {
        &self.trace
    }

    /// The last data written to a general-purpose output bank, if any.
    #[must_use]
    pub fn gpo(&self, bank: u8) -> Option<u32> {
        self.gpo.get(&bank).copied()
    }

    /// Whether the PMIC's LDO4, the ADC supply, is enabled.
    #[must_use]
    pub fn adc_supply_enabled(&self) -> bool {
        self.pmic.reg(TPS6594_LDO4_CTRL) & TPS6594_LDO_EN != 0
    }

    /// Whether the Si549 drives its output clock (`ODC` `OE`).
    #[must_use]
    pub fn adc_clock_enabled(&self) -> bool {
        self.oscillator.reg(SI549_ODC) & 1 != 0
    }

    /// A register of the TPS6594 PMIC.
    #[must_use]
    pub fn pmic_reg(&self, addr: u8) -> u8 {
        self.pmic.reg(addr)
    }

    /// A register of the ADC on SPI `index` (0 or 1), or `None` where none is fitted.
    #[must_use]
    pub fn adc_reg(&self, index: usize, addr: u8) -> Option<u8> {
        self.adcs.get(index)?.as_ref().map(|adc| adc.reg(addr))
    }

    /// A register of the LMK04832, or `None` where none is fitted.
    #[must_use]
    pub fn lmk_reg(&self, addr: u16) -> Option<u8> {
        self.lmk.as_ref().map(|lmk| lmk.reg(addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpo_writes_latch_per_bank() {
        let mut board = SimBoard::new(Variant::SingleAdc);
        board.write_reg(REG_GPO, 2 << 24 | 0x03);
        board.write_reg(REG_GPO, 5 << 24 | 0x0f);
        assert_eq!(board.gpo(2), Some(0x03));
        assert_eq!(board.gpo(5), Some(0x0f));
    }

    #[test]
    fn a_dead_board_reads_all_ones_where_open_looks_first() {
        let mut board = SimBoard::new(Variant::SingleAdc).dead();
        assert_eq!(board.read_reg(REG_GPO), u32::MAX);
    }

    #[test]
    fn a_single_adc_board_has_nothing_on_the_second_adc_or_clock_bus() {
        let mut board = SimBoard::new(Variant::SingleAdc);
        let hf0 = 0x8000 | 0x0d << 8;
        assert_eq!(board.spi(SPI_ADC0, hf0).map(|r| r & 0xfe), Ok(0x6c));
        assert_eq!(board.spi(SPI_ADC1, hf0), Ok(u32::MAX));
        assert_eq!(board.spi(SPI_LMK04832, 0x80_0400), Ok(u32::MAX));
        assert_eq!(board.adc_reg(1, 0x0d), None);
    }

    #[test]
    fn spi_cores_beyond_the_fifth_are_errors() {
        let mut board = SimBoard::new(Variant::SingleAdc);
        assert_eq!(
            board.spi(4, 0),
            Ok(u32::MAX),
            "the front-end connector exists"
        );
        assert_eq!(board.spi(5, 0), Err(SimError::NoSpiBus(5)));
    }

    #[test]
    fn the_supplies_and_clock_are_seen_through_their_chips() {
        let mut board = SimBoard::new(Variant::SingleAdc);
        assert!(!board.adc_supply_enabled());
        board.i2c(I2C_TPS6594, &[TPS6594_LDO4_CTRL, 0x33], 0);
        assert!(board.adc_supply_enabled());
        board.i2c(I2C_SI549, &[SI549_ODC, 1], 0);
        assert!(board.adc_clock_enabled());
    }

    #[test]
    fn every_operation_is_traced_in_order() {
        let mut board = SimBoard::new(Variant::SingleAdc);
        board.write_reg(REG_GPO, 0);
        board.sleep_us(5);
        board.i2c(I2C_SI549, &[0x00], 1);
        assert_eq!(board.trace().len(), 3);
        assert_eq!(board.trace()[1], Op::Sleep { us: 5 });
        assert_eq!(board.now_us(), 5);
    }
}
