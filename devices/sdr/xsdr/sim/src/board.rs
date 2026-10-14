//! The simulated board: FPGA register space plus the chips behind its SPI and I2C cores.
//!
//! Register and bus facts below cite the pinned libusdr (`device/m2_lm7_1/xsdr_ctrl.{c,h}`,
//! `device/generic_usdr/generic_regs.h`).

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use lms7002m_sim::Lms7002m;

use crate::chips::{Dac80501, I2cChip, Lp8758};
use crate::trace::Op;

/// FPGA register holding the general-purpose outputs: `dev_gpo_set` writes
/// `(bank & 0x7f) << 24 | (data & 0xff)` (`M2PCI_REG_STAT_CTRL`, an 8-bit GPO core).
const REG_GPO: u32 = 0;
/// General-purpose input word 3, the hardware ID: `dev_gpi_get32` reads `16 + bank / 4`
/// with `IGPI_HWID` = 12.
const REG_GPI_HWID: u32 = 16 + 12 / 4;
/// The RX PHY's configuration port (`REG_CFG_PHY_0`, `generic_regs.h`). libusdr writes a
/// bank and address there (`xsdr_phy_rx_reg`) and reads back what it selected: on the
/// rate path, the `LimeLight` LFSR checkers' error counters (`xsdr_phy_lfsr_mimo_state`).
/// A sim choice: the link is error-free, so every read of the port returns 0, unless a test
/// scripts the reads ([`SimBoard::with_rx_phy_reads`]). The capture and DC-estimator banks
/// behind it are not modelled.
const REG_CFG_PHY_0: u32 = 56;

/// GPO bank `IGPO_LMS_PWR`: LMS7002M LDO enable, reset and RX/TX enables.
const GPO_LMS_PWR: u8 = 0;
/// `IGPO_LMS_PWR` bit 4 hands the second I2C bus to the FPGA's I2C core. Evidence:
/// `_xsdr_init_revo` sets it ("Take control of second I2C bus") before talking to the LMS
/// PMIC on bus 1 and clears it ("Switch second I2C to gpio control") afterwards.
const LMS_PWR_SECOND_I2C: u32 = 1 << 4;
/// GPO bank `IGPO_LMS8_CTRL` (sSDR only).
const GPO_LMS8_CTRL: u8 = 15;
/// `IGPO_LMS8_CTRL` bit 0 steers SPI bus 0 to the LMS8001. Evidence: `xsdr_trspi_lms8`
/// writes 0x81 before an LMS8001 transfer and 0x80 after it.
const LMS8_SPI_SELECT: u32 = 1;

/// SPI bus carrying the LMS7002M (`SPI_LMS7`), and on sSDR also the LMS8001.
const SPI_LMS: u32 = 0;
/// The LMS7002M's metal-mask revision. `lms7002m_create`, the only version check on this
/// family's path, ignores it; 0 is a sim choice.
const LMS7_MASK: u8 = 0;
/// The LMS7002M's reference clock, which its VCO comparator model needs: the internal
/// 26 MHz reference `_xsdr_init_revx` selects (`XSDR_INT_REFCLK` in `xsdr_ctrl.c`, the
/// `fref` libusdr tunes with). The model does not follow a later switch to an external
/// reference.
const LMS7_REFERENCE_HZ: u32 = 26_000_000;

/// The LP8758 every variant has (`I2C_BUS_LP8758_FPGA`: bus 0, `I2C_DEV_PMIC_FPGA` 0x60).
const I2C_FPGA_PMIC: I2cAddress = I2cAddress { bus: 0, addr: 0x60 };
/// The XTRX's LMS7002M supply LP8758 (`I2C_BUS_LP8758_LMSINIT`: bus 1, 0x60).
const I2C_LMS_PMIC: I2cAddress = I2cAddress { bus: 1, addr: 0x60 };
/// The XTRX revision-5 DAC80501 (`I2C_BUS_DAC80501`: bus 0, `I2C_DEV_DAC80501M_A0_SCL`
/// 0x4b).
const I2C_DAC80501: I2cAddress = I2cAddress { bus: 0, addr: 0x4b };

