//! The R820T2's I2C framing and its 32-register file.
//!
//! Writes are page writes: the first byte after the address names the first register and
//! each further byte fills the next one (p. 3, Figure 1-1). Reads always start at register 0
//! and each byte arrives least significant bit first (p. 4), so a byte read by an
//! MSB-first I2C master is the register bit-reversed.

use core::fmt;

use super::field::Field;
use super::setting::FieldValue;

/// The 7-bit I2C slave address. prov: R820-DS-01
pub const I2C_ADDRESS: u8 = 0x1A;

/// How many registers the chip has. prov: R820-DS-02
pub const REGISTER_COUNT: usize = 32;

/// The first register the host may write; R0 to R4 are read-only. prov: R820-DS-06
pub const FIRST_WRITABLE: u8 = 0x05;

/// R0's fixed content, the read-mode reference check point. prov: R820-DS-05
pub const CHIP_ID: u8 = 0x96;

/// A request the register file cannot honour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterError {
    /// A value wider than its field.
    TooWide {
        /// The field.
        field: Field,

        /// The value offered.
        value: u8,
    },

    /// A write starting below [`FIRST_WRITABLE`], past the last register, or empty.
    NotWritable {
        /// The first register.
        first: u8,

        /// How many registers.
        count: usize,
    },

    /// A read longer than the register file.
    TooLong(usize),

    /// A page write covering a register whose contents were never read from the chip:
    /// writing it would send guessed bits, and the power-on defaults and Table 1-2's fixed
    /// bits are `R820-GAP-08`.
    NotRead(u8),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooWide { field, value } => write!(f, "{value:#x} does not fit {field:?}"),
            Self::NotWritable { first, count } => {
                write!(f, "registers {first:#04x}+{count} are not a writable range")
            }
            Self::TooLong(len) => write!(f, "a {len}-byte read exceeds the register file"),
            Self::NotRead(register) => write!(
                f,
                "register {register:#04x} was never read; its other bits are R820-GAP-08"
            ),
        }
    }
}

impl core::error::Error for RegisterError {}

/// The host's image of the register file, and which registers in it were read from the
/// chip. Only those may be page-written: the chip's power-on defaults, and whether Table
/// 1-2's fixed bits are required values, are not documented (`R820-GAP-08`), so a register
/// the image has not read would go out with guessed bits. [`Registers::default`] knows no
/// register: it serves the field maths, not writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Registers {
    /// The bytes, by address.
    bytes: [u8; REGISTER_COUNT],

    /// Bit `n` set: register `n` holds what the chip returned (then edited by `set`).
    read: u32,
}

impl Registers {
    /// The registers a read returned, `wire` being the bytes an MSB-first I2C master
    /// received from register 0 on.
    ///
    /// # Errors
    ///
    /// [`RegisterError::TooLong`] past the last register.
    pub fn from_read(wire: &[u8]) -> Result<Self, RegisterError> {
        if wire.len() > REGISTER_COUNT {
            return Err(RegisterError::TooLong(wire.len()));
        }
        let mut registers = Self::default();
        for (register, byte) in registers.bytes.iter_mut().zip(wire) {
            *register = byte.reverse_bits();
        }
        // The first `wire.len()` (at most 32) registers were read.
        registers.read = u32::try_from((1_u64 << wire.len()) - 1).unwrap_or(u32::MAX);
        Ok(registers)
    }

    /// Register `address`'s byte.
    #[must_use]
    pub fn byte(&self, address: u8) -> Option<u8> {
        self.bytes.get(usize::from(address)).copied()
    }

    /// Whether R0 holds the R820T2's reference value.
    #[must_use]
    pub fn has_chip_id(&self) -> bool {
        self.bytes[0] == CHIP_ID
    }

    /// `field`'s value.
    #[must_use]
    pub fn get(&self, field: Field) -> u8 {
        field.extract(self.bytes[usize::from(field.register())])
    }

    /// Sets `field` to `value`, leaving the register's other bits alone.
    ///
    /// # Errors
    ///
    /// [`RegisterError::TooWide`] if `value` does not fit the field.
    pub fn set(&mut self, field: Field, value: u8) -> Result<(), RegisterError> {
        let register = &mut self.bytes[usize::from(field.register())];
        *register = field.insert(*register, value)?;
        Ok(())
    }

    /// Sets a typed setting's field.
    ///
    /// # Panics
    ///
    /// If `V::bits` returns a value wider than `V::FIELD`, a broken [`FieldValue`] impl.
    pub fn put<V: FieldValue>(&mut self, value: V) {
        self.set(V::FIELD, value.bits())
            .expect("invariant: a setting's bits fit its field");
    }

    /// A typed setting's field, if its bits name a documented setting.
    #[must_use]
    pub fn take<V: FieldValue>(&self) -> Option<V> {
        V::from_bits(self.get(V::FIELD))
    }

