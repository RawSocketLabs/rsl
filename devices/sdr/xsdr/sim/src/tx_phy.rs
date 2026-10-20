//! The TX PHY's configuration port (`REG_CFG_PHY_1`, register 57) and the TX MMCM behind
//! it, as libusdr drives them (`device/m2_lm7_1/xsdr_ctrl.c`): `xsdr_phy_tx_reg` writes
//! `register << 24 | value` (24 bits), and a read returns the port's status.
//!
//! Modelled, from the C that drives it (no gateware source is vendored):
//! - The MMCM's dynamic reconfiguration port, reached through PHY register 1
//!   (`PHY_REG_MMCM_DRP`), as `xsdr_override_drp` serves `m2_lm7_1`'s TX DRP port: the
//!   value's bits 22:16 address a 16-bit MMCM register, bit 23 makes it a write of bits
//!   15:0. A read latches the register, which the port reports in bits 31:16 of its next
//!   reads (`*pin = rb >> 16`). Every transaction completes at once: the busy flag (bit
//!   9, which libusdr polls) never reads set.
//! - The MMCM's registers are plain storage; unwritten ones read 0. Real MMCMs start from
//!   the bitstream's configuration, which libusdr's read-modify-writes
//!   (`ipblks/xlnx_mmcm.c`) keep in their unwritten fields; no vendored source gives it, so
//!   the sim's 0 is a sim choice those fields inherit.
//! - Lock (status bit 8, `rb & (1 << 8)` in `xsdr_configure_lml_mmcm_tx`): the MMCM locks
//!   at once whenever PHY register 2 (`PHY_REG_MMCM_CTRL`) last took a value with bits 1:0
//!   clear: `_xsdr_mmcm_pd` writes 2 (power down) and the configuration writes `sel | 1`
//!   around the DRP writes, then `sel | 0`. Whatever the DRP registers hold, the model
//!   locks; it does not check the configuration. The "no clock" flags (bits 11:10) never
//!   read set.
//! - The generator's A/B-I/Q tables (`PHY_REG_IQAB_PSHA`, `_PSHB`): each push shifts a
//!   24-bit word into its channel's table, which keeps the last 64. libusdr fills both
//!   with 64 words at a time (`xsdr_phy_load_sincos_64`, and `xsdr_phy_load_iqab_check`'s
//!   32 rounds of two each), the evidence for the depth. What the RX PHY's capture reads
//!   of them is the board's (`board.rs`).
//! - Faults, injected, not modelled ([`MmcmFault`]): the first DRP commands staying busy
//!   for some reads, an MMCM that stops locking after some releases, or one reporting no
//!   clock.
//! - Every other PHY register (port control, delay, I/Q select, the generators' run and
//!   pattern selection) is accepted and has no effect the port reports.

use std::collections::{BTreeMap, VecDeque};

/// The TX PHY's configuration port (`REG_CFG_PHY_1`, `generic_regs.h`).
pub(crate) const REG_CFG_PHY_1: u32 = 57;

/// PHY register: the MMCM's dynamic reconfiguration port (`PHY_REG_MMCM_DRP`).
const PHY_MMCM_DRP: u32 = 1;
/// PHY register: the MMCM's reset and power-down (`PHY_REG_MMCM_CTRL`).
const PHY_MMCM_CTRL: u32 = 2;
/// PHY register: a push into channel A's A/B-I/Q table (`PHY_REG_IQAB_PSHA`).
const PHY_IQAB_PUSH_A: u32 = 8;
/// PHY register: channel B's (`PHY_REG_IQAB_PSHB`).
const PHY_IQAB_PUSH_B: u32 = 9;
/// Words each A/B-I/Q table keeps.
const TABLE_DEPTH: usize = 64;