/// The family members libusdr tells apart by HWID bits 23:16 (`enum xsdr_devices`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// XSDR (`XSDR_DEV` 0x30): libusdr's `new_rev` path with one PMIC and a front-end probe.
    Xsdr,
    /// XTRX (`XTRX_DEV` 0x2e): the older `revo` path with a second, LMS-side PMIC and a
    /// reference-trim DAC. Modelled as revision 5 (DAC80501 fitted).
    Xtrx,
    /// sSDR (`SSDR_DEV` 0x31). Identified only: the sim has no LMS8001.
    Ssdr,
    /// sSDR Pro (`SSDRPRO_DEV` 0x33). Identified only: the sim has no LMS8001.
    SsdrPro,
}

impl Variant {
    /// The device ID libusdr reads from HWID bits 23:16.
    #[must_use]
    pub const fn device_id(self) -> u8 {
        match self {
            Self::Xsdr => 0x30,
            Self::Xtrx => 0x2e,
            Self::Ssdr => 0x31,
            Self::SsdrPro => 0x33,
        }
    }

    /// The FPGA-side PMIC's revision word. libusdr expects 0xe302 on sSDR Pro and 0xe001
    /// on the others (`_xsdr_init_revx`, `_xsdr_init_revo`).
    const fn fpga_pmic_revision(self) -> u16 {
        match self {
            Self::SsdrPro => 0xe302,
            Self::Xsdr | Self::Xtrx | Self::Ssdr => 0xe001,
        }
    }
}

/// An I2C device address: FPGA I2C bus number and 7-bit device address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct I2cAddress {
    /// The FPGA I2C bus the device sits on.
    pub bus: u8,
    /// The 7-bit device address.
    pub addr: u16,
}

impl I2cAddress {
    /// Decodes libusdr's `ls_op` I2C address (`MAKE_LSOP_I2C_ADDR`):
    /// `[31:24] instance, [23:16] bus, [15:0] device`. The board has one I2C core, so the
    /// instance is ignored.
    #[must_use]
    pub fn from_lsop(lsop: u32) -> Self {
        let [_instance, bus, hi, lo] = lsop.to_be_bytes();
        Self {
            bus,
            addr: u16::from_be_bytes([hi, lo]),
        }
    }
}

impl fmt::Display for I2cAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:#04x}", self.bus, self.addr)
    }
}

/// A transfer the board cannot complete.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SimError {
    /// No SPI device is wired to this bus.
    #[error("no SPI device on bus {0}")]
    NoSpiDevice(u32),
}

/// A healthy LMS7 family board whose chips answer as the pinned libusdr expects.
#[derive(Debug)]
pub struct SimBoard {
    /// FPGA registers, by address; unwritten registers read zero.
    regs: BTreeMap<u32, u32>,
    /// Last data written to each general-purpose output bank.
    gpo: BTreeMap<u8, u32>,
    /// The RF transceiver on SPI bus 0.
    lms: Lms7002m,
    /// The FPGA-side LP8758 on bus 0.
    fpga_pmic: Lp8758,
    /// The XTRX's LMS-side LP8758 on bus 1.
    lms_pmic: Option<Lp8758>,
    /// The XTRX's reference-trim DAC.
    dac: Option<Dac80501>,
    /// Values the RX PHY port returns, in order, before [`Self::rx_phy_after`].
    rx_phy_script: VecDeque<u32>,
    /// What the RX PHY port returns once the script is spent.
    rx_phy_after: u32,
    /// Virtual time, advanced only by [`SimBoard::sleep_us`].
    now_us: u64,
    /// Every operation performed, in order.
    trace: Vec<Op>,
}