    /// The page write that sends registers `first..first + count`.
    ///
    /// # Errors
    ///
    /// [`RegisterError::NotWritable`] for an empty range, one starting below
    /// [`FIRST_WRITABLE`] or one running past the last register;
    /// [`RegisterError::NotRead`] for a register in it the image has not read.
    pub fn page_write(&self, first: u8, count: usize) -> Result<PageWrite, RegisterError> {
        let start = usize::from(first);
        let refused = RegisterError::NotWritable { first, count };
        if first < FIRST_WRITABLE || count == 0 || start + count > REGISTER_COUNT {
            return Err(refused);
        }
        if let Some(unread) = (first..).take(count).find(|&r| self.read >> r & 1 == 0) {
            return Err(RegisterError::NotRead(unread));
        }
        let mut bytes = [0; REGISTER_COUNT + 1];
        bytes[0] = first;
        bytes[1..=count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(PageWrite {
            bytes,
            len: count + 1,
        })
    }
}

/// An I2C write's payload: the first register's address, then the registers' bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageWrite {
    /// The payload, `len` bytes of it used.
    bytes: [u8; REGISTER_COUNT + 1],

    /// How many bytes are used.
    len: usize,
}

impl PageWrite {
    /// The bytes to send after the slave address.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::r820t2::field;

    #[test]
    fn the_slave_address_gives_table_1_1s_write_and_read_bytes() {
        // R820-DS-01: Table 1-1 lists 0x34 for write and 0x35 for read.
        assert_eq!(I2C_ADDRESS << 1, 0x34);
        assert_eq!(I2C_ADDRESS << 1 | 1, 0x35);
    }

    #[test]
    fn table_1_2s_r0_bits_are_the_chip_id() {
        // R820-DS-05: Table 1-2 row R0, B7..B0 = 1 0 0 1 0 1 1 0.
        assert_eq!(CHIP_ID, 0b1001_0110);
    }

    #[test]
    fn a_read_is_bit_reversed_into_registers_from_r0() {
        // R820-DS-04: bytes arrive LSB first, from register 0.
        let registers = Registers::from_read(&[CHIP_ID.reverse_bits(), 0x01, 0x80]).unwrap();
        assert!(
            registers.has_chip_id(),
            "R0 is the first byte, bit-reversed"
        );
        assert_eq!(registers.byte(1), Some(0x80));
        assert_eq!(registers.byte(2), Some(0x01));
        assert_eq!(
            Registers::from_read(&[0; REGISTER_COUNT + 1]),
            Err(RegisterError::TooLong(REGISTER_COUNT + 1))
        );
    }

    /// An image of all 32 registers read back as `fill` (R0 as the chip ID).
    fn read_all(fill: u8) -> Registers {
        let mut wire = [fill.reverse_bits(); REGISTER_COUNT];
        wire[0] = CHIP_ID.reverse_bits();
        Registers::from_read(&wire).unwrap()
    }

    #[test]
    fn a_page_write_starts_with_the_first_register_and_follows_in_order() {
        // R820-DS-03: Figure 1-1.
        let mut registers = read_all(0);
        registers.set(field::LNA_GAIN, 0xA).unwrap();
        registers.set(field::MIX_GAIN, 0x5).unwrap();
        let write = registers.page_write(0x05, 3).unwrap();
        assert_eq!(write.as_bytes(), &[0x05, 0x0A, 0x00, 0x05]);
    }

    #[test]
    fn page_writes_refuse_the_read_only_registers_and_the_end_of_the_file() {
        let registers = read_all(0);
        assert!(registers.page_write(0x04, 1).is_err(), "R4 is read-only");
        assert!(registers.page_write(0x05, 0).is_err(), "nothing to write");
        assert!(registers.page_write(0x1F, 2).is_err(), "R32 does not exist");
        assert_eq!(registers.page_write(0x05, 27).unwrap().as_bytes().len(), 28);
    }

    #[test]
    fn page_writes_refuse_registers_never_read_rather_than_guess_their_bits() {
        assert_eq!(
            Registers::default().page_write(0x05, 1),
            Err(RegisterError::NotRead(0x05))
        );
        let partial = Registers::from_read(&[0; 8]).unwrap();
        assert!(partial.page_write(0x05, 3).is_ok(), "R5-R7 were read");
        assert_eq!(
            partial.page_write(0x06, 3),
            Err(RegisterError::NotRead(0x08))
        );
        let all = read_all(0xA5);
        assert_eq!(all.page_write(0x1F, 1).unwrap().as_bytes(), &[0x1F, 0xA5]);
    }

    #[test]
    fn setting_a_field_keeps_the_registers_other_bits() {
        let mut registers = Registers::default();
        registers.set(field::PWD_LT, 1).unwrap();
        registers.set(field::LNA_GAIN, 0xF).unwrap();
        registers.set(field::LNA_GAIN, 0x3).unwrap();
        assert_eq!(registers.byte(0x05), Some(0b1000_0011));
        assert_eq!(
            registers.set(field::LNA_GAIN, 0x10),
            Err(RegisterError::TooWide {
                field: field::LNA_GAIN,
                value: 0x10
            })
        );
    }
}
