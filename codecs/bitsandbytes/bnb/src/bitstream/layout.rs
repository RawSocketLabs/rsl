//! The wire [`Layout`] and the bit-packing primitives the cursors share.

use alloc::vec::Vec;

#[cfg(doc)]
use super::{BitReader, BitWriter, Source};
#[cfg(all(doc, feature = "std"))]
use super::{BufSeekReader, BufSource, SeekReader};
use crate::field::{BitOrder, Bits, ByteOrder};

/// The wire layout: bit packing order **and** byte order, threaded through the
/// cursors and entry points. `#[bin(big|little)]` and `#[bin(bits = msb|lsb)]`
/// set it; the default is MSB-first, big-endian (RFC/network order).
///
/// # Examples
///
/// ```
/// use bnb::{BitReader, BitOrder, ByteOrder, Layout};
///
/// // Read a 16-bit value little-endian instead of the default big-endian.
/// let layout = Layout { bit: BitOrder::Msb, byte: ByteOrder::Little };
/// let mut r = BitReader::with_layout(&[0x34, 0x12], layout);
/// assert_eq!(r.read::<u16>().unwrap(), 0x1234);
/// assert_eq!(Layout::default(), Layout { bit: BitOrder::Msb, byte: ByteOrder::Big });
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// Bit packing order — does the first bit land in the high or low bit.
    pub bit: BitOrder,
    /// Byte order, applied to byte-multiple values.
    pub byte: ByteOrder,
}

/// `raw` (a `T` value's bits) reordered for the wire: `T`'s own declared byte order if it has
/// one, else the cursor's `byte`. Its own inverse, so read and write share it.
#[inline]
pub(super) fn value_byte_order<T: Bits>(raw: u128, bit: BitOrder, byte: ByteOrder) -> u128 {
    apply_byte_order(raw, T::BITS, bit, T::__BNB_BYTE_ORDER.unwrap_or(byte))
}

/// `len` bytes as a bit count. Where `usize` cannot count that many bits (a 32-bit target with
/// 512 MiB or more) it saturates at the last whole byte that fits, so such a buffer reads as
/// ending there: reads past it fail and seeks are refused, never a wrapped short length.
#[inline]
pub(super) const fn bits_in(len: usize) -> usize {
    match len.checked_mul(8) {
        Some(bits) => bits,
        None => usize::MAX & !7,
    }
}

/// `bits_in(len) / 8`: how many of a buffer's `len` bytes a reader may read, spelled as a
/// clamp so a bounds check against it folds into one compare.
#[inline]
pub(super) const fn bytes_in(len: usize) -> usize {
    if len < usize::MAX / 8 {
        len
    } else {
        usize::MAX / 8
    }
}

/// Reverses the low `bits / 8` bytes of `raw` when the declared byte order differs from the
/// bit order's **natural** layout, and the width is a whole number of bytes (byte order
/// applies only to byte-multiple values); a no-op otherwise. It is its own inverse, so read
/// and write share it.
///
/// The natural layout is what the bit cursor produces with no transform: MSB-first emits a
/// value's high bits first, so its bytes land **big-endian**; LSB-first emits low bits first,
/// so its bytes land **little-endian** (bit *k* of the value goes to stream bit *k* — exactly
/// the DBC/"Intel" layout `raw |= v << start; frame = raw.to_le_bytes()`). So the transform
/// swaps only for `Msb`+`Little` and `Lsb`+`Big`; `Msb`+`Big` (network order) and
/// `Lsb`+`Little` (DBC Intel, SMB) are identities. See `DESIGN.md` § byte order × bit order.
#[inline]
fn apply_byte_order(raw: u128, bits: u32, bit: BitOrder, byte: ByteOrder) -> u128 {
    let natural = match bit {
        BitOrder::Msb => ByteOrder::Big,
        BitOrder::Lsb => ByteOrder::Little,
    };
    if byte == natural || bits % 8 != 0 || bits > 128 {
        return raw;
    }
    let n = (bits / 8) as usize;
    let le = raw.to_le_bytes();
    let mut out = 0u128;
    let mut i = 0;
    while i < n {
        out |= u128::from(le[i]) << (8 * (n - 1 - i));
        i += 1;
    }
    out
}