impl SimBoard {
    /// A board of the given variant.
    ///
    /// Its HWID is `device_id << 16`. Assumption: gateware revision (bits 15:8) and PHY
    /// feature bits (7:0) are 0, since the real values are not in the source. `xsdr_init`
    /// only stores the PHY bits, and the revision matters only to sSDR's LMS8001 stepping,
    /// so neither changes the XSDR/XTRX init trace; [`SimBoard::with_hwid`] overrides both.
    #[must_use]
    pub fn new(variant: Variant) -> Self {
        let xtrx = variant == Variant::Xtrx;
        Self {
            regs: BTreeMap::from([(REG_GPI_HWID, u32::from(variant.device_id()) << 16)]),
            gpo: BTreeMap::new(),
            lms: Lms7002m::new(LMS7_MASK).with_reference(LMS7_REFERENCE_HZ),
            fpga_pmic: Lp8758::with_revision(variant.fpga_pmic_revision()),
            // The C requires 0xe001 here too (`_xsdr_init_revo`).
            lms_pmic: xtrx.then(|| Lp8758::with_revision(0xe001)),
            dac: xtrx.then(Dac80501::new),
            rx_phy_script: VecDeque::new(),
            rx_phy_after: 0,
            now_us: 0,
            trace: Vec::new(),
        }
    }

    /// The same board whose RX PHY port returns `script`, one value per read, then `after`
    /// on every read: LFSR error counts for the rate path's link calibration (four
    /// counters per check, a check ending at the first with errors).
    #[must_use]
    pub fn with_rx_phy_reads(mut self, script: impl IntoIterator<Item = u32>, after: u32) -> Self {
        self.rx_phy_script = script.into_iter().collect();
        self.rx_phy_after = after;
        self
    }

    /// The same board with its LMS7002M's VCO comparators reading "fail" at cap code `cap`
    /// (`lms7002m-sim`'s fault injection), to drive libusdr's tuning error paths.
    #[must_use]
    pub fn with_lms_comparator_failure(mut self, cap: u8) -> Self {
        self.lms = Lms7002m::new(LMS7_MASK)
            .with_reference(LMS7_REFERENCE_HZ)
            .with_comparator_failure(cap);
        self
    }

    /// The same board reporting `hwid` instead, for gateware the model does not name.
    #[must_use]
    pub fn with_hwid(mut self, hwid: u32) -> Self {
        self.regs.insert(REG_GPI_HWID, hwid);
        self
    }

    /// The same board with an FPGA-side PMIC reporting `revision` (`OTP_REV << 8 |
    /// DEV_REV`) instead of the one libusdr expects, to exercise its refusal.
    #[must_use]
    pub fn with_fpga_pmic_revision(mut self, revision: u16) -> Self {
        self.fpga_pmic = Lp8758::with_revision(revision);
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
        let value = if addr == REG_CFG_PHY_0 {
            self.rx_phy_script.pop_front().unwrap_or(self.rx_phy_after)
        } else {
            self.regs.get(&addr).copied().unwrap_or(0)
        };
        self.trace.push(Op::RegRead { addr, value });
        value
    }

    /// Shifts one 32-bit word through the SPI device on `bus` and returns the word read back.
    ///
    /// While `IGPO_LMS8_CTRL` selects the LMS8001, the transfer goes to it instead of the
    /// LMS7002M. No variant the sim models fits one, so, as a sim choice, the floating MISO
    /// reads all ones.
    ///
    /// # Errors
    ///
    /// [`SimError::NoSpiDevice`] when nothing is wired to `bus`.
    pub fn spi(&mut self, bus: u32, out: u32) -> Result<u32, SimError> {
        if bus != SPI_LMS {
            return Err(SimError::NoSpiDevice(bus));
        }
        let lms8_selected = self.gpo(GPO_LMS8_CTRL).unwrap_or(0) & LMS8_SPI_SELECT != 0;
        let read = if lms8_selected {
            u32::MAX
        } else {
            self.lms.transact(out)
        };
        self.trace.push(Op::Spi { bus, out, read });
        Ok(read)
    }

