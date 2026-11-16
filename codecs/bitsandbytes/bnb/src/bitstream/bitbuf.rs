//! [`BitBuf`], the push/pull bit-aware decode buffer.

use alloc::vec::Vec;
use core::fmt;

use crate::field::{BitOrder, ByteOrder};

#[cfg(doc)]
use super::MAX_DECODE_DEPTH;
use super::incremental::decode_attempt;
use super::layout::bits_in;
use super::source::{aligned_run_at, read_via_slice};
use super::{
    BitDecode, BitEncode, BitError, BitReader, DecodeWith, ErrorKind, Layout, SeekSource, Source,
    sealed,
};

/// A **push/pull, bit-aware** decode buffer for incremental framing.
///
/// Feed bytes with [`push`](Self::push) as they arrive — from a socket, a channel, a callback,
/// anything that delivers bytes — and take whole messages off the front with [`pull`](Self::pull),
/// which returns `Ok(None)` when it needs more bytes (push more and call again).
///
/// Unlike a byte cursor (`bytes::BytesMut::advance`), `BitBuf` tracks a **bit** position, so a
/// stream of messages that *don't* end on byte boundaries (bit-packed frames) reassembles cleanly:
/// `pull` advances past the consumed whole bytes and retains any partial trailing byte for the
/// next message. It's the *pushable*, in-memory counterpart to `BufSource` (which pulls from a
/// `Read`). `no_std`-compatible (`alloc` only).
///
/// **Reclaim is deferred and in place.** `pull` doesn't drain consumed bytes — a later
/// [`push`](Self::push) reclaims them in place (one memmove), once the consumed bytes are
/// at least as many as the unread ones. A steady push/pull loop so reuses the same
/// allocation without per-message churn, and a consumer that falls behind does not have
/// its whole backlog moved on every push: an unbounded buffer grows instead, to at most
/// about twice the unread bytes it holds, and never shrinks. For a guaranteed-fixed
/// footprint (real-time / `no_std`), construct a [`bounded`](Self::bounded) buffer: it
/// allocates once, [`push`](Self::push) refuses bytes past the cap instead of growing, and
/// [`grow`](Self::grow) is the only thing that reallocates. A bounded buffer reclaims
/// whenever new bytes would not otherwise fit, so one kept nearly full moves its backlog on
/// each push; give it room for twice the backlog you expect if that matters.
///
/// `BitBuf` is also a [`SeekSource`], so it reads through the same [`decode`](crate::BitDecode)
/// entry points as every other cursor: `Type::decode(&mut bitbuf)` advances its cursor (then call
/// [`compact`](Self::compact) to reclaim). For streaming, prefer [`pull`](Self::pull) — it bakes
/// the message's own [`LAYOUT`](BitEncode::LAYOUT) (so `little`/`lsb` messages are always correct),
/// decodes **and** reclaims, and reports "need more bytes" as `Ok(None)`. The bare `Source` path
/// instead uses the buffer's own [`with_layout`](Self::with_layout) order (default msb/big).
///
/// ```
/// use bnb::{bin, BitBuf};
/// #[bin(big)]
/// #[derive(Debug, PartialEq, Eq)]
/// struct Ping { seq: u16 }
///
/// let mut bb = BitBuf::new();
/// bb.push(&[0x00]).unwrap();                         // only half of the first message
/// assert_eq!(bb.pull::<Ping>().unwrap(), None);      // not a whole message yet
/// bb.push(&[0x01, 0x00, 0x02]).unwrap();             // rest of msg 1 + all of msg 2
/// assert_eq!(bb.pull::<Ping>().unwrap(), Some(Ping { seq: 1 }));
/// assert_eq!(bb.pull::<Ping>().unwrap(), Some(Ping { seq: 2 }));
/// assert_eq!(bb.pull::<Ping>().unwrap(), None);      // drained
/// ```
///
#[derive(Debug, Default)]
pub struct BitBuf {
    /// Buffered bytes. The bytes before `cursor`'s byte are **consumed** (dead) — physically
    /// reclaimed lazily (on a [`push`](Self::push) that would otherwise grow, or [`compact`](Self::compact)),
    /// not on every [`pull`](Self::pull), so a push/pull loop doesn't churn memory.
    pub(super) buf: Vec<u8>,
    /// Live read position, in bits, into `buf` (`0..=buf.len() * 8`).
    pub(super) cursor: usize,
    /// `Some(cap)` for a **bounded** buffer (alloc-once: [`push`](Self::push) never grows
    /// past `cap`, [`grow`](Self::grow) raises it explicitly); `None` for an auto-growing buffer.
    cap: Option<usize>,
    /// Byte/bit order for the [`Source`] impl (the `decode(&mut bitbuf)` path); default msb/big.
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

impl Clone for BitBuf {
    fn clone(&self) -> Self {
        // Vec::clone reserves only len; that would break a bounded clone's alloc-once
        // promise when the original has unused reserved space.
        let buf = if let Some(cap) = self.cap {
            let mut buf = Vec::with_capacity(cap.max(self.buf.len()));
            buf.extend_from_slice(&self.buf);
            buf
        } else {
            self.buf.clone()
        };
        Self {
            buf,
            cursor: self.cursor,
            cap: self.cap,
            layout: self.layout,
            depth: 0,
        }
    }
}

impl BitBuf {
    /// An empty auto-growing buffer (msb/big order for the [`Source`] path).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty auto-growing buffer with room for `cap` bytes before reallocating. Like
    /// [`new`](Self::new) but pre-reserved; it still grows past `cap` on demand (use
    /// [`bounded`](Self::bounded) for a hard cap).
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            cursor: 0,
            cap: None,
            layout: Layout::default(),
            depth: 0,
        }
    }

    /// An empty **bounded** buffer: it allocates `cap` bytes once and never reallocates on its own.
    /// [`push`](Self::push) refuses bytes that would exceed `cap` (reclaiming consumed
    /// bytes first), and [`grow`](Self::grow) is the only thing that allocates again — so a
    /// real-time / `no_std` caller can guarantee a fixed footprint.
    #[must_use]
    pub fn bounded(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            cursor: 0,
            cap: Some(cap),
            layout: Layout::default(),
            depth: 0,
        }
    }

    /// Set the byte/bit order used by the [`Source`] impl (the `decode(&mut bitbuf)` path).
    /// [`pull`](Self::pull) ignores this — it always bakes the message's own `LAYOUT`.
    #[must_use]
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// The buffer's hard capacity in bytes for a [`bounded`](Self::bounded) buffer, or `None` if
    /// it auto-grows.
    #[must_use]
    pub fn capacity(&self) -> Option<usize> {
        self.cap
    }

    /// Reclaim consumed whole bytes (drain everything before the cursor's byte) once they are
    /// at least as many as the unread ones, or, in a bounded buffer, whenever the new bytes
    /// would not otherwise fit. Keeps the footprint within twice the working set without
    /// moving a backlog on every push.
    fn make_room(&mut self, additional: usize) {
        let dead = self.cursor / 8;
        if dead == 0 {
            return;
        }
        let live = self.buf.len() - dead;
        // Allocator slack after grow() must not allow physical length past the logical
        // cap: Source can rewind into the dead prefix and make every retained byte live.
        let limit = self.cap.unwrap_or(self.buf.capacity());
        let would_grow = additional > limit - self.buf.len();
        // A bounded buffer cannot grow, so it compacts whenever the bytes would not fit.
        // An unbounded one compacts only once the dead prefix is at least the live
        // bytes, so each byte moved is paid for by a byte reclaimed however large the
        // backlog; until then it grows instead.
        if dead >= live || (would_grow && self.cap.is_some()) {
            self.buf.drain(..dead);
            self.cursor -= dead * 8;
        }
    }

    /// Append bytes, reclaiming consumed whole bytes in place when they are at least as many
    /// as the unread ones (or, in a bounded buffer, when the new bytes would not otherwise
    /// fit). An unbounded buffer grows as needed; a bounded buffer never reallocates here.
    ///
    /// # Errors
    /// [`CapacityError`] if the bytes don't fit a [`bounded`](Self::bounded) buffer's capacity
    /// (the live bytes plus the new bytes exceed `cap`). Rejection leaves **all** state
    /// unchanged, including the cursor: the caller still owns the rejected input.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), CapacityError> {
        let live = self.buf.len() - self.cursor / 8;
        if let Some(cap) = self.cap {
            if bytes.len() > cap - live {
                return Err(CapacityError {
                    cap,
                    requested: live.saturating_add(bytes.len()),
                });
            }
        }
        self.make_room(bytes.len());
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    /// Grow a [`bounded`](Self::bounded) buffer's capacity by `additional` bytes (raising the cap
    /// and reserving the space — the one operation that reallocates a bounded buffer). On an
    /// unbounded buffer it just reserves.
    ///
    /// # Panics
    /// Panics if the requested capacity overflows or exceeds `Vec`'s allocation limit.
    pub fn grow(&mut self, additional: usize) {
        if let Some(cap) = self.cap {
            let cap = cap
                .checked_add(additional)
                .expect("bitbuf capacity overflow");
            // reserve() is relative to len, not the previous capacity.
            self.buf.reserve(cap.saturating_sub(self.buf.len()));
            self.cap = Some(cap);
        } else {
            self.buf.reserve(additional);
        }
    }

    /// The number of unconsumed bits currently buffered.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        bits_in(self.buf.len()) - self.cursor
    }

    /// Whether no unconsumed bits remain.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bit_len() == 0
    }

    /// Drop all buffered bytes and reset the cursor (keeps the allocation and any bound).
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    /// Physically reclaim the fully-consumed whole bytes now (drop everything before the cursor's
    /// byte), keeping any partial trailing byte. [`pull`](Self::pull) defers this; call it
    /// yourself when consuming via the [`Source`] path (`decode(&mut bitbuf)`) to bound the buffer.
    pub fn compact(&mut self) {
        let whole = self.cursor / 8;
        self.buf.drain(..whole);
        self.cursor -= whole * 8;
    }

    /// Decode the next complete message off the front, advancing past the bytes it consumed.
    ///
    /// Returns `Ok(None)` when the buffer doesn't yet hold a whole message — push more bytes and
    /// call again; bytes and cursor are left untouched. A malformed message is an
    /// `Err`. The byte/bit order is taken from `T`'s [`LAYOUT`](BitEncode::LAYOUT), so it decodes
    /// `little`/`lsb` messages correctly regardless of [`with_layout`](Self::with_layout).
    ///
    /// Consumed bytes are **not** drained here — they are reclaimed in place by the next
    /// [`push`](Self::push) (or an explicit [`compact`](Self::compact)),
    /// so a steady push/pull loop reuses the same allocation without per-message memmoves.
    ///
    /// # Errors
    /// A codec [`BitError`] for a malformed message.
    pub fn pull<T: BitDecode + BitEncode>(&mut self) -> Result<Option<T>, BitError> {
        match self.try_pull::<T>() {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => e.into_retry().map(|_| None),
        }
    }

    /// Decode one message, returning [`ErrorKind::Incomplete`] with position, field, and
    /// an additional-byte hint when more input is needed. An empty buffer reports an
    /// unknown hint without invoking the codec. Drain repeatedly to obtain zero or more
    /// messages, stopping on `Incomplete`; append bytes and continue later.
    ///
    /// The hint concerns the next blocked operation, not the final frame size. An attempt
    /// replays decoding from the current cursor: it does not resume inside a field. Callbacks
    /// must be retry-safe; their side effects are **not** rolled back. Fragmented variable-size
    /// collections can require repeated work. Errors never consume buffered input.
    ///
    /// Positions are buffer-local and may be rebased by compaction. Protocols with absolute
    /// message-relative pointers should first extract an owned envelope, then decode its slice.
    ///
    /// # Errors
    /// `Incomplete` for physical backing exhaustion, a definitive codec error otherwise,
    /// or [`ErrorKind::NoProgress`] if decoding succeeds without advancing.
    pub fn try_pull<T: BitDecode + BitEncode>(&mut self) -> Result<T, BitError> {
        self.try_pull_with(T::LAYOUT, ())
    }

    /// Context-aware, directional counterpart of [`try_pull`](Self::try_pull). The caller
    /// supplies the layout and fresh arguments for **each** attempt; `A` need not be `Clone`.
    /// This also supports `#[bin(read_only)]` messages without a `BitEncode` implementation.
    ///
    /// # Errors
    /// As [`try_pull`](Self::try_pull); all errors leave the buffer unchanged.
    pub fn try_pull_with<T: DecodeWith<A>, A>(
        &mut self,
        layout: Layout,
        args: A,
    ) -> Result<T, BitError> {
        if self.is_empty() {
            return Err(BitError::new(
                ErrorKind::Incomplete { needed: None },
                self.cursor,
            ));
        }
        let (value, end) = decode_attempt(&self.buf, self.cursor, layout, args, false)?;
        self.cursor = end;
        Ok(value)
    }

    /// Decode with the currently buffered input declared **finite**. Empty input is
    /// `Ok(None)`; a truncated message is a definitive error. EOF is not stored: the caller
    /// may append bytes or change message type after this attempt, even after an error.
    ///
    /// # Errors
    /// A finite codec error, [`ErrorKind::IncompleteAtEof`] for a custom codec's request
    /// for more input, or [`ErrorKind::NoProgress`]. Errors do not consume input.
    pub fn pull_eof<T: BitDecode + BitEncode>(&mut self) -> Result<Option<T>, BitError> {
        self.pull_eof_with(T::LAYOUT, ())
    }

    /// Context-aware, directional counterpart of [`pull_eof`](Self::pull_eof).
    ///
    /// # Errors
    /// As [`pull_eof`](Self::pull_eof); arguments are consumed, buffered input is not.
    pub fn pull_eof_with<T: DecodeWith<A>, A>(
        &mut self,
        layout: Layout,
        args: A,
    ) -> Result<Option<T>, BitError> {
        if self.is_empty() {
            return Ok(None);
        }
        let (value, end) = decode_attempt(&self.buf, self.cursor, layout, args, true)?;
        self.cursor = end;
        Ok(Some(value))
    }

    #[cfg(feature = "net")]
    pub(crate) fn read_capacity(&self) -> Option<usize> {
        self.cap.map(|cap| cap - (self.buf.len() - self.cursor / 8))
    }

    /// Makes room for `additional` more bytes as [`push`](Self::push) would, and says
    /// whether the cursor stayed where it was: reclaiming the consumed bytes rebases it.
    #[cfg(feature = "net")]
    pub(crate) fn make_room_in_place(&mut self, additional: usize) -> bool {
        let cursor = self.cursor;
        self.make_room(additional);
        self.cursor == cursor
    }

    /// The storage, for a transport to append what it reads without a copy. The caller
    /// only appends, never more than [`read_capacity`](Self::read_capacity) allows, and
    /// only after [`make_room_in_place`](Self::make_room_in_place) for as much, so a
    /// bounded buffer has the room and does not grow.
    #[cfg(feature = "net")]
    pub(crate) fn storage(&mut self) -> &mut Vec<u8> {
        &mut self.buf
    }

    #[cfg(feature = "net")]
    pub(crate) fn finish_byte(&mut self) {
        self.cursor = self.cursor.div_ceil(8) * 8;
    }

    #[cfg(feature = "net")]
    pub(crate) fn read_buffered(&mut self, out: &mut [u8]) -> Result<usize, BitError> {
        if self.cursor % 8 != 0 {
            return Err(BitError::new(ErrorKind::NotByteAligned, self.cursor));
        }
        let start = self.cursor / 8;
        let n = out.len().min(self.bit_len() / 8);
        out[..n].copy_from_slice(&self.buf[start..start + n]);
        self.cursor += n * 8;
        Ok(n)
    }
}

