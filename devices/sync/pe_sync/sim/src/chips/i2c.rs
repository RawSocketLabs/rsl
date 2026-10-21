//! The register file shared by the I2C parts (LMK05318, LMK1D1208I, LP87524): byte-wide
//! registers behind a register pointer of one or two bytes, sent MSB first, that
//! auto-increments past each byte written or read.
//!
//! Assumptions, each a sim choice the pinned source cannot settle:
//! - Undocumented addresses reset to 0 and are plain read/write storage.
//! - Read-only bits ignore writes.
//! - A transfer without a whole pointer reads the idle bus (`0xff`); libusdr always sends
//!   one.

use std::collections::BTreeMap;

use super::lock::{Lock, LockDetect};

/// One register's power-on state: address, reset value, read-only mask.
pub(crate) type Reset = (u16, u8, u8);

/// A chip's byte-wide registers.
#[derive(Debug)]
pub(crate) struct I2cRegisters {
    /// Register pointer width in bytes.
    pointer_len: usize,
    /// Register contents and read-only masks, by address; absent registers read 0.
    regs: BTreeMap<u16, (u8, u8)>,
    /// The PLLs' lock detects, on chips that report them.
    locks: Vec<LockDetect>,
}

impl I2cRegisters {
    /// A chip fresh out of power-on reset, addressed by a `pointer_len`-byte pointer.
    pub(crate) fn new(pointer_len: usize, reset: &[Reset]) -> Self {
        let regs = reset
            .iter()
            .map(|&(addr, value, read_only)| (addr, (value, read_only)))
            .collect();
        Self {
            pointer_len,
            regs,
            locks: Vec::new(),
        }
    }

    /// The same chip with bits `mask` of register `addr` reporting a PLL's lock: `locked`
    /// once locked, their reset value until then.
    ///
    /// # Panics
    ///
    /// When `mask` covers writable bits, which would let the driver overwrite the status.
    pub(crate) fn with_lock_detect(mut self, addr: u16, mask: u8, locked: u8) -> Self {
        let read_only = self.regs.get(&addr).map_or(0, |&(_, read_only)| read_only);
        assert_eq!(
            read_only & mask,
            mask,
            "invariant: a status occupies read-only bits"
        );
        self.locks
            .push(LockDetect::new(addr, mask.into(), locked.into()));
        self
    }

    /// Makes the lock detect at `addr` report as `lock`.
    ///
    /// # Panics
    ///
    /// When no lock detect is at `addr`.
    pub(crate) fn set_lock(&mut self, addr: u16, lock: Lock) {
        self.locks
            .iter_mut()
            .find(|detect| detect.addr == addr)
            .expect("invariant: only PLL status registers are given a lock behaviour")
            .set(lock);
    }

    /// Performs a write-then-read transfer: `write` is the bytes sent (register pointer
    /// first), and the returned bytes are what the chip transmits for `read_len` bytes.
    pub(crate) fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((pointer, data)) = write.split_at_checked(self.pointer_len) else {
            return vec![0xff; read_len];
        };
        let start = pointer
            .iter()
            .fold(0u16, |addr, &byte| addr << 8 | u16::from(byte));
        let mut addr = start;
        for &byte in data {
            let (value, read_only) = self.regs.entry(addr).or_default();
            *value = *value & *read_only | byte & !*read_only;
            addr = addr.wrapping_add(1);
        }
        (0..read_len)
            .map(|_| {
                let value = self.read(addr);
                addr = addr.wrapping_add(1);
                value
            })
            .collect()
    }

    /// A register's stored contents, lock detects aside.
    pub(crate) fn reg(&self, addr: u16) -> u8 {
        self.regs.get(&addr).map_or(0, |&(value, _)| value)
    }

    /// One read of a register: its contents as a lock detect there reports them.
    fn read(&mut self, addr: u16) -> u8 {
        let stored = self.reg(addr);
        self.locks
            .iter_mut()
            .find(|detect| detect.addr == addr)
            .map_or(stored, |detect| {
                u8::try_from(detect.read(stored.into()))
                    .expect("invariant: a byte register's lock bits are a byte")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_byte_read_auto_increments_from_a_16_bit_pointer() {
        let mut chip = I2cRegisters::new(2, &[(0x0100, 0x11, 0), (0x0101, 0x22, 0)]);
        assert_eq!(chip.transfer(&[0x01, 0x00], 3), vec![0x11, 0x22, 0x00]);
    }

    #[test]
    fn a_write_keeps_read_only_bits() {
        let mut chip = I2cRegisters::new(1, &[(0x05, 0x20, 0xf0)]);
        chip.transfer(&[0x05, 0xff], 0);
        assert_eq!(chip.reg(0x05), 0x2f);
    }

    #[test]
    fn a_lock_detect_reports_in_a_burst_read() {
        let mut chip =
            I2cRegisters::new(2, &[(0x0165, 0x00, 0xff)]).with_lock_detect(0x0165, 0x20, 0x20);
        assert_eq!(chip.transfer(&[0x01, 0x64], 2), vec![0x00, 0x20]);
        chip.set_lock(0x0165, Lock::Never);
        assert_eq!(chip.transfer(&[0x01, 0x65], 1), vec![0x00]);
    }

    #[test]
    fn a_short_pointer_reads_the_idle_bus() {
        let mut chip = I2cRegisters::new(2, &[]);
        assert_eq!(chip.transfer(&[0x00], 1), vec![0xff]);
    }
}