/// Extracts `n` (`<= 128`) bits starting at absolute bit offset `pos` from `buf`, in
/// `order`, returned right-aligned in a `u128` (byte order is applied separately by
/// `read`). The bit-extraction routine behind every slice-backed [`Source`]
/// ([`BitReader`], [`BufSource`], [`SeekReader`], [`BufSeekReader`]); only a typed read of
/// whole bytes from a byte-aligned [`BitReader`] goes around it
/// (`BitReader::take_whole_bytes`). The caller must have bounds-checked
/// `pos + n <= buf.len() * 8` and `n <= 128`.
///
/// Work is per touched byte, never per bit. A byte-aligned read (`pos % 8 == 0` and
/// `n % 8 == 0`) concatenates its bytes directly, a field of at most eight bits is taken
/// from the one or two bytes it touches, and any other read goes through
/// [`extract_unaligned_bits`].
#[inline]
pub(super) fn extract_bits(buf: &[u8], pos: usize, n: usize, order: BitOrder) -> u128 {
    if pos % 8 == 0 && n % 8 == 0 {
        let start = pos / 8;
        let nbytes = n / 8;
        let mut acc = 0u128;
        match order {
            // MSB-first byte-aligned == big-endian byte concatenation.
            BitOrder::Msb => {
                for j in 0..nbytes {
                    acc = (acc << 8) | u128::from(buf[start + j]);
                }
            }
            // LSB-first byte-aligned == little-endian byte concatenation.
            BitOrder::Lsb => {
                for j in 0..nbytes {
                    acc |= u128::from(buf[start + j]) << (8 * j);
                }
            }
        }
        return acc;
    }
    // A field of at most eight bits touches one or two bytes: cheap enough to keep inline,
    // where a generated decoder's constant width folds it to a load, a shift and a mask.
    if n != 0 && n <= 8 {
        let lead = pos % 8;
        let end = lead + n;
        let first = u16::from(buf[pos / 8]);
        let second = if end > 8 {
            u16::from(buf[pos / 8 + 1])
        } else {
            0
        };
        let field = match order {
            BitOrder::Msb => ((first << 8) | second) >> (16 - end),
            BitOrder::Lsb => ((second << 8) | first) >> lead,
        };
        return u128::from(field & (0xFF >> (8 - n)));
    }
    extract_unaligned_bits(buf, pos, n, order)
}

/// The general case of [`extract_bits`]: a read that starts or ends inside a byte. It
/// gathers the bytes the field touches into one window and shifts the field out of it.
///
/// Kept out of line so the byte-aligned path its callers inline stays small: with this
/// folded in, byte-aligned decode measured 8% slower (DESIGN §19).
#[inline(never)]
fn extract_unaligned_bits(buf: &[u8], pos: usize, n: usize, order: BitOrder) -> u128 {
    if n == 0 {
        return 0;
    }
    // The field starts `lead` bits into its first byte.
    let lead = pos % 8;
    // With sixteen bytes at hand, load the window whole, whatever the field's width: the
    // fold below costs about ten instructions per byte (DESIGN.md §22, row 3).
    if let Some(window) = buf.get(pos / 8..).and_then(<[u8]>::first_chunk::<16>) {
        // A 128-bit field from a non-zero offset touches 17 bytes, one more than the
        // window holds: the 17th is merged in separately.
        let spills = lead + n > 128;
        return match order {
            BitOrder::Msb => {
                // The field's first bit to the top; `lead` vacated bits at the bottom.
                let mut top = u128::from_be_bytes(*window) << lead;
                if spills {
                    top |= u128::from(buf[pos / 8 + 16] >> (8 - lead));
                }
                top >> (128 - n)
            }
            BitOrder::Lsb => {
                // The field's first bit to the bottom; `lead` vacated bits at the top.
                let mut low = u128::from_le_bytes(*window) >> lead;
                if spills {
                    low |= u128::from(buf[pos / 8 + 16]) << (128 - lead);
                }
                low & (u128::MAX >> (128 - n))
            }
        };
    }
    // Within a buffer's last fifteen bytes the field touches at most fifteen, so they fit
    // one window, folded in a byte at a time.
    let span = &buf[pos / 8..(pos + n).div_ceil(8)];
    // Up to eight bytes fold in a `u64`, at a third of the `u128` fold's cost per byte.
    if span.len() <= 8 {
        let mask = u64::MAX >> (64 - n);
        let field = match order {
            BitOrder::Msb => {
                let window = span
                    .iter()
                    .fold(0u64, |acc, &byte| (acc << 8) | u64::from(byte));
                window >> (span.len() * 8 - lead - n)
            }
            BitOrder::Lsb => {
                let window = span
                    .iter()
                    .rev()
                    .fold(0u64, |acc, &byte| (acc << 8) | u64::from(byte));
                window >> lead
            }
        };
        return u128::from(field & mask);
    }
    let mask = u128::MAX >> (128 - n);
    match order {
        // The window is the bytes as one big-endian number: the field's first bit is the
        // highest, `lead` bits below the window's top.
        BitOrder::Msb => {
            let window = span
                .iter()
                .fold(0u128, |acc, &byte| (acc << 8) | u128::from(byte));
            (window >> (span.len() * 8 - lead - n)) & mask
        }
        // The window is the bytes as one little-endian number: the field's first bit is
        // `lead` bits above the window's bottom.
        BitOrder::Lsb => {
            let window = span
                .iter()
                .rev()
                .fold(0u128, |acc, &byte| (acc << 8) | u128::from(byte));
            (window >> lead) & mask
        }
    }
}

