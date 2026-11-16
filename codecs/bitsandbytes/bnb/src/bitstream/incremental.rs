//! The incremental decode attempt over a prefix of buffered input.

use alloc::vec::Vec;

use crate::field::{BitOrder, Bits, ByteOrder};

use super::layout::bits_in;
use super::source::{read_by_bits, read_into_per_byte, skip_bytes_in_steps};
use super::{BitError, BitReader, DecodeWith, ErrorKind, Layout, SeekSource, Source, sealed};

// Only this backing reader turns physical exhaustion into a retry signal. In particular,
// an enclosing LimitedSource's logical boundary errors must remain definitive.
pub(super) struct IncrementalReader<'a>(pub(super) BitReader<'a>);

impl sealed::Sealed for IncrementalReader<'_> {}
impl SeekSource for IncrementalReader<'_> {}

impl IncrementalReader<'_> {
    /// The retry signal for a read that is `missing` bits short of the buffered input:
    /// what [`shortfall`](Self::shortfall) makes of the reader's end-of-input error.
    #[cold]
    #[inline(never)]
    fn incomplete(&self, missing: usize) -> BitError {
        BitError::new(
            ErrorKind::Incomplete {
                needed: Some(missing.div_ceil(8)),
            },
            self.0.bit_pos(),
        )
    }

    /// The retry signal for a byte run that is `missing` whole bytes short.
    #[cold]
    #[inline(never)]
    fn incomplete_bytes(&self, missing: usize) -> BitError {
        BitError::new(
            ErrorKind::Incomplete {
                needed: Some(missing),
            },
            self.0.bit_pos(),
        )
    }

    /// [`Source::read_into`] for a run that is not lent whole: from between bytes, or
    /// short. The inner reader's per-byte loop fails exactly where this reader's would,
    /// so mapping that one error afterwards equals mapping each byte's.
    #[inline(never)]
    fn read_into_per_byte(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        read_into_per_byte(&mut self.0, buf).map_err(Self::shortfall)
    }

    /// [`Source::read_bytes`] from between bytes, for a run known to be all here.
    #[inline(never)]
    fn read_bytes_unaligned(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        let mut bytes = Vec::with_capacity(n);
        for _ in 0..n {
            bytes.push(self.read::<u8>()?);
        }
        Ok(bytes)
    }

    fn shortfall(mut error: BitError) -> BitError {
        if let ErrorKind::UnexpectedEof { needed, remaining } = error.kind {
            error.kind = ErrorKind::Incomplete {
                needed: Some(needed.saturating_sub(remaining).div_ceil(8)),
            };
        }
        error
    }
}

impl Source for IncrementalReader<'_> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        self.0.__bnb_depth()
    }
    // Inline, so a generated decoder's constant width reaches the reader's fast paths. The
    // shortfall is built directly rather than converted from the reader's end-of-input
    // error: one out-of-line construction instead of a build, a move and a rewrite.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let remaining = self.0.remaining_bits();
        if n <= 128 && n as usize > remaining {
            return Err(self.incomplete(n as usize - remaining));
        }
        self.0.read_bits(n)
    }

    // As `BitReader::read`, with a short read reported as `read_bits` reports it.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        if !self.0.at_whole_bytes::<T>() {
            return read_by_bits(self);
        }
        let Some(value) = self.0.take_whole_bytes() else {
            return Err(self.incomplete(T::BITS as usize - self.0.remaining_bits()));
        };
        Ok(value)
    }

    fn bit_pos(&self) -> usize {
        self.0.bit_pos()
    }

    fn byte_order(&self) -> ByteOrder {
        self.0.byte
    }

    fn bit_order(&self) -> BitOrder {
        self.0.order
    }

    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.0.seek_to_bit(pos).map_err(Self::shortfall)
    }

    // Inline down to the copy, so the `Vec` reaches the caller in registers. Returned from a
    // call it is written in three narrow stores, and a decoder that is itself out of line
    // reloads it in one wide load: a store-forwarding stall per blob, measured in DESIGN
    // §22, phase 2. The shortage is tested first and its error built out of line.
    #[inline]
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        let available = self.0.remaining_bits() / 8;
        if n > available {
            return Err(self.incomplete_bytes(n - available));
        }
        // Only reserve after proving the full byte payload is physically available.
        // Retrying a fragmented length-prefixed blob therefore does not repeatedly
        // allocate/copy its growing prefix. Unaligned reads retain their bit-order semantics.
        match self.0.aligned_run(n) {
            Some(run) => Ok(run.to_vec()),
            None => self.read_bytes_unaligned(n),
        }
    }

    // Inline, like `BitReader`'s: an aligned run that is all here is a plain copy, and for
    // a `[u8; N]` field one of constant size. Anything else goes out of line.
    #[inline]
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        match self.0.aligned_run(buf.len()) {
            Some(run) => {
                buf.copy_from_slice(run);
                Ok(())
            }
            None => self.read_into_per_byte(buf),
        }
    }

    // As `read_bytes`: the whole shortfall in one signal, so a transport reads what a
    // skipped body still lacks at once instead of attempting again after every read.
    fn skip_bytes(&mut self, n: usize) -> Result<(), BitError> {
        let available = self.0.remaining_bits() / 8;
        if n > available {
            return Err(self.incomplete_bytes(n - available));
        }
        skip_bytes_in_steps(self, n)
    }

    // A run that is all here needs no retry signal; a short one is `None`, read per element.
    #[inline]
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        self.0.aligned_run(n)
    }
}

