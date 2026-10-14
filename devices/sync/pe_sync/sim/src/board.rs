//! The simulated board: FPGA register space plus the chips behind its SPI and I2C cores.
//!
//! Register and bus facts below cite the pinned libusdr (`device/pe_sync/pe_sync.c`,
//! `device/pe_sync/sync_const.h`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::chips::{
    I2cRegisters, SpiRegisters, lmk1d1208i, lmk05318, lmx1204, lmx1214, lmx2820, lp87524,
};
use crate::trace::Op;

/// FPGA register holding the general-purpose outputs (`REG_WR_GPO`): `dev_gpo_set` writes
/// `(bank & 0x7f) << 24 | (data & 0xff)`.
const REG_GPO: u32 = 0;

/// SPI bus of `LMXGen0` (`SPI_LMX2820_0`).
const SPI_LMX2820_0: u32 = 2;
/// SPI bus of `LMXGen1` (`SPI_LMX2820_1`).
const SPI_LMX2820_1: u32 = 3;
/// SPI bus of the clock distribution buffer (`SPI_LMX1204`).
const SPI_LMX1204: u32 = 4;
/// SPI bus of the LO distribution buffer (`SPI_LMX1214`).
const SPI_LMX1214: u32 = 5;

/// `I2C_BUS_LMK05318B`: core 0, bus 0, 0x64.
const I2C_LMK05318: I2cAddress = I2cAddress {
    core: 0,
    bus: 0,
    addr: 0x64,
};
/// `I2C_BUS_LMK1D1208I_LCK`: core 0, bus 1, 0x68.
const I2C_LMK1D1208I_LCK: I2cAddress = I2cAddress {
    core: 0,
    bus: 1,
    addr: 0x68,
};
/// `I2C_BUS_LMK1D1208I_LRF`: core 0, bus 1, 0x69.
const I2C_LMK1D1208I_LRF: I2cAddress = I2cAddress {
    core: 0,
    bus: 1,
    addr: 0x69,
};
/// `I2C_BUS_LP87524`: core 1, bus 0, 0x60.
const I2C_LP87524: I2cAddress = I2cAddress {
    core: 1,
    bus: 0,
    addr: 0x60,
};

/// An I2C device address: FPGA I2C core, bus on that core, and 7-bit device address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct I2cAddress {
    /// The FPGA I2C core (`ls_op` instance). `PE_SYNC` has two (`/ll/i2c/0`, `/ll/i2c/1`).
    pub core: u8,
    /// The bus on that core.
    pub bus: u8,
    /// The 7-bit device address.
    pub addr: u16,
}

impl I2cAddress {
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

impl fmt::Display for I2cAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{:#04x}", self.core, self.bus, self.addr)
    }
}

/// A transfer the board cannot complete.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SimError {
    /// No modelled SPI device on this bus. Buses 0, 1 and 6 carry AD5662 DACs and bus 7
    /// the external front-end SPI (`pe_sync.c`); libusdr's open and close touch none of them.
    #[error("no modelled SPI device on bus {0}")]
    NoSpiDevice(u32),
}

/// A healthy `PE_SYNC` board whose chips answer as the pinned libusdr expects.
///
/// Assumptions, beyond each chip's own:
/// - FPGA registers are plain storage; unwritten ones read 0. That includes the status word
///   `IGPI_STAT` (register 20 = 16 + 16/4), which libusdr only logs; its meaning is not in
///   the source.
/// - Every chip answers regardless of the power GPOs (`IGPO_SY0_CTRL`, `IGPO_GEN_CTRL`,
///   `IGPO_DISTRIB_CTRL`, ...): the sim does not tie supplies to buses.
/// - Absent I2C devices read `0xff` per byte, as in `usdr-sim` (no NACK is reported by
///   libusdr's transports).
#[derive(Debug)]
pub struct SimBoard {
    /// FPGA registers, by address; unwritten registers read zero.
    regs: BTreeMap<u32, u32>,
    /// Last data written to each general-purpose output bank.
    gpo: BTreeMap<u8, u32>,
    /// `LMXGen0` and `LMXGen1`.
    lmx2820: [SpiRegisters; 2],
    /// The clock distribution buffer.
    lmx1204: SpiRegisters,
    /// The LO distribution buffer.
    lmx1214: SpiRegisters,
    /// The network synchronizer.
    lmk05318: I2cRegisters,
    /// The `LCK` and `LRF` fan-out buffers.
    lmk1d1208i: [I2cRegisters; 2],
    /// The PMIC.
    lp87524: I2cRegisters,
    /// I2C devices removed with [`SimBoard::without`].
    absent: BTreeSet<I2cAddress>,
    /// Virtual time, advanced only by [`SimBoard::sleep_us`].
    now_us: u64,
    /// Every operation performed, in order.
    trace: Vec<Op>,
}

