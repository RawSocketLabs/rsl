//! Test fixtures the bitstream modules' unit tests share.

use crate::field::{Bits, ByteOrder};

/// A fixed xorshift stream, so the differential tests see the same bytes every run.
pub(super) fn noise(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// A 24-bit value that declares its own byte order, as a `#[bitfield(bytes = …)]` does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Own<const LITTLE: bool>(pub(super) u32);

impl<const LITTLE: bool> Bits for Own<LITTLE> {
    const BITS: u32 = 24;
    const __BNB_BYTE_ORDER: Option<ByteOrder> = Some(if LITTLE {
        ByteOrder::Little
    } else {
        ByteOrder::Big
    });

    fn into_bits(self) -> u128 {
        u128::from(self.0)
    }

    fn from_bits(raw: u128) -> Self {
        Self(u32::try_from(raw).expect("a 24-bit value"))
    }
}

/// A value of any width that keeps no bits: zero-width, or wider than a read returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Blank<const WIDTH: u32>;

impl<const WIDTH: u32> Bits for Blank<WIDTH> {
    const BITS: u32 = WIDTH;

    fn into_bits(self) -> u128 {
        0
    }

    fn from_bits(_: u128) -> Self {
        Self
    }
}