/// The error [`BitBuf::push`] returns when bytes won't fit a [`bounded`](BitBuf::bounded)
/// buffer — the live (unconsumed) bytes plus the new bytes exceed its fixed `cap`. Grow it with
/// [`BitBuf::grow`], or drain messages with [`pull`](BitBuf::pull) before pushing more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapacityError {
    /// The buffer's fixed capacity, in bytes.
    pub cap: usize,
    /// The bytes that were needed (live bytes + the rejected push).
    pub requested: usize,
}

impl fmt::Display for CapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bitbuf is full: {} bytes needed exceeds the {}-byte capacity",
            self.requested, self.cap
        )
    }
}

impl core::error::Error for CapacityError {}

impl sealed::Sealed for BitBuf {}

impl Source for BitBuf {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        read_via_slice(&self.buf, self.layout, &mut self.cursor, |r| r.read_bits(n))
    }

    fn bit_pos(&self) -> usize {
        self.cursor
    }

    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }

    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        // Validate against the buffered bits (mirrors BitReader's bounds), then move the cursor.
        let mut probe = BitReader::with_layout(&self.buf, self.layout);
        probe.seek_to_bit(pos)?;
        self.cursor = pos;
        Ok(())
    }

    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        if n == 0 {
            return Ok(Vec::new());
        }
        read_via_slice(&self.buf, self.layout, &mut self.cursor, |r| {
            r.read_bytes(n)
        })
    }

    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        if buf.is_empty() {
            return Ok(());
        }
        read_via_slice(&self.buf, self.layout, &mut self.cursor, |r| {
            r.read_into(buf)
        })
    }

    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        aligned_run_at(&self.buf, &mut self.cursor, n)
    }
}

impl SeekSource for BitBuf {}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn bitbuf_bounded_reports_its_capacity() {
        assert_eq!(BitBuf::bounded(64).capacity(), Some(64));
        assert_eq!(BitBuf::new().capacity(), None);
        assert_eq!(BitBuf::with_capacity(64).capacity(), None); // pre-reserved, not a hard cap
    }

    #[test]
    fn capacity_error_display() {
        let e = CapacityError {
            cap: 4,
            requested: 5,
        };
        assert_eq!(
            e.to_string(),
            "bitbuf is full: 5 bytes needed exceeds the 4-byte capacity"
        );
    }
}