impl SimBoard {
    /// A board with every chip fitted and fresh out of power-on reset.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: BTreeMap::new(),
            gpo: BTreeMap::new(),
            lmx2820: [lmx2820(), lmx2820()],
            lmx1204: lmx1204(),
            lmx1214: lmx1214(),
            lmk05318: lmk05318(),
            lmk1d1208i: [lmk1d1208i(), lmk1d1208i()],
            lp87524: lp87524(),
            absent: BTreeSet::new(),
            now_us: 0,
            trace: Vec::new(),
        }
    }

    /// The same board without the I2C device at `addr`, which then reads as the idle bus.
    #[must_use]
    pub fn without(mut self, addr: I2cAddress) -> Self {
        self.absent.insert(addr);
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

    /// Shifts one SPI word through the device on `bus` and returns the word read back.
    ///
    /// # Errors
    ///
    /// [`SimError::NoSpiDevice`] when the sim models nothing on `bus`.
    pub fn spi(&mut self, bus: u32, out: u32) -> Result<u32, SimError> {
        let chip = match bus {
            SPI_LMX2820_0 => &mut self.lmx2820[0],
            SPI_LMX2820_1 => &mut self.lmx2820[1],
            SPI_LMX1204 => &mut self.lmx1204,
            SPI_LMX1214 => &mut self.lmx1214,
            _ => return Err(SimError::NoSpiDevice(bus)),
        };
        let read = chip.transact(out);
        self.trace.push(Op::Spi { bus, out, read });
        Ok(read)
    }

    /// Writes `write` to the device at `addr`, then reads `read_len` bytes in wire order.
    pub fn i2c(&mut self, addr: I2cAddress, write: &[u8], read_len: usize) -> Vec<u8> {
        let chip = match addr {
            _ if self.absent.contains(&addr) => None,
            I2C_LMK05318 => Some(&mut self.lmk05318),
            I2C_LMK1D1208I_LCK => Some(&mut self.lmk1d1208i[0]),
            I2C_LMK1D1208I_LRF => Some(&mut self.lmk1d1208i[1]),
            I2C_LP87524 => Some(&mut self.lp87524),
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
}

impl Default for SimBoard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MAKE_LMX2820_REG_RD(R74)`: lock detect `RB_LD` in bits 15:14.
    const LMX2820_READ_R74: u32 = 0x00ca_0000;
    /// `MAKE_LMX1204_REG_RD(R75)`: lock detect `RB_LD` in bits 9:8.
    const LMX1204_READ_R75: u32 = 0x00cb_0000;
    /// `RB_LD_LOCKED` on both chips.
    const RB_LD_LOCKED: u32 = 2;

    #[test]
    fn gpo_writes_latch_per_bank() {
        let mut board = SimBoard::new();
        board.write_reg(REG_GPO, 6 << 24 | 0x55);
        board.write_reg(REG_GPO, 0x0d);
        assert_eq!(board.gpo(6), Some(0x55));
        assert_eq!(board.gpo(0), Some(0x0d));
    }

    #[test]
    fn each_synthesizer_has_its_own_register_file() {
        let mut board = SimBoard::new();
        board
            .spi(SPI_LMX2820_0, 0x0000_1234)
            .expect("bus 2 is wired");
        assert_eq!(board.spi(SPI_LMX2820_0, 0x0080_0000), Ok(0x1234));
        assert_eq!(board.spi(SPI_LMX2820_1, 0x0080_0000), Ok(0x4070));
    }

    /// libusdr's lock waits time out by wall clock, so an unlocked report would make the
    /// trace depend on host speed instead of failing at its cause.
    #[test]
    fn every_polled_lock_report_reads_locked() {
        let mut board = SimBoard::new();
        for bus in [SPI_LMX2820_0, SPI_LMX2820_1] {
            let r74 = board.spi(bus, LMX2820_READ_R74).expect("wired");
            assert_eq!(
                r74 >> 14 & 0b11,
                RB_LD_LOCKED,
                "LMX2820 on bus {bus} locked"
            );
        }
        let r75 = board.spi(SPI_LMX1204, LMX1204_READ_R75).expect("wired");
        assert_eq!(r75 >> 8 & 0b11, RB_LD_LOCKED, "LMX1204 locked");
        for calstat in [[0x01, 0x65], [0x01, 0x6f]] {
            let vm = board.i2c(I2C_LMK05318, &calstat, 1);
            assert_eq!(vm, [0x20], "LMK05318 PLLn_VM_INSIDE at {calstat:02x?}");
        }
    }

    #[test]
    fn unmodelled_spi_buses_are_errors() {
        let mut board = SimBoard::new();
        assert_eq!(board.spi(0, 0), Err(SimError::NoSpiDevice(0)));
        assert_eq!(board.trace(), [], "a refused word is not traced");
    }

    #[test]
    fn i2c_devices_are_told_apart_by_core_as_well_as_bus() {
        let mut board = SimBoard::new();
        let other_core = I2cAddress {
            core: 0,
            ..I2C_LP87524
        };
        assert_eq!(board.i2c(I2C_LP87524, &[0x04], 1), vec![0xc4]);
        assert_eq!(board.i2c(other_core, &[0x04], 1), vec![0xff]);
    }

    #[test]
    fn a_removed_device_reads_the_idle_bus() {
        let mut board = SimBoard::new().without(I2C_LMK05318);
        assert_eq!(
            board.i2c(I2C_LMK05318, &[0x00, 0x00], 4),
            vec![0xff; 4],
            "no identity bytes"
        );
    }

    #[test]
    fn every_operation_is_traced_in_order() {
        let mut board = SimBoard::new();
        board.write_reg(REG_GPO, 0);
        board.sleep_us(5);
        board.i2c(I2C_LMK05318, &[0x00, 0x00], 4);
        assert_eq!(board.trace().len(), 3);
        assert_eq!(board.trace()[1], Op::Sleep { us: 5 });
        assert_eq!(
            board.trace()[2],
            Op::I2c {
                addr: I2C_LMK05318,
                write: vec![0, 0],
                read: vec![0x10, 0x0b, 0x35, 0x42],
            }
        );
        assert_eq!(board.now_us(), 5);
    }
}