// The attempt is transactional with respect to its caller's cursor, not codec side effects.
// Keep BytesMut framing on this slice path too: no second accumulation buffer or copy.
pub(crate) fn decode_attempt<T: DecodeWith<A>, A>(
    bytes: &[u8],
    start: usize,
    layout: Layout,
    args: A,
    eof: bool,
) -> Result<(T, usize), BitError> {
    // Check the start once, then let each branch build its own reader in place, already
    // positioned. Building one first and moving it into `IncrementalReader` reloads, in two
    // wide loads, what was just written in narrow stores: two store-forwarding stalls per
    // attempt (DESIGN.md §22, row 2).
    let available = bits_in(bytes.len());
    if start > available {
        // What `BitReader::seek_to_bit` reports from bit 0.
        return Err(BitError::new(
            ErrorKind::UnexpectedEof {
                needed: start,
                remaining: available,
            },
            0,
        ));
    }
    let reader = || BitReader {
        bytes,
        bit_pos: start,
        order: layout.bit,
        byte: layout.byte,
        depth: 0,
    };
    let (value, end) = if eof {
        let mut reader = reader();
        let value = T::decode_with(&mut reader, args).map_err(|mut error| {
            if let ErrorKind::Incomplete { needed } = error.kind {
                // The kind replaced owns nothing: let it go without the drop code.
                core::mem::forget(core::mem::replace(
                    &mut error.kind,
                    ErrorKind::IncompleteAtEof { needed },
                ));
            }
            error
        })?;
        (value, reader.bit_pos())
    } else {
        let mut reader = IncrementalReader(reader());
        let value = T::decode_with(&mut reader, args)?;
        (value, reader.bit_pos())
    };
    if end <= start {
        Err(BitError::new(ErrorKind::NoProgress, start))
    } else {
        Ok((value, end))
    }
}

#[cfg(test)]
mod unit {
    use crate::bitstream::source::read_bytes_per_byte;

    use super::*;

    #[test]
    fn the_incremental_reader_reads_a_byte_run_as_the_per_byte_loop_does_or_waits() {
        let wire = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        for order in [BitOrder::Msb, BitOrder::Lsb] {
            for pos in 0..=wire.len() * 8 {
                let at = || {
                    let mut reader = BitReader::with_order(&wire, order);
                    reader.bit_pos = pos;
                    IncrementalReader(reader)
                };
                let available = (wire.len() * 8 - pos) / 8;
                for n in (0..=7).chain([usize::MAX]) {
                    let mut fast = at();
                    let got = fast.read_bytes(n).map_err(|e| (e.kind, e.at));
                    if n > available {
                        // A wait for exactly the missing bytes, with nothing consumed and
                        // nothing allocated from `n`.
                        let needed = Some(n - available);
                        assert_eq!(got, Err((ErrorKind::Incomplete { needed }, pos)));
                        assert_eq!(fast.bit_pos(), pos);
                    } else {
                        let mut slow = at();
                        let expected =
                            read_bytes_per_byte(&mut slow, n).map_err(|e| (e.kind, e.at));
                        assert_eq!(got, expected, "{order:?} pos {pos} n {n}");
                        assert_eq!(fast.bit_pos(), slow.bit_pos());
                    }
                }
            }
        }
    }

    #[test]
    fn the_incremental_reader_fills_a_buffer_as_the_per_byte_loop_does() {
        let wire = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
        for order in [BitOrder::Msb, BitOrder::Lsb] {
            for pos in 0..=wire.len() * 8 {
                let at = || {
                    let mut reader = BitReader::with_order(&wire, order);
                    reader.bit_pos = pos;
                    IncrementalReader(reader)
                };
                // Runs that are all there, from a byte boundary and from inside a byte, and
                // runs that end past the input: the wait, where it is raised, and the bytes
                // already written when it is.
                for n in 0..=7 {
                    let (mut fast, mut slow) = (at(), at());
                    let (mut got, mut expected) = ([0xEE; 7], [0xEE; 7]);
                    let result = fast.read_into(&mut got[..n]).map_err(|e| (e.kind, e.at));
                    let reference = read_into_per_byte(&mut slow, &mut expected[..n])
                        .map_err(|e| (e.kind, e.at));
                    let context = format!("{order:?} pos {pos} n {n}");
                    assert_eq!((result, got), (reference, expected), "{context}");
                    assert_eq!(fast.bit_pos(), slow.bit_pos(), "{context}");
                }
            }
        }
    }
}