/// `PHY_REG_MMCM_CTRL`'s reset (bit 0) and power-down (bit 1) bits.
const MMCM_HELD: u32 = 0b11;
/// A DRP command's write flag (`1 << 23` in `xsdr_override_drp`).
const DRP_WRITE: u32 = 1 << 23;
/// The status's lock flag.
const LOCKED: u32 = 1 << 8;
/// The status's busy flag (a DRP transaction in progress).
const BUSY: u32 = 1 << 9;
/// One of the status's "no clock" flags (`rb & 0xc00`).
const NO_CLOCK: u32 = 1 << 10;

/// A fault the TX MMCM's model can be given, to drive libusdr's error and wait paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MmcmFault {
    /// None: every DRP transaction completes at once and the MMCM locks when released.
    #[default]
    None,

    /// The first `commands` DRP commands each read busy for `polls` status reads.
    Busy {
        /// DRP commands that stay busy, from the first.
        commands: u32,
        /// Status reads each stays busy for.
        polls: u32,
    },

    /// The MMCM locks on its first `releases` releases from reset only, never after.
    LocksOnly {
        /// Releases that still lock.
        releases: u32,
    },

    /// The status reports no clock (bit 10) on every read.
    NoClock,
}

/// The TX PHY port's observable state.
#[derive(Debug, Default)]
pub(crate) struct TxPhy {
    /// The MMCM's DRP registers, by address.
    mmcm: BTreeMap<u8, u16>,

    /// The register the last DRP read latched.
    read_data: u16,

    /// The MMCM control last written has its reset and power-down bits clear.
    locked: bool,

    /// Channel A's and B's A/B-I/Q tables, oldest word first.
    tables: [VecDeque<u32>; 2],

    /// The fault injected.
    fault: MmcmFault,

    /// Status reads the current DRP command still reads busy for.
    busy_reads: u32,

    /// Releases of the MMCM from reset so far.
    releases: u32,
}

impl TxPhy {
    /// Follows a write to the port.
    pub(crate) fn on_write(&mut self, value: u32) {
        let (register, data) = (value >> 24, value & 0x00ff_ffff);
        match register {
            PHY_MMCM_DRP => {
                // A new command ends any busy reads the last one left.
                self.busy_reads = 0;
                if let MmcmFault::Busy { commands, polls } = &mut self.fault {
                    if *commands > 0 {
                        *commands -= 1;
                        self.busy_reads = *polls;
                    }
                }
                let addr = u8::try_from(data >> 16 & 0x7f).expect("7 bits");
                if data & DRP_WRITE != 0 {
                    let word = u16::try_from(data & 0xffff).expect("16 bits");
                    self.mmcm.insert(addr, word);
                } else {
                    self.read_data = self.mmcm.get(&addr).copied().unwrap_or(0);
                }
            }
            PHY_MMCM_CTRL => {
                self.locked = data & MMCM_HELD == 0;
                self.releases += u32::from(self.locked);
            }
            PHY_IQAB_PUSH_A | PHY_IQAB_PUSH_B => {
                let table = &mut self.tables[usize::from(register == PHY_IQAB_PUSH_B)];
                table.push_back(data);
                if table.len() > TABLE_DEPTH {
                    table.pop_front();
                }
            }
            _ => {}
        }
    }

    /// Injects `fault` from now on.
    pub(crate) const fn set_fault(&mut self, fault: MmcmFault) {
        self.fault = fault;
    }

    /// The port's status word on a read: the last DRP read in bits 31:16, busy in bit 9
    /// while an injected busy command lasts, the lock flag in bit 8, and with
    /// [`MmcmFault::NoClock`] bit 10.
    pub(crate) fn status(&mut self) -> u32 {
        let busy = if self.busy_reads > 0 {
            self.busy_reads -= 1;
            BUSY
        } else {
            0
        };
        let locked = self.locked
            && match self.fault {
                MmcmFault::LocksOnly { releases } => self.releases <= releases,
                _ => true,
            };
        let no_clock = if self.fault == MmcmFault::NoClock {
            NO_CLOCK
        } else {
            0
        };
        u32::from(self.read_data) << 16 | busy | no_clock | if locked { LOCKED } else { 0 }
    }

