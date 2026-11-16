//! [`LimitedSource`], a length-bounded window over a parent source.

use alloc::vec::Vec;
use core::fmt;

use crate::field::{BitOrder, Bits, ByteOrder};

use super::source::{read_bytes_per_byte, read_into_per_byte, skip_bytes_in_steps};
use super::{BitError, ErrorKind, SeekSource, Source, sealed};

/// A borrowed [`Source`] window that prevents a nested decoder from crossing its declared length.
///
/// Positions remain absolute offsets in the parent source, so errors retain their original wire
/// location. Dropping a window permits unconsumed input; call [`finish`](Self::finish) when the
/// nested grammar requires exact consumption.
pub struct LimitedSource<'a, S: Source> {
    source: &'a mut S,
    start: usize,
    end: usize,
}

impl<'a, S: Source> LimitedSource<'a, S> {
    pub(super) fn bytes(source: &'a mut S, n: usize) -> Result<Self, BitError> {
        let start = source.bit_pos();
        if start % 8 != 0 {
            return Err(BitError::new(ErrorKind::NotByteAligned, start));
        }
        let bits = n
            .checked_mul(8)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, start))?;
        let end = start
            .checked_add(bits)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, start))?;
        Ok(Self { source, start, end })
    }

    /// Absolute bit offset at which this window begins.
    #[must_use]
    pub const fn start_bit(&self) -> usize {
        self.start
    }

    /// Absolute bit offset immediately after this window.
    #[must_use]
    pub const fn end_bit(&self) -> usize {
        self.end
    }

    /// Bits not yet consumed inside this window.
    #[must_use]
    pub fn remaining_bits(&self) -> usize {
        self.end.saturating_sub(self.source.bit_pos())
    }

    /// Ends the borrow only if the child consumed its complete declared region.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::TrailingBits`] when bytes or bits remain in the window.
    pub fn finish(self) -> Result<(), BitError> {
        let remaining = self.remaining_bits();
        if remaining == 0 {
            Ok(())
        } else {
            Err(BitError::new(
                ErrorKind::TrailingBits { remaining },
                self.source.bit_pos(),
            ))
        }
    }

    /// The error for a read of `needed` bits with `remaining` left in the window.
    #[cold]
    #[inline(never)]
    fn past_end(&self, needed: usize, remaining: usize) -> BitError {
        BitError::new(
            ErrorKind::UnexpectedEof { needed, remaining },
            self.source.bit_pos(),
        )
    }
}

impl<S: Source> fmt::Debug for LimitedSource<'_, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LimitedSource")
            .field("start", &self.start)
            .field("end", &self.end)
            .field("position", &self.source.bit_pos())
            .finish()
    }
}

impl<S: Source> sealed::Sealed for LimitedSource<'_, S> {}

impl<S: Source> Source for LimitedSource<'_, S> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        self.source.__bnb_depth()
    }
    // Inline, so a generated decoder's constant width reaches the wrapped reader's fast
    // paths through the window check; the boundary error is built out of line.
    #[inline]
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let remaining = self.remaining_bits();
        if n as usize > remaining {
            return Err(self.past_end(n as usize, remaining));
        }
        self.source.read_bits(n)
    }

    // The window check of `read_bits`, then the wrapped reader's own typed read, so a
    // slice reader's whole-byte load is reached through the window. Forced inline as that
    // read is: left to the optimizer it stayed a call per field (DESIGN §22).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        let remaining = self.remaining_bits();
        if T::BITS as usize > remaining {
            return Err(self.past_end(T::BITS as usize, remaining));
        }
        self.source.read()
    }

    fn bit_pos(&self) -> usize {
        self.source.bit_pos()
    }

    fn byte_order(&self) -> ByteOrder {
        self.source.byte_order()
    }

    fn bit_order(&self) -> BitOrder {
        self.source.bit_order()
    }

    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        if pos < self.start || pos > self.end {
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: pos.saturating_sub(self.start),
                    remaining: self.end - self.start,
                },
                self.source.bit_pos(),
            ));
        }
        self.source.seek_to_bit(pos)
    }

    // A skip the window has room for is the wrapped source's own: over a decode attempt
    // that asks for the whole shortfall at once, where the loop through this window asks
    // for a byte, and an attempt after every read is work in the square of a large body.
    // One that would leave the window runs through the window, which stops it at the edge.
    fn skip_bytes(&mut self, n: usize) -> Result<(), BitError> {
        if n > self.remaining_bits() / 8 {
            skip_bytes_in_steps(self, n)
        } else {
            self.source.skip_bytes(n)
        }
    }

    // As `BitReader`: a run the window lends is copied whole, and any other runs the
    // per-byte loop through this window, so errors, positions and hints are that loop's.
    // Not the wrapped source's `read_bytes`: over the incremental reader that reports the
    // whole shortfall, where the loop reports the first missing byte.
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        match self.aligned_run(n) {
            Some(run) => Ok(run.to_vec()),
            None => read_bytes_per_byte(self, n),
        }
    }

    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        match self.aligned_run(buf.len()) {
            Some(run) => {
                buf.copy_from_slice(run);
                Ok(())
            }
            None => read_into_per_byte(self, buf),
        }
    }

    // The wrapped source's run when it lies inside the window; a run that would cross the
    // window's end is `None`, so the element-by-element path reports the boundary.
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        if n > self.remaining_bits() / 8 {
            return None;
        }
        self.source.aligned_run(n)
    }
}