/// The byte buffer [`emit_bits`] appends to: a writer's own `Vec`, or the tail of a
/// buffer a message is encoded into in place. Positions count from the first byte the
/// message wrote.
pub(super) trait Appendable {
    /// Bytes written so far.
    fn len(&self) -> usize;
    /// Appends one byte.
    fn push(&mut self, byte: u8);
    /// ORs `bits` into the byte at `index`, which is already there.
    fn or_into(&mut self, index: usize, bits: u8);
}

impl Appendable for Vec<u8> {
    #[inline]
    fn len(&self) -> usize {
        self.len()
    }
    #[inline]
    fn push(&mut self, byte: u8) {
        self.push(byte);
    }
    #[inline]
    fn or_into(&mut self, index: usize, bits: u8) {
        self[index] |= bits;
    }
}

/// Appends the low `n` (`<= 128`) bits of `value` to `out` at absolute bit offset
/// `bit_pos`, in `order` — the write dual of [`extract_bits`], used by [`BitWriter`] and
/// by the in-place frame writer; only a typed write of whole bytes at a byte-aligned
/// cursor goes around it (`append_whole_bytes`). The cursor must not be past the end of
/// `out` (`bit_pos / 8 <= out.len()`).
///
/// Work is per touched byte, never per bit. Appending byte-aligned at the end
/// (`bit_pos % 8 == 0`, `n % 8 == 0`, cursor at `out.len()`) pushes the bytes whole; any
/// other write goes through [`emit_unaligned_bits`].
#[inline]
#[allow(clippy::cast_possible_truncation)] // Extract one low byte of the shifted carrier at a time.
pub(super) fn emit_bits<B: Appendable>(
    out: &mut B,
    bit_pos: usize,
    value: u128,
    n: usize,
    order: BitOrder,
) {
    if n % 8 == 0 && bit_pos % 8 == 0 && bit_pos / 8 == out.len() {
        let nbytes = n / 8;
        match order {
            BitOrder::Msb => {
                for j in 0..nbytes {
                    out.push((value >> (8 * (nbytes - 1 - j))) as u8);
                }
            }
            BitOrder::Lsb => {
                for j in 0..nbytes {
                    out.push((value >> (8 * j)) as u8);
                }
            }
        }
        return;
    }
    emit_unaligned_bits(out, bit_pos, value, n, order);
}

/// The general case of [`emit_bits`]: a write that starts or ends inside a byte, or lands
/// on bytes already present. It ORs into each touched byte the run of bits that falls
/// there. Out of line for the same reason as [`extract_unaligned_bits`].
#[inline(never)]
#[allow(clippy::cast_possible_truncation)] // Each chunk is masked to at most eight bits.
fn emit_unaligned_bits<B: Appendable>(
    out: &mut B,
    bit_pos: usize,
    value: u128,
    n: usize,
    order: BitOrder,
) {
    // `take` bits go into each touched byte, starting `lead` bits in.
    let mut pos = bit_pos;
    let mut remaining = n;
    while remaining > 0 {
        let lead = pos % 8;
        let take = (8 - lead).min(remaining);
        let byte_idx = pos / 8;
        if byte_idx == out.len() {
            out.push(0);
        }
        let low_bits = (1u128 << take) - 1;
        let bits = match order {
            // MSB-first emits the field's high bits first, into the byte's high end.
            BitOrder::Msb => {
                (((value >> (remaining - take)) & low_bits) as u8) << (8 - lead - take)
            }
            // LSB-first emits the field's low bits first, into the byte's low end.
            BitOrder::Lsb => (((value >> (n - remaining)) & low_bits) as u8) << lead,
        };
        out.or_into(byte_idx, bits);
        pos += take;
        remaining -= take;
    }
}

#[cfg(test)]
mod unit {
    use crate::bitstream::fixtures::noise;

    use super::*;

