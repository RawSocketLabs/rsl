//! The simulated board: FPGA register space plus the chips behind its SPI and I2C cores.

use std::collections::BTreeMap;
use std::fmt;

use crate::chips::{DcCalibration, I2cChip, Lms6002d, PllLock, Reg8File, Si5332, Tmp114};
use crate::trace::Op;

/// FPGA register holding the general-purpose outputs: `bank << 24 | data`. Writes with
/// bit 31 set load the I2C address LUT instead and leave the outputs alone.
const REG_GPO: u32 = 0;
/// Bit 31 of a [`REG_GPO`] write selects the I2C LUT.
const GPO_I2C_LUT: u32 = 1 << 31;
/// Board temperature the sim cools towards, in millidegrees Celsius.
const AMBIENT_MC: i32 = 25_000;

/// General-purpose output bank enabling the revision-3 on-board oscillator.
const GPO_ENABLE_OSC: u8 = 17;
/// General-purpose input bank 3: the hardware ID.
const REG_GPI_HWID: u32 = 16 + 3;
/// HWID bits 25:24: the board has RX and TX chains.
const HWID_RX_TX: u32 = 0b11 << 24;
/// HWID bit 24: the board has a TX chain.
const HWID_TX: u32 = 1 << 24;

/// SPI bus carrying the LMS6002D.
const SPI_LMS6: u32 = 0;

/// The `LP8758` PMIC.
const I2C_PMIC: I2cAddress = I2cAddress { bus: 0, addr: 0x60 };
/// The TMP114 temperature sensor, on every revision: libusdr's `usdr_gettemp` reads it here
/// regardless of revision, though only revision 3 checks its ID during init.
const I2C_TEMP: I2cAddress = I2cAddress { bus: 0, addr: 0x4e };
/// The Si5332 clock generator.
const I2C_CLOCK: I2cAddress = I2cAddress { bus: 0, addr: 0x6a };
/// The `TPS6381x` boost converter.
const I2C_BOOST: I2cAddress = I2cAddress { bus: 0, addr: 0x75 };