    /// Writes `write` to the device at `addr`, then reads `read_len` bytes in wire order.
    ///
    /// Neither libusdr transport reports a missing acknowledge, so an absent device is not
    /// an error: as a sim choice (the same as `usdr-sim`'s), its reads return the
    /// idle-high bus, `0xff` per byte. libusdr's front-end probe relies on that to find no
    /// front end on XSDR's bus 1.
    pub fn i2c(&mut self, addr: I2cAddress, write: &[u8], read_len: usize) -> Vec<u8> {
        let second_bus_to_fpga = self.gpo(GPO_LMS_PWR).unwrap_or(0) & LMS_PWR_SECOND_I2C != 0;
        let chip: Option<&mut dyn I2cChip> = match addr {
            I2C_FPGA_PMIC => Some(&mut self.fpga_pmic),
            I2C_LMS_PMIC if second_bus_to_fpga => {
                self.lms_pmic.as_mut().map(|c| c as &mut dyn I2cChip)
            }
            I2C_DAC80501 => self.dac.as_mut().map(|c| c as &mut dyn I2cChip),
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

    /// An LMS7002M register.
    #[must_use]
    pub fn lms7_reg(&self, addr: u16) -> u16 {
        self.lms.reg(addr)
    }

    /// A register of the FPGA-side LP8758.
    #[must_use]
    pub fn fpga_pmic_reg(&self, addr: u8) -> u8 {
        self.fpga_pmic.reg(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hwid_carries_the_device_id_libusdr_dispatches_on() {
        let mut board = SimBoard::new(Variant::Xtrx);
        assert_eq!((board.read_reg(REG_GPI_HWID) >> 16) & 0xff, 0x2e);
    }

    #[test]
    fn gpo_writes_latch_per_bank() {
        let mut board = SimBoard::new(Variant::Xsdr);
        board.write_reg(REG_GPO, 17 << 24 | 1);
        board.write_reg(REG_GPO, 9);
        assert_eq!(board.gpo(17), Some(1));
        assert_eq!(board.gpo(GPO_LMS_PWR), Some(9));
    }

    #[test]
    fn the_xtrx_lms_pmic_answers_only_while_the_fpga_owns_bus_1() {
        let mut board = SimBoard::new(Variant::Xtrx);
        assert_eq!(board.i2c(I2C_LMS_PMIC, &[0x00], 1), vec![0xff]);
        board.write_reg(REG_GPO, LMS_PWR_SECOND_I2C);
        assert_eq!(board.i2c(I2C_LMS_PMIC, &[0x00], 1), vec![0x01]);
    }

    #[test]
    fn xsdr_has_nothing_on_bus_1() {
        let mut board = SimBoard::new(Variant::Xsdr);
        board.write_reg(REG_GPO, LMS_PWR_SECOND_I2C);
        assert_eq!(board.i2c(I2C_LMS_PMIC, &[0x00], 1), vec![0xff]);
    }

    #[test]
    fn selecting_the_lms8001_hides_the_lms7002m() {
        let mut board = SimBoard::new(Variant::Ssdr);
        board.write_reg(REG_GPO, u32::from(GPO_LMS8_CTRL) << 24 | 0x81);
        assert_eq!(board.spi(SPI_LMS, 0x002f_0000), Ok(u32::MAX));
        board.write_reg(REG_GPO, u32::from(GPO_LMS8_CTRL) << 24 | 0x80);
        assert_ne!(board.spi(SPI_LMS, 0x002f_0000), Ok(u32::MAX));
    }

    #[test]
    fn the_rx_phy_port_reads_an_error_free_link() {
        let mut board = SimBoard::new(Variant::Xsdr);
        board.write_reg(REG_CFG_PHY_0, 0x0308_0000);
        assert_eq!(board.read_reg(REG_CFG_PHY_0), 0, "no LFSR errors counted");
    }

    #[test]
    fn scripted_rx_phy_reads_come_in_order_then_repeat_the_last_value() {
        let mut board = SimBoard::new(Variant::Xsdr).with_rx_phy_reads([3, 0], 7);
        let reads: Vec<u32> = (0..4).map(|_| board.read_reg(REG_CFG_PHY_0)).collect();
        assert_eq!(reads, [3, 0, 7, 7]);
    }

    #[test]
    fn unwired_spi_buses_are_errors() {
        let mut board = SimBoard::new(Variant::Xsdr);
        assert_eq!(board.spi(1, 0), Err(SimError::NoSpiDevice(1)));
    }

    #[test]
    fn every_operation_is_traced_in_order() {
        let mut board = SimBoard::new(Variant::Xsdr);
        board.write_reg(REG_GPO, 0);
        board.sleep_us(5);
        board.i2c(I2C_FPGA_PMIC, &[0x00], 1);
        assert_eq!(board.trace().len(), 3);
        assert_eq!(board.trace()[1], Op::Sleep { us: 5 });
        assert_eq!(board.now_us(), 5);
    }
}