    /// The original one-bit-per-iteration extraction: slow, and simple enough to be the
    /// reference the byte-wise [`extract_bits`] is checked against.
    fn extract_bits_bitwise(buf: &[u8], pos: usize, n: usize, order: BitOrder) -> u128 {
        (0..n).fold(0u128, |acc, k| {
            let p = pos + k;
            match order {
                BitOrder::Msb => (acc << 1) | u128::from((buf[p / 8] >> (7 - p % 8)) & 1),
                BitOrder::Lsb => acc | (u128::from((buf[p / 8] >> (p % 8)) & 1) << k),
            }
        })
    }

    /// The original one-bit-per-iteration emission, the reference for [`emit_bits`].
    fn emit_bits_bitwise(
        out: &mut Vec<u8>,
        bit_pos: usize,
        value: u128,
        n: usize,
        order: BitOrder,
    ) {
        for k in 0..n {
            let p = bit_pos + k;
            let (bit, shift) = match order {
                BitOrder::Msb => (n - 1 - k, 7 - p % 8),
                BitOrder::Lsb => (k, p % 8),
            };
            if p / 8 == out.len() {
                out.push(0);
            }
            if (value >> bit) & 1 != 0 {
                out[p / 8] |= 1 << shift;
            }
        }
    }

    #[test]
    fn extract_bits_matches_the_bitwise_reference_at_every_offset_and_width() {
        let mut state = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..8 {
            // 19 bytes: a 128-bit field starting up to 23 bits in.
            let noisy: Vec<u8> = (0..19)
                .map(|_| noise(&mut state).to_le_bytes()[0])
                .collect();
            for order in [BitOrder::Msb, BitOrder::Lsb] {
                for pos in 0..24 {
                    for n in 0..=128 {
                        // Exactly as long as the read needs, so one that reaches for a
                        // byte past its last touched one is out of bounds.
                        let buf = &noisy[..usize::div_ceil(pos + n, 8)];
                        let reference = extract_bits_bitwise(buf, pos, n, order);
                        assert_eq!(
                            extract_bits(buf, pos, n, order),
                            reference,
                            "{order:?} pos {pos} n {n}"
                        );
                        // And with bytes after the field, where a narrow read can load a
                        // whole window: the bytes it does not want must not leak in.
                        assert_eq!(
                            extract_bits(&noisy, pos, n, order),
                            reference,
                            "{order:?} pos {pos} n {n}, with a tail"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn emit_bits_matches_the_bitwise_reference_at_every_offset_and_width() {
        let mut state = 0xD1B5_4A32_D192_ED03;
        for _ in 0..8 {
            let value = u128::from(noise(&mut state)) << 64 | u128::from(noise(&mut state));
            for order in [BitOrder::Msb, BitOrder::Lsb] {
                for bit_pos in 0..24 {
                    for n in 0..=128 {
                        // Appending: the buffer ends in the cursor's byte, whose unwritten
                        // bits are zero. Overwriting: the bytes exist and hold noise, which
                        // a write ORs into and must otherwise leave alone.
                        let mut appended = vec![0xFF; bit_pos / 8];
                        if bit_pos % 8 != 0 {
                            appended.push(match order {
                                BitOrder::Msb => 0xFF << (8 - bit_pos % 8),
                                BitOrder::Lsb => 0xFF >> (8 - bit_pos % 8),
                            });
                        }
                        let mut overwritten = appended.clone();
                        while overwritten.len() < 20 {
                            overwritten.push(noise(&mut state).to_le_bytes()[0]);
                        }
                        for start in [appended, overwritten] {
                            let (mut fast, mut reference) = (start.clone(), start);
                            emit_bits(&mut fast, bit_pos, value, n, order);
                            emit_bits_bitwise(&mut reference, bit_pos, value, n, order);
                            assert_eq!(fast, reference, "{order:?} bit_pos {bit_pos} n {n}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bits_in_saturates_at_the_last_whole_byte_instead_of_wrapping() {
        assert_eq!(bits_in(0), 0);
        assert_eq!(bits_in(3), 24);
        assert_eq!(bits_in(usize::MAX / 8), usize::MAX / 8 * 8);
        assert_eq!(bits_in(usize::MAX / 8 + 1), usize::MAX & !7);
        assert_eq!(bits_in(usize::MAX), usize::MAX & !7);
    }

    #[test]
    fn bytes_in_is_bits_in_counted_in_bytes() {
        let edge = usize::MAX / 8;
        for len in [0, 1, 3, edge - 1, edge, edge + 1, usize::MAX] {
            assert_eq!(bytes_in(len), bits_in(len) / 8, "{len}");
        }
    }
}