impl<S: SeekSource> SeekSource for LimitedSource<'_, S> {}

#[cfg(test)]
mod unit {
    use crate::bitstream::fixtures::{Blank, Own, noise};
    use crate::bitstream::incremental::IncrementalReader;
    use crate::bitstream::{BitReader, Layout};

    use super::*;

    /// A window as it was before it lent runs and forwarded typed reads: `read_bits` checks
    /// the boundary, seeks stay inside it, and every other read is the trait's default.
    struct PlainWindow<'a, S: Source>(LimitedSource<'a, S>);

    impl<S: Source> sealed::Sealed for PlainWindow<'_, S> {}

    impl<S: Source> Source for PlainWindow<'_, S> {
        fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
            self.0.read_bits(n)
        }
        fn __bnb_depth(&mut self) -> &mut u16 {
            self.0.__bnb_depth()
        }
        fn bit_pos(&self) -> usize {
            self.0.bit_pos()
        }
        fn byte_order(&self) -> ByteOrder {
            self.0.byte_order()
        }
        fn bit_order(&self) -> BitOrder {
            self.0.bit_order()
        }
        fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
            self.0.seek_to_bit(pos)
        }
    }

    /// Every read a window serves, at `pos` in a window of `window` bytes from byte 1 of
    /// `source`, against [`PlainWindow`]: the value or the error and its position, the
    /// cursor afterwards, and for `read_into` the bytes written before a failure.
    fn assert_window_matches_the_plain_one<S: Source>(
        make: impl Fn() -> S,
        window: usize,
        pos: usize,
        context: &str,
    ) {
        use crate::{u4, u24};
        fn flat<T>(result: Result<T, BitError>) -> Result<T, (ErrorKind, usize)> {
            result.map_err(|e| (e.kind, e.at))
        }
        // `$fast` and `$plain`: the two windows over fresh sources, both at `pos`.
        macro_rules! windows {
            ($fast:ident, $plain:ident) => {
                let (mut fast_source, mut plain_source) = (make(), make());
                for source in [&mut fast_source, &mut plain_source] {
                    source.seek_to_bit(8).unwrap();
                }
                let mut $fast = LimitedSource::bytes(&mut fast_source, window).unwrap();
                let mut $plain =
                    PlainWindow(LimitedSource::bytes(&mut plain_source, window).unwrap());
                $fast.seek_to_bit(pos).unwrap();
                $plain.seek_to_bit(pos).unwrap();
            };
        }
        // Both windows run `op`, and agree on its result and where the cursor ends.
        macro_rules! same {
            ($what:expr, |$w:ident| $op:expr) => {{
                windows!(fast, plain);
                let got = {
                    let $w = &mut fast;
                    ($op, Source::bit_pos($w))
                };
                let expected = {
                    let $w = &mut plain;
                    ($op, Source::bit_pos($w))
                };
                assert_eq!(
                    got, expected,
                    "{context} window {window} pos {pos}, {}",
                    $what
                );
            }};
        }
        macro_rules! typed {
            ($($value:ty),*) => {
                $(same!(stringify!($value), |w| flat(w.read::<$value>()));)*
            };
        }
        typed!(
            u4,
            u8,
            u16,
            u24,
            u32,
            u64,
            u128,
            Own<true>,
            Own<false>,
            Blank<0>,
            Blank<136>
        );
        for n in (0..=5).chain([usize::MAX]) {
            same!(format!("read_bytes({n})"), |w| flat(w.read_bytes(n)));
        }
        for n in 0..=5 {
            same!(format!("read_into({n})"), |w| {
                let mut buf = [0xEE; 5];
                (flat(w.read_into(&mut buf[..n])), buf)
            });
            // A lent run is the bytes the per-byte loop reads, and leaves the cursor where
            // that loop does; a declined one consumes nothing.
            windows!(fast, plain);
            let lent = fast.aligned_run(n).map(<[u8]>::to_vec);
            let what = format!("{context} window {window} pos {pos}, aligned_run({n})");
            match lent {
                Some(run) => {
                    assert_eq!(flat(plain.read_bytes(n)), Ok(run), "{what}");
                    assert_eq!(Source::bit_pos(&fast), plain.bit_pos(), "{what}");
                }
                None => assert_eq!(Source::bit_pos(&fast), pos, "{what}"),
            }
        }
    }

    #[test]
    fn a_window_reports_its_boundary_at_the_cursor_and_lends_up_to_it() {
        let mut reader = BitReader::new(&[0x11, 0x22, 0x33, 0x44]);
        reader.seek_to_bit(8).unwrap();
        let mut window = reader.limit_bytes(2).unwrap();
        // A run that fills the window exactly is lent; one byte more is not.
        assert_eq!(window.aligned_run(3), None);
        assert_eq!(window.aligned_run(2), Some(&[0x22, 0x33][..]));
        window.seek_to_bit(16).unwrap();
        // One byte is left: a wider read stops at the boundary, typed or by bits, and says
        // so from where the cursor is.
        let boundary = ErrorKind::UnexpectedEof {
            needed: 16,
            remaining: 8,
        };
        let error = window.read::<u16>().unwrap_err();
        assert_eq!((error.kind, error.at), (boundary.clone(), 16));
        let error = window.read_bits(16).unwrap_err();
        assert_eq!((error.kind, error.at), (boundary, 16));
        assert_eq!(window.read::<u8>().unwrap(), 0x33);
    }

    #[test]
    fn a_window_reads_as_its_per_bit_form_does_over_both_slice_readers() {
        let mut state = 0xC2B2_AE3D_27D4_EB4F;
        let noisy: Vec<u8> = (0..20)
            .map(|_| noise(&mut state).to_le_bytes()[0])
            .collect();
        for bit in [BitOrder::Msb, BitOrder::Lsb] {
            for byte in [ByteOrder::Big, ByteOrder::Little] {
                let layout = Layout { bit, byte };
                // Buffers that end before, at and after the window's end, so a read meets
                // the window's boundary and the input's end in either order; the widest
                // window holds a 128-bit read from inside a byte, and a too-wide one.
                for len in [1, 2, 3, 5, 9, 14, 20] {
                    let buf = &noisy[..len];
                    for window in [0, 1, 2, 3, 4, 8, 13, 18] {
                        let reachable = (8 + window * 8).min(len * 8);
                        for pos in 8..=reachable {
                            let slice = || BitReader::with_layout(buf, layout);
                            let context = format!("{layout:?} len {len}");
                            assert_window_matches_the_plain_one(slice, window, pos, &context);
                            let incremental = || IncrementalReader(slice());
                            let context = format!("{layout:?} len {len}, incremental");
                            assert_window_matches_the_plain_one(incremental, window, pos, &context);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn limited_source_confines_nested_reads_and_requires_exact_consumption() {
        let mut source = BitReader::new(&[0xAA, 0xBB, 0xCC]);
        {
            let mut child = source.limit_bytes(2).unwrap();
            assert_eq!((child.start_bit(), child.end_bit()), (0, 16));
            assert_eq!(child.read::<u8>().unwrap(), 0xAA);
            assert_eq!(child.remaining_bits(), 8);
            assert_eq!(
                child.finish().unwrap_err().kind,
                ErrorKind::TrailingBits { remaining: 8 }
            );
        }

        let mut child = source.limit_bytes(1).unwrap();
        assert_eq!(child.read::<u8>().unwrap(), 0xBB);
        assert_eq!(
            child.read::<u8>().unwrap_err().kind,
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 0,
            }
        );
        child.finish().unwrap();
        assert_eq!(source.read::<u8>().unwrap(), 0xCC);
    }

    #[test]
    fn limited_seek_source_rejects_positions_outside_its_window() {
        let mut source = BitReader::new(&[0x00, 0x11, 0x22]);
        source.read::<u8>().unwrap();
        let mut child = source.limit_bytes(1).unwrap();
        child.seek_to_bit(16).unwrap();
        assert_eq!(
            child.read::<u8>().unwrap_err().kind,
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 0,
            }
        );
        assert!(matches!(
            child.seek_to_bit(0).unwrap_err().kind,
            ErrorKind::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn limited_source_reports_absolute_positions_for_a_window_past_the_start() {
        let mut source = BitReader::new(&[0x00, 0x11, 0x22, 0x33]);
        source.read::<u8>().unwrap();
        let mut child = source.limit_bytes(2).unwrap();
        assert_eq!((child.start_bit(), child.end_bit()), (8, 24));
        assert_eq!(child.bit_pos(), 8, "positions stay parent-absolute");
        child.read_bits(4).unwrap();
        assert_eq!(child.bit_pos(), 12);
    }

    #[test]
    fn limited_source_seek_accepts_both_edges_of_its_window() {
        // Window = bits 8..24 of a 32-bit parent.
        let mut source = BitReader::new(&[0x00, 0x11, 0x22, 0x33]);
        source.read::<u8>().unwrap();
        let mut child = source.limit_bytes(2).unwrap();
        child.seek_to_bit(24).unwrap();
        assert_eq!(child.remaining_bits(), 0);
        child.seek_to_bit(8).unwrap();
        assert_eq!(child.read::<u8>().unwrap(), 0x11);
    }

    #[test]
    fn limited_source_seek_rejects_one_bit_outside_either_edge() {
        // Window = bits 8..24 of a 32-bit parent: both targets are valid parent positions,
        // so only the window can reject them.
        let mut source = BitReader::new(&[0x00, 0x11, 0x22, 0x33]);
        source.read::<u8>().unwrap();
        let mut child = source.limit_bytes(2).unwrap();
        child.read_bits(4).unwrap();
        for outside in [7, 25] {
            assert!(
                matches!(
                    child.seek_to_bit(outside).unwrap_err().kind,
                    ErrorKind::UnexpectedEof { .. }
                ),
                "seek to bit {outside} must be rejected"
            );
            assert_eq!(
                child.bit_pos(),
                12,
                "a rejected seek must not move the cursor"
            );
        }
    }

    #[test]
    fn limited_source_refuses_a_window_between_bytes_or_past_the_address_space() {
        let mut source = BitReader::new(&[0x00, 0x11]);
        source.read_bits(3).unwrap();
        let error = source.limit_bytes(1).unwrap_err();
        assert_eq!((error.kind, error.at), (ErrorKind::NotByteAligned, 3));

        let mut source = BitReader::new(&[0x00, 0x11]);
        source.read_bits(8).unwrap();
        for n in [usize::MAX, usize::MAX / 8] {
            let error = source.limit_bytes(n).unwrap_err();
            assert_eq!(
                (error.kind, error.at),
                (ErrorKind::PositionOverflow, 8),
                "a window of {n} bytes has no representable end"
            );
        }
    }

    #[test]
    fn limited_source_seek_outside_reports_the_target_and_window_size() {
        let mut source = BitReader::new(&[0x00, 0x11, 0x22, 0x33]);
        source.read::<u8>().unwrap();
        let mut child = source.limit_bytes(2).unwrap();
        child.read_bits(4).unwrap();
        let past = child.seek_to_bit(30).unwrap_err();
        let before = child.seek_to_bit(0).unwrap_err();
        assert_eq!(
            (past.kind, past.at),
            (
                ErrorKind::UnexpectedEof {
                    needed: 22,
                    remaining: 16
                },
                12
            )
        );
        assert_eq!(
            (before.kind, before.at),
            (
                ErrorKind::UnexpectedEof {
                    needed: 0,
                    remaining: 16
                },
                12
            )
        );
    }

    #[test]
    fn limited_source_reads_in_its_parents_layout() {
        // LSB-first, little-endian is the one layout where dropping *either* order to its
        // default changes a byte-multiple value (see `Source::bit_order`).
        let layout = Layout {
            bit: BitOrder::Lsb,
            byte: ByteOrder::Little,
        };
        let mut source = BitReader::with_layout(&[0x34, 0x12], layout);
        let mut child = source.limit_bytes(2).unwrap();
        assert_eq!(child.bit_order(), BitOrder::Lsb);
        assert_eq!(child.byte_order(), ByteOrder::Little);
        assert_eq!(child.read::<u16>().unwrap(), 0x1234);
    }
}
