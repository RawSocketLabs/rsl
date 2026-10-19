//! The simulated board: FPGA register space plus the chips behind its SPI and I2C cores.

use std::collections::BTreeMap;
use std::fmt;

use sdr_frontend_sim::{FrontEnd, Model};

use crate::chips::{DcCalibration, I2cChip, Lms6002d, PllLock, Reg8File, Si5332, Tmp114};
use crate::flash::{FlashController, SimFlash};
use crate::loopback::{Chains, Loopback};
use crate::stream::RxEngine;
use crate::sync::SyncUnit;
use crate::trace::Op;
use crate::tx_stream::{TxEngine, TxGrant, TxPacket};

/// FPGA register holding the general-purpose outputs: `bank << 24 | data`. Writes with
/// bit 31 set load the I2C address LUT instead and leave the outputs alone.
const REG_GPO: u32 = 0;
/// Bit 31 of a [`REG_GPO`] write selects the I2C LUT.
const GPO_I2C_LUT: u32 = 1 << 31;
/// Board temperature the sim cools towards, in millidegrees Celsius.
const AMBIENT_MC: i32 = 25_000;

/// General-purpose output bank enabling the revision-3 on-board oscillator.
const GPO_ENABLE_OSC: u8 = 17;
/// General-purpose input bank 0: the gateware build (`USR_ACCESS2`).
const REG_GPI_BUILD: u32 = 16;
/// The gateware build a board reports by default: the oldest libusdr streams TX from
/// (`MINIM_FWID_COMPAT`). Real builds stamp a date; this one is a valid date.
const DEFAULT_BUILD: u32 = 0xd2b1_0c09;
/// General-purpose input bank 3: the hardware ID.
const REG_GPI_HWID: u32 = 16 + 3;
/// General-purpose input word 24, in bank 6: the clock measurement libusdr's
/// `/dm/sdr/0/clkmeas` reads.
const REG_GPI_CLOCK_MEASUREMENT: u32 = 16 + 6;
/// HWID bits 25:24: the board has RX and TX chains.
const HWID_RX_TX: u32 = 0b11 << 24;
/// HWID bit 24: the board has a TX chain.
const HWID_TX: u32 = 1 << 24;
/// HWID bit 25: the board has an RX chain.
const HWID_RX: u32 = 1 << 25;
/// HWID bit 26: the TX chain has the extended stream core.
const HWID_EXTENDED_TX: u32 = 1 << 26;

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
/// The M.2 connector's I2C bus (`I2C_BUS_FRONTEND`), where an attached front end's chips
/// answer.
const FRONT_END_BUS: u8 = 1;

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
    /// The FPGA's RX stream engine.
    rx: RxEngine,
    /// The FPGA's TX stream core and its DMA buffers.
    tx: TxEngine,
    /// The FPGA's stream sync unit.
    sync: SyncUnit,
    /// The FPGA's QSPI flash controller and its flash, when the test fits one; without it,
    /// the controller's registers are plain storage as before.
    flash: Option<FlashController>,
    /// The front end on the M.2 connector, if any; see [`SimBoard::with_front_end`].
    front_end: Option<Model>,
    /// The DSP chains' configuration ports, the RX DC estimator and the loopback.
    chains: Chains,
}

impl SimBoard {
    /// A board of the given revision at 25 °C.
    #[must_use]
    pub fn new(revision: BoardRevision) -> Self {
        let hwid = HWID_RX_TX | (revision as u32) << 8;
        Self {
            regs: BTreeMap::from([(REG_GPI_BUILD, DEFAULT_BUILD), (REG_GPI_HWID, hwid)]),
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
            rx: RxEngine::default(),
            tx: TxEngine::default(),
            sync: SyncUnit::default(),
            flash: None,
            front_end: None,
            chains: Chains::default(),
        }
    }

    /// A board whose FPGA flash controller has `flash` behind it; see [`SimFlash`] for what
    /// the controller model assumes.
    #[must_use]
    pub fn with_flash(mut self, flash: SimFlash) -> Self {
        self.flash = Some(FlashController::new(flash));
        self
    }

    /// A board whose flash controller reports busy forever, so its status polls time out.
    ///
    /// # Panics
    ///
    /// Unless [`SimBoard::with_flash`] fitted a flash first.
    #[must_use]
    pub fn with_stuck_flash_controller(mut self) -> Self {
        self.flash
            .as_mut()
            .expect("fit a flash with `with_flash` first")
            .stuck = true;
        self
    }

    /// A board whose transmitter reaches its receiver as `loopback` describes, for the RX
    /// DC estimator to measure; see [`Loopback`] for the model. Without one, the estimator
    /// measures nothing (zero).
    #[must_use]
    pub fn with_loopback(mut self, loopback: Loopback) -> Self {
        self.chains.loopback = Some(loopback);
        self
    }

