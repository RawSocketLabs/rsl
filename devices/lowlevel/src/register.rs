//! Typed registers: a register's value type names its address.
//!
//! A chip declares a `Reg` enum for every address it touches. Registers whose contents the
//! driver interprets get a value type (a `bnb` bitfield or `BitEnum`) that implements
//! [`Register`] or, when the chip repeats it per channel or output, [`IndexedRegister`].
//! Writing a value then needs no address argument, and the wrong value for a register does
//! not compile. Registers written as opaque bytes stay `(Reg, u8)`.

/// An 8-bit register at one fixed address.
///
/// The byte conversions are `From`: `bnb` generates them for a bitfield and for a
/// whole-byte `BitEnum` with a catch-all.
pub trait Register: Copy + From<u8> + Into<u8> {
    /// The chip's register-address enum.
    type Map: Into<u8>;
    /// Where this register lives.
    const ADDR: Self::Map;

    /// This value as an `(address, byte)` write.
    fn entry(self) -> (Self::Map, u8) {
        (Self::ADDR, self.into())
    }
}

/// An 8-bit register the chip repeats; `Index` (a channel or output) selects the copy.
pub trait IndexedRegister: Copy + Into<u8> {
    /// The chip's register-address enum.
    type Map: Into<u8>;
    /// What selects one copy of the register.
    type Index: Copy;
    /// Where the copy for `index` lives.
    fn addr(index: Self::Index) -> Self::Map;

    /// This value, for the copy at `index`, as an `(address, byte)` write.
    fn entry_at(self, index: Self::Index) -> (Self::Map, u8) {
        (Self::addr(index), self.into())
    }
}