    /// Word `index` of channel B's (`channel_b`) or A's table counted from the newest,
    /// wrapping at its length; 0 for a table never pushed.
    pub(crate) fn table_word(&self, channel_b: bool, index: usize) -> u32 {
        let table = &self.tables[usize::from(channel_b)];
        let Some(last) = table.len().checked_sub(1) else {
            return 0;
        };
        table[last - index % table.len()]
    }

    /// An MMCM DRP register.
    pub(crate) fn mmcm_reg(&self, addr: u8) -> u16 {
        self.mmcm.get(&addr).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `xsdr_phy_tx_reg(d, register, value)`.
    const fn phy(register: u32, value: u32) -> u32 {
        register << 24 | value & 0x00ff_ffff
    }

    #[test]
    fn a_drp_read_returns_what_a_write_left_in_the_upper_half() {
        let mut port = TxPhy::default();
        port.on_write(phy(PHY_MMCM_DRP, DRP_WRITE | 0x14 << 16 | 0xbeef));
        port.on_write(phy(PHY_MMCM_DRP, 0x14 << 16));
        assert_eq!(port.status() >> 16, 0xbeef);
        assert_eq!(port.status() & 1 << 9, 0, "never busy");
        port.on_write(phy(PHY_MMCM_DRP, 0x15 << 16));
        assert_eq!(port.status() >> 16, 0, "an unwritten register reads 0");
    }

    #[test]
    fn injected_faults_hold_the_drp_busy_or_the_lock_off() {
        let mut port = TxPhy::default();
        port.set_fault(MmcmFault::Busy {
            commands: 1,
            polls: 2,
        });
        port.on_write(phy(PHY_MMCM_DRP, 0x14 << 16));
        let busy: Vec<bool> = (0..3).map(|_| port.status() & BUSY != 0).collect();
        assert_eq!(busy, [true, true, false], "two busy reads");
        port.on_write(phy(PHY_MMCM_DRP, 0x14 << 16));
        assert_eq!(port.status() & BUSY, 0, "only the first command");
        port.set_fault(MmcmFault::LocksOnly { releases: 1 });
        port.on_write(phy(PHY_MMCM_CTRL, 4));
        assert_eq!(port.status() & LOCKED, LOCKED, "the first release locks");
        port.on_write(phy(PHY_MMCM_CTRL, 4));
        assert_eq!(port.status() & LOCKED, 0, "the second never does");
        port.set_fault(MmcmFault::NoClock);
        assert_ne!(port.status() & 0xc00, 0, "no clock");
    }

    #[test]
    fn a_table_keeps_its_last_64_pushes_newest_first() {
        let mut port = TxPhy::default();
        assert_eq!(port.table_word(false, 0), 0, "never pushed");
        for word in 0..70 {
            port.on_write(phy(PHY_IQAB_PUSH_A, word));
        }
        port.on_write(phy(PHY_IQAB_PUSH_B, 0x00ab_cdef));
        assert_eq!(port.table_word(false, 0), 69, "the newest");
        assert_eq!(port.table_word(false, 63), 6, "the oldest kept");
        assert_eq!(port.table_word(false, 64), 69, "wrapping");
        assert_eq!(port.table_word(true, 5), 0x00ab_cdef, "B's own table");
    }

    #[test]
    fn the_mmcm_locks_once_released_and_unlocks_in_reset_or_powered_down() {
        let mut port = TxPhy::default();
        assert_eq!(port.status() & LOCKED, 0, "unlocked from reset");
        for (control, locked) in [(4 | 1, false), (4, true), (2, false), (0, true)] {
            port.on_write(phy(PHY_MMCM_CTRL, control));
            assert_eq!(port.status() & LOCKED != 0, locked, "control {control}");
        }
    }
}