    /// A board with `front_end` on its M.2 connector. Without one, as by default, the
    /// connector's I2C bus reads idle and libusdr's probe settles on a breakout without a
    /// DAC.
    #[must_use]
    pub fn with_front_end(mut self, front_end: FrontEnd) -> Self {
        self.front_end = Some(Model::new(front_end));
        self
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

    /// A board whose gateware has a TX chain but no RX chain (HWID bit 25 clear).
    #[must_use]
    pub fn with_tx_only(mut self) -> Self {
        if let Some(hwid) = self.regs.get_mut(&REG_GPI_HWID) {
            *hwid &= !HWID_RX;
        }
        self
    }

    /// A board whose gateware reports build `build` (`USR_ACCESS2`, GPI word 0) instead of
    /// the oldest libusdr streams TX from.
    #[must_use]
    pub fn with_gateware_build(mut self, build: u32) -> Self {
        self.regs.insert(REG_GPI_BUILD, build);
        self
    }

    /// A board whose gateware has the extended TX stream core (HWID bit 26), announced
    /// through registers 28 to 31; see the TX stream module.
    #[must_use]
    pub fn with_extended_tx_core(mut self) -> Self {
        if let Some(hwid) = self.regs.get_mut(&REG_GPI_HWID) {
            *hwid |= HWID_EXTENDED_TX;
        }
        self.tx.extended = true;
        self
    }

    /// A board whose RX PLL comparator answers as `lock` describes, instead of
    /// [`PllLock::default`]; the TX PLL keeps the default.
    #[must_use]
    pub fn with_rx_pll_lock(mut self, lock: PllLock) -> Self {
        self.lms.rx_lock = lock;
        self
    }

    /// A board whose TX PLL comparator answers as `lock` describes, instead of
    /// [`PllLock::default`]; the RX PLL keeps the default.
    #[must_use]
    pub fn with_tx_pll_lock(mut self, lock: PllLock) -> Self {
        self.lms.tx_lock = lock;
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

    /// Sets the word the gateware reports at GPI word 24 (register 22), which libusdr's
    /// `/dm/sdr/0/clkmeas` reads, without tracing anything. The sim does not model a clock
    /// measurement: the pinned source documents neither the word nor its control (GPO 16),
    /// so the word is whatever a test loads, and writes to the control leave it alone.
    pub fn set_clock_measurement(&mut self, word: u32) {
        self.regs.insert(REG_GPI_CLOCK_MEASUREMENT, word);
    }

    /// Writes a 32-bit FPGA register.
    pub fn write_reg(&mut self, addr: u32, value: u32) {
        self.trace.push(Op::RegWrite { addr, value });
        self.rx.on_write(addr, value);
        self.tx.on_write(addr, value);
        self.sync.on_write(addr, value);
        self.chains.on_write(addr, value, self.now_us);
        self.play_tx();
        if let Some(flash) = self.flash.as_mut().filter(|_| FlashController::owns(addr)) {
            flash.write(addr, value, self.now_us);
            return;
        }
        if addr == REG_GPO && value & GPO_I2C_LUT == 0 {
            let [bank, ..] = value.to_be_bytes();
            self.gpo.insert(bank, value & 0x00ff_ffff);
        }
        self.regs.insert(addr, value);
    }

    /// Reads a 32-bit FPGA register.
    pub fn read_reg(&mut self, addr: u32) -> u32 {
        let value = match &self.flash {
            Some(flash) if FlashController::owns(addr) => flash.read(addr),
            _ => self
                .tx
                .status_word(addr)
                .or_else(|| self.chains.read(addr, self.now_us, &self.lms))
                .or_else(|| self.regs.get(&addr).copied())
                .unwrap_or(0),
        };
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
            I2cAddress {
                bus: FRONT_END_BUS,
                addr,
            } => self
                .front_end
                .as_mut()
                .and_then(|front_end| front_end.chip(addr)),
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

    /// The next RX block the stream engine delivers, with its two out-of-band words, or
    /// `None` while the engine is stopped. Below the register seam both drivers share, so
    /// not traced; see the stream module for what the engine does and does not model.
    pub fn rx_next_block(&mut self) -> Option<(Vec<u8>, [u64; 2])> {
        let released = self.sync.releases(self.rx.running(), self.tx.running());
        self.rx.next_block(released)
    }

    /// Sets the TX buffers up as the `PCIe` driver does: 32 of `block_bytes`, free, none
    /// with a completion record. Below the register seam, so not traced; see the TX
    /// stream module.
    pub fn tx_open(&mut self, block_bytes: usize) {
        self.tx.open(block_bytes);
    }

    /// Bytes in each TX buffer.
    #[must_use]
    pub fn tx_block_bytes(&self) -> usize {
        self.tx.block_bytes()
    }

    /// Hands out the next free TX buffer, with its completion record (the TX core's first
    /// three status words and the kernel time) if a played packet freed it; `None` while
    /// every buffer holds a packet not yet played. Not traced.
    pub fn tx_grant(&mut self) -> Option<TxGrant> {
        self.tx.grant()
    }

    /// The host's bytes for the buffer it was last granted, which the next announcement
    /// at registers 12 and 13 hands to the core. Not traced.
    pub fn tx_load(&mut self, bytes: &[u8]) {
        self.tx.load(bytes);
    }

    /// Releases the TX buffers. Not traced.
    pub fn tx_close(&mut self) {
        self.tx.close();
    }

    /// The packets the TX core has played, in order.
    #[must_use]
    pub fn tx_packets(&self) -> &[TxPacket] {
        self.tx.played()
    }

    /// Sets the TX core's four status words (registers 28 to 31), which later completion
    /// records also carry.
    pub fn set_tx_status(&mut self, words: [u32; 4]) {
        self.tx.status = words;
    }

    /// Plays the TX core's announced packets if it runs and the sync unit lets it.
    fn play_tx(&mut self) {
        let released = self.sync.releases(self.rx.running(), self.tx.running());
        // The kernel's nanosecond clock, low 32 bits, as the driver records it.
        let kernel_time =
            u32::try_from(self.now_us.wrapping_mul(1000) & 0xffff_ffff).expect("32 bits");
        self.tx.play(released, kernel_time);
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

    /// The flash behind the FPGA's flash controller, if [`SimBoard::with_flash`] fitted one.
    #[must_use]
    pub fn flash(&self) -> Option<&SimFlash> {
        self.flash.as_ref().map(|flash| &flash.chip)
    }

    /// An LMS6002D register.
    #[must_use]
    pub fn lms_reg(&self, addr: u8) -> u8 {
        self.lms.regs[usize::from(addr & 0x7f)]
    }

    /// An LP8758 PMIC register.
    #[must_use]
    pub fn pmic_reg(&self, addr: u8) -> u8 {
        self.pmic.reg(addr)
    }

    /// A `TPS6381x` boost-converter register.
    #[must_use]
    pub fn boost_reg(&self, addr: u8) -> u8 {
        self.boost.reg(addr)
    }

    /// A board whose front-end DAC identifies as `id` instead of a DAC80501M (0x0195);
    /// 0x0000 or 0xBEEF is what libusdr takes for no DAC.
    ///
    /// # Panics
    ///
    /// If no front end with a DAC is attached.
    #[must_use]
    pub fn with_front_end_dac_id(mut self, id: u16) -> Self {
        let has_dac = self
            .front_end
            .as_mut()
            .is_some_and(|front_end| front_end.set_dac_id(id));
        assert!(has_dac, "no front end with a DAC is attached");
        self
    }

    /// A board whose `PCIe` front end's RF-path expander reads all inputs low, as a failed
    /// part might.
    ///
    /// # Panics
    ///
    /// If the attached front end is not the `PCIe` front end.
    #[must_use]
    pub fn with_dead_front_end_expander(mut self) -> Self {
        let pciefe = self
            .front_end
            .as_mut()
            .is_some_and(Model::stick_expander_low);
        assert!(pciefe, "the PCIe front end is not attached");
        self
    }

    /// A board whose front end's LMK05318 (`SimpleSync`, XMASS) reports its charge pumps
    /// out of range forever, so its APLLs never lock.
    ///
    /// # Panics
    ///
    /// If no front end with an LMK05318 is attached.
    #[must_use]
    pub fn with_unlocked_front_end_clock(mut self) -> Self {
        let unlocked = self.front_end.as_mut().is_some_and(Model::unlock_lmk05318);
        assert!(unlocked, "no front end with an LMK05318 is attached");
        self
    }

    /// A board whose front end's LMK05318 (`SimpleSync`, XMASS) does not answer: its reads
    /// return the idle bus, as for an absent chip.
    ///
    /// # Panics
    ///
    /// If no front end with an LMK05318 is attached.
    #[must_use]
    pub fn without_front_end_clock(mut self) -> Self {
        let removed = self.front_end.as_mut().is_some_and(Model::remove_lmk05318);
        assert!(removed, "no front end with an LMK05318 is attached");
        self
    }

    /// The front end's oscillator DAC code, if a front end with one is attached.
    #[must_use]
    pub fn front_end_dac_code(&self) -> Option<u16> {
        self.front_end.as_ref().and_then(Model::dac_code)
    }

    /// A register of the front end's clock synchronizer, if a front end with one is
    /// attached.
    #[must_use]
    pub fn front_end_clock_reg(&self, addr: u16) -> Option<u8> {
        self.front_end
            .as_ref()
            .and_then(|front_end| front_end.clock_reg(addr))
    }

    /// A Si5332 clock-generator register.
    #[must_use]
    pub fn clock_reg(&self, addr: u8) -> u8 {
        self.clock.file.reg(addr)
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