/// The uSDR board revisions libusdr supports; the revision sits in HWID bits 15:8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardRevision {
    /// Revision 1.
    Rev1 = 1,
    /// Revision 2.
    Rev2 = 2,
    /// Revision 3: adds the TMP114 check and the oscillator-enable output.
    Rev3 = 3,
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
    /// Decodes libusdr's `ls_op` I2C address: `[31:24] core, [23:16] bus, [15:0] device`.
    /// The board has one I2C core, so the core index is ignored.
    #[must_use]
    pub fn from_lsop(lsop: u32) -> Self {
        let [_core, bus, hi, lo] = lsop.to_be_bytes();
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

/// A healthy uSDR board whose chips respond as their datasheets describe.
#[derive(Debug)]
pub struct SimBoard {
    /// FPGA registers, by address; unwritten registers read zero.
    regs: BTreeMap<u32, u32>,
    /// Last value written to each general-purpose output bank.
    gpo: BTreeMap<u8, u32>,
    /// The RF transceiver on SPI bus 0.
    lms: Lms6002d,
    /// The LP8758 PMIC.
    pmic: Reg8File,
    /// The `TPS6381x` boost converter.
    boost: Reg8File,
    /// The Si5332 clock generator.
    clock: Si5332,
    /// The Si5332's reference stays off until [`GPO_ENABLE_OSC`] is set.
    oscillator_gated: bool,
    /// The TMP114 temperature sensor.
    temp: Tmp114,
    /// Whether the TMP114 is fitted; see [`SimBoard::without_temperature_sensor`].
    temp_fitted: bool,
    /// Board temperature, in millidegrees Celsius.
    temperature_mc: i32,
    /// How fast the board cools towards [`AMBIENT_MC`] while time passes.
    cooling_mc_per_s: i32,
    /// Virtual time, advanced only by [`SimBoard::sleep_us`].
    now_us: u64,
    /// Every operation performed, in order.
    trace: Vec<Op>,
}

impl SimBoard {
    /// A board of the given revision at 25 °C.
    #[must_use]
    pub fn new(revision: BoardRevision) -> Self {
        let hwid = HWID_RX_TX | (revision as u32) << 8;
        Self {
            regs: BTreeMap::from([(REG_GPI_HWID, hwid)]),
            gpo: BTreeMap::new(),
            lms: Lms6002d::new(),
            // DEV_REV 0x01 and OTP_REV 0xe0: libusdr requires revision 0xe001.
            pmic: Reg8File::with_resets(&[(0x00, 0x01), (0x01, 0xe0)]),
            // DEVID (0x03) reads 4.
            boost: Reg8File::with_resets(&[(0x03, 0x04)]),
            clock: Si5332::new(),
            oscillator_gated: false,
            temp: Tmp114::new(AMBIENT_MC),
            temp_fitted: true,
            temperature_mc: AMBIENT_MC,
            cooling_mc_per_s: 0,
            now_us: 0,
            trace: Vec::new(),
        }
    }

    /// A board whose clock generator has no input until the driver enables the on-board
    /// oscillator (GPO 17), as on a cold revision-3 board: until then the Si5332 reports
    /// "no input clock".
    #[must_use]
    pub fn with_oscillator_off(mut self) -> Self {
        self.oscillator_gated = true;
        self
    }

    /// A board whose gateware has an RX chain but no TX chain (HWID bit 24 clear), where
    /// libusdr neither decimates nor accepts rates below 1 MS/s.
    #[must_use]
    pub fn with_rx_only(mut self) -> Self {
        if let Some(hwid) = self.regs.get_mut(&REG_GPI_HWID) {
            *hwid &= !HWID_TX;
        }
        self
    }

    /// A board whose RX PLL comparator answers as `lock` describes, instead of
    /// [`PllLock::default`]; the TX PLL keeps the default.
    #[must_use]
    pub fn with_rx_pll_lock(mut self, lock: PllLock) -> Self {
        self.lms.rx_lock = lock;
        self
    }

    /// A board whose LMS6002D DC calibration engines answer as `calibration` describes,
    /// instead of [`DcCalibration::default`].
    #[must_use]
    pub fn with_dc_calibration(mut self, calibration: DcCalibration) -> Self {
        self.lms.dc_calibration = calibration;
        self
    }

    /// A board at `millicelsius` instead of 25 °C.
    #[must_use]
    pub fn with_temperature(mut self, millicelsius: i32) -> Self {
        self.set_temperature(millicelsius);
        self
    }

    /// A board with no TMP114 answering: its reads return the idle bus.
    #[must_use]
    pub fn without_temperature_sensor(mut self) -> Self {
        self.temp_fitted = false;
        self
    }

    /// A board that cools towards 25 °C at `millicelsius_per_second` of virtual time.
    #[must_use]
    pub fn with_cooling(mut self, millicelsius_per_second: i32) -> Self {
        self.cooling_mc_per_s = millicelsius_per_second;
        self
    }

    /// Sets the board temperature the TMP114 reports.
    pub fn set_temperature(&mut self, millicelsius: i32) {
        self.temperature_mc = millicelsius;
        self.temp.set_millicelsius(millicelsius);
    }

    /// Writes a 32-bit FPGA register.
    pub fn write_reg(&mut self, addr: u32, value: u32) {
        self.trace.push(Op::RegWrite { addr, value });
        if addr == REG_GPO && value & GPO_I2C_LUT == 0 {
            let [bank, ..] = value.to_be_bytes();
            self.gpo.insert(bank, value & 0x00ff_ffff);
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
    /// # Errors
    ///
    /// [`SimError::NoSpiDevice`] when nothing is wired to `bus`.
    pub fn spi(&mut self, bus: u32, out: u32) -> Result<u32, SimError> {
        if bus != SPI_LMS6 {
            return Err(SimError::NoSpiDevice(bus));
        }
        let read = self.lms.transact(out);
        self.trace.push(Op::Spi { bus, out, read });
        Ok(read)
    }

    /// Writes `write` to the device at `addr`, then reads `read_len` bytes in wire order.
    ///
    /// Neither libusdr transport reports a missing acknowledge, so an absent device is not an
    /// error: as a sim choice, its reads return the idle-high bus (`0xff` per byte).
    pub fn i2c(&mut self, addr: I2cAddress, write: &[u8], read_len: usize) -> Vec<u8> {
        let chip: Option<&mut dyn I2cChip> = match addr {
            I2C_PMIC => Some(&mut self.pmic),
            I2C_TEMP if self.temp_fitted => Some(&mut self.temp),
            I2C_CLOCK => {
                self.clock.input_clock =
                    !self.oscillator_gated || self.gpo.get(&GPO_ENABLE_OSC) == Some(&1);
                Some(&mut self.clock)
            }
            I2C_BOOST => Some(&mut self.boost),
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
        if self.temperature_mc > AMBIENT_MC && self.cooling_mc_per_s > 0 {
            let cooled = i64::from(self.cooling_mc_per_s) * i64::try_from(us).unwrap_or(i64::MAX)
                / 1_000_000;
            let cooled = i32::try_from(cooled).unwrap_or(i32::MAX);
            self.set_temperature(self.temperature_mc.saturating_sub(cooled).max(AMBIENT_MC));
        }
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

    /// The last value written to a general-purpose output bank, if any.
    #[must_use]
    pub fn gpo(&self, bank: u8) -> Option<u32> {
        self.gpo.get(&bank).copied()
    }

    /// An LMS6002D register.
    #[must_use]
    pub fn lms_reg(&self, addr: u8) -> u8 {
        self.lms.regs[usize::from(addr & 0x7f)]
    }

    /// An LP8758 PMIC register.
    #[must_use]
    pub fn pmic_reg(&self, addr: u8) -> u8 {
        self.pmic.regs[usize::from(addr)]
    }

    /// A `TPS6381x` boost-converter register.
    #[must_use]
    pub fn boost_reg(&self, addr: u8) -> u8 {
        self.boost.regs[usize::from(addr)]
    }

    /// A Si5332 clock-generator register.
    #[must_use]
    pub fn clock_reg(&self, addr: u8) -> u8 {
        self.clock.file.regs[usize::from(addr)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsop_i2c_address_drops_the_core_index() {
        let lsop = 0x01 << 24 | 0x01 << 16 | 0x004e;
        assert_eq!(
            I2cAddress::from_lsop(lsop),
            I2cAddress { bus: 1, addr: 0x4e }
        );
    }

    #[test]
    fn tmp114_sends_its_id_msb_first() {
        let mut board = SimBoard::new(BoardRevision::Rev3);
        assert_eq!(board.i2c(I2C_TEMP, &[0x0b], 2), vec![0x11, 0x14]);
    }

    #[test]
    fn gpo_writes_latch_per_bank_but_lut_writes_do_not() {
        let mut board = SimBoard::new(BoardRevision::Rev3);
        board.write_reg(REG_GPO, 7 << 24 | 1);
        board.write_reg(REG_GPO, GPO_I2C_LUT | 7 << 24);
        assert_eq!(board.gpo(7), Some(1));
    }

    #[test]
    fn absent_i2c_devices_read_idle_high_and_are_traced() {
        let mut board = SimBoard::new(BoardRevision::Rev3);
        let missing = I2cAddress { bus: 1, addr: 0x10 };
        assert_eq!(board.i2c(missing, &[0], 2), vec![0xff, 0xff]);
        assert_eq!(board.trace().len(), 1);
    }

    #[test]
    fn a_gated_oscillator_starves_the_clock_until_enabled() {
        let mut board = SimBoard::new(BoardRevision::Rev3).with_oscillator_off();
        assert_eq!(board.i2c(I2C_CLOCK, &[0x07], 1), vec![0x89]);
        board.write_reg(REG_GPO, u32::from(GPO_ENABLE_OSC) << 24 | 1);
        assert_eq!(board.i2c(I2C_CLOCK, &[0x07], 1), vec![0x01]);
    }

    #[test]
    fn unwired_spi_buses_are_errors() {
        let mut board = SimBoard::new(BoardRevision::Rev3);
        assert_eq!(board.spi(1, 0), Err(SimError::NoSpiDevice(1)));
    }
}
