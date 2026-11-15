//! A bit-level stream codec — read/write fields at arbitrary *bit* offsets, not
//! just byte boundaries.
//!
//! A byte-oriented `Read + Seek` codec can only address byte boundaries, so a field
//! that starts mid-byte (a 108-bit DMR payload, a 48-bit sync pattern) forces
//! hand-rolled backward seeks and nibble shifts.
//! [`BitReader`]/[`BitWriter`] track a **bit** cursor over a byte buffer and
//! read/write any [`Bits`] value (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`)
//! directly — bit-aware *and* fast (shift/mask, no `bitvec`).
//!
//! The wire [`Layout`] is configurable: bit order (MSB-first default — bit 0 is the
//! high bit of byte 0, the RFC/ETSI convention — or LSB-first) and byte order (big-
//! endian default, or little-endian for byte-multiple values).
//!
//! ```
//! use bnb::{u4, u12, BitReader, BitWriter};
//!
//! // Pack a 4-bit then a 12-bit field into a 16-bit (2-byte) stream.
//! let mut w = BitWriter::new();
//! w.write(u4::new(0xA)).unwrap();
//! w.write(u12::new(0xBCD)).unwrap();
//! let bytes = w.into_bytes();
//! assert_eq!(bytes, [0xAB, 0xCD]);
//!
//! let mut r = BitReader::new(&bytes);
//! assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
//! assert_eq!(r.read::<u12>().unwrap(), u12::new(0xBCD));
//! ```

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::any::{Any, TypeId};
use core::fmt;
#[cfg(feature = "net")]
use core::mem;

use crate::field::{BitOrder, Bits, ByteOrder};

/// A position-aware bit-codec error (it carries a span-like position). It records the
/// **bit offset** where decoding/encoding failed and, when the derive can supply it,
/// the **field** being processed.
///
/// # Examples
///
/// ```
/// use bnb::{bin, ErrorKind};
///
/// #[bin(big)]
/// #[derive(Debug)]
/// struct Pair { a: u16, b: u16 }
///
/// let err = Pair::decode_exact(&[0x00]).unwrap_err(); // only one byte of four
/// assert_eq!(err.at, 0);             // the bit offset where it failed
/// assert_eq!(err.field, Some("a"));  // the field being read (the span)
/// assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct BitError {
    /// The cause.
    pub kind: ErrorKind,
    /// Absolute bit offset where the error occurred.
    pub at: usize,
    /// The field being decoded/encoded when it occurred, if recorded by the
    /// derive (the innermost field — the "span"). `None` for low-level reader
    /// errors with no field context.
    pub field: Option<&'static str>,
}

/// The selection strategy of a closed `#[bin]` enum that found no variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DispatchKind {
    /// Selection by a wire magic value.
    Magic,
    /// Selection by an external context tag.
    Tag,
    /// Selection by a context tag first, then wire magic if no tag matched.
    TagOrMagic,
}

/// A discriminator already read by the failed dispatch operation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DispatchValue {
    /// An unsigned magic integer, interpreted using the decoder's wire layout.
    Integer(u128),
    /// A fixed-width byte-string magic, copied only on terminal failure.
    Bytes(Vec<u8>),
}

/// Details of a terminal closed-enum dispatch miss, not a selected payload failure.
///
/// Use [`BitError::dispatch_error_for`] to check the originating enum's type identity.
/// Names alone cannot distinguish enums with identical names in different modules.
/// The details own their data and never borrow or retain the input frame.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnumDispatchError {
    origin: TypeId,
    // The macro promotes this immutable pair instead of copying it into every error.
    metadata: &'static (&'static str, DispatchKind),
    observed: Option<DispatchValue>,
}

impl EnumDispatchError {
    /// The enum's name for diagnostics; not a unique type identifier.
    #[must_use]
    pub fn enum_name(&self) -> &'static str {
        self.metadata.0
    }

    /// The selection strategy that found no matching variant.
    #[must_use]
    pub fn dispatch_kind(&self) -> DispatchKind {
        self.metadata.1
    }

    /// The already-read discriminator, if capturing it required no additional reads.
    ///
    /// `None` means **not captured**, not incomplete input: external context tags and
    /// variable-width magic probes are not captured. Hybrid dispatch captures the final
    /// wire magic, never the context tag. No whole payload is copied.
    #[must_use]
    pub fn observed(&self) -> Option<&DispatchValue> {
        self.observed.as_ref()
    }
}

/// One side of a magic that did not match: see [`MagicMismatch`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MagicValue {
    /// A numeric magic, as its type-erased low bits ([`Bits::into_bits`]).
    Int(u128),
    /// A byte-string magic.
    Bytes(&'static [u8]),
}

/// A magic that did not match ([`ErrorKind::BadMagic`]). It owns no allocation, so a
/// caller that tries one message type after another pays for none on a miss.
///
/// A side that was not kept reads as `None`: what a byte-string magic found on the wire,
/// and what a numeric magic expected when either value needs more than 64 bits. Two
/// mismatches are equal when the sides they kept are, so for such a wide magic equality
/// compares only what was read.
///
/// ```
/// use bnb::{MagicMismatch, MagicValue};
///
/// let miss = MagicMismatch::new(0xC0DE, 0xBEEF);
/// assert_eq!(miss.expected(), Some(MagicValue::Int(0xC0DE)));
/// assert_eq!(miss.found(), Some(MagicValue::Int(0xBEEF)));
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MagicMismatch(MagicRepr);

// Three payloads of two words each, so the error stays small with no `Box`. A `u128`
// field would align the whole error to 16 bytes; halves do not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MagicRepr {
    Int { expected: u64, found: u64 },
    Wide { found_low: u64, found_high: u64 },
    Bytes { expected: &'static [u8] },
}

impl MagicMismatch {
    /// A numeric magic that expected `expected` and read `found`. If either needs more than
    /// 64 bits, only `found` is kept.
    #[must_use]
    pub fn new(expected: u128, found: u128) -> Self {
        match (u64::try_from(expected), u64::try_from(found)) {
            (Ok(expected), Ok(found)) => Self(MagicRepr::Int { expected, found }),
            // Wider than 64 bits: what was read is kept, the constant is the type's own.
            #[expect(clippy::cast_possible_truncation, reason = "the two halves of a u128")]
            _ => Self(MagicRepr::Wide {
                found_low: found as u64,
                found_high: (found >> 64) as u64,
            }),
        }
    }

    /// A byte-string magic that expected `expected`. What was read is not kept.
    #[must_use]
    pub const fn bytes(expected: &'static [u8]) -> Self {
        Self(MagicRepr::Bytes { expected })
    }

    /// What the codec expected, unless either side of a numeric magic needs more than 64 bits.
    #[must_use]
    pub fn expected(&self) -> Option<MagicValue> {
        match self.0 {
            MagicRepr::Int { expected, .. } => Some(MagicValue::Int(u128::from(expected))),
            MagicRepr::Wide { .. } => None,
            MagicRepr::Bytes { expected } => Some(MagicValue::Bytes(expected)),
        }
    }

    /// What was read, unless the magic is a byte string.
    #[must_use]
    pub fn found(&self) -> Option<MagicValue> {
        match self.0 {
            MagicRepr::Int { found, .. } => Some(MagicValue::Int(u128::from(found))),
            MagicRepr::Wide {
                found_low,
                found_high,
            } => Some(MagicValue::Int(
                u128::from(found_high) << 64 | u128::from(found_low),
            )),
            MagicRepr::Bytes { .. } => None,
        }
    }
}

/// `expected 0xc0de, found 0xbeef`, with whichever side was kept.
impl fmt::Display for MagicMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sides = [("expected", self.expected()), ("found", self.found())];
        let mut first = true;
        for (name, value) in sides {
            let Some(value) = value else { continue };
            if !first {
                f.write_str(", ")?;
            }
            first = false;
            match value {
                MagicValue::Int(value) => write!(f, "{name} {value:#x}")?,
                MagicValue::Bytes(bytes) => {
                    write!(f, "{name} 0x")?;
                    bytes.iter().try_for_each(|byte| write!(f, "{byte:02x}"))?;
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for MagicMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MagicMismatch")
            .field("expected", &self.expected())
            .field("found", &self.found())
            .finish()
    }
}

/// A failed conversion of the caller's own ([`ErrorKind::Convert`]): a message, and a tag
/// if the code that raised it attached one.
///
/// The message is for people. To tell one failure from another in code, attach a tag
/// where the error is raised and compare that: a constant of your own, prefixed with your
/// crate's name so that it cannot collide with another codec's.
///
/// A tag is attached where a [`BitError`] is built, so it is for `parse_with`/`write_with`
/// functions and codecs. The error a `try_map` or `try_wire` closure returns is rendered
/// to its message, whatever its type, and carries none.
///
/// ```
/// use bnb::{BitError, Conversion, ErrorKind};
///
/// const POINTER_LOOP: &str = "dns.pointer_loop";
///
/// let error = BitError::convert("compression pointer loop", 96).with_tag(POINTER_LOOP);
/// assert_eq!(error.tag(), Some(POINTER_LOOP));
/// assert_eq!(
///     error.kind,
///     ErrorKind::Convert(Conversion::new("compression pointer loop").with_tag(POINTER_LOOP))
/// );
/// ```
///
/// Two conversions are equal when their messages and tags are.
#[derive(Clone, PartialEq, Eq)]
pub struct Conversion(Box<ConversionDetail>);

// Behind one pointer: the error stays small, and a field added here later changes no size.
#[derive(Clone, PartialEq, Eq)]
struct ConversionDetail {
    message: Cow<'static, str>,
    tag: Option<&'static str>,
}

impl Conversion {
    /// A conversion failure with `message` and no tag. A `&'static str` is kept as it is; a
    /// `String` is taken over.
    #[must_use]
    pub fn new(message: impl Into<Cow<'static, str>>) -> Self {
        Self(Box::new(ConversionDetail {
            message: message.into(),
            tag: None,
        }))
    }

    /// The same failure carrying `tag`.
    #[must_use]
    pub fn with_tag(mut self, tag: &'static str) -> Self {
        self.0.tag = Some(tag);
        self
    }

    /// The message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.0.message
    }

    /// The tag, if one was attached.
    #[must_use]
    pub fn tag(&self) -> Option<&'static str> {
        self.0.tag
    }
}

impl fmt::Debug for Conversion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Conversion")
            .field("message", &self.message())
            .field("tag", &self.tag())
            .finish()
    }
}

// The error is returned from every read: its size is the decoder's frame and the cost of
// each copy on a failing path (DESIGN.md §27). No variant holds a `u128` or more than three
// words, and nothing allocates but a conversion, a dispatch miss and a formatted assert.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<MagicMismatch>() == 24);
    assert!(size_of::<Conversion>() == 8);
    assert!(size_of::<ErrorKind>() == 32);
    assert!(size_of::<BitError>() == 56);
};

// It crosses threads inside an `io::Error`, and a caller may catch a panic around a decode.
const _: fn() = || {
    fn all<T: Send + Sync + Unpin + core::panic::UnwindSafe + core::panic::RefUnwindSafe>() {}
    all::<BitError>();
};

/// The cause of a [`BitError`]. `#[non_exhaustive]`: new variants may be added
/// without a major version bump, so match with a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Ran past the end of a finite input (a slice, or a `BufSource` whose reader hit
    /// EOF): `needed` bits were requested,
    /// `remaining` were left. Definitive — distinct from [`Incomplete`](ErrorKind::Incomplete).
    UnexpectedEof {
        /// Bits requested.
        needed: usize,
        /// Bits still available.
        remaining: usize,
    },
    /// More backing input is required. Buffered callers can append bytes and retry.
    /// `needed` describes the next blocked operation, **not** the full frame length or
    /// a promise that a transport will not read ahead. See [`BitBuf::try_pull`].
    Incomplete {
        /// Minimum additional **whole bytes**, appended at the physical end of the
        /// retained input, before this attempt can stop being incomplete. `Some(n > 0)`
        /// guarantees that appending fewer than `n` bytes cannot produce either a value
        /// **or a definitive error**, with the same retained prefix, numeric attempt-start
        /// cursor, layout, context, and codec-visible state. It does not promise that `n`
        /// bytes will suffice. Compaction that rebases the cursor invalidates an outstanding
        /// hint: retry immediately, because absolute seeks or position-dependent codecs may
        /// now behave differently. The whole-message readers detect this automatically.
        ///
        /// Use `None` when that lower bound cannot be proved (including speculative
        /// codecs whose choice can change with earlier bytes). Readers treat `Some(0)`
        /// as unknown: retry after at least one additional byte. Physical EOF overrides
        /// all hints and requires a finite attempt. Hints do not make a forward-only
        /// source transactional; safe whole-message retry needs retained input.
        needed: Option<usize>,
    },
    /// A custom codec requested more input after the caller declared finite EOF.
    /// Ordinary finite-source exhaustion uses [`UnexpectedEof`](Self::UnexpectedEof).
    IncompleteAtEof {
        /// The custom codec's additional-byte hint, if known; informational at finite EOF.
        needed: Option<usize>,
    },
    /// A streamed message, counted element, or message in a `decode_all`/`decode_iter` slice
    /// walk succeeded without ending past where it started (zero-width, or a backward
    /// `seek`). Rejecting it prevents a drain loop from emitting unlimited values from no input.
    NoProgress,
    /// `decode_exact` left whole bytes unconsumed after the message.
    TrailingBytes {
        /// Number of trailing bytes.
        remaining: usize,
    },
    /// A single field exceeded the 128-bit carrier width.
    TooWide {
        /// The offending width.
        width: usize,
    },
    /// An I/O error while encoding to a [`std::io::Write`] sink (the `std` feature).
    #[cfg(feature = "std")]
    Io(std::io::ErrorKind),
    /// A `magic` constant read off the wire did not match: a numeric magic or a byte
    /// string. Never allocates.
    BadMagic(MagicMismatch),
    /// No variant of a closed `#[bin]` enum matched. Details are allocated only on
    /// failure and boxed so they do not enlarge the inline error representation.
    /// Signature, prefix, and selected-payload failures keep their original kinds.
    NoMatchingVariant(Box<EnumDispatchError>),
    /// A conversion of the caller's own failed: a `try_map` or `try_wire` that returned
    /// an error, or a `parse_with`/`write_with` function or codec that raised one with
    /// [`BitError::convert`]. The crate's own failures have variants of their own.
    Convert(Conversion),
    /// A `#[br(assert(...))]` rejected the decoded value. `message` is the directive's
    /// message, or the condition in backticks when it gave none.
    AssertFailed {
        /// The assertion's message.
        message: Cow<'static, str>,
    },
    /// A string codec read bytes that are not UTF-8.
    InvalidUtf8 {
        /// How many bytes from the start of the string were valid.
        valid_up_to: usize,
    },
    /// A value does not fit an integer of `bits` bits: what a failed
    /// [`UInt::try_new`](crate::UInt::try_new) (a [`WidthError`](crate::WidthError))
    /// becomes when it is propagated with `?`. The value itself is not kept.
    ValueOutOfRange {
        /// The width of the target, in bits.
        bits: u32,
    },
    /// A collection is too long for the count or length that describes it on the wire
    /// (a `count_prefix`, a [`WireLen`](crate::WireLen), a string's length prefix), or a
    /// `count` expression does not fit a `usize` (then `count` is `usize::MAX` and `bits`
    /// is `usize::BITS`).
    CountOutOfRange {
        /// The number of elements or bytes.
        count: usize,
        /// The width of the count on the wire, in bits.
        bits: u32,
    },
    /// A variable-length integer (LEB128) overflows its `bits`-bit type, or has no final
    /// byte within the bytes that type can take.
    VarintOverflow {
        /// The width of the target type, in bits.
        bits: u32,
    },
    /// A NUL-terminated string to be written contains a NUL at byte `index`: the decoded
    /// value would stop there.
    EmbeddedNul {
        /// The offset of the first NUL in the value.
        index: usize,
    },
    /// A [`WireLen::Auto`](crate::WireLen) reached encode with no `auto_len` directive to
    /// derive it and no explicit value.
    UnresolvedWireLen,
    /// A position directive (`restore_position`/seek) ran on a non-seekable
    /// [`Source`] (a forward-only stream). Decode from a slice ([`BitReader`]) or a
    /// seekable source instead. Also a seek on a `BufSource` (with `std`) to a position
    /// before the bytes it still retains: `BufSource::compact` discarded them, and the
    /// reader underneath cannot go back.
    NotSeekable,
    /// A `BufSource` (with `std`) would have to retain more than its cap to finish the read —
    /// the bytes held since the start, or since the last `BufSource::compact`, plus the
    /// message in progress exceed the configured bound (never unbounded). Also a message
    /// longer than a `BinCodec::with_max_frame` cap (with `tokio`), reported at the cap.
    BufferFull {
        /// The cap, in bytes.
        cap: usize,
    },
    /// A byte-only operation was attempted while the cursor was between bytes.
    NotByteAligned,
    /// Computing a wire cursor or bounded region's end position overflowed `usize`.
    PositionOverflow,
    /// A bounded region was not consumed exactly.
    TrailingBits {
        /// Number of bits left inside the region.
        remaining: usize,
    },
    /// Decoding would nest more than [`MAX_DECODE_DEPTH`] non-empty `Vec`s of messages (a
    /// message that contains itself, directly or through another). Reported where the
    /// over-limit `Vec`'s first element would start, with that field's name.
    TooDeep,
}

/// How many non-empty `Vec`s of messages a decode nests before failing with
/// [`ErrorKind::TooDeep`]. A message can contain itself only through such a `Vec`, so the
/// element loops are what count (one level per non-empty `Vec`, not per element); a run of
/// leaves (integers, `bool`, `uN`, bitfields, enums, flags) is never charged, so the result does
/// not depend on the source. Recursion inside a `parse_with` codec or a hand-written
/// [`BitDecode::bit_decode`] is that code's own to bound.
///
/// The limit counts levels, not bytes of stack: 128 levels of a small message fit a 2 MiB
/// thread stack in a debug build, but a message with a large decoder frame (big arrays held
/// inline, many fields) can still overflow a small stack before the limit. Decode such types
/// on a thread whose stack holds `MAX_DECODE_DEPTH` of their frames, or cap the input.
pub const MAX_DECODE_DEPTH: u32 = 128;

// Sources keep the count in a `u16`: beside the two one-byte order fields it costs no extra
// store when a reader is built.
const _: () = assert!(MAX_DECODE_DEPTH < u16::MAX as u32);

impl BitError {
    /// Builds an error at absolute bit offset `at`, with no field recorded yet.
    #[must_use]
    pub fn new(kind: ErrorKind, at: usize) -> Self {
        Self {
            kind,
            at,
            field: None,
        }
    }

    /// Builds a [`ErrorKind::BadMagic`] error (a `magic` constant mismatched) at
    /// absolute bit offset `at`. `expected`/`found` are the type-erased low-bit
    /// values ([`Bits::into_bits`]).
    #[must_use]
    pub fn bad_magic(expected: u128, found: u128, at: usize) -> Self {
        Self::new(ErrorKind::BadMagic(MagicMismatch::new(expected, found)), at)
    }

    /// Builds an [`ErrorKind::BadMagic`] error for a byte-string magic at bit offset `at`.
    #[doc(hidden)]
    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn bad_magic_bytes(expected: &'static [u8], at: usize) -> Self {
        Self::new(ErrorKind::BadMagic(MagicMismatch::bytes(expected)), at)
    }

    /// Builds an [`ErrorKind::AssertFailed`] error at bit offset `at`.
    #[doc(hidden)]
    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn assert_failed(message: Cow<'static, str>, at: usize) -> Self {
        Self::new(ErrorKind::AssertFailed { message }, at)
    }

    /// Builds an [`ErrorKind::CountOutOfRange`] error at bit offset `at`.
    #[doc(hidden)]
    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn count_out_of_range(count: usize, bits: u32, at: usize) -> Self {
        Self::new(ErrorKind::CountOutOfRange { count, bits }, at)
    }

    /// Builds an [`ErrorKind::Convert`] error (a value conversion or codec check failed) at
    /// absolute bit offset `at`.
    #[must_use]
    pub fn convert(message: impl Into<Cow<'static, str>>, at: usize) -> Self {
        // One body for every message type: the generic part is the conversion alone.
        #[cold]
        #[inline(never)]
        fn build(message: Cow<'static, str>, at: usize) -> BitError {
            BitError::new(ErrorKind::Convert(Conversion::new(message)), at)
        }
        build(message.into(), at)
    }

    /// Attaches `tag` to an [`ErrorKind::Convert`] error, for the caller to tell this
    /// failure from the codec's others without reading its message; see [`Conversion`].
    /// Any other kind of error is returned unchanged. An error returned from a `try_map`
    /// or `try_wire` closure does not keep a tag: only its message is taken.
    #[must_use]
    pub fn with_tag(mut self, tag: &'static str) -> Self {
        if let ErrorKind::Convert(conversion) = &mut self.kind {
            conversion.0.tag = Some(tag);
        }
        self
    }

    /// The tag of an [`ErrorKind::Convert`] error, if it carries one.
    #[must_use]
    pub fn tag(&self) -> Option<&'static str> {
        match &self.kind {
            ErrorKind::Convert(conversion) => conversion.tag(),
            _ => None,
        }
    }

    /// Macro implementation detail: construct a type-attributed dispatch miss.
    ///
    /// Non-generic and out of line so the allocation stays outside every generated
    /// decode frame; the macro passes `TypeId::of::<Self>()` as `origin`.
    #[doc(hidden)]
    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn no_matching_variant(
        origin: TypeId,
        metadata: &'static (&'static str, DispatchKind),
        observed: Option<DispatchValue>,
        at: usize,
    ) -> Self {
        Self::new(
            ErrorKind::NoMatchingVariant(Box::new(EnumDispatchError {
                origin,
                metadata,
                observed,
            })),
            at,
        )
    }

    /// Return dispatch details only when the miss originated in enum `T`.
    ///
    /// Checks type identity, not the diagnostic name or field path. Nested dispatch
    /// errors retain their innermost origin. This is not a parser or a retry operation.
    /// `#[bin]` enums currently do not support generic parameters, including lifetimes.
    #[must_use]
    pub fn dispatch_error_for<T: 'static>(&self) -> Option<&EnumDispatchError> {
        match &self.kind {
            ErrorKind::NoMatchingVariant(details) if details.origin == TypeId::of::<T>() => {
                Some(details)
            }
            _ => None,
        }
    }

    /// Records the field being processed, **if one is not already set** — so the
    /// innermost field (set first as the error propagates up) wins. The derive
    /// calls this per field.
    #[must_use]
    pub fn in_field(mut self, field: &'static str) -> Self {
        if self.field.is_none() {
            self.field = Some(field);
        }
        self
    }

    /// Whether this is the streaming "need more bytes" signal
    /// ([`ErrorKind::Incomplete`]), as opposed to a definitive parse failure.
    /// Retrying safely requires retained input, such as a `BitBuf` attempt;
    /// this predicate alone does not promise rollback by a forward-only reader.
    #[must_use]
    pub fn is_incomplete(&self) -> bool {
        matches!(self.kind, ErrorKind::Incomplete { .. })
    }

    /// The additional-byte hint if this is the retry signal ([`ErrorKind::Incomplete`]),
    /// else the error back. A retry signal owns nothing, so it is let go without the
    /// error's drop code, which is otherwise a call on every short attempt.
    #[inline]
    pub(crate) fn into_retry(self) -> Result<Option<usize>, Self> {
        if let ErrorKind::Incomplete { needed } = self.kind {
            // Every field named and shown to own nothing, so that a new one, or one of
            // these becoming a type with drop code, stops this compiling.
            fn owns_nothing<T: Copy>(_: &T) {}
            let Self { kind: _, at, field } = &self;
            owns_nothing(at);
            owns_nothing(field);
            core::mem::forget(self);
            Ok(needed)
        } else {
            Err(self)
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for BitError {
    /// Recovers a typed `BitError` carried by an I/O adapter, including its position and
    /// field. Other I/O errors become [`ErrorKind::Io`] at unknown offset `0`.
    fn from(e: std::io::Error) -> Self {
        e.get_ref()
            .and_then(|inner| inner.downcast_ref::<Self>())
            .cloned()
            .unwrap_or_else(|| Self::new(ErrorKind::Io(e.kind()), 0))
    }
}

impl fmt::Display for BitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::UnexpectedEof { needed, remaining } => write!(
                f,
                "unexpected end of input: needed {needed} bits, {remaining} remain"
            )?,
            ErrorKind::Incomplete { needed } => match needed {
                Some(n) if *n != 0 => write!(f, "incomplete: need at least {n} more bytes")?,
                _ => write!(f, "incomplete: need more bytes")?,
            },
            ErrorKind::IncompleteAtEof { needed } => {
                write!(
                    f,
                    "codec requested more input at EOF ({needed:?} additional bytes)"
                )?;
            }
            ErrorKind::NoProgress => f.write_str("decoder made no forward progress")?,
            ErrorKind::TooDeep => write!(f, "messages nested deeper than {MAX_DECODE_DEPTH}")?,
            ErrorKind::TrailingBytes { remaining } => {
                write!(f, "{remaining} trailing bytes after the message")?;
            }
            ErrorKind::TooWide { width } => {
                write!(f, "field width {width} exceeds the 128-bit carrier")?;
            }
            #[cfg(feature = "std")]
            ErrorKind::Io(kind) => write!(f, "I/O error: {kind:?}")?,
            ErrorKind::BadMagic(mismatch) => write!(f, "bad magic: {mismatch}")?,
            ErrorKind::NoMatchingVariant(details) => {
                write!(f, "unrecognized {} discriminant", details.enum_name())?;
                match &details.observed {
                    Some(DispatchValue::Integer(value)) => write!(f, ": {value:#x}")?,
                    Some(DispatchValue::Bytes(bytes)) => {
                        f.write_str(": 0x")?;
                        for byte in bytes {
                            write!(f, "{byte:02x}")?;
                        }
                    }
                    None => {}
                }
            }
            ErrorKind::Convert(conversion) => {
                write!(f, "conversion failed: {}", conversion.message())?;
            }
            ErrorKind::AssertFailed { message } => write!(f, "assertion failed: {message}")?,
            ErrorKind::InvalidUtf8 { valid_up_to } => {
                write!(
                    f,
                    "invalid UTF-8 after {valid_up_to} valid bytes of a string"
                )?;
            }
            ErrorKind::ValueOutOfRange { bits } => {
                write!(f, "value does not fit in {bits} bits")?;
            }
            ErrorKind::CountOutOfRange {
                count: usize::MAX,
                bits: usize::BITS,
            } => f.write_str("count does not fit in a usize")?,
            ErrorKind::CountOutOfRange { count, bits } => {
                write!(f, "count {count} does not fit in {bits} bits")?;
            }
            ErrorKind::VarintOverflow { bits } => {
                write!(f, "variable-length integer does not fit in {bits} bits")?;
            }
            ErrorKind::EmbeddedNul { index } => write!(
                f,
                "embedded NUL at byte {index}: a NUL-terminated string cannot represent it"
            )?,
            ErrorKind::UnresolvedWireLen => f.write_str(
                "unresolved `WireLen::Auto`: this field needs a \
                 `#[bw(auto_len = count(f)|bytes(f))]` (or `#[bin(auto_len(...))]`) directive \
                 to derive its value, or set it explicitly with `WireLen::set(n)`",
            )?,
            ErrorKind::NotSeekable => {
                write!(f, "seek to a position this source cannot reach")?;
            }
            ErrorKind::BufferFull { cap } => {
                write!(f, "buffered source exceeded its {cap}-byte cap")?;
            }
            ErrorKind::NotByteAligned => {
                f.write_str("operation requires a byte-aligned cursor")?;
            }
            ErrorKind::PositionOverflow => {
                f.write_str("wire cursor position overflowed")?;
            }
            ErrorKind::TrailingBits { remaining } => {
                write!(f, "{remaining} trailing bits in the bounded region")?;
            }
        }
        write!(f, " at bit {}", self.at)?;
        if let Some(field) = self.field {
            write!(f, " (field `{field}`)")?;
        }
        Ok(())
    }
}

impl core::error::Error for BitError {}

impl From<crate::error::WidthError> for BitError {
    /// Bridges a construction error (e.g. `UInt::try_new`) into a codec error, so it
    /// `?`-propagates inside a custom `parse_with`/`write_with` fn or a converter
    /// that returns [`BitError`]. The offset is unknown (`0`) — the codec's own
    /// reads/writes carry the real bit offset; this is only for borrowed construction
    /// failures with no cursor context.
    #[inline]
    fn from(e: crate::error::WidthError) -> Self {
        let (crate::error::WidthError::ValueTooLarge { bits, .. }
        | crate::error::WidthError::Negative { bits, .. }) = e;
        BitError::new(ErrorKind::ValueOutOfRange { bits }, 0)
    }
}

/// The count or length prefix for `len` elements, or [`ErrorKind::CountOutOfRange`] at
/// bit offset `at` if `P` cannot hold it. Behind `count_prefix`, [`WireLen`](crate::WireLen)
/// and the length-prefixed string codec.
#[doc(hidden)]
#[inline]
pub fn prefix_for_count<P: CountPrefix>(len: usize, at: usize) -> Result<P, BitError> {
    P::try_from_count(len).map_err(|_| BitError::count_out_of_range(len, P::BITS, at))
}

/// A `#[br(assert(...))]` message: borrowed when it is a plain literal, formatted (once)
/// when it interpolates anything.
#[doc(hidden)]
#[must_use]
pub fn assert_message(args: fmt::Arguments<'_>) -> Cow<'static, str> {
    match args.as_str() {
        Some(literal) => Cow::Borrowed(literal),
        None => Cow::Owned(alloc::fmt::format(args)),
    }
}

/// [`ErrorKind::InvalidUtf8`] at bit offset `at`, where the caller's read of the string
/// (its length prefix, if it has one) began.
#[cold]
pub(crate) fn invalid_utf8(error: &alloc::string::FromUtf8Error, at: usize) -> BitError {
    BitError::new(
        ErrorKind::InvalidUtf8 {
            valid_up_to: error.utf8_error().valid_up_to(),
        },
        at,
    )
}

/// The bit width of a [`Bits`] value's type. Generated `BIT_LEN` consts and the
/// alignment guard call this to size a `magic` constant whose type they only have
/// as an expression (the value is taken by reference purely to infer `T`).
#[doc(hidden)]
#[must_use]
pub const fn bits_of<T: Bits>(_value: &T) -> u32 {
    T::BITS
}

/// Reads a `magic` constant and verifies it equals `expected`, compared as
/// type-erased bits (so `T` needs only [`Bits`] — no `Copy`/`PartialEq`, and `T`
/// is pinned by the argument so the generated call site needs no turbofish). On
/// mismatch: [`ErrorKind::BadMagic`] at the magic's offset.
#[doc(hidden)]
pub fn verify_magic<T: Bits, S: Source>(r: &mut S, expected: T) -> Result<(), BitError> {
    let at = r.bit_pos();
    let found: T = r.read()?;
    let (e, g) = (expected.into_bits(), found.into_bits());
    if e != g {
        return Err(BitError::bad_magic(e, g, at));
    }
    Ok(())
}

/// Reads a wire value `W` (inferred from `f`'s argument type) and maps it to the
/// field type `T` — backs `#[br(map = …)]`.
///
/// # Errors
/// Propagates the read [`BitError`].
#[doc(hidden)]
pub fn read_mapped<W, T, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: Bits,
    S: Source,
    F: FnOnce(W) -> T,
{
    let raw: W = r.read()?;
    Ok(f(raw))
}

/// Fallible variant — backs `#[br(try_map = …)]`. A conversion error becomes an
/// [`ErrorKind::Convert`] at the value's offset.
///
/// # Errors
/// The read [`BitError`], or the converter's failure as [`ErrorKind::Convert`].
#[doc(hidden)]
pub fn read_try_mapped<W, T, E, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: Bits,
    S: Source,
    E: fmt::Display,
    F: FnOnce(W) -> Result<T, E>,
{
    let at = r.bit_pos();
    let raw: W = r.read()?;
    f(raw).map_err(|e| BitError::convert(e.to_string(), at))
}

/// Maps the field `T` to its wire value `W` and writes it — backs `#[bw(map = …)]`.
///
/// # Errors
/// Propagates the write [`BitError`].
#[doc(hidden)]
pub fn write_mapped<W, T, K, F>(w: &mut K, value: &T, f: F) -> Result<(), BitError>
where
    W: Bits,
    K: Sink,
    F: FnOnce(&T) -> W,
{
    w.write(f(value))
}

/// Decodes a whole wire **message** `W` (a `BitDecode`, inferred from `f`'s argument) and
/// maps it to the logical type — backs struct-level `#[bin(map = …)]`. The message dual of
/// [`read_mapped`] (whose `W` is a single `Bits` field).
///
/// # Errors
/// Propagates the decode [`BitError`].
#[doc(hidden)]
pub fn decode_mapped_msg<W, T, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: BitDecode,
    S: Source,
    F: FnOnce(W) -> T,
{
    Ok(f(W::bit_decode(r)?))
}

/// Fallible message map — backs struct-level `#[bin(try_map = …)]`. A conversion error
/// becomes an [`ErrorKind::Convert`] at the message's start offset.
///
/// # Errors
/// The decode [`BitError`], or the converter's failure as [`ErrorKind::Convert`].
#[doc(hidden)]
pub fn decode_try_mapped_msg<W, T, E, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: BitDecode,
    S: Source,
    E: fmt::Display,
    F: FnOnce(W) -> Result<T, E>,
{
    let at = r.bit_pos();
    let w = W::bit_decode(r)?;
    f(w).map_err(|e| BitError::convert(e.to_string(), at))
}

/// Maps the logical type to its wire **message** `W` (a `BitEncode`) and encodes it —
/// backs struct-level `#[bin(bw_map = …)]`. The message dual of [`write_mapped`].
///
/// # Errors
/// Propagates the encode [`BitError`].
#[doc(hidden)]
pub fn encode_mapped_msg<W, T, K, F>(w: &mut K, value: &T, f: F) -> Result<(), BitError>
where
    W: BitEncode,
    K: Sink,
    F: FnOnce(&T) -> W,
{
    f(value).bit_encode(w)
}

/// A typed bit/byte amount for positioning directives — `4.bits()`, `3.bytes()` —
/// resolving to a bit count. Bring it in with `use bnb::prelude::*`.
///
/// # Examples
///
/// ```
/// use bnb::prelude::*;
/// assert_eq!(4u32.bits(), 4);
/// assert_eq!(3u32.bytes(), 24);
/// ```
///
/// Used by the positioning directives, e.g. `#[br(pad_before = 2u32.bytes())]` — see
/// [`guide::directives`](crate::guide::directives).
/// The result retains the low 32 bits, including multiplication by eight for `bytes`;
/// negative and overflowing amounts wrap. For untrusted lengths, validate the range
/// before converting: this convenience trait is not a checked length conversion.
pub trait BitAmount: Copy + sealed::Sealed {
    /// This many **bits**.
    fn bits(self) -> u32;
    /// This many **bytes** (× 8 bits).
    fn bytes(self) -> u32;
}

macro_rules! impl_bit_amount {
    ($($t:ty),*) => {$(
        impl BitAmount for $t {
            // Preserve the existing signed/unsigned low-32-bit conversion contract.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless)]
            fn bits(self) -> u32 { self as u32 }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_lossless)]
            fn bytes(self) -> u32 { (self as u32).wrapping_mul(8) }
        }
    )*};
}
impl_bit_amount!(u8, u16, u32, u64, usize, i16, i32, i64, isize);
// Seal the `BitAmount` types that aren't already sealed by `CountPrefix`
// (`u8, u16, u32, u64` get their `Sealed` impl from `count_prefix_prim!`).
impl sealed::Sealed for usize {}
impl sealed::Sealed for i16 {}
impl sealed::Sealed for i32 {}
impl sealed::Sealed for i64 {}
impl sealed::Sealed for isize {}

/// Skips `bits` forward (consuming and discarding) — backs `#[br(pad_before/after)]`.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn skip_read<S: Source>(r: &mut S, bits: u32) -> Result<(), BitError> {
    let mut left = bits;
    while left > 0 {
        let n = left.min(128);
        r.read_bits(n)?;
        left -= n;
    }
    Ok(())
}

/// Writes `bits` zero bits forward — the write dual of [`skip_read`].
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn skip_write<K: Sink>(w: &mut K, bits: u32) -> Result<(), BitError> {
    let mut left = bits;
    while left > 0 {
        let n = left.min(128);
        w.write_bits(0, n)?;
        left -= n;
    }
    Ok(())
}

/// Skips forward to the next byte boundary — backs `#[br(align_before/after)]`.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn align_read<S: Source>(r: &mut S) -> Result<(), BitError> {
    let pad = (8 - (r.bit_pos() % 8)) % 8;
    skip_read(
        r,
        u32::try_from(pad).expect("alignment padding is below eight bits"),
    )
}

/// Pads with zero bits to the next byte boundary — the write dual of [`align_read`].
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn align_write<K: Sink>(w: &mut K) -> Result<(), BitError> {
    let pad = (8 - (w.bit_pos() % 8)) % 8;
    skip_write(
        w,
        u32::try_from(pad).expect("alignment padding is below eight bits"),
    )
}

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
fn value_byte_order<T: Bits>(raw: u128, bit: BitOrder, byte: ByteOrder) -> u128 {
    apply_byte_order(raw, T::BITS, bit, T::__BNB_BYTE_ORDER.unwrap_or(byte))
}

/// `len` bytes as a bit count. Where `usize` cannot count that many bits (a 32-bit target with
/// 512 MiB or more) it saturates at the last whole byte that fits, so such a buffer reads as
/// ending there: reads past it fail and seeks are refused, never a wrapped short length.
#[inline]
const fn bits_in(len: usize) -> usize {
    match len.checked_mul(8) {
        Some(bits) => bits,
        None => usize::MAX & !7,
    }
}

/// `bits_in(len) / 8`: how many of a buffer's `len` bytes a reader may read, spelled as a
/// clamp so a bounds check against it folds into one compare.
#[inline]
const fn bytes_in(len: usize) -> usize {
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
fn extract_bits(buf: &[u8], pos: usize, n: usize, order: BitOrder) -> u128 {
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
trait Appendable {
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
fn emit_bits<B: Appendable>(out: &mut B, bit_pos: usize, value: u128, n: usize, order: BitOrder) {
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

/// A cursor that reads values at arbitrary bit offsets from a byte slice, in a
/// chosen [`BitOrder`] (MSB-first by default — `bit 0` is the high bit of byte 0,
/// the RFC/ETSI ASCII-art convention; LSB-first for serial/PHY layers).
///
/// # Examples
///
/// ```
/// use bnb::{BitReader, u4, u12};
///
/// let mut r = BitReader::new(&[0xAB, 0xCD]);
/// assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA)); // 4 bits
/// assert_eq!(r.read::<u12>().unwrap(), u12::new(0xBCD)); // the next 12, straddling a byte
/// assert_eq!(r.remaining_bits(), 0);
/// ```
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
    order: BitOrder,
    byte: ByteOrder,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

impl<'a> BitReader<'a> {
    /// Wraps `bytes`, positioned at bit 0, **MSB-first**, big-endian.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self::with_order(bytes, BitOrder::Msb)
    }

    /// Wraps `bytes`, positioned at bit 0, in the given bit order (big-endian).
    #[must_use]
    pub fn with_order(bytes: &'a [u8], order: BitOrder) -> Self {
        Self::with_layout(
            bytes,
            Layout {
                bit: order,
                byte: ByteOrder::Big,
            },
        )
    }

    /// Wraps `bytes`, positioned at bit 0, in the given [`Layout`] (bit + byte order).
    #[must_use]
    pub fn with_layout(bytes: &'a [u8], layout: Layout) -> Self {
        Self {
            bytes,
            bit_pos: 0,
            order: layout.bit,
            byte: layout.byte,
            depth: 0,
        }
    }

    /// The current absolute bit offset.
    #[must_use]
    pub fn bit_pos(&self) -> usize {
        self.bit_pos
    }

    /// Bits not yet consumed.
    #[must_use]
    pub fn remaining_bits(&self) -> usize {
        bits_in(self.bytes.len()) - self.bit_pos
    }

    /// Borrows and consumes the next `n` whole bytes without copying.
    ///
    /// This is the exact-span boundary for formats whose signed or hashed representation must be
    /// retained byte-for-byte. Unlike [`Source::read_bytes`], the returned slice aliases the
    /// original input and therefore exists only on the slice-backed reader.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotByteAligned`] when the cursor is between bytes,
    /// [`ErrorKind::UnexpectedEof`] when fewer than `n` bytes remain, or
    /// [`ErrorKind::PositionOverflow`] when the requested end cannot be represented.
    pub fn read_slice(&mut self, n: usize) -> Result<&'a [u8], BitError> {
        if self.bit_pos % 8 != 0 {
            return Err(BitError::new(ErrorKind::NotByteAligned, self.bit_pos));
        }
        let start = self.bit_pos / 8;
        let end = start
            .checked_add(n)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        if end > bits_in(self.bytes.len()) / 8 {
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: n.saturating_mul(8),
                    remaining: self.remaining_bits(),
                },
                self.bit_pos,
            ));
        }
        self.bit_pos = end * 8;
        Ok(&self.bytes[start..end])
    }

    /// Reads `n` (`<= 128`) bits into the low bits of a `u128`, in the reader's
    /// bit order (MSB-first by default).
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::UnexpectedEof`] if fewer
    /// than `n` bits remain. Either carries the current bit offset.
    // Forced inline: it is a bounds check, the byte-aligned fast path and two calls, and a
    // generated decoder passes a constant width, so the check and the path fold at each
    // field. Left to its heuristics the optimizer flipped between builds (DESIGN §19).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §19")]
    #[inline(always)]
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let n = n as usize;
        if n > 128 || n > self.remaining_bits() {
            return Err(self.read_error(n));
        }
        let acc = extract_bits(self.bytes, self.bit_pos, n, self.order);
        self.bit_pos += n;
        Ok(acc)
    }

    /// The error for a refused [`read_bits`](Self::read_bits) of `n` bits: too wide, or
    /// past the end. Out of line and cold so the inlined read carries one call for both.
    #[cold]
    #[inline(never)]
    fn read_error(&self, n: usize) -> BitError {
        let kind = if n > 128 {
            ErrorKind::TooWide { width: n }
        } else {
            ErrorKind::UnexpectedEof {
                needed: n,
                remaining: self.remaining_bits(),
            }
        };
        BitError::new(kind, self.bit_pos)
    }

    /// Reads one [`Bits`] value of its declared width, applying the byte order to a
    /// byte-multiple value.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    #[inline]
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        Source::read(self)
    }

    /// Moves the cursor to absolute bit `pos`. This needs no `Seek` trait — the whole
    /// buffer is in hand, so a seek is just cursor arithmetic. (Enables e.g. DNS
    /// name-compression pointers.)
    ///
    /// # Errors
    /// [`ErrorKind::UnexpectedEof`] if `pos` is past the end of the buffer.
    pub fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        let end = bits_in(self.bytes.len());
        if pos > end {
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: pos,
                    remaining: end,
                },
                self.bit_pos,
            ));
        }
        self.bit_pos = pos;
        Ok(())
    }

    /// Advances the cursor to the next byte boundary (a no-op if already aligned).
    pub fn align_to_byte(&mut self) {
        self.bit_pos = (self.bit_pos + 7) & !7;
    }
}

/// A sink that appends values at arbitrary bit offsets in a chosen [`BitOrder`]
/// (MSB-first by default), growing a byte buffer (the final partial byte is
/// zero-padded).
///
/// # Examples
///
/// ```
/// use bnb::{BitWriter, u4, u12};
///
/// let mut w = BitWriter::new();
/// w.write(u4::new(0xA)).unwrap();
/// w.write(u12::new(0xBCD)).unwrap();
/// assert_eq!(w.bit_len(), 16);
/// assert_eq!(w.into_bytes(), [0xAB, 0xCD]);
/// ```
#[derive(Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: usize,
    order: BitOrder,
    byte: ByteOrder,
    /// Optional per-encode scratch (see [`Sink::scratch`]); not part of the written bytes.
    scratch: Option<Box<dyn Any + Send>>,
}

// Hand-written so the public `Clone`/`Debug` API survives the (un-`Clone`, un-`Debug`)
// `scratch` slot. Cloning starts a fresh encode session, so the scratch is **not**
// carried (a clone gets `None`); `Debug` reports only its presence.
impl Clone for BitWriter {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            bit_pos: self.bit_pos,
            order: self.order,
            byte: self.byte,
            scratch: None,
        }
    }
}

impl fmt::Debug for BitWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitWriter")
            .field("bytes", &self.bytes)
            .field("bit_pos", &self.bit_pos)
            .field("order", &self.order)
            .field("byte", &self.byte)
            .field("scratch", &self.scratch.is_some())
            .finish()
    }
}

impl BitWriter {
    /// An empty **MSB-first**, big-endian writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty writer in the given bit order (big-endian).
    #[must_use]
    pub fn with_order(order: BitOrder) -> Self {
        Self::with_layout(Layout {
            bit: order,
            byte: ByteOrder::Big,
        })
    }

    /// An empty writer in the given [`Layout`] (bit + byte order).
    #[must_use]
    pub fn with_layout(layout: Layout) -> Self {
        Self {
            bytes: Vec::new(),
            bit_pos: 0,
            order: layout.bit,
            byte: layout.byte,
            scratch: None,
        }
    }

    /// Attach a type-erased **scratch** value, reachable from any codec during the encode
    /// via [`Sink::scratch`] and recovered by `Any::downcast_mut`.
    ///
    /// The escape hatch for codecs that need mutable state shared across a whole message's
    /// fields — a back-reference / compression dictionary (e.g. DNS name compression). The
    /// scratch lives for the encode and is dropped by [`into_bytes`](Self::into_bytes); it
    /// is never written to the output and is not carried by [`Clone`]. It must be [`Send`],
    /// so a writer holding one can still move to another thread or task.
    ///
    /// ```
    /// use bnb::{BitWriter, Sink};
    ///
    /// let mut w = BitWriter::new().with_scratch(Box::new(0u32));
    /// if let Some(n) = w.scratch().and_then(|s| s.downcast_mut::<u32>()) {
    ///     *n += 1;
    /// }
    /// assert_eq!(w.scratch().and_then(|s| s.downcast_ref::<u32>()), Some(&1));
    /// ```
    #[must_use]
    pub fn with_scratch(mut self, scratch: Box<dyn Any + Send>) -> Self {
        self.scratch = Some(scratch);
        self
    }

    /// Bits written so far.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.bit_pos
    }

    /// Appends the low `n` (`<= 128`) bits of `value`, in the writer's bit order.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::PositionOverflow`] if the output
    /// would hold more bits than `usize` can count.
    #[inline]
    pub fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n }, self.bit_pos));
        }
        let end = self
            .bit_pos
            .checked_add(n)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        emit_bits(&mut self.bytes, self.bit_pos, value, n, self.order);
        self.bit_pos = end;
        Ok(())
    }

    /// Appends one [`Bits`] value of its declared width, applying the byte order to
    /// a byte-multiple value.
    ///
    /// # Errors
    /// As [`write_bits`](Self::write_bits).
    #[inline]
    pub fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        Sink::write(self, value)
    }

    /// Consumes the writer, returning the packed bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// A bit-level **input** the codec recurses over. Implemented by [`BitReader`]
/// (in-memory slice), `StreamBitReader` (forward `Read`), `BufSource` (a
/// retain-and-seek socket adapter), and `SeekReader`/`BufSeekReader` (`Read + Seek`); the codec is
/// generic over `Source`, so one decoder runs over any of them — see
/// [`guide::io`](crate::guide::io).
///
/// # Examples
///
/// ```
/// use bnb::{BitReader, Source, u4};
///
/// // A reader generic over any `Source`.
/// fn first_nibble<S: Source>(s: &mut S) -> u4 { s.read().unwrap() }
///
/// let mut r = BitReader::new(&[0xA5]);
/// assert_eq!(first_nibble(&mut r), u4::new(0xA));
/// ```
pub trait Source: sealed::Sealed {
    /// Reads `n` (`<= 128`) bits into the low bits of a `u128`, in the source's
    /// bit order (MSB-first by default).
    ///
    /// # Errors
    /// Propagates the reader's [`BitError`].
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError>;

    /// How many generated element loops are open on this source, for
    /// [`MAX_DECODE_DEPTH`]. Required, not defaulted, so every source (wrappers included)
    /// keeps its own count or forwards to the one it wraps.
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16;

    /// The current absolute bit offset (for position-aware errors).
    fn bit_pos(&self) -> usize;

    /// The byte order applied to a byte-multiple value (default big-endian).
    fn byte_order(&self) -> ByteOrder {
        ByteOrder::Big
    }

    /// The bit order this source reads in (default MSB-first). Paired with
    /// [`byte_order`](Source::byte_order) to decide whether a byte-multiple value needs its
    /// bytes swapped — each bit order has a *natural* byte layout (big-endian under MSB,
    /// little-endian under LSB), and only the opposite declaration swaps.
    fn bit_order(&self) -> BitOrder {
        BitOrder::Msb
    }

    /// Moves the cursor to absolute bit `pos`. The default — for a forward-only
    /// source — fails with [`ErrorKind::NotSeekable`]; seekable sources (the slice
    /// [`BitReader`]) override it. A [`SeekSource`] guarantees this works.
    ///
    /// # Errors
    /// [`ErrorKind::NotSeekable`] unless the source is seekable.
    fn seek_to_bit(&mut self, _pos: usize) -> Result<(), BitError> {
        Err(BitError::new(ErrorKind::NotSeekable, self.bit_pos()))
    }

    /// Reads one [`Bits`] value of its declared width, applying the byte order.
    ///
    /// # Errors
    /// As [`read_bits`](Source::read_bits).
    #[inline]
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        read_by_bits(self)
    }

    /// Reads `n` bytes into a fresh `Vec` (at any bit offset — the bytes need not be
    /// aligned). The bulk form of the per-byte `read::<u8>()` loop, for blob/payload
    /// reads in custom codecs and container formats.
    ///
    /// `n` is often attacker-controlled, so **nothing is pre-allocated from it**: bytes
    /// are pushed as they are read, bounded by the input — a hostile huge `n` against a
    /// short source is a fast [`UnexpectedEof`](ErrorKind::UnexpectedEof), not an
    /// allocation. The default is the correct-first per-byte loop; the slice-backed sources
    /// ([`BitReader`], `BitBuf`, `BytesReader`, and a [`LimitedSource`] window over one of
    /// them) copy a byte-aligned, fully available run in one step and otherwise run that
    /// same loop, so values, errors, and the cursor match. The readers over `std::io::Read`
    /// (`StreamBitReader`, `BufSource`) ask their reader for a byte-aligned run in steps: at
    /// most 4 KiB before any of it has arrived, then as much again as has, with the values,
    /// errors and cursor of the loop.
    ///
    /// # Errors
    /// As [`read_bits`](Source::read_bits).
    fn read_bytes(&mut self, n: usize) -> Result<alloc::vec::Vec<u8>, BitError> {
        read_bytes_per_byte(self, n)
    }

    /// Fills `buf` with bytes from the source — the no-alloc dual of
    /// [`read_bytes`](Source::read_bytes), for fixed scratch buffers and tight
    /// `no_std` paths. Bulk-copied under the same conditions as `read_bytes`.
    ///
    /// # Errors
    /// As [`read_bits`](Source::read_bits). What `buf` holds after an error is unspecified.
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        read_into_per_byte(self, buf)
    }

    /// Consumes `n` bytes and drops them (at any bit offset):
    /// [`read_bytes`](Source::read_bytes) without the `Vec`. For a body this decoder does
    /// not parse: an unknown record kind, padding a length field covers, a field of a newer
    /// version. It works on a source that cannot seek, and it fails if the bytes are not
    /// there, where a seek past the end of the data succeeds on `BufSource`, `SeekReader`,
    /// `BufSeekReader` and `BytesReader`.
    ///
    /// No `Vec` is made for the bytes, whatever `n` says: a run already in memory is passed
    /// over in one step, and any other is read through a small fixed buffer. Each source
    /// otherwise does what a read of the same bytes does. `BufSource` keeps what it skips,
    /// as it keeps all it reads so that it can seek back, and past its cap the skip fails
    /// with [`BufferFull`](ErrorKind::BufferFull); `SeekReader` reads a byte at a time
    /// (`BufSeekReader` does not).
    ///
    /// ```
    /// use bnb::{BitReader, Source};
    ///
    /// let mut r = BitReader::new(&[1, 2, 3, 4, 5]);
    /// r.skip_bytes(3).unwrap();
    /// assert_eq!(r.read::<u8>().unwrap(), 4);
    /// assert!(r.skip_bytes(2).is_err()); // one byte is left
    /// ```
    ///
    /// # Errors
    /// As [`read_into`](Source::read_into), for the first part of the run that is not
    /// there. In a decode attempt on partial input (`BitBuf::try_pull`, `decode_prefix`,
    /// the transports) the retry signal carries the whole shortfall, as `read_bytes`'s
    /// does, also from inside a [`limit_bytes`](Source::limit_bytes) region.
    fn skip_bytes(&mut self, n: usize) -> Result<(), BitError> {
        skip_bytes_in_steps(self, n)
    }

    /// Consumes and lends the next `n` bytes when the cursor is byte-aligned and all of them
    /// are already in memory; otherwise `None`, consuming nothing. The hook behind bulk
    /// counted runs of byte-multiple integers ([`BitDecode::decode_vec`]), behind
    /// byte-string magic dispatch and behind the window a fixed-length message decodes
    /// from: a caller that gets `None` reads element by element, or byte by byte, so every
    /// error is that path's. A source that lends must be able to
    /// [`seek_to_bit`](Source::seek_to_bit) back over what it lent, because a caller that
    /// decides against a run (a magic that did not match, a message that failed in its
    /// window) rewinds. The default lends nothing; the sources that bulk-copy
    /// [`read_bytes`](Source::read_bytes) override it.
    #[doc(hidden)]
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        let _ = n;
        None
    }

    /// Lends this source through a window of exactly `n` bytes.
    ///
    /// Reads and seeks through the returned [`LimitedSource`] cannot cross the declared region.
    /// Call [`LimitedSource::finish`] after decoding a nested value to require exact consumption.
    /// This is useful for length-delimited containers: the child decoder cannot accidentally
    /// consume bytes belonging to its parent.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotByteAligned`] when the source is between bytes, or
    /// [`ErrorKind::PositionOverflow`] when the region's end cannot be represented.
    fn limit_bytes(&mut self, n: usize) -> Result<LimitedSource<'_, Self>, BitError>
    where
        Self: Sized,
    {
        LimitedSource::bytes(self, n)
    }

    /// Borrows this source as a [`std::io::Read`] over its bytes — for handing the
    /// cursor to `std::io`-based code from a `#[br(parse_with = …)]` (e.g. a decoder, or
    /// a `Read`-based parser). Reads 8 bits per byte; see [`SourceReader`]. Only with
    /// the `std` feature.
    #[cfg(feature = "std")]
    fn as_read(&mut self) -> SourceReader<'_, Self>
    where
        Self: Sized,
    {
        SourceReader(self)
    }
}

/// [`Source::read`] for any source and width: the bits, then the byte order.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
fn read_by_bits<S: Source + ?Sized, T: Bits>(s: &mut S) -> Result<T, BitError> {
    let raw = s.read_bits(T::BITS)?;
    Ok(T::from_bits(value_byte_order::<T>(
        raw,
        s.bit_order(),
        s.byte_order(),
    )))
}

// The per-byte loops behind the defaulted bulk methods. A bulk override takes its fast path
// only when the whole run is byte-aligned and available, and otherwise calls these, so every
// error, position, and partial-consumption effect is the per-byte loop's by construction.
fn read_bytes_per_byte<S: Source + ?Sized>(s: &mut S, n: usize) -> Result<Vec<u8>, BitError> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(s.read::<u8>()?);
    }
    Ok(v)
}

/// [`Source::skip_bytes`] for any source: a run it lends is passed over whole, and any other
/// is read through a buffer on the stack, a step at a time, with that source's own bulk
/// `read_into`.
fn skip_bytes_in_steps<S: Source + ?Sized>(s: &mut S, n: usize) -> Result<(), BitError> {
    if s.aligned_run(n).is_some() {
        return Ok(());
    }
    let mut dropped = [0; 256];
    let mut left = n;
    while left > 0 {
        let step = left.min(dropped.len());
        s.read_into(&mut dropped[..step])?;
        left -= step;
    }
    Ok(())
}

// Out of line: it is the fallback of every bulk `read_into`, and folded into a fixed
// `[u8; N]` read it is unrolled N times around the one copy that read wants.
#[inline(never)]
fn read_into_per_byte<S: Source + ?Sized>(s: &mut S, buf: &mut [u8]) -> Result<(), BitError> {
    for slot in buf.iter_mut() {
        *slot = s.read::<u8>()?;
    }
    Ok(())
}

/// [`Sink::write`] for any sink and width: the byte order, then the bits.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
fn write_by_bits<K: Sink + ?Sized, T: Bits>(k: &mut K, value: T) -> Result<(), BitError> {
    let raw = value_byte_order::<T>(value.into_bits(), k.bit_order(), k.byte_order());
    k.write_bits(raw, T::BITS)
}

/// Whether a `T` written at `bit_pos` is whole bytes from a byte-aligned cursor, the case
/// [`append_whole_bytes`] serves.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
const fn writes_whole_bytes<T: Bits>(bit_pos: usize) -> bool {
    T::BITS % 8 == 0 && T::BITS <= 128 && bit_pos % 8 == 0
}

/// Hands `append` the bytes a whole-byte `value` puts on the wire. Aligned whole bytes
/// are the value big-endian or little-endian by the effective byte order alone, whatever
/// the bit order (the mirror of `BitReader::take_whole_bytes`), so an appending writer
/// stores them in one step.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
fn append_whole_bytes<T: Bits>(value: T, byte: ByteOrder, append: impl FnOnce(&[u8])) {
    debug_assert!(T::BITS % 8 == 0 && T::BITS <= 128);
    let n = (T::BITS / 8) as usize;
    let raw = value.into_bits();
    match T::__BNB_BYTE_ORDER.unwrap_or(byte) {
        ByteOrder::Big => append(&raw.to_be_bytes()[16 - n..]),
        ByteOrder::Little => append(&raw.to_le_bytes()[..n]),
    }
}

/// The error for a write that would move a writer at `bit_pos` past what `usize` counts.
#[cold]
#[inline(never)]
fn position_overflow(bit_pos: usize) -> BitError {
    BitError::new(ErrorKind::PositionOverflow, bit_pos)
}

fn write_bytes_per_byte<K: Sink + ?Sized>(k: &mut K, bytes: &[u8]) -> Result<(), BitError> {
    for &b in bytes {
        k.write(b)?;
    }
    Ok(())
}

// Runs a bulk read on a fresh slice reader at `*cursor`, then stores where it stopped — also
// after a failure, as the per-byte default leaves the cursor past every byte it read.
fn read_via_slice<T>(
    bytes: &[u8],
    layout: Layout,
    cursor: &mut usize,
    read: impl FnOnce(&mut BitReader<'_>) -> Result<T, BitError>,
) -> Result<T, BitError> {
    let mut r = BitReader::with_layout(bytes, layout);
    r.seek_to_bit(*cursor)?;
    let result = read(&mut r);
    *cursor = r.bit_pos();
    result
}

/// [`BitReader::aligned_run`] for a source that keeps its own `cursor` into `bytes`: the
/// next `n` bytes, with the cursor moved past them, when it is byte-aligned and they are all
/// there. A cursor past the end (a `BytesReader` may seek there) has no run.
fn aligned_run_at<'a>(bytes: &'a [u8], cursor: &mut usize, n: usize) -> Option<&'a [u8]> {
    let mut r = BitReader::new(bytes);
    r.seek_to_bit(*cursor).ok()?;
    let run = r.aligned_run(n)?;
    *cursor = r.bit_pos();
    Some(run)
}

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
    fn bytes(source: &'a mut S, n: usize) -> Result<Self, BitError> {
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

/// A [`std::io::Read`] view over a [`Source`], from [`Source::as_read`]. Each `read`
/// pulls 8 bits per byte through [`Source::read_bits`], so it works at any bit
/// alignment (you will normally be byte-aligned). A read failure surfaces as an
/// `io::Error` when no bytes were produced, or ends the read short once some were — the
/// `std::io` convention. This is the outbound dual of [`BufSource`]/[`SeekReader`] (which
/// adapt a `std::io::Read` *into* a `Source`).
#[cfg(feature = "std")]
pub struct SourceReader<'a, S: Source>(&'a mut S);

#[cfg(feature = "std")]
impl<S: Source> std::io::Read for SourceReader<'_, S> {
    #[allow(clippy::cast_possible_truncation)] // read_bits(8) yields at most one byte.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        for (i, slot) in buf.iter_mut().enumerate() {
            match self.0.read_bits(8) {
                Ok(b) => *slot = b as u8,
                Err(e) if i == 0 => {
                    // Map the actual failure — only a genuine end-of-input is `UnexpectedEof`;
                    // a non-EOF error (e.g. `NotSeekable`, `Convert`) must not masquerade as one.
                    let kind = match e.kind {
                        ErrorKind::UnexpectedEof { .. } | ErrorKind::Incomplete { .. } => {
                            std::io::ErrorKind::UnexpectedEof
                        }
                        _ => std::io::ErrorKind::InvalidData,
                    };
                    return Err(std::io::Error::new(kind, e));
                }
                Err(_) => return Ok(i),
            }
        }
        Ok(buf.len())
    }
}

/// A [`Source`] that can seek (its [`seek_to_bit`](Source::seek_to_bit) is real, not
/// the failing default). A `#[bin]` message that uses `restore_position` bounds its
/// generated `decode` on this trait, so a forward-only stream is rejected at
/// compile time. Implemented by [`BitReader`], `BufSource`, `SeekReader`, and `BufSeekReader`
/// (and, with the `bytes` feature, `BytesReader`).
pub trait SeekSource: Source {}

impl SeekSource for BitReader<'_> {}

// Only this backing reader turns physical exhaustion into a retry signal. In particular,
// an enclosing LimitedSource's logical boundary errors must remain definitive.
struct IncrementalReader<'a>(BitReader<'a>);

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

/// A bit-level **output** the codec writes to — the in-memory [`BitWriter`]
/// (and, under the `bytes` feature, `BytesWriter`). Encode to any
/// `std::io::Write` via the `EncodeExt::encode` extension method (with `std`).
///
/// # Examples
///
/// ```
/// use bnb::{BitWriter, Sink, u4};
///
/// // A writer generic over any `Sink`.
/// fn put_nibble<K: Sink>(k: &mut K, v: u4) { k.write(v).unwrap(); }
///
/// let mut w = BitWriter::new();
/// put_nibble(&mut w, u4::new(0xA));
/// put_nibble(&mut w, u4::new(0x5));
/// assert_eq!(w.into_bytes(), [0xA5]);
/// ```
pub trait Sink: sealed::Sealed {
    /// Appends the low `n` (`<= 128`) bits of `value`, in the sink's bit order
    /// (MSB-first by default).
    ///
    /// # Errors
    /// Propagates the writer's [`BitError`].
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError>;

    /// The number of bits written so far.
    fn bit_pos(&self) -> usize;

    /// The byte order applied to a byte-multiple value (default big-endian).
    fn byte_order(&self) -> ByteOrder {
        ByteOrder::Big
    }

    /// The bit order this sink writes in (default MSB-first). Paired with
    /// [`byte_order`](Sink::byte_order) exactly as on [`Source`]: each bit order has a
    /// *natural* byte layout (big-endian under MSB, little-endian under LSB), and only the
    /// opposite declaration swaps a byte-multiple value.
    fn bit_order(&self) -> BitOrder {
        BitOrder::Msb
    }

    /// A type-erased, **encode-scoped scratch** value for codecs that need mutable state
    /// shared across a whole message's fields — a back-reference / compression dictionary
    /// (e.g. DNS name compression). Recover the concrete type with
    /// `Any::downcast_mut`.
    ///
    /// Returns `None` unless the sink was built carrying one (see
    /// [`BitWriter::with_scratch`]); the default sink has none. The scratch is the shared
    /// thread the `Sink` already provides — since one sink is passed by `&mut` through
    /// every field's encode, a value stored here is visible to them all.
    fn scratch(&mut self) -> Option<&mut dyn Any> {
        None
    }

    /// Appends one [`Bits`] value of its declared width, applying the byte order.
    ///
    /// # Errors
    /// As [`write_bits`](Sink::write_bits).
    #[inline]
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        write_by_bits(self, value)
    }

    /// Appends a run of bytes — the bulk dual of [`Source::read_bytes`], replacing the
    /// per-byte `write(b)?` loop in custom codecs. Works at any bit offset. [`BitWriter`]
    /// (and `BytesWriter`) append a byte-aligned run in one copy and otherwise run the same
    /// per-byte loop as this default.
    ///
    /// # Errors
    /// As [`write_bits`](Sink::write_bits).
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
        write_bytes_per_byte(self, bytes)
    }

    /// Appends `n` zero bytes and lends them for filling, when the cursor is byte-aligned
    /// and the longer output's bits can still be counted; otherwise `None`, writing nothing.
    /// The hook behind bulk counted runs of byte-multiple integers
    /// ([`BitEncode::encode_slice`]): a caller that gets `None` writes the run element by
    /// element, so every error is that path's. The default lends nothing; the sinks that
    /// bulk-copy [`write_bytes`](Sink::write_bytes) override it.
    #[doc(hidden)]
    fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
        let _ = n;
        None
    }

    /// Borrows this sink as a [`std::io::Write`] — the dual of [`Source::as_read`], for
    /// handing the cursor to `std::io`-based code from a `#[bw(write_with = …)]`. Writes 8
    /// bits per byte; see [`SinkWriter`]. Only with the `std` feature.
    #[cfg(feature = "std")]
    fn as_write(&mut self) -> SinkWriter<'_, Self>
    where
        Self: Sized,
    {
        SinkWriter(self)
    }
}

/// A [`std::io::Write`] view over a [`Sink`], from [`Sink::as_write`]. Each `write`
/// pushes 8 bits per byte through [`Sink::write_bits`]. The outbound dual of
/// [`SourceReader`]; `flush` is a no-op (the sink owns its buffer).
#[cfg(feature = "std")]
pub struct SinkWriter<'a, K: Sink>(&'a mut K);

#[cfg(feature = "std")]
impl<K: Sink> std::io::Write for SinkWriter<'_, K> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &b in buf {
            self.0
                .write_bits(u128::from(b), 8)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl sealed::Sealed for BitReader<'_> {}

impl Source for BitReader<'_> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    // Forced for the same reason as the inherent method it forwards to.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §19")]
    #[inline(always)]
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        BitReader::read_bits(self, n)
    }
    // Forced inline: this is the call a generated decoder makes for each scalar field, and
    // out of line it costs a call and an 80-byte `Result` through memory (DESIGN §22).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        if !self.at_whole_bytes::<T>() {
            return read_by_bits(self);
        }
        let Some(value) = self.take_whole_bytes() else {
            return Err(self.read_error(T::BITS as usize));
        };
        Ok(value)
    }
    #[inline]
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    #[inline]
    fn byte_order(&self) -> ByteOrder {
        self.byte
    }
    #[inline]
    fn bit_order(&self) -> BitOrder {
        self.order
    }
    #[inline]
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        BitReader::seek_to_bit(self, pos)
    }
    #[inline]
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        match self.aligned_run(n) {
            Some(run) => Ok(run.to_vec()),
            None => read_bytes_per_byte(self, n),
        }
    }
    #[inline]
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        match self.aligned_run(buf.len()) {
            Some(run) => {
                buf.copy_from_slice(run);
                Ok(())
            }
            None => read_into_per_byte(self, buf),
        }
    }
    #[inline]
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        BitReader::aligned_run(self, n)
    }
}

impl<'a> BitReader<'a> {
    /// Whether a `T` read here is whole bytes from a byte-aligned cursor, the case
    /// [`take_whole_bytes`](Self::take_whole_bytes) serves.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn at_whole_bytes<T: Bits>(&self) -> bool {
        T::BITS % 8 == 0 && T::BITS <= 128 && self.bit_pos % 8 == 0
    }

    /// Reads a `T` as [`at_whole_bytes`](Self::at_whole_bytes) allows; `None`, consuming
    /// nothing, when fewer bytes remain. Aligned whole bytes are the value big-endian or
    /// little-endian whatever the bit order (each order's natural layout, swapped or not:
    /// `apply_byte_order`), so this is one load and at most a swap.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn take_whole_bytes<T: Bits>(&mut self) -> Option<T> {
        debug_assert!(self.at_whole_bytes::<T>());
        let n = (T::BITS / 8) as usize;
        let start = self.bit_pos / 8;
        let end = bytes_in(self.bytes.len());
        let run = self.bytes[..end].get(start..start + n)?;
        let mut wide = [0; 16];
        let raw = match T::__BNB_BYTE_ORDER.unwrap_or(self.byte) {
            ByteOrder::Big => {
                wide[16 - n..].copy_from_slice(run);
                u128::from_be_bytes(wide)
            }
            ByteOrder::Little => {
                wide[..n].copy_from_slice(run);
                u128::from_le_bytes(wide)
            }
        };
        self.bit_pos += T::BITS as usize;
        Some(T::from_bits(raw))
    }

    // Consumes the next `n` bytes when the cursor is byte-aligned and all of them remain.
    // Whole aligned bytes read identically in both bit orders, so this is a plain copy.
    #[inline]
    fn aligned_run(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.bit_pos % 8 != 0 {
            return None;
        }
        let start = self.bit_pos / 8;
        let end = start
            .checked_add(n)
            .filter(|&end| end <= bits_in(self.bytes.len()) / 8)?;
        let run = self.bytes.get(start..end)?;
        self.bit_pos = end * 8;
        Some(run)
    }
}

impl sealed::Sealed for BitWriter {}

impl Sink for BitWriter {
    #[inline]
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        BitWriter::write_bits(self, value, n)
    }
    // Forced inline: this is the call a generated encoder makes for each scalar field. A
    // whole-byte value at a byte-aligned cursor, which is always the end of the buffer, is
    // appended in one step; any other write inlines the bit path (DESIGN §22).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        if !writes_whole_bytes::<T>(self.bit_pos) {
            return write_by_bits(self, value);
        }
        let Some(end) = self.bit_pos.checked_add(T::BITS as usize) else {
            return Err(position_overflow(self.bit_pos));
        };
        // An aligned cursor is the end of the buffer: no write leaves it anywhere else.
        debug_assert_eq!(self.bit_pos / 8, self.bytes.len());
        let out = &mut self.bytes;
        append_whole_bytes(value, self.byte, |bytes| out.extend_from_slice(bytes));
        self.bit_pos = end;
        Ok(())
    }
    #[inline]
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    #[inline]
    fn byte_order(&self) -> ByteOrder {
        self.byte
    }
    #[inline]
    fn bit_order(&self) -> BitOrder {
        self.order
    }
    #[inline]
    fn scratch(&mut self) -> Option<&mut dyn Any> {
        // The stored box is `dyn Any + Send`; the trait hands out plain `dyn Any`.
        self.scratch
            .as_deref_mut()
            .map(|scratch| scratch as &mut dyn Any)
    }
    // An aligned cursor sits exactly at the end of the buffer, and whole aligned bytes write
    // identically in both bit orders, so the run is appended as-is. It fails only when the
    // output would hold more bits than `usize` can count.
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
        if self.bit_pos % 8 != 0 {
            return write_bytes_per_byte(self, bytes);
        }
        debug_assert_eq!(self.bit_pos / 8, self.bytes.len());
        let end = bytes
            .len()
            .checked_mul(8)
            .and_then(|bits| self.bit_pos.checked_add(bits))
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        self.bytes.extend_from_slice(bytes);
        self.bit_pos = end;
        Ok(())
    }
    // As `write_bytes`: an aligned cursor sits at the end of the buffer, so the tail is new.
    #[inline]
    fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
        if self.bit_pos % 8 != 0 {
            return None;
        }
        let end = n
            .checked_mul(8)
            .and_then(|bits| self.bit_pos.checked_add(bits))?;
        debug_assert_eq!(self.bit_pos / 8, self.bytes.len());
        let start = self.bytes.len();
        self.bytes.resize(start + n, 0);
        self.bit_pos = end;
        Some(&mut self.bytes[start..])
    }
}

/// A message decoded from a bit stream — the recursion point a
/// `#[derive(BitDecode)]` struct implements (reading each field in declaration
/// order). Leaf fields are any [`Bits`] type; nested messages recurse. Fixed- or
/// variable-length; a fixed-length message *also* implements [`FixedBitLen`].
///
/// Most users reach for [`#[bin]`](macro@crate::bin) (which derives this plus
/// [`BitEncode`] and a builder); the bare derives are the codec on its own, for fields
/// that straddle byte boundaries.
///
/// # Examples
///
/// ```
/// use bnb::{BitDecode, BitEncode, u4, u12};
///
/// // A 4-bit tag + a 12-bit length, straddling the byte boundary.
/// #[derive(BitDecode, BitEncode, Debug, PartialEq)]
/// struct Frame { tag: u4, len: u12 }
///
/// let f = Frame::decode_exact(&[0xAB, 0xCD]).unwrap();
/// assert_eq!(f, Frame { tag: u4::new(0xA), len: u12::new(0xBCD) });
/// assert_eq!(f.to_bytes().unwrap(), [0xAB, 0xCD]); // round-trips
/// ```
pub trait BitDecode: Sized {
    /// Decodes `Self` from any [`Source`], advancing its cursor.
    ///
    /// # Errors
    /// Propagates the source's [`BitError`].
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError>;

    /// Decode `count` consecutive elements. Used by generated context-free counted fields.
    /// The default grows only after each successful, forward-moving element; `u8` uses
    /// [`Source::read_bytes`] so buffered byte payloads can check availability before allocating.
    /// `u16`, `u32`, `u64` and `u128` convert a byte-aligned run that is wholly in memory in
    /// one pass, allocating once those bytes are known to be there, and otherwise read per
    /// element like the default.
    /// An override must return **exactly** `count` elements in wire order, preserve element
    /// decoding, and advance for every nonempty result. It must not allocate from an
    /// untrusted count before proving the corresponding input is available. Wrappers
    /// using the default `Source::read_bytes` retain its per-byte allocation behavior.
    ///
    /// # Errors
    /// A source/element error, or [`ErrorKind::NoProgress`] for a zero-width element.
    /// Like [`bit_decode`](Self::bit_decode), this direct source operation is not transactional;
    /// use a `BitBuf` attempt for rollback of buffered input.
    ///
    /// The default counts one level of [`MAX_DECODE_DEPTH`] for a non-empty `Vec`, because a
    /// message can contain itself only this way; the crate's leaf types (integers, `bool`,
    /// `uN`, and what `#[bitfield]`, `BitEnum` and `#[bitflags]` generate) override it and are
    /// never charged.
    fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
        nested_vec(r, count, |r| decode_vec_each(r, count))
    }
}

/// The element-by-element loop behind [`BitDecode::decode_vec`]'s default (which runs it one
/// depth level down), what leaf types decode with, and what every bulk override falls back
/// to and is tested against: it grows only after each successful, forward-moving element.
#[doc(hidden)]
#[inline]
pub fn decode_vec_each<T: BitDecode, S: Source>(
    r: &mut S,
    count: usize,
) -> Result<Vec<T>, BitError> {
    let mut values = Vec::new();
    for _ in 0..count {
        let start = r.bit_pos();
        let value = T::bit_decode(r)?;
        if r.bit_pos() <= start {
            return Err(BitError::new(ErrorKind::NoProgress, start));
        }
        values.push(value);
    }
    Ok(values)
}

/// A slice reader over the next `N` bytes, when the source lends them (a byte-aligned
/// cursor, all of them in memory), in the source's layout and at its nesting depth; the
/// source's cursor moves past them. A fixed-length message decodes its fields from this
/// reader with one availability check for the whole message: its length is the constant
/// `N`, so each field's bounds check and alignment test fold.
///
/// `N` only sizes the window. The generated caller moves the source's cursor to where the
/// fields actually stopped, and on any failure inside the window puts it back and decodes
/// field by field, so every error a message reports is that path's.
#[doc(hidden)]
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub fn fixed_window<S: Source, const N: usize>(r: &mut S) -> Option<BitReader<'_>> {
    let (order, byte) = (r.bit_order(), r.byte_order());
    let depth = *r.__bnb_depth();
    let bytes = r.aligned_run(N)?.first_chunk::<N>()?;
    Some(BitReader {
        bytes,
        bit_pos: 0,
        order,
        byte,
        depth,
    })
}

/// Runs the loop decoding `count` elements one level deeper, failing with
/// [`ErrorKind::TooDeep`] past [`MAX_DECODE_DEPTH`]; an empty `Vec` enters nothing. The
/// previous depth is restored whatever `f` returns, so a source that outlives a failed
/// decode is not charged for it.
#[doc(hidden)]
#[inline]
pub fn nested_vec<S: Source, T>(
    r: &mut S,
    count: usize,
    f: impl FnOnce(&mut S) -> Result<Vec<T>, BitError>,
) -> Result<Vec<T>, BitError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let depth = *r.__bnb_depth();
    if u32::from(depth) >= MAX_DECODE_DEPTH {
        return Err(too_deep(r.bit_pos()));
    }
    *r.__bnb_depth() = depth + 1;
    let out = f(r);
    *r.__bnb_depth() = depth;
    out
}

#[cold]
#[inline(never)]
fn too_deep(at: usize) -> BitError {
    BitError::new(ErrorKind::TooDeep, at)
}

/// The element-by-element loop behind [`BitEncode::encode_slice`]'s default, and what every
/// bulk override falls back to and is tested against.
#[inline]
fn encode_slice_each<T: BitEncode, K: Sink>(items: &[T], w: &mut K) -> Result<(), BitError> {
    for item in items {
        item.bit_encode(w)?;
    }
    Ok(())
}

/// [`BitDecode::decode_vec`] for an `N`-byte integer: a byte-aligned run that is all in
/// memory is converted in one pass, `be` or `le` by the source's byte order, into a `Vec`
/// allocated only once [`Source::aligned_run`] has shown the bytes are there. Whole aligned
/// bytes read the same in both bit orders, so the bit order does not enter. Any other run
/// (unaligned, short, or a count whose byte length overflows) is [`decode_vec_each`]'s.
#[inline]
fn decode_int_run<T: BitDecode, S: Source, const N: usize>(
    r: &mut S,
    count: usize,
    be: impl Fn([u8; N]) -> T,
    le: impl Fn([u8; N]) -> T,
) -> Result<Vec<T>, BitError> {
    let big = r.byte_order() == ByteOrder::Big;
    let Some(run) = count.checked_mul(N).and_then(|bytes| r.aligned_run(bytes)) else {
        return decode_vec_each(r, count);
    };
    let wire = run
        .chunks_exact(N)
        .map(|chunk| <[u8; N]>::try_from(chunk).expect("chunks_exact yields N bytes"));
    Ok(if big {
        wire.map(be).collect()
    } else {
        wire.map(le).collect()
    })
}

/// [`BitEncode::encode_slice`] for an `N`-byte integer, the dual of [`decode_int_run`]: on
/// a byte-aligned sink the run is written in one pass into the tail [`Sink::aligned_tail`]
/// lends, `be` or `le` by the sink's byte order. Any other run is [`encode_slice_each`]'s.
#[inline]
fn encode_int_run<T: BitEncode + Copy, K: Sink, const N: usize>(
    items: &[T],
    w: &mut K,
    be: impl Fn(T) -> [u8; N],
    le: impl Fn(T) -> [u8; N],
) -> Result<(), BitError> {
    let big = w.byte_order() == ByteOrder::Big;
    let Some(tail) = items
        .len()
        .checked_mul(N)
        .and_then(|bytes| w.aligned_tail(bytes))
    else {
        return encode_slice_each(items, w);
    };
    let wire = tail.chunks_exact_mut(N).zip(items);
    if big {
        wire.for_each(|(slot, &item)| slot.copy_from_slice(&be(item)));
    } else {
        wire.for_each(|(slot, &item)| slot.copy_from_slice(&le(item)));
    }
    Ok(())
}

/// A message whose encoded length is a **compile-time constant** — i.e. it has no
/// variable-length (`count`-driven `Vec`) field. The derive implements this only
/// for fixed messages; it sizes a fixed byte region when the message is embedded
/// as a field in another message (its contribution to the parent's width). A
/// `count`-bearing message implements [`BitDecode`]/[`BitEncode`] but **not** this.
/// `Bits` leaves also implement it (their `BIT_LEN` is `Bits::BITS`), so a field's
/// width is computed uniformly whether it's a leaf or a nested message.
pub trait FixedBitLen {
    /// Total encoded width of the message in bits — the sum of its fields' widths.
    const BIT_LEN: u32;

    /// How this type reads, for a parent deciding whether to decode from a window of its
    /// whole length ([`fixed_window`]): [`window_kind`]'s `LEAF` for a value read in one
    /// `read` of `BIT_LEN` bits, `WHOLE` for a generated message every field of which
    /// suits a window, and the default, unknown, for anything else (a hand-written
    /// impl), which keeps a parent decoding field by field.
    #[doc(hidden)]
    const __BNB_WINDOW: u8 = WINDOW_UNKNOWN;
}

/// [`FixedBitLen::__BNB_WINDOW`]: nothing is known about how the type reads.
const WINDOW_UNKNOWN: u8 = 0;
/// [`FixedBitLen::__BNB_WINDOW`]: one `read` of the type's `BIT_LEN` bits.
#[doc(hidden)]
pub const WINDOW_LEAF: u8 = 1;
/// [`FixedBitLen::__BNB_WINDOW`]: a message whose fields all suit a window.
#[doc(hidden)]
pub const WINDOW_WHOLE: u8 = 2;

/// Whether a fixed-length message suits a window of its own length, from its fields'
/// `(width in bits, kind)` in order: [`WINDOW_WHOLE`] if every field is a leaf of at
/// most eight bits, a leaf of whole bytes that starts on a byte boundary, or a nested
/// message that itself suits a window and starts on one; otherwise unknown.
///
/// The rule leaves out a wider field that starts or ends inside a byte. Such a read is
/// fastest with sixteen bytes behind it (DESIGN §22, row 3), and a window the message's
/// own length takes them away, so a message with one decodes field by field as before.
#[doc(hidden)]
#[must_use]
pub const fn window_kind(fields: &[(u32, u8)]) -> u8 {
    let (mut at, mut index) = (0u32, 0);
    while index < fields.len() {
        let (width, kind) = fields[index];
        let aligned = at % 8 == 0;
        let suits = match kind {
            WINDOW_LEAF => width <= 8 || (width % 8 == 0 && aligned),
            WINDOW_WHOLE => aligned,
            _ => false,
        };
        if !suits {
            return WINDOW_UNKNOWN;
        }
        at = at.wrapping_add(width);
        index += 1;
    }
    WINDOW_WHOLE
}

/// A message encoded to a bit stream — the dual of [`BitDecode`].
///
/// Encoding has two forms: the required [`bit_encode`](Self::bit_encode)
/// is **verbatim** (exactly what's stored), and [`canonical_bit_encode`](Self::canonical_bit_encode)
/// is **canonical** (`reserved` → spec value, `calc` → recomputed). The default canonical
/// impl just calls `bit_encode`, so the two are identical unless a `#[bin]` message has a
/// `reserved` or non-`temp` `calc` field — in which case the derive overrides it.
pub trait BitEncode {
    /// The message's bit/byte order, used to size a fresh [`BitWriter`] when
    /// encoding to a `Vec`/writer. The derive sets it from the struct's declared
    /// `bit_order`/`bytes`; a hand-written impl that only ever encodes into a
    /// caller-supplied [`Sink`] can leave the default.
    const LAYOUT: Layout = Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Big,
    };

    /// Encodes `self` **verbatim** into any [`Sink`], advancing its cursor.
    ///
    /// # Errors
    /// Propagates the sink's [`BitError`].
    fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError>;

    /// Encodes `self`'s **canonical** form into any [`Sink`]: `reserved` fields as their
    /// spec value, `calc` fields recomputed. Defaults to [`bit_encode`](Self::bit_encode)
    /// (verbatim == canonical) for messages with no `reserved`/`calc` field.
    ///
    /// # Errors
    /// Propagates the sink's [`BitError`].
    fn canonical_bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
        self.bit_encode(w)
    }

    /// Encode every element of `items` verbatim, in order — the encode dual of
    /// [`BitDecode::decode_vec`], used by generated context-free `Vec` fields. The default
    /// calls [`bit_encode`](Self::bit_encode) per element; `u8` uses [`Sink::write_bytes`],
    /// which a byte-aligned [`BitWriter`] appends in one copy, and `u16`, `u32`, `u64` and
    /// `u128` write a byte-aligned run into it in one pass. An override must write exactly
    /// the bits, and fail with exactly the error, of that per-element loop.
    ///
    /// # Errors
    /// The first element's or sink's [`BitError`].
    fn encode_slice<K: Sink>(items: &[Self], w: &mut K) -> Result<(), BitError>
    where
        Self: Sized,
    {
        encode_slice_each(items, w)
    }
}

// A `Bits` leaf (a `uN`, a `#[bitfield]`, a `BitEnum`/`#[bitflags]`) is *also* field-codable:
// it decodes by reading its `BITS` bits and encodes by writing them. This lets `#[bin]` treat
// **every** field uniformly through `bit_decode`/`bit_encode`, so it needs no `#[nested]` marker
// to choose between "read bits" and "recurse into a message". (The `Bits` packing role — the
// reason these types exist — is untouched; this only *adds* the stream-codec impls.) No blanket
// `impl<T: Bits>` is possible (it would collide with the per-message derives under coherence),
// so the leaves are covered concretely here and the macros emit one for each user `Bits` type.
macro_rules! bits_leaf_codec {
    ($($t:ident),* $(,)?) => {$(
        impl BitDecode for $t {
            #[inline]
            fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
                r.read::<$t>()
            }
            bits_leaf_codec!(@decode_bytes $t);
        }
        impl BitEncode for $t {
            #[inline]
            fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
                w.write(*self)
            }
            bits_leaf_codec!(@encode_bytes $t);
        }
        // A leaf's fixed width is its `Bits::BITS`, so `#[bin]` can size it the same way it
        // sizes a fixed nested message — uniformly via `FixedBitLen`.
        impl FixedBitLen for $t {
            const BIT_LEN: u32 = <$t as Bits>::BITS;
            const __BNB_WINDOW: u8 = WINDOW_LEAF;
        }
    )*};
    // A `u8` run is a byte run: the sources/sinks bulk-copy it when byte-aligned. Matching the
    // impl (not the field's spelling) reaches aliases and generic element types too.
    (@decode_bytes u8) => {
        #[inline]
        fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
            r.read_bytes(count)
        }
    };
    // A leaf cannot contain itself, so its runs are not charged against the depth budget.
    (@decode_bytes bool) => {
        #[inline]
        fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
            decode_vec_each(r, count)
        }
    };
    // A wider integer is whole bytes too: a byte-aligned run is converted in one pass.
    (@decode_bytes $int:ident) => {
        #[inline]
        fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
            decode_int_run(r, count, <$int>::from_be_bytes, <$int>::from_le_bytes)
        }
    };
    (@encode_bytes u8) => {
        #[inline]
        fn encode_slice<K: Sink>(items: &[Self], w: &mut K) -> Result<(), BitError> {
            w.write_bytes(items)
        }
    };
    (@encode_bytes bool) => {};
    (@encode_bytes $int:ident) => {
        #[inline]
        fn encode_slice<K: Sink>(items: &[Self], w: &mut K) -> Result<(), BitError> {
            encode_int_run(items, w, <$int>::to_be_bytes, <$int>::to_le_bytes)
        }
    };
}
bits_leaf_codec!(u8, u16, u32, u64, u128, bool);

// `std::net` address types as `#[bin]` fields — the network-codec convenience the protocols
// dogfooding surfaced (IPv4 headers otherwise model addresses as a raw `u32`). An address
// serializes as its `to_bits`/`from_bits` integer, so it follows the struct's byte order like
// any other integer field: in a `#[bin(big)]` message that's the octets in network order
// (`192.168.1.1` → `C0 A8 01 01`), which is what every real protocol wants. `std` only, since
// the types are `std::net`.
macro_rules! ip_addr_codec {
    ($($t:ty => $int:ty, $bits:expr);* $(;)?) => {$(
        #[cfg(feature = "std")]
        impl BitDecode for $t {
            #[inline]
            fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
                Ok(<$t>::from_bits(r.read::<$int>()?))
            }
            #[inline]
            fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
                decode_vec_each(r, count)
            }
        }
        #[cfg(feature = "std")]
        impl BitEncode for $t {
            #[inline]
            fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
                w.write(self.to_bits())
            }
        }
        #[cfg(feature = "std")]
        impl FixedBitLen for $t {
            const BIT_LEN: u32 = $bits;
            const __BNB_WINDOW: u8 = WINDOW_LEAF;
        }
    )*};
}
ip_addr_codec!(
    std::net::Ipv4Addr => u32, 32;
    std::net::Ipv6Addr => u128, 128;
);

impl<T, const N: usize> BitDecode for crate::int::UInt<T, N>
where
    crate::int::UInt<T, N>: Bits,
{
    #[inline]
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
        r.read::<Self>()
    }

    #[inline]
    fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
        decode_vec_each(r, count)
    }
}

impl<T, const N: usize> BitEncode for crate::int::UInt<T, N>
where
    crate::int::UInt<T, N>: Bits,
{
    #[inline]
    fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
        w.write(*self)
    }
}

impl<T, const N: usize> FixedBitLen for crate::int::UInt<T, N>
where
    crate::int::UInt<T, N>: Bits,
{
    const BIT_LEN: u32 = <Self as Bits>::BITS;
    const __BNB_WINDOW: u8 = WINDOW_LEAF;
}

/// Seals [`Source`], [`Sink`], [`BitAmount`], [`CountPrefix`] and
/// [`codecs::leb128::Varint`](crate::codecs::leb128::Varint): each impl set is
/// crate-owned, so growing it later is non-breaking while a downstream impl can never
/// observe a bound change.
pub(crate) mod sealed {
    /// The sealing supertrait — unnameable downstream, so the traits above cannot be
    /// implemented outside this crate.
    pub trait Sealed {}
}

/// Checked length ⇄ wire-prefix conversions: the types usable as a length/count prefix —
/// by the [`#[brw(count_prefix = <Ty>)]`](macro@crate::bin) directive and by the
/// [`codecs::prefixed`](crate::codecs::prefixed) string codec.
///
/// The prefix is computed from `len()` on encode and turned back into an element count on
/// decode. `try_from_count` is **checked and never truncates**: the length is widened (never
/// narrowed) before the range compare, so 300 elements against a `u8` prefix is
/// [`WidthError::ValueTooLarge`] — not a silently wrapped `44`.
///
/// [`WidthError::ValueTooLarge`]: crate::WidthError::ValueTooLarge
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a `count_prefix` type",
    note = "supported prefix types: u8, u16, u32, u64, u128 and the arbitrary-width `uN` aliases (e.g. `u12`)",
    note = "this trait is sealed — the supported prefix types are built in"
)]
pub trait CountPrefix: Bits + sealed::Sealed {
    /// The prefix for a collection of `len` elements.
    ///
    /// # Errors
    /// Returns [`crate::WidthError::ValueTooLarge`] when `len` exceeds the prefix's range.
    fn try_from_count(len: usize) -> core::result::Result<Self, crate::error::WidthError>
    where
        Self: Sized;

    /// The prefix as an element count.
    ///
    /// Counts too large for this host saturate at `usize::MAX`, never wrap to a smaller
    /// frame. Decoders must bound allocation and require input progress per element.
    fn to_count(self) -> usize;
}

macro_rules! count_prefix_prim {
    ($($t:ty),* $(,)?) => {$(
        impl sealed::Sealed for $t {}

        impl CountPrefix for $t {
            #[inline]
            fn try_from_count(len: usize) -> core::result::Result<Self, crate::error::WidthError> {
                <$t>::try_from(len).map_err(|_| crate::error::WidthError::ValueTooLarge {
                    value: len as u128,
                    bits: <$t>::BITS,
                })
            }

            #[inline]
            fn to_count(self) -> usize {
                usize::try_from(self).unwrap_or(usize::MAX)
            }
        }
    )*};
}
count_prefix_prim!(u8, u16, u32, u64, u128);

macro_rules! count_prefix_uint {
    ($($t:ty),* $(,)?) => {$(
        impl<const N: usize> sealed::Sealed for crate::int::UInt<$t, N> {}

        impl<const N: usize> CountPrefix for crate::int::UInt<$t, N> {
            #[inline]
            fn try_from_count(len: usize) -> core::result::Result<Self, crate::error::WidthError> {
                // Widen-then-compare: narrowing first (`len as $t`) would truncate
                // *before* the range check and let an oversized length slip through.
                let wide = len as u128;
                if wide > u128::from(Self::MASK) {
                    return Err(crate::error::WidthError::ValueTooLarge {
                        value: wide,
                        bits: Self::BITS,
                    });
                }
                Ok(Self::from_raw(<$t>::try_from(len).expect("count was checked against the UInt mask")))
            }

            #[inline]
            fn to_count(self) -> usize {
                usize::try_from(self.value()).unwrap_or(usize::MAX)
            }
        }
    )*};
}
count_prefix_uint!(u8, u16, u32, u64, u128);

/// `encode(writer)` for any [`BitEncode`] message — encodes to a `Vec` (using the type's
/// [`LAYOUT`](BitEncode::LAYOUT)) **verbatim** and writes it to a [`std::io::Write`] sink.
/// A blanket-implemented extension trait, so bring it into scope
/// (`use bnb::prelude::*` or `use bnb::EncodeExt`) to call `.encode(&mut w)`. Only
/// with the `std` feature; in `no_std` use the generated `to_bytes`/`to_canonical_bytes`, or
/// [`bit_encode`](BitEncode::bit_encode)/[`canonical_bit_encode`](BitEncode::canonical_bit_encode)
/// over a [`Sink`].
#[cfg(feature = "std")]
pub trait EncodeExt: BitEncode {
    /// Encodes `self` **verbatim** to any [`std::io::Write`] (socket, file, `Vec`) — exactly
    /// what's stored. For the canonical form, encode `self.to_canonical().encode(&mut w)`, or
    /// use the inherent `to_bytes` (verbatim) / `to_canonical_bytes` (canonical) instead.
    ///
    /// # Errors
    /// [`ErrorKind::Io`] on a write failure, else the encode error.
    fn encode<W: std::io::Write>(&self, w: &mut W) -> Result<(), BitError>
    where
        Self: Sized,
    {
        encode_to_writer_with(w, Self::LAYOUT, |bw| self.bit_encode(bw))
    }
}

#[cfg(feature = "std")]
impl<T: BitEncode> EncodeExt for T {}

/// Polymorphic decode **with context** `A` — the companion to a `#[bin(ctx(...))]`
/// type's inherent `decode_with`, for hand-written generic combinators and
/// trait-object parsing (ctx Layer 2). Every [`BitDecode`] type is `DecodeWith<()>`
/// (blanket), and a ctx type is `DecodeWith<…Ctx>`, so one bound `T: DecodeWith<A>`
/// spans both context-free and context-taking messages. Inherent `Type::decode_with`
/// call sites are unaffected.
pub trait DecodeWith<A>: Sized {
    /// Decodes `Self` from a [`Source`] given `args`.
    ///
    /// # Errors
    /// Propagates the decode [`BitError`].
    fn decode_with<S: Source>(r: &mut S, args: A) -> Result<Self, BitError>;
}

/// The dual of [`DecodeWith`] — polymorphic encode with context `A`.
pub trait EncodeWith<A> {
    /// Encodes `self` into a [`Sink`] given `args`.
    ///
    /// # Errors
    /// Propagates the encode [`BitError`].
    fn encode_with<K: Sink>(&self, w: &mut K, args: A) -> Result<(), BitError>;
}

impl<T: BitDecode> DecodeWith<()> for T {
    fn decode_with<S: Source>(r: &mut S, _args: ()) -> Result<Self, BitError> {
        T::bit_decode(r)
    }
}

impl<T: BitEncode> EncodeWith<()> for T {
    fn encode_with<K: Sink>(&self, w: &mut K, _args: ()) -> Result<(), BitError> {
        self.bit_encode(w)
    }
}

// ---------------------------------------------------------------------------
// Entry-point helpers — the logic behind the `#[derive]`-generated inherent
// methods (`Type::decode`/`peek`/`decode_exact`/`encode`/`to_bytes`). Kept here
// so the logic lives in one place rather than monomorphized inline per type;
// doc-hidden because the public surface is the generated methods.
// ---------------------------------------------------------------------------

/// Decode every message from `bytes` into a `Vec`, with the message's own byte/bit order baked
/// in — bit-aware, so messages that don't end on byte boundaries reassemble correctly. Backs
/// `Type::decode_all`. Whole bytes must hold whole messages; a sub-byte tail in the final byte
/// that does not decode is padding, as in [`decode_exact`]. A tail that does decode (a message
/// narrower than a byte) is kept: padding and such a message cannot be told apart.
///
/// # Errors
/// The first decode [`BitError`] (e.g. a truncated trailing message), or
/// [`ErrorKind::NoProgress`] at a message's start when it decodes without ending past that
/// start (a zero-width message, or one whose `seek` leaves the cursor at or before it).
#[doc(hidden)]
pub fn decode_all<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<Vec<T>, BitError> {
    decode_iter(bytes, layout).collect()
}

/// Whether a failed decode that started at bit `at` only ran into the final byte's padding:
/// fewer than 8 bits were left, and the decode either ran out of them or they are all zero.
/// Any other failure on a nonzero tail is a real rejection and is reported.
fn is_padding(bytes: &[u8], at: usize, bit: BitOrder, error: &BitError) -> bool {
    let Some(&last) = bytes.last() else {
        return false;
    };
    if bits_in(bytes.len()) - at >= 8 {
        return false;
    }
    // The unread bits of the final byte: the low ones under MSB-first, the high ones under LSB.
    let unread = match bit {
        BitOrder::Msb => 0xFF >> (at % 8),
        BitOrder::Lsb => 0xFF << (at % 8),
    };
    matches!(error.kind, ErrorKind::UnexpectedEof { .. }) || last & unread == 0
}

/// A lazy iterator decoding successive `T` from `bytes` (layout baked in) until the buffer is
/// drained, ending after the first error if one occurs. Backs `Type::decode_iter`. A sub-byte
/// tail that does not decode ends it cleanly, as in [`decode_all`]. A message that decodes
/// without ending past its start is that final error, [`ErrorKind::NoProgress`], and is not
/// yielded.
#[doc(hidden)]
pub fn decode_iter<T: BitDecode>(
    bytes: &[u8],
    layout: Layout,
) -> impl Iterator<Item = Result<T, BitError>> + '_ {
    let mut r = BitReader::with_layout(bytes, layout);
    let mut stopped = false;
    core::iter::from_fn(move || {
        if stopped || r.remaining_bits() == 0 {
            return None;
        }
        let before = r.bit_pos();
        match T::bit_decode(&mut r) {
            // Zero-width, or a backward `seek`: without this the iterator never ends.
            Ok(_) if r.bit_pos() <= before => {
                stopped = true;
                Some(Err(BitError::new(ErrorKind::NoProgress, before)))
            }
            Ok(v) => Some(Ok(v)),
            Err(e) if is_padding(bytes, before, layout.bit, &e) => {
                stopped = true;
                None
            }
            Err(e) => {
                stopped = true;
                Some(Err(e))
            }
        }
    })
}

/// Decodes one message from `bytes` without consuming the caller's buffer
/// (tail-tolerant). Backs `Type::peek`. Not a prefix decoder: a short slice is a definitive
/// [`ErrorKind::UnexpectedEof`], indistinguishable from malformed input — see
/// [`decode_prefix`] for that.
///
/// # Errors
/// Propagates the decode [`BitError`].
#[doc(hidden)]
pub fn decode_peek<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<T, BitError> {
    T::bit_decode(&mut BitReader::with_layout(bytes, layout))
}

/// `decode_peek` over a caller-supplied closure (no consumption requirement) — backs a
/// `#[bin]` enum's `peek_variant`, which runs only the dispatch decision over `bytes`.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn decode_peek_with<T, F>(bytes: &[u8], layout: Layout, f: F) -> Result<T, BitError>
where
    F: FnOnce(&mut BitReader) -> Result<T, BitError>,
{
    f(&mut BitReader::with_layout(bytes, layout))
}

/// `decode_exact` over a caller-supplied decode closure rather than the
/// [`BitDecode`] trait — backs the `ctx`-parameterized `Type::decode_with_exact`
/// (a `ctx` type takes a context argument, so it has no plain `bit_decode`).
///
/// # Errors
/// [`ErrorKind::TrailingBytes`] if whole bytes remain, else the closure's error.
#[doc(hidden)]
pub fn decode_exact_with<T, F>(bytes: &[u8], layout: Layout, f: F) -> Result<T, BitError>
where
    F: FnOnce(&mut BitReader) -> Result<T, BitError>,
{
    let mut r = BitReader::with_layout(bytes, layout);
    let v = f(&mut r)?;
    let consumed = r.bit_pos().div_ceil(8);
    if consumed < bytes.len() {
        return Err(BitError::new(
            ErrorKind::TrailingBytes {
                remaining: bytes.len() - consumed,
            },
            r.bit_pos(),
        ));
    }
    Ok(v)
}

/// `to_bytes` over a caller-supplied encode closure — backs the `ctx`-parameterized
/// `Type::to_bytes_with`.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn encode_to_vec_with<F>(layout: Layout, f: F) -> Result<Vec<u8>, BitError>
where
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut w = BitWriter::with_layout(layout);
    f(&mut w)?;
    Ok(w.into_bytes())
}

/// [`encode_to_vec`] into a buffer the caller keeps: `out` is emptied, the message is
/// encoded into its allocation, and it is handed back holding the message (or, after an
/// error, whatever was written before it). A caller that reuses `out` allocates only when
/// a message outgrows it.
///
/// # Errors
/// Propagates the encoder's [`BitError`].
#[cfg(feature = "net")]
pub(crate) fn encode_into<T: BitEncode>(
    value: &T,
    layout: Layout,
    out: &mut Vec<u8>,
) -> Result<(), BitError> {
    let mut bytes = mem::take(out);
    bytes.clear();
    let mut w = BitWriter {
        bytes,
        ..BitWriter::with_layout(layout)
    };
    let result = value.bit_encode(&mut w);
    *out = w.into_bytes();
    result
}

/// [`encode_to_vec_with`] into a buffer allocated once, for a message of `bit_len` bits —
/// backs `Type::to_bytes`/`to_canonical_bytes` of a fixed-length message, whose
/// [`FixedBitLen::BIT_LEN`] the macro passes. The length only sizes the allocation: an
/// encoder that writes more still grows the buffer, and one that writes less returns less.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn encode_fixed_to_vec<F>(layout: Layout, bit_len: u32, f: F) -> Result<Vec<u8>, BitError>
where
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut w = BitWriter::with_layout(layout);
    w.bytes.reserve_exact(bit_len.div_ceil(8) as usize);
    f(&mut w)?;
    Ok(w.into_bytes())
}

/// [`encode_to_vec_with`] into a buffer reserved for `bits` bits: the macro's estimate for
/// a message with no [`FixedBitLen`] to size from (the widths of its fixed fields plus
/// the element widths of its `Vec`s; zero for anything it cannot see). Only a hint. A
/// low estimate grows the buffer from at least the eight bytes an unhinted one starts
/// with, a zero estimate reserves nothing, and a reservation that fails is skipped. The
/// estimate never exceeds what a successful encode of the message writes.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn encode_hinted_to_vec<F>(layout: Layout, bits: usize, f: F) -> Result<Vec<u8>, BitError>
where
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut w = BitWriter::with_layout(layout);
    let bytes = bits.div_ceil(8);
    if bytes != 0 {
        // Never less than a growing `Vec<u8>` starts with, so an estimate that saw only
        // part of the message costs no allocation the unhinted buffer would not make.
        w.bytes.try_reserve_exact(bytes.max(8)).ok();
    }
    f(&mut w)?;
    Ok(w.into_bytes())
}

/// Decodes and requires every **whole byte** consumed; a sub-byte tail in the
/// final byte is treated as padding. Backs `Type::decode_exact`.
///
/// # Errors
/// [`ErrorKind::TrailingBytes`] if whole bytes remain, else the decode error.
#[doc(hidden)]
pub fn decode_exact<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<T, BitError> {
    decode_exact_with(bytes, layout, |r| T::bit_decode(r))
}

/// Decodes one message from the front of `bytes` and returns it with the **bits** it consumed —
/// a stateless prefix decode that, unlike [`decode_peek`], tells a short slice from a bad one.
/// Backs `Type::decode_prefix` (`eof = false`) and `Type::decode_prefix_eof` (`eof = true`).
///
/// Each call replays the decode from bit 0 of `bytes`; nothing is retained between calls, and
/// positions in errors are relative to `bytes`. Trailing input is ignored.
///
/// # Errors
/// With `eof = false`, physical shortage is [`ErrorKind::Incomplete`]: append input and retry
/// (a positive `needed` is a lower bound in bytes). With `eof = true`, shortage is definitive
/// ([`ErrorKind::UnexpectedEof`], or [`ErrorKind::IncompleteAtEof`] when a custom codec asks for
/// more). Either way, codec errors and logical-region ([`LimitedSource`]) overruns are
/// definitive, and a message that consumes no bits is [`ErrorKind::NoProgress`].
#[doc(hidden)]
pub fn decode_prefix_with<T: DecodeWith<A>, A>(
    bytes: &[u8],
    layout: Layout,
    args: A,
    eof: bool,
) -> Result<(T, usize), BitError> {
    decode_attempt(bytes, 0, layout, args, eof)
}

/// [`decode_prefix_with`] for a context-free [`BitDecode`] message.
///
/// # Errors
/// As [`decode_prefix_with`].
#[doc(hidden)]
pub fn decode_prefix<T: BitDecode>(
    bytes: &[u8],
    layout: Layout,
    eof: bool,
) -> Result<(T, usize), BitError> {
    decode_prefix_with(bytes, layout, (), eof)
}

/// Encodes `value` to a `Vec<u8>`. Backs `Type::to_bytes`.
///
/// # Errors
/// Propagates the encode [`BitError`].
#[doc(hidden)]
pub fn encode_to_vec<T: BitEncode>(value: &T, layout: Layout) -> Result<Vec<u8>, BitError> {
    let mut w = BitWriter::with_layout(layout);
    value.bit_encode(&mut w)?;
    Ok(w.into_bytes())
}

/// Encode to a [`std::io::Write`] over a caller-supplied encode closure — backs
/// [`EncodeExt::encode`] (the closure picks `bit_encode` vs `canonical_bit_encode`).
///
/// # Errors
/// [`ErrorKind::Io`] on a write failure, else the closure's error.
#[cfg(feature = "std")]
#[doc(hidden)]
pub fn encode_to_writer_with<W, F>(w: &mut W, layout: Layout, f: F) -> Result<(), BitError>
where
    W: std::io::Write,
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut bw = BitWriter::with_layout(layout);
    f(&mut bw)?;
    let at = bw.bit_len();
    w.write_all(&bw.into_bytes())
        .map_err(|e| BitError::new(ErrorKind::Io(e.kind()), at))
}

/// Reads a fixed `[u8; N]` byte array (`N * 8` bits) from the cursor. Backs a
/// `[u8; N]` payload field; `N` is inferred from the field type. Variable-length
/// payloads (`Vec` + `#[br(count = …)]`) take a separate push-based path that
/// grows by element, so an attacker-controlled count can't over-allocate.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn read_byte_array<const N: usize, S: Source>(r: &mut S) -> Result<[u8; N], BitError> {
    let mut arr = [0u8; N];
    r.read_into(&mut arr)?;
    Ok(arr)
}

/// Match one byte-string magic in declaration order, consuming it only on a full match.
/// A finite EOF rules out this candidate; incremental exhaustion with a matching prefix
/// requests more input before any later candidate or fallback can be selected.
/// No lookahead allocation is needed: a magic that is all in memory at a byte boundary is
/// compared where it lies, and any other is read a byte at a time.
///
/// # Errors
/// Propagates non-finite-EOF source errors, rewinding before returning.
// Inline, so the generated chain compares each constant magic in place when the whole of
// it is in memory at a byte boundary. Every read of the per-byte loop would then have
// succeeded, so its answer is this one; anything else takes that loop, out of line.
#[doc(hidden)]
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub fn match_magic<S: Source>(r: &mut S, magic: &[u8]) -> Result<bool, BitError> {
    let start = r.bit_pos();
    if let Some(matched) = r.aligned_run(magic.len()).map(|run| run == magic) {
        if !matched {
            r.seek_to_bit(start)?;
        }
        return Ok(matched);
    }
    match_magic_per_byte(r, magic, start)
}

/// [`match_magic`] one byte at a time from `start`, for a magic that is not all in memory
/// at a byte boundary: it stops at the first differing byte, and reports running out of
/// input as the source does.
#[inline(never)]
fn match_magic_per_byte<S: Source>(
    r: &mut S,
    magic: &[u8],
    start: usize,
) -> Result<bool, BitError> {
    for &expected in magic {
        match r.read_bits(8) {
            Ok(found) if found == u128::from(expected) => {}
            Ok(_) => {
                r.seek_to_bit(start)?;
                return Ok(false);
            }
            Err(error) => {
                r.seek_to_bit(start)?;
                return if matches!(error.kind, ErrorKind::UnexpectedEof { .. }) {
                    Ok(false)
                } else {
                    Err(error.in_field("magic"))
                };
            }
        }
    }
    Ok(true)
}

/// With the longest magic's worth of bytes in memory at a byte boundary: the index of the
/// first of `magics`, in declaration order, that the input starts with, consuming it, or
/// `magics.len()` and nothing consumed when none does. No read of a [`match_magic`] chain
/// can fail on such input, so this is that chain's answer, from one lend and constant
/// compares. Otherwise `None`, consuming nothing: the caller runs the chain.
///
/// # Errors
/// The source's seek, after a lend. No source in this crate lends a run it cannot seek
/// back over, so none returns it.
#[doc(hidden)]
#[inline]
pub fn peek_magic<S: Source>(r: &mut S, magics: &[&[u8]]) -> Result<Option<usize>, BitError> {
    let start = r.bit_pos();
    let longest = magics.iter().map(|magic| magic.len()).max().unwrap_or(0);
    let peeked = r.aligned_run(longest).map(|window| {
        magics
            .iter()
            .position(|magic| window.starts_with(magic))
            .map_or((magics.len(), 0), |hit| (hit, magics[hit].len()))
    });
    let Some((hit, len)) = peeked else {
        return Ok(None);
    };
    // The lend consumed the longest magic: leave the cursor after the winner's.
    r.seek_to_bit(start + len * 8)?;
    Ok(Some(hit))
}

/// Writes a fixed `[u8; N]` byte array. Backs a `[u8; N]` payload field.
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn write_byte_array<const N: usize, K: Sink>(arr: &[u8; N], w: &mut K) -> Result<(), BitError> {
    w.write_bytes(arr)
}

/// A *forward-only* bit reader over any [`std::io::Read`] — the streaming counterpart
/// to the in-memory [`BitReader`], for a stream you read once and don't seek.
///
/// It is bounded on `Read` **only, not `Seek`**, so it works over inputs that can't
/// seek (a socket, or a `&[u8]`, which is `Read` but not `Seek`). A message that needs
/// to seek (`#[br(restore_position)]`) won't decode through it — use a [`BufSource`] or
/// [`SeekReader`] for that. Reads up to 128 bits per call (the [`Source`] width
/// ceiling); running out mid-message yields [`ErrorKind::Incomplete`]. This reader is
/// **not transactional**: input already consumed during a failed field cannot be replayed.
/// Use [`BitBuf::try_pull`] for buffered, retryable message decoding. Non-EOF I/O failures
/// keep their original [`ErrorKind::Io`] kind.
///
/// Whole bytes read from between two bytes (a `u32`, a `[u8; N]`, a counted `Vec<u8>`) are
/// asked of the reader together, through [`Read::read`](std::io::Read::read), again while
/// it returns fewer than were asked for. A read that starts or ends inside a byte takes the
/// stream one byte at a time, through [`Read::read_exact`](std::io::Read::read_exact).
/// Either way the reader is never asked for more than the field in hand needs.
///
/// # Examples
///
/// ```
/// use bnb::{bin, StreamBitReader};
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Word { value: u32 }
///
/// // `&[u8]` is `Read` but not `Seek` — exactly the forward-only case.
/// let data: &[u8] = &[0x12, 0x34, 0x56, 0x78];
/// let mut s = StreamBitReader::new(data);
/// assert_eq!(Word::decode(&mut s).unwrap(), Word { value: 0x1234_5678 });
/// ```
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct StreamBitReader<R> {
    inner: R,
    /// Leftover bits from the last partially-consumed byte, right-aligned in the low
    /// `lead_bits` bits. Always fewer than 8. Consumed high-first under `Msb`, low-first
    /// under `Lsb`.
    lead: u32,
    lead_bits: u32,
    /// Total bits consumed so far (for position-aware errors).
    pos: usize,
    /// The bit/byte order this source decodes in. `read_bits` honors the **bit** order
    /// (like every other `Source`); **byte** order is applied to whole values by
    /// [`read`](Self::read) / `Source::read`.
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read> StreamBitReader<R> {
    /// Wraps a byte source, decoding in the default **MSB-first, big-endian** order. For a
    /// `#[bin(little)]`/`bits = lsb` message, use [`with_layout`](Self::with_layout) with
    /// that type's [`LAYOUT`](BitEncode::LAYOUT), or the decode reads the wrong order.
    pub fn new(inner: R) -> Self {
        Self::with_layout(inner, Layout::default())
    }

    /// Wraps a byte source, decoding in the given [`Layout`] (bit + byte order) — pass a
    /// message's `LAYOUT` so a non-default-order `#[bin]` type round-trips over a stream.
    pub fn with_layout(inner: R, layout: Layout) -> Self {
        Self {
            inner,
            lead: 0,
            lead_bits: 0,
            pos: 0,
            layout,
            depth: 0,
        }
    }

    /// The total number of bits consumed so far.
    #[must_use]
    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    /// Reads `n` (`<= 128`) bits in the source's [bit order](Layout), pulling bytes from
    /// the source as needed. This matches every other [`Source`]: byte order is applied to
    /// whole values by [`read`](Self::read) / `Source::read`, not here.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::Incomplete`] if the
    /// source runs out mid-field; [`ErrorKind::PositionOverflow`] once the stream has
    /// delivered more bits than `usize` can count. Each carries the bit offset. A failed read may
    /// have consumed input; do not retry the field on this reader. Use `BitBuf`
    /// when the caller needs transactional incremental attempts.
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if self.lead_bits != 0 || n % 8 != 0 || n > 128 {
            return self.read_bits_per_byte(n);
        }
        // Whole bytes from between two bytes: asked of the reader together. Bytes in
        // stream order are the value most significant first under `Msb` and least
        // significant first under `Lsb`, which is what the loop below accumulates.
        let at = self.pos;
        let end = at
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, at))?;
        let mut bytes = [0u8; 16];
        let count = (n / 8) as usize;
        let wanted = match self.layout.bit {
            BitOrder::Msb => &mut bytes[16 - count..],
            BitOrder::Lsb => &mut bytes[..count],
        };
        if let (_, Some(stopped)) = pull(&mut self.inner, wanted) {
            return Err(BitError::new(stopped.in_a_stream(), at));
        }
        self.pos = end;
        Ok(match self.layout.bit {
            BitOrder::Msb => u128::from_be_bytes(bytes),
            BitOrder::Lsb => u128::from_le_bytes(bytes),
        })
    }

    /// [`read_bits`](Self::read_bits) one byte of the stream at a time: the form that
    /// serves a read starting or ending inside a byte.
    fn read_bits_per_byte(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 128 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.pos,
            ));
        }
        let at = self.pos;
        let end = at
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, at))?;
        // Consume the leftover bits of the current byte, then whole bytes, honoring the
        // source's bit order so a streaming read matches the slice-backed sources exactly
        // (`extract_bits`). The accumulator never holds more than `n` (<= 128) bits, so it
        // can't overflow.
        let mut result: u128 = 0;
        let mut need = n;
        let mut placed = 0u32; // bits already placed (the LSB-path result cursor)
        while need > 0 {
            if self.lead_bits == 0 {
                let mut b = [0u8; 1];
                if let Err(error) = self.inner.read_exact(&mut b) {
                    return Err(BitError::new(stream_failure(error.kind()), at));
                }
                self.lead = u32::from(b[0]);
                self.lead_bits = 8;
            }
            let take = need.min(self.lead_bits);
            match self.layout.bit {
                // MSB-first: consume the *high* `take` bits, accumulate high-to-low.
                BitOrder::Msb => {
                    let shift = self.lead_bits - take;
                    let chunk = (self.lead >> shift) & ((1u32 << take) - 1);
                    result = (result << take) | u128::from(chunk);
                    self.lead_bits -= take;
                    self.lead &= (1u32 << self.lead_bits) - 1; // keep the unconsumed low bits
                }
                // LSB-first: consume the *low* `take` bits, place them at the bit cursor.
                BitOrder::Lsb => {
                    let chunk = self.lead & ((1u32 << take) - 1);
                    result |= u128::from(chunk) << placed;
                    self.lead >>= take; // drop the consumed low bits
                    self.lead_bits -= take;
                }
            }
            placed += take;
            need -= take;
        }
        self.pos = end;
        Ok(result)
    }

    /// Reads one [`Bits`] value (width `<= 128`) of its declared width, applying the
    /// source's byte order to whole values — matching [`BitReader::read`] and
    /// `Source::read`, so `sr.read::<T>()` agrees with `T::decode(&mut sr)`.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        let raw = self.read_bits(T::BITS)?;
        Ok(T::from_bits(value_byte_order::<T>(
            raw,
            self.layout.bit,
            self.layout.byte,
        )))
    }

    /// Reads whole bytes into `dst` from a byte-aligned cursor and counts them, stopping
    /// where a byte-by-byte read would: at the end of the input, at a reader error, or at
    /// the last byte the position can count. Says what stopped it short, if anything did.
    fn fill(&mut self, dst: &mut [u8]) -> Option<BitError> {
        debug_assert_eq!(self.lead_bits, 0);
        let fits = dst.len().min((usize::MAX - self.pos) / 8);
        let (got, stopped) = pull(&mut self.inner, &mut dst[..fits]);
        self.pos += got * 8;
        stopped
            .map(Stopped::in_a_stream)
            // Lazily: an error kind built and dropped unused is a call into its drop code.
            .or_else(|| (fits < dst.len()).then_some(ErrorKind::PositionOverflow))
            .map(|kind| BitError::new(kind, self.pos))
    }
}

/// Why a reader gave fewer bytes than it was asked for.
#[cfg(feature = "std")]
enum Stopped {
    /// It has no more: a read returned nothing.
    End,
    Failed(std::io::ErrorKind),
}

#[cfg(feature = "std")]
impl Stopped {
    /// What it means under a [`StreamBitReader`], where the end of the input is a message
    /// that is not all there yet.
    fn in_a_stream(self) -> ErrorKind {
        match self {
            Stopped::End => ErrorKind::Incomplete { needed: None },
            Stopped::Failed(kind) => stream_failure(kind),
        }
    }
}

/// Reads from `inner` until `dst` is full, as reading it one byte at a time would: an
/// interrupted read is tried again, the end of the input or an error stops it. Returns how
/// many bytes came and what stopped them, if anything did.
#[cfg(feature = "std")]
fn pull<R: std::io::Read>(inner: &mut R, dst: &mut [u8]) -> (usize, Option<Stopped>) {
    let mut got = 0;
    while got < dst.len() {
        match inner.read(&mut dst[got..]) {
            Ok(0) => return (got, Some(Stopped::End)),
            // A reader cannot have filled more than it was lent.
            Ok(count) => got += count.min(dst.len() - got),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return (got, Some(Stopped::Failed(error.kind()))),
        }
    }
    (got, None)
}

/// What a failed read of the stream under a [`StreamBitReader`] means: the end of the
/// input is a message that is not all there yet, anything else is the reader's own error.
#[cfg(feature = "std")]
fn stream_failure(kind: std::io::ErrorKind) -> ErrorKind {
    if kind == std::io::ErrorKind::UnexpectedEof {
        ErrorKind::Incomplete { needed: None }
    } else {
        ErrorKind::Io(kind)
    }
}

/// The most bytes a byte run asks a reader for before any have arrived; after that, as
/// many again as it has. A count read off the wire therefore allocates for input that
/// came, not for what it claims.
#[cfg(feature = "std")]
const RUN_STEP: usize = 4096;

#[cfg(feature = "std")]
impl<R: std::io::Read> sealed::Sealed for StreamBitReader<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read> Source for StreamBitReader<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        StreamBitReader::read_bits(self, n)
    }
    /// A run from a byte-aligned cursor asks the reader for as much as it still needs at
    /// a time; values, errors and the bytes consumed are those of reading it byte by byte.
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        if self.lead_bits != 0 {
            return read_bytes_per_byte(self, n);
        }
        let mut bytes = Vec::new();
        while bytes.len() < n {
            let have = bytes.len();
            bytes.resize(have + (n - have).min(have.max(RUN_STEP)), 0);
            if let Some(error) = self.fill(&mut bytes[have..]) {
                return Err(error);
            }
        }
        Ok(bytes)
    }
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        if self.lead_bits != 0 {
            return read_into_per_byte(self, buf);
        }
        match self.fill(buf) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn bit_pos(&self) -> usize {
        self.pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
}

/// A **seekable** [`Source`] over a forward `Read` (a socket): it *retains* the bytes
/// it has read, so a seek-using message (`restore_position`) works over a non-seekable
/// stream by seeking within the retained buffer, reading more on demand. It is
/// **bounded** — a retention `cap` (default 64 KiB) past which it errors
/// [`ErrorKind::BufferFull`] rather than buffering unboundedly. The
/// "continuously-receiving peer that also needs to seek" case. Once the reader returns
/// EOF, a read past the retained bytes is [`ErrorKind::UnexpectedEof`], as on a slice;
/// for input that may still grow, push into a [`BitBuf`] instead.
///
/// **Reclaim is explicit.** Every byte read stays retained, and counts against the cap,
/// until [`compact`](Self::compact) discards the bytes before the cursor. A loop decoding
/// message after message from one `BufSource` must call it between messages, or the stream
/// ends in [`ErrorKind::BufferFull`] once `cap` bytes have gone by. Nothing is reclaimed
/// automatically, because a later seek may still want those bytes. Positions
/// ([`bit_pos`](Source::bit_pos), [`seek_to_bit`](Source::seek_to_bit), error offsets) count
/// from the start of the stream and do not move when bytes are discarded.
///
/// # Examples
///
/// ```
/// use bnb::{bin, BufSource, Source};
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Word { value: u32 }
///
/// let wire = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
/// let mut src = BufSource::with_capacity(&wire[..], 4); // any `Read`; retain 4 bytes at most
/// assert_eq!(Word::decode(&mut src).unwrap(), Word { value: 0x1234_5678 });
/// src.compact(); // done with that message: release its bytes
/// assert_eq!(Word::decode(&mut src).unwrap(), Word { value: 0x9ABC_DEF0 });
/// assert_eq!(src.bit_pos(), 64, "positions stay stream-absolute");
/// ```
#[cfg(feature = "std")]
#[derive(Clone, Debug)]
pub struct BufSource<R> {
    inner: R,
    /// The retained bytes: stream bytes `base..base + buf.len()`.
    buf: Vec<u8>,
    /// Stream offset, in bytes, of `buf[0]`: how much [`compact`](Self::compact) has discarded.
    base: usize,
    /// The cursor, in bits from the start of the stream; never before `base`.
    bit_pos: usize,
    cap: usize,
    layout: Layout,
    eof: bool,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read> BufSource<R> {
    /// Wraps `inner` with the default 64 KiB retention cap, MSB-first big-endian.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self::with_capacity(inner, 64 * 1024)
    }

    /// Wraps `inner` with a retention `cap` (bytes), MSB-first big-endian.
    #[must_use]
    pub fn with_capacity(inner: R, cap: usize) -> Self {
        Self::with_capacity_and_layout(inner, cap, Layout::default())
    }

    /// Wraps `inner` with a retention `cap` (bytes) and [`Layout`].
    #[must_use]
    pub fn with_capacity_and_layout(inner: R, cap: usize, layout: Layout) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            base: 0,
            bit_pos: 0,
            cap,
            layout,
            eof: false,
            depth: 0,
        }
    }

    /// Discards the retained bytes before the cursor's byte, keeping a partially consumed
    /// byte, so they stop counting against the retention cap. Call it between messages in a
    /// long-lived decode loop; the allocation is reused.
    ///
    /// Positions stay stream-absolute: [`bit_pos`](Source::bit_pos) is unchanged. The
    /// discarded bytes are gone, so a later [`seek_to_bit`](Source::seek_to_bit) before the
    /// retained bytes fails with [`ErrorKind::NotSeekable`] — do not call this while a
    /// `restore_position` or pointer of the message in progress may still seek back.
    ///
    /// Because positions never rewind, the stream as a whole is limited to `usize::MAX`
    /// bits: on a 32-bit target a compacting loop ends in [`ErrorKind::PositionOverflow`]
    /// after 512 MiB. Start a new `BufSource` over the reader before then.
    pub fn compact(&mut self) {
        // A forward seek can leave the cursor past everything read so far.
        let whole = (self.bit_pos / 8 - self.base).min(self.buf.len());
        self.buf.drain(..whole);
        self.base += whole;
    }

    /// Reads from `inner` until the retained bytes reach stream byte `byte_end` (or
    /// EOF/cap), never past it. An [`Interrupted`](std::io::ErrorKind::Interrupted) read is
    /// retried; any other reader error is [`ErrorKind::Io`].
    fn fill_to(&mut self, byte_end: usize) -> Result<(), BitError> {
        while self.base + self.buf.len() < byte_end && !self.eof {
            if self.buf.len() >= self.cap {
                return Err(BitError::new(
                    ErrorKind::BufferFull { cap: self.cap },
                    self.bit_pos,
                ));
            }
            let want = (byte_end - self.base - self.buf.len())
                .min(self.cap - self.buf.len())
                .min(self.buf.len().max(RUN_STEP));
            // Cleared once for the step, however many reads the reader takes to fill it.
            let start = self.buf.len();
            self.buf.resize(start + want, 0);
            let (got, stopped) = pull(&mut self.inner, &mut self.buf[start..]);
            self.buf.truncate(start + got);
            match stopped {
                None => {}
                Some(Stopped::End) => self.eof = true,
                Some(Stopped::Failed(kind)) => {
                    return Err(BitError::new(ErrorKind::Io(kind), self.bit_pos));
                }
            }
        }
        Ok(())
    }

    /// Retains and consumes up to `n` whole bytes from a byte-aligned cursor, stopping
    /// where `n` single-byte reads would. Returns where the bytes it got lie in `buf`, and
    /// what stopped it short, if anything did.
    fn take_run(&mut self, n: usize) -> (core::ops::Range<usize>, Option<BitError>) {
        debug_assert_eq!(self.bit_pos % 8, 0);
        let start = self.bit_pos / 8;
        let fits = n.min((usize::MAX - self.bit_pos) / 8);
        let (mut end, mut stopped) = (start, None);
        // A run of no bytes reads nothing, not even what a forward seek skipped.
        if fits > 0 {
            let filled = self.fill_to(start + fits);
            let retained = self.base + self.buf.len();
            end = retained.clamp(start, start + fits);
            stopped = match filled {
                Err(error) => Some(error.kind),
                Ok(()) if end < start + fits => Some(ErrorKind::UnexpectedEof {
                    needed: 8,
                    remaining: 0,
                }),
                Ok(()) => None,
            };
        }
        if stopped.is_none() && fits < n {
            stopped = Some(ErrorKind::PositionOverflow);
        }
        self.bit_pos = end * 8;
        // After a forward seek the cursor can lie past everything retained.
        let at = (start - self.base).min(self.buf.len());
        (
            at..at + (end - start),
            stopped.map(|kind| BitError::new(kind, self.bit_pos)),
        )
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> sealed::Sealed for BufSource<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read> Source for BufSource<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 128 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.bit_pos,
            ));
        }
        let end = self
            .bit_pos
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        let byte_end = end.div_ceil(8);
        self.fill_to(byte_end)?;
        let retained_end = self.base + self.buf.len();
        if retained_end < byte_end {
            // `fill_to` stops short only at the reader's EOF, so the input is finite and
            // finished: report it as a slice would, letting dispatch fall through.
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: n as usize,
                    remaining: bits_in(retained_end).saturating_sub(self.bit_pos),
                },
                self.bit_pos,
            ));
        }
        // `buf` starts at stream byte `base`, and the cursor is never before it.
        let at = self.bit_pos - self.base * 8;
        let acc = extract_bits(&self.buf, at, n as usize, self.layout.bit);
        self.bit_pos += n as usize;
        Ok(acc)
    }
    /// A run from a byte-aligned cursor is retained in as few reads as the reader allows
    /// and copied out once; values, errors, the cursor and what is retained are those of
    /// reading it byte by byte.
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        if self.bit_pos % 8 != 0 {
            return read_bytes_per_byte(self, n);
        }
        match self.take_run(n) {
            (_, Some(error)) => Err(error),
            (run, None) => Ok(self.buf[run].to_vec()),
        }
    }
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        if self.bit_pos % 8 != 0 {
            return read_into_per_byte(self, buf);
        }
        let (run, stopped) = self.take_run(buf.len());
        buf[..run.len()].copy_from_slice(&self.buf[run]);
        match stopped {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
    /// Moves the cursor to stream-absolute bit `pos`.
    ///
    /// # Errors
    /// [`ErrorKind::NotSeekable`] if `pos` is before the retained bytes, which
    /// [`compact`](BufSource::compact) discarded; the cursor does not move.
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        // Seek within the retained buffer; a later read fills more on demand.
        // Backward seeks (`restore_position`) hit already-retained bytes.
        if pos / 8 < self.base {
            return Err(BitError::new(ErrorKind::NotSeekable, self.bit_pos));
        }
        self.bit_pos = pos;
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> SeekSource for BufSource<R> {}

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
    buf: Vec<u8>,
    /// Live read position, in bits, into `buf` (`0..=buf.len() * 8`).
    cursor: usize,
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

/// A [`SeekSource`] over a seekable reader (`Read + Seek`, e.g. a `File`): it seeks
/// via [`std::io::Seek`] to the byte holding the bit cursor, **without buffering** —
/// the large-file / container-format case. For a *non*-seekable stream that still
/// needs to seek, use [`BufSource`].
///
/// It re-seeks to an absolute offset before **every** read, so it stays correct when the
/// cursor is shared (a `&File`, a clone of this reader, a handle others also move). The
/// price is one `seek` plus one `read` per value, and a `BufReader` inside `R` is emptied
/// by each seek. When the reader is exclusively owned, [`BufSeekReader`] reads the same
/// bytes with far fewer calls.
///
/// # Examples
///
/// ```
/// use bnb::{bin, SeekReader};
/// use std::io::Cursor;
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Word { value: u32 }
///
/// let mut f = SeekReader::new(Cursor::new(vec![0x12u8, 0x34, 0x56, 0x78]));
/// assert_eq!(Word::decode(&mut f).unwrap(), Word { value: 0x1234_5678 });
/// ```
#[cfg(feature = "std")]
#[derive(Clone, Debug)]
pub struct SeekReader<R> {
    inner: R,
    bit_pos: usize,
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> SeekReader<R> {
    /// Wraps `inner` at bit 0, MSB-first big-endian.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self::with_layout(inner, Layout::default())
    }

    /// Wraps `inner` at bit 0 with the given [`Layout`].
    #[must_use]
    pub fn with_layout(inner: R, layout: Layout) -> Self {
        Self {
            inner,
            bit_pos: 0,
            layout,
            depth: 0,
        }
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> sealed::Sealed for SeekReader<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> Source for SeekReader<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let span = ByteSpan::new(self.bit_pos, n)?;
        self.inner
            .seek(std::io::SeekFrom::Start(span.start))
            .map_err(|e| span.io_error(&e))?;
        let acc = span.read(&mut self.inner, self.layout.bit)?;
        self.bit_pos = span.end;
        Ok(acc)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.bit_pos = pos; // the actual `io::Seek` happens on the next read
        Ok(())
    }
}

/// The whole bytes an `n`-bit read at a bit offset touches — the shared read path of
/// [`SeekReader`] and [`BufSeekReader`], so both validate, fail, and extract identically.
#[cfg(feature = "std")]
struct ByteSpan {
    /// The read's starting bit offset (the position every error reports).
    at: usize,
    /// The read's width in bits (`<= 128`).
    width: u32,
    /// Absolute byte offset of the first touched byte.
    start: u64,
    /// Number of touched bytes (`<= 17`: 7 leading bits plus 128).
    len: usize,
    /// Bit offset of the read within the first byte.
    bit_off: usize,
    /// The bit offset just past the read.
    end: usize,
}

#[cfg(feature = "std")]
impl ByteSpan {
    /// The largest span: a 128-bit read starting 7 bits into a byte.
    const MAX_LEN: usize = (7 + 128usize).div_ceil(8);

    /// Validates an `n`-bit read at bit `at` before any I/O.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] for `n > 128`, [`ErrorKind::PositionOverflow`] when the end
    /// bit is unrepresentable.
    fn new(at: usize, n: u32) -> Result<Self, BitError> {
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n as usize }, at));
        }
        let end = at
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, at))?;
        let bit_off = at % 8;
        Ok(Self {
            at,
            width: n,
            start: (at / 8) as u64,
            len: (bit_off + n as usize).div_ceil(8),
            bit_off,
            end,
        })
    }

    /// Maps a positioning (seek) failure to [`ErrorKind::Io`] at the read's offset.
    fn io_error(&self, e: &std::io::Error) -> BitError {
        BitError::new(ErrorKind::Io(e.kind()), self.at)
    }

    /// Reads the span's bytes from `inner` (already positioned at [`start`](Self::start))
    /// into a stack buffer and extracts the value in `order`.
    ///
    /// # Errors
    /// End of input is [`ErrorKind::UnexpectedEof`]; any other failure is
    /// [`ErrorKind::Io`].
    fn read(&self, inner: &mut impl std::io::Read, order: BitOrder) -> Result<u128, BitError> {
        let mut buf = [0u8; Self::MAX_LEN];
        let bytes = &mut buf[..self.len];
        inner.read_exact(bytes).map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof {
                ErrorKind::UnexpectedEof {
                    needed: self.width as usize,
                    remaining: 0,
                }
            } else {
                ErrorKind::Io(e.kind())
            };
            BitError::new(kind, self.at)
        })?;
        Ok(extract_bits(
            bytes,
            self.bit_off,
            self.width as usize,
            order,
        ))
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> SeekSource for SeekReader<R> {}

/// A [`SeekSource`] that **owns** a seekable reader behind a [`BufReader`](std::io::BufReader)
/// and tracks the stream position, so it seeks only when a read does not start where the
/// last one ended — and then by a relative seek that keeps the buffer when the target is
/// already buffered. Sequential and nearby reads cost a buffer copy, not a system call.
///
/// Reads, values, errors, and [`bit_pos`](Source::bit_pos) match [`SeekReader`] over the
/// same bytes, provided `R` supports [`SeekFrom::Current`](std::io::SeekFrom::Current) as
/// well as `Start` (every `std` seekable does): a jump outside the buffer is a relative
/// seek, where `SeekReader` only ever seeks from the start. The difference is the ownership
/// contract:
///
/// - Use `BufSeekReader` when the reader is **exclusively owned** — a `File` you opened for
///   this parse, a `Cursor`. Pass the unbuffered reader; it supplies its own buffer.
/// - Use [`SeekReader`] when the cursor may be **shared** — a `&File` (every `&File` moves
///   the one OS file offset), a cloned handle, or a reader others also move. `SeekReader`
///   re-seeks to an absolute offset before every read, so interleaved users stay correct.
///
/// There is deliberately no `get_mut` and no `Clone`: either would let the stream move
/// behind the tracked position. Do not move the cursor through [`get_ref`](Self::get_ref)
/// either (e.g. reading a `&File`).
///
/// # Examples
///
/// ```
/// use bnb::{BitOrder, BufSeekReader, ByteOrder, Layout, Source};
/// use std::io::Cursor;
///
/// // A TIFF-like header: the byte order is known only at run time.
/// let bytes = vec![b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
/// let byte = if bytes[..2] == *b"II" { ByteOrder::Little } else { ByteOrder::Big };
/// let mut r = BufSeekReader::with_layout(Cursor::new(bytes), Layout { bit: BitOrder::Msb, byte });
///
/// r.seek_to_bit(2 * 8).unwrap();
/// assert_eq!(r.read::<u16>().unwrap(), 42);
/// assert_eq!(r.read::<u32>().unwrap(), 8); // continues from the buffer, no seek
/// ```
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct BufSeekReader<R> {
    /// The owned reader and its buffer.
    inner: std::io::BufReader<R>,
    /// The byte offset the buffered stream is logically at, or `None` when unknown (before
    /// the first read and after any I/O error), which forces an absolute seek.
    stream_pos: Option<u64>,
    /// The bit cursor; reads start here.
    bit_pos: usize,
    /// The bit and byte order values are read in.
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> BufSeekReader<R> {
    /// Wraps `inner` at bit 0, MSB-first big-endian, with a default-sized buffer.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self::with_layout(inner, Layout::default())
    }

    /// Wraps `inner` at bit 0 with the given [`Layout`] (which may be chosen at run time),
    /// with a default-sized buffer.
    #[must_use]
    pub fn with_layout(inner: R, layout: Layout) -> Self {
        Self {
            inner: std::io::BufReader::new(inner),
            stream_pos: None,
            bit_pos: 0,
            layout,
            depth: 0,
        }
    }

    /// Borrows the underlying reader. Do not move its cursor through this borrow (a
    /// `&File` can): the tracked position would go stale.
    #[must_use]
    pub fn get_ref(&self) -> &R {
        self.inner.get_ref()
    }

    /// Unwraps the underlying reader. Buffered-but-unread bytes are discarded, so its
    /// cursor is at an unspecified offset: seek it before reading.
    #[must_use]
    pub fn into_inner(self) -> R {
        self.inner.into_inner()
    }

    /// Positions the buffered stream at byte `target`: nothing when already there, a
    /// relative seek (which keeps buffered bytes in range) when the position is known, and
    /// an absolute seek otherwise.
    fn position(&mut self, target: u64) -> std::io::Result<()> {
        let delta = self
            .stream_pos
            .and_then(|pos| i64::try_from(i128::from(target) - i128::from(pos)).ok());
        match delta {
            Some(0) => Ok(()),
            Some(delta) => self.inner.seek_relative(delta),
            None => {
                std::io::Seek::seek(&mut self.inner, std::io::SeekFrom::Start(target)).map(drop)
            }
        }
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> sealed::Sealed for BufSeekReader<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> Source for BufSeekReader<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let span = ByteSpan::new(self.bit_pos, n)?;
        let positioned = self.position(span.start);
        // Unknown until the read succeeds: a failed seek or `read_exact` leaves the offset
        // unspecified, so the next read re-seeks absolutely.
        self.stream_pos = None;
        positioned.map_err(|e| span.io_error(&e))?;
        let acc = span.read(&mut self.inner, self.layout.bit)?;
        self.stream_pos = Some(span.start + span.len as u64);
        self.bit_pos = span.end;
        Ok(acc)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.bit_pos = pos; // the stream is positioned (if needed) on the next read
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> SeekSource for BufSeekReader<R> {}

/// `bytes`-crate adapters (the `bytes` feature): own a `Bytes` frame to decode
/// (zero copy), encode into an owned buffer you `freeze()` to a `Bytes` — the
/// async/tokio framing case. Off by default so the core stays dependency-light.
#[cfg(feature = "bytes")]
mod bytes_io {
    use alloc::vec::Vec;

    #[cfg(feature = "tokio")]
    use super::{
        Appendable, ErrorKind, append_whole_bytes, emit_bits, position_overflow, write_by_bits,
        write_bytes_per_byte, writes_whole_bytes,
    };
    use super::{
        BitError, BitOrder, BitReader, BitWriter, Bits, ByteOrder, Layout, SeekSource, Sink,
        Source, aligned_run_at, read_via_slice,
    };

    /// A [`SeekSource`](super::SeekSource) that **owns** a `bytes::Bytes` frame (no
    /// borrow), decoding bits from it. Constructing it from a `Bytes` is a refcount
    /// bump (zero copy).
    #[derive(Clone, Debug)]
    pub struct BytesReader {
        data: bytes::Bytes,
        bit_pos: usize,
        layout: Layout,
        /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`](super::MAX_DECODE_DEPTH).
        depth: u16,
    }

    impl BytesReader {
        /// Owns `data`, positioned at bit 0, MSB-first big-endian.
        #[must_use]
        pub fn new(data: bytes::Bytes) -> Self {
            Self::with_layout(data, Layout::default())
        }

        /// Owns `data` with the given [`Layout`](super::Layout).
        #[must_use]
        pub fn with_layout(data: bytes::Bytes, layout: Layout) -> Self {
            Self {
                data,
                bit_pos: 0,
                layout,
                depth: 0,
            }
        }
    }

    impl super::sealed::Sealed for BytesReader {}

    impl Source for BytesReader {
        #[doc(hidden)]
        fn __bnb_depth(&mut self) -> &mut u16 {
            &mut self.depth
        }
        #[inline]
        fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
            let mut br = BitReader::with_layout(&self.data, self.layout);
            br.seek_to_bit(self.bit_pos)?;
            let v = br.read_bits(n)?;
            self.bit_pos = Source::bit_pos(&br);
            Ok(v)
        }
        // The slice reader's typed read on the bytes in hand, so a whole-byte value is its
        // one load here too. Forced inline as that read is (DESIGN §22).
        #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
        #[inline(always)]
        fn read<T: Bits>(&mut self) -> Result<T, BitError> {
            let mut br = BitReader::with_layout(&self.data, self.layout);
            br.seek_to_bit(self.bit_pos)?;
            let value = br.read()?;
            self.bit_pos = Source::bit_pos(&br);
            Ok(value)
        }
        fn bit_pos(&self) -> usize {
            self.bit_pos
        }
        fn byte_order(&self) -> ByteOrder {
            self.layout.byte
        }
        fn bit_order(&self) -> BitOrder {
            self.layout.bit
        }
        fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
            self.bit_pos = pos;
            Ok(())
        }
        // An empty run reads nothing, so it must not trip the per-read seek check either.
        fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
            if n == 0 {
                return Ok(Vec::new());
            }
            read_via_slice(&self.data, self.layout, &mut self.bit_pos, |r| {
                r.read_bytes(n)
            })
        }
        fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
            if buf.is_empty() {
                return Ok(());
            }
            read_via_slice(&self.data, self.layout, &mut self.bit_pos, |r| {
                r.read_into(buf)
            })
        }
        fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
            aligned_run_at(&self.data, &mut self.bit_pos, n)
        }
    }

    impl SeekSource for BytesReader {}

    /// A [`Sink`](super::Sink) that encodes into an owned [`BitWriter`](super::BitWriter)
    /// (a `Vec<u8>`, not a `bytes::BytesMut`); [`freeze`] converts that buffer into a
    /// `Bytes`.
    ///
    /// [`freeze`]: BytesWriter::freeze
    #[derive(Clone, Debug, Default)]
    pub struct BytesWriter {
        inner: BitWriter,
    }

    impl BytesWriter {
        /// An empty MSB-first, big-endian writer.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// An empty writer in the given [`Layout`](super::Layout).
        #[must_use]
        pub fn with_layout(layout: Layout) -> Self {
            Self {
                inner: BitWriter::with_layout(layout),
            }
        }

        /// The encoded bytes as a zero-copy `Bytes` (the final partial byte is
        /// zero-padded).
        #[must_use]
        pub fn freeze(self) -> bytes::Bytes {
            bytes::Bytes::from(self.inner.into_bytes())
        }
    }

    impl super::sealed::Sealed for BytesWriter {}

    impl Sink for BytesWriter {
        fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
            self.inner.write_bits(value, n)
        }
        // The wrapped writer's typed write, so a whole-byte value is one append here too.
        #[inline]
        fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
            Sink::write(&mut self.inner, value)
        }
        fn bit_pos(&self) -> usize {
            Sink::bit_pos(&self.inner)
        }
        fn byte_order(&self) -> ByteOrder {
            Sink::byte_order(&self.inner)
        }
        fn bit_order(&self) -> BitOrder {
            Sink::bit_order(&self.inner)
        }
        fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
            self.inner.write_bytes(bytes)
        }
        fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
            self.inner.aligned_tail(n)
        }
    }

    /// The bytes one message appends to a `BytesMut`: [`Appendable`] over the buffer's tail
    /// from `start`, where the message begins.
    #[cfg(feature = "tokio")]
    struct Appended<'a> {
        dst: &'a mut bytes::BytesMut,
        start: usize,
    }

    #[cfg(feature = "tokio")]
    impl Appendable for Appended<'_> {
        #[inline]
        fn len(&self) -> usize {
            self.dst.len() - self.start
        }
        #[inline]
        fn push(&mut self, byte: u8) {
            self.dst.extend_from_slice(&[byte]);
        }
        #[inline]
        fn or_into(&mut self, index: usize, bits: u8) {
            self.dst[self.start + index] |= bits;
        }
    }

    /// A [`Sink`] that appends one message to a `BytesMut` in place, for an encoder that
    /// would otherwise build the message in a `Vec` and copy it over. Positions count
    /// from the message's first bit, as a [`BitWriter`]'s do, so alignment, padding and
    /// error positions do not depend on what the buffer already holds.
    ///
    /// The message stays only if [`keep`](Self::keep) is called. Dropped without it (an
    /// encode error, or a panic in the encoder unwinding through), the writer takes back
    /// whatever it appended, so the buffer never holds part of a message.
    #[cfg(feature = "tokio")]
    pub(crate) struct FrameWriter<'a> {
        out: Appended<'a>,
        bit_pos: usize,
        layout: Layout,
        kept: bool,
    }

    #[cfg(feature = "tokio")]
    impl<'a> FrameWriter<'a> {
        /// A writer that appends to `dst` in `layout`, starting at its current end.
        pub(crate) fn new(dst: &'a mut bytes::BytesMut, layout: Layout) -> Self {
            let start = dst.len();
            Self {
                out: Appended { dst, start },
                bit_pos: 0,
                layout,
                kept: false,
            }
        }

        /// Ends the writer leaving the message in the buffer: the encode succeeded.
        pub(crate) fn keep(mut self) {
            self.kept = true;
        }
    }

    #[cfg(feature = "tokio")]
    impl Drop for FrameWriter<'_> {
        fn drop(&mut self) {
            if !self.kept {
                self.out.dst.truncate(self.out.start);
            }
        }
    }

    #[cfg(feature = "tokio")]
    impl super::sealed::Sealed for FrameWriter<'_> {}

    #[cfg(feature = "tokio")]
    impl Sink for FrameWriter<'_> {
        // As `BitWriter::write_bits`, into the buffer's tail.
        #[inline]
        fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
            let n = n as usize;
            if n > 128 {
                return Err(BitError::new(ErrorKind::TooWide { width: n }, self.bit_pos));
            }
            let end = self
                .bit_pos
                .checked_add(n)
                .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
            emit_bits(&mut self.out, self.bit_pos, value, n, self.layout.bit);
            self.bit_pos = end;
            Ok(())
        }
        // As `BitWriter::write`: an aligned cursor is the end of what the message wrote.
        #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
        #[inline(always)]
        fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
            if !writes_whole_bytes::<T>(self.bit_pos) {
                return write_by_bits(self, value);
            }
            let Some(end) = self.bit_pos.checked_add(T::BITS as usize) else {
                return Err(position_overflow(self.bit_pos));
            };
            debug_assert_eq!(self.bit_pos / 8, self.out.len());
            let dst = &mut *self.out.dst;
            let byte = self.layout.byte;
            append_whole_bytes(value, byte, |bytes| dst.extend_from_slice(bytes));
            self.bit_pos = end;
            Ok(())
        }
        #[inline]
        fn bit_pos(&self) -> usize {
            self.bit_pos
        }
        #[inline]
        fn byte_order(&self) -> ByteOrder {
            self.layout.byte
        }
        #[inline]
        fn bit_order(&self) -> BitOrder {
            self.layout.bit
        }
        // As `BitWriter`: an aligned cursor sits at the end of what the message has
        // written, so the run is appended as it is.
        #[inline]
        fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
            if self.bit_pos % 8 != 0 {
                return write_bytes_per_byte(self, bytes);
            }
            let end = bytes
                .len()
                .checked_mul(8)
                .and_then(|bits| self.bit_pos.checked_add(bits))
                .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
            self.out.dst.extend_from_slice(bytes);
            self.bit_pos = end;
            Ok(())
        }
        #[inline]
        fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
            if self.bit_pos % 8 != 0 {
                return None;
            }
            let end = n
                .checked_mul(8)
                .and_then(|bits| self.bit_pos.checked_add(bits))?;
            let tail = self.out.dst.len();
            self.out.dst.resize(tail + n, 0);
            self.bit_pos = end;
            Some(&mut self.out.dst[tail..])
        }
    }

    #[cfg(all(test, feature = "tokio"))]
    mod component {
        //! `FrameWriter` against `BitWriter`: the same writes leave the same bytes after
        //! whatever the buffer already held, the same cursor and the same errors.
        use super::FrameWriter;
        use crate::{BitError, BitOrder, BitWriter, ByteOrder, ErrorKind, Layout, Sink};
        use bytes::BytesMut;

        /// A fixed xorshift stream.
        fn noise(state: &mut u64) -> u64 {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            *state
        }

        /// One write, chosen by `pick`, applied to any sink: bits of a noisy width, a byte
        /// run, a lent tail filled in, or a width no sink accepts.
        fn write<K: Sink>(sink: &mut K, pick: u64, value: u128) -> Result<(), BitError> {
            let bytes = value.to_le_bytes();
            // Reduced first: the noise does not fit a 32-bit `usize`.
            let small = usize::try_from((pick >> 8) % 6).unwrap();
            match pick % 8 {
                0..=3 => sink.write_bits(value, u32::try_from((pick >> 8) % 129).unwrap()),
                4 => sink.write_bits(value, 8 * u32::try_from((pick >> 8) % 17).unwrap()),
                5 => sink.write_bytes(&bytes[..small]),
                6 => {
                    if let Some(tail) = sink.aligned_tail(small) {
                        tail.copy_from_slice(&bytes[..small]);
                    }
                    Ok(())
                }
                _ => sink.write_bits(value, 129 + u32::try_from(pick >> 60).unwrap()),
            }
        }

        #[test]
        fn a_frame_written_in_place_is_the_frame_a_bit_writer_builds() {
            let mut state = 0x94D0_49BB_1331_11EB;
            for round in 0..400 {
                let layout = Layout {
                    bit: [BitOrder::Msb, BitOrder::Lsb][round % 2],
                    byte: [ByteOrder::Big, ByteOrder::Little][round / 2 % 2],
                };
                // What the buffer held before the frame: it must come through untouched.
                let before: Vec<u8> = (0..round % 4)
                    .map(|_| noise(&mut state).to_le_bytes()[0])
                    .collect();
                let mut dst = BytesMut::from(&before[..]);
                let mut frame = FrameWriter::new(&mut dst, layout);
                let mut reference = BitWriter::with_layout(layout);
                for step in 0..12 {
                    let pick = noise(&mut state);
                    let value = u128::from(noise(&mut state)) << 64 | u128::from(noise(&mut state));
                    let got = write(&mut frame, pick, value).map_err(|e| (e.kind, e.at));
                    let expected = write(&mut reference, pick, value).map_err(|e| (e.kind, e.at));
                    assert_eq!(got, expected, "round {round} step {step}");
                    assert_eq!(
                        frame.bit_pos(),
                        reference.bit_pos(),
                        "round {round} step {step}"
                    );
                    assert_eq!(
                        (frame.bit_order(), frame.byte_order()),
                        (layout.bit, layout.byte)
                    );
                }
                frame.keep();
                let written = reference.into_bytes();
                assert_eq!(
                    dst[..before.len()],
                    before[..],
                    "round {round}: what was there"
                );
                assert_eq!(dst[before.len()..], written[..], "round {round}: the frame");
            }
        }

        #[test]
        fn a_frame_writer_refuses_a_position_it_cannot_count() {
            let mut dst = BytesMut::from(&b"kept"[..]);
            let mut frame = FrameWriter::new(&mut dst, Layout::default());
            frame.bit_pos = usize::MAX - 3;
            let error = frame.write_bits(0, 8).unwrap_err();
            assert_eq!(
                (error.kind, error.at),
                (ErrorKind::PositionOverflow, usize::MAX - 3)
            );
            frame.bit_pos = usize::MAX - 7;
            let error = frame.write_bytes(&[0, 0]).unwrap_err();
            assert_eq!(
                (error.kind, error.at),
                (ErrorKind::PositionOverflow, usize::MAX - 7)
            );
            assert!(frame.aligned_tail(2).is_none());
            let error = frame.write(0x0102u16).unwrap_err();
            assert_eq!(
                (error.kind, error.at),
                (ErrorKind::PositionOverflow, usize::MAX - 7)
            );
            assert_eq!(frame.bit_pos, usize::MAX - 7);
            frame.keep();
            assert_eq!(&dst[..], b"kept");
        }

        #[test]
        fn a_frame_writer_dropped_without_keep_takes_its_bytes_back() {
            let mut dst = BytesMut::from(&b"kept"[..]);
            let mut frame = FrameWriter::new(&mut dst, Layout::default());
            frame.write_bits(0xABCDE, 20).unwrap();
            frame.write_bytes(&[1, 2, 3]).unwrap();
            drop(frame);
            assert_eq!(&dst[..], b"kept");
        }
    }
}

#[cfg(feature = "tokio")]
pub(crate) use bytes_io::FrameWriter;
#[cfg(feature = "bytes")]
pub use bytes_io::{BytesReader, BytesWriter};

#[cfg(test)]
mod unit {
    use super::*;
    use crate::{u4, u12};

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

    /// A fixed xorshift stream, so the differential tests see the same bytes every run.
    fn noise(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
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
    fn writers_stay_send_so_an_encode_can_move_across_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<BitWriter>();
        #[cfg(feature = "bytes")]
        assert_send::<BytesWriter>();
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

    /// A 24-bit value that declares its own byte order, as a `#[bitfield(bytes = …)]` does.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Own<const LITTLE: bool>(u32);

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
    struct Blank<const WIDTH: u32>;

    impl<const WIDTH: u32> Bits for Blank<WIDTH> {
        const BITS: u32 = WIDTH;

        fn into_bits(self) -> u128 {
            0
        }

        fn from_bits(_: u128) -> Self {
            Self
        }
    }

    /// One `T` read at `pos` by each slice reader's `read`, against the same read taken
    /// bit by bit ([`read_by_bits`], the path every other source uses): the value or the
    /// error and its position, and where the cursor is left.
    fn assert_read_matches_the_bit_path<T: Bits + PartialEq + fmt::Debug>(
        buf: &[u8],
        layout: Layout,
        pos: usize,
    ) {
        type Outcome<T> = (Result<T, (ErrorKind, usize)>, usize);
        fn outcome<S: Source, T>(
            reader: &mut S,
            read: impl FnOnce(&mut S) -> Result<T, BitError>,
        ) -> Outcome<T> {
            let result = read(reader).map_err(|e| (e.kind, e.at));
            (result, reader.bit_pos())
        }
        let at = || {
            let mut reader = BitReader::with_layout(buf, layout);
            reader.bit_pos = pos;
            reader
        };
        let context = format!("{layout:?} len {} pos {pos} width {}", buf.len(), T::BITS);

        let expected: Outcome<T> = outcome(&mut at(), read_by_bits);
        assert_eq!(outcome(&mut at(), Source::read), expected, "{context}");
        assert_eq!(
            outcome(&mut at(), BitReader::read),
            expected,
            "{context}, inherent"
        );

        let expected: Outcome<T> = outcome(&mut IncrementalReader(at()), read_by_bits);
        let got = outcome(&mut IncrementalReader(at()), Source::read);
        assert_eq!(got, expected, "{context}, incremental");
    }

    #[test]
    fn read_takes_whole_bytes_exactly_as_the_bit_path_does() {
        use crate::{u24, u40, u72, u104};
        let mut state = 0xA076_1D64_78BD_642F;
        let noisy: Vec<u8> = (0..20)
            .map(|_| noise(&mut state).to_le_bytes()[0])
            .collect();
        for bit in [BitOrder::Msb, BitOrder::Lsb] {
            for byte in [ByteOrder::Big, ByteOrder::Little] {
                let layout = Layout { bit, byte };
                // Every length and every cursor in it, so each width meets its bytes all
                // there, one short and absent, from a byte boundary and from inside a byte.
                for len in 0..=noisy.len() {
                    let buf = &noisy[..len];
                    for pos in 0..=len * 8 {
                        macro_rules! each {
                            ($($value:ty),*) => {
                                $(assert_read_matches_the_bit_path::<$value>(buf, layout, pos);)*
                            };
                        }
                        each!(bool, u4, u12, u8, u16, u24, u32, u40, u64, u128);
                        each!(u72, u104, Own<true>, Own<false>, Blank<0>, Blank<136>);
                    }
                }
            }
        }
    }

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

    /// Magic dispatch as the generated code runs it: the winner from one look at the longest
    /// magic when that is lent, else from the chain; `one` is how the chain tries a magic.
    /// Returns the winner's index (`magics.len()` for none) or the chain's error.
    fn dispatch<S: Source>(
        r: &mut S,
        magics: &[&[u8]],
        peek: bool,
        one: impl Fn(&mut S, &[u8]) -> Result<bool, BitError>,
    ) -> Result<usize, (ErrorKind, usize)> {
        let flat = |e: BitError| (e.kind, e.at);
        if peek {
            if let Some(hit) = peek_magic(r, magics).map_err(flat)? {
                return Ok(hit);
            }
        }
        for (index, magic) in magics.iter().enumerate() {
            if one(r, magic).map_err(flat)? {
                return Ok(index);
            }
        }
        Ok(magics.len())
    }

    #[test]
    fn magic_dispatch_from_a_lent_run_picks_what_the_byte_by_byte_chain_picks() {
        // A magic that is a prefix of a later one, one that is a prefix of an earlier one,
        // a shared first byte, and the empty magic last (it matches anything).
        let sets: [&[&[u8]]; 3] = [
            &[b"AB", b"ABC", b"CDE", b"C", b"TUVWXY"],
            &[b"TUVWXY", b"TU", b"Z"],
            &[b"OK", b"ERROR:", b""],
        ];
        for magics in sets {
            // Every magic whole, cut short at each length, and with each byte changed, alone
            // and with bytes after it, so the longest magic is sometimes there and sometimes
            // not.
            let mut inputs = vec![Vec::new(), b"??".to_vec()];
            for magic in magics {
                for cut in 0..=magic.len() {
                    for tail in [&b""[..], b"\x07", b"\x07\x08\x09\x0A\x0B\x0C"] {
                        inputs.push([&magic[..cut], tail].concat());
                    }
                }
                for flip in 0..magic.len() {
                    let mut near = [magic, &b"\x07\x08\x09\x0A\x0B\x0C"[..]].concat();
                    near[flip] ^= 0x20;
                    inputs.push(near);
                }
            }
            for input in &inputs {
                // From a byte boundary and from inside a byte, behind one lead byte.
                let wire = [&[0xA5][..], input].concat();
                for start in [8, 5] {
                    let context = format!("{magics:?} input {input:02X?} from bit {start}");
                    let slice = || {
                        let mut reader = BitReader::new(&wire);
                        reader.seek_to_bit(start).unwrap();
                        reader
                    };
                    let chain = |r: &mut BitReader<'_>, magic: &[u8]| {
                        let from = r.bit_pos();
                        match_magic_per_byte(r, magic, from)
                    };
                    let (mut fast, mut slow) = (slice(), slice());
                    let expected = (dispatch(&mut slow, magics, false, chain), slow.bit_pos());
                    let got = (
                        dispatch(&mut fast, magics, true, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}");
                    let mut fast = slice();
                    let got = (
                        dispatch(&mut fast, magics, false, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}, chain only");

                    // The incremental reader waits where the slice reader rules a magic out.
                    let chain = |r: &mut IncrementalReader<'_>, magic: &[u8]| {
                        let from = r.bit_pos();
                        match_magic_per_byte(r, magic, from)
                    };
                    let (mut fast, mut slow) =
                        (IncrementalReader(slice()), IncrementalReader(slice()));
                    let expected = (dispatch(&mut slow, magics, false, chain), slow.bit_pos());
                    let got = (
                        dispatch(&mut fast, magics, true, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}, incremental");
                    let mut fast = IncrementalReader(slice());
                    let got = (
                        dispatch(&mut fast, magics, false, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}, incremental, chain only");
                }
            }
        }
    }

    #[test]
    fn a_peek_that_cannot_see_the_longest_magic_consumes_nothing() {
        let magics: &[&[u8]] = &[b"OK", b"ERROR:"];
        // Five bytes: one short of the longest magic, though the first magic is all there.
        let mut reader = BitReader::new(b"OK\x07\x08\x09");
        assert_eq!(peek_magic(&mut reader, magics).unwrap(), None);
        assert_eq!(reader.bit_pos(), 0);
        // Six: the first magic wins and only its two bytes are consumed.
        let mut reader = BitReader::new(b"OK\x07\x08\x09\x0A");
        assert_eq!(peek_magic(&mut reader, magics).unwrap(), Some(0));
        assert_eq!(reader.bit_pos(), 16);
        // No magic there: the count of magics, and nothing consumed.
        let mut reader = BitReader::new(b"NOPE!!");
        assert_eq!(peek_magic(&mut reader, magics).unwrap(), Some(2));
        assert_eq!(reader.bit_pos(), 0);
    }

    #[test]
    fn a_hinted_encode_reserves_its_bytes_once_and_shrugs_off_a_hint_it_cannot_meet() {
        let write = |n: usize| {
            move |w: &mut BitWriter| {
                (0..n).try_for_each(|_| w.write(0xA5u8))?;
                w.write(u4::new(0xF))
            }
        };
        // 84 bits: the partial eleventh byte is reserved with the first ten.
        let bytes = encode_hinted_to_vec(Layout::default(), 84, write(10)).unwrap();
        assert_eq!(bytes[8..], [0xA5, 0xA5, 0xF0]);
        assert_eq!(bytes.capacity(), 11);
        // A small estimate starts where an unhinted buffer does, and no estimate reserves
        // nothing at all.
        let bytes = encode_hinted_to_vec(Layout::default(), 20, write(2)).unwrap();
        assert_eq!(bytes.capacity(), 8);
        let empty = encode_hinted_to_vec(Layout::default(), 0, |_| Ok(())).unwrap();
        assert_eq!(empty.capacity(), 0);
        // A low hint grows; one no allocator can meet is skipped, not fatal.
        for hint in [0, 8, usize::MAX] {
            let bytes = encode_hinted_to_vec(Layout::default(), hint, write(40)).unwrap();
            assert_eq!(bytes.len(), 41, "hint {hint}");
        }
        // The closure's error is the result.
        let error = encode_hinted_to_vec(Layout::default(), 16, |w| w.write_bits(0, 129));
        assert_eq!(error.unwrap_err().kind, ErrorKind::TooWide { width: 129 });
    }

    /// A 16-bit value whose `into_bits` leaves bits set above its width, which no write
    /// may put on the wire.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Dirty(u16);

    impl Bits for Dirty {
        const BITS: u32 = 16;

        fn into_bits(self) -> u128 {
            u128::from(self.0) | (0xA5A5 << 16) | (0xFF << 120)
        }

        fn from_bits(raw: u128) -> Self {
            Self(u16::try_from(raw).expect("a 16-bit value"))
        }
    }

    /// What a writer holds before the write under test: `lead` bits, then by `kind`
    /// nothing more, a byte run, a typed write, or (from a byte boundary) a lent tail
    /// filled in. Each leaves the cursor and the buffer in step in its own way.
    fn write_prefix<K: Sink>(sink: &mut K, lead: u32, kind: u8) {
        sink.write_bits(0x1_5555, lead).unwrap();
        match kind {
            1 => sink.write_bytes(&[0x11, 0x22, 0x33]).unwrap(),
            2 => sink.write(0xBEEFu16).unwrap(),
            3 => {
                if let Some(tail) = sink.aligned_tail(2) {
                    tail.copy_from_slice(&[0x44, 0x55]);
                }
            }
            _ => {}
        }
    }

    /// One `T` written after `lead` bits, by each appending writer's `write`, against the
    /// same write taken bit by bit ([`write_by_bits`], the path every other sink uses):
    /// the result, the cursor and the bytes.
    fn assert_write_matches_the_bit_path<T: Bits>(
        layout: Layout,
        (lead, kind): (u32, u8),
        raw: u128,
    ) {
        type Outcome = (Result<(), (ErrorKind, usize)>, usize, Vec<u8>);
        fn flat(result: Result<(), BitError>) -> Result<(), (ErrorKind, usize)> {
            result.map_err(|e| (e.kind, e.at))
        }
        let mask = match T::BITS {
            0 => 0,
            bits if bits >= 128 => u128::MAX,
            bits => (1 << bits) - 1,
        };
        let value = || T::from_bits(raw & mask);
        let context = format!("{layout:?} lead {lead} prefix {kind} width {}", T::BITS);
        let start = || {
            let mut writer = BitWriter::with_layout(layout);
            write_prefix(&mut writer, lead, kind);
            // A clone starts a session of its own and must write as the original does.
            writer.clone()
        };
        let done = |result, writer: BitWriter| -> Outcome {
            (flat(result), writer.bit_pos, writer.into_bytes())
        };

        let mut slow = start();
        let result = write_by_bits(&mut slow, value());
        let expected = done(result, slow);
        let mut fast = start();
        let result = Sink::write(&mut fast, value());
        assert_eq!(done(result, fast), expected, "{context}");
        let mut inherent = start();
        let result = BitWriter::write(&mut inherent, value());
        assert_eq!(done(result, inherent), expected, "{context}, inherent");

        #[cfg(feature = "bytes")]
        {
            let mut writer = BytesWriter::with_layout(layout);
            write_prefix(&mut writer, lead, kind);
            let result = flat(writer.write(value()));
            let got = (result, writer.bit_pos(), writer.freeze().to_vec());
            assert_eq!(got, expected, "{context}, bytes");
        }
        #[cfg(feature = "tokio")]
        {
            let mut dst = bytes::BytesMut::from(&b"kept"[..]);
            let mut frame = FrameWriter::new(&mut dst, layout);
            write_prefix(&mut frame, lead, kind);
            let result = flat(frame.write(value()));
            let end = frame.bit_pos();
            frame.keep();
            assert_eq!(&dst[..4], b"kept", "{context}, frame");
            assert_eq!(
                (result, end, dst[4..].to_vec()),
                expected,
                "{context}, frame"
            );
        }
    }

    #[test]
    fn write_appends_whole_bytes_exactly_as_the_bit_path_does() {
        use crate::{u24, u40, u48, u56, u72, u80, u88, u96, u104, u112, u120};
        let mut state = 0xD6E8_FEB8_6659_FD93;
        for bit in [BitOrder::Msb, BitOrder::Lsb] {
            for byte in [ByteOrder::Big, ByteOrder::Little] {
                let layout = Layout { bit, byte };
                // From a byte boundary and from every offset inside a byte, after zero,
                // one and two whole bytes, behind each kind of earlier write.
                for lead in 0..=17 {
                    for kind in 0..4 {
                        let raw =
                            u128::from(noise(&mut state)) << 64 | u128::from(noise(&mut state));
                        let before = (lead, kind);
                        macro_rules! each {
                            ($($value:ty),*) => {$(
                                assert_write_matches_the_bit_path::<$value>(layout, before, raw);
                            )*};
                        }
                        each!(bool, u4, u12, u8, u16, u24, u32, u40, u48, u56, u64, u72);
                        each!(u80, u88, u96, u104, u112, u120, u128);
                        each!(Own<true>, Own<false>, Dirty, Blank<0>, Blank<136>);
                    }
                }
            }
        }
    }

    #[test]
    fn a_typed_write_refuses_a_position_it_cannot_count() {
        let mut w = BitWriter::new();
        // Byte-aligned, eight bits from the bound: a sixteen-bit value does not fit.
        w.bit_pos = usize::MAX - 7;
        let error = w.write(0x0102u16).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::PositionOverflow, usize::MAX - 7)
        );
        // A width no write accepts is refused for that first, as by the bit path.
        let error = w.write(Blank::<136>).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::TooWide { width: 136 }, usize::MAX - 7)
        );
        // From inside a byte the bit path reports the same bound.
        w.bit_pos = usize::MAX - 3;
        let error = w.write(0u8).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::PositionOverflow, usize::MAX - 3)
        );
        assert_eq!(w.bit_pos, usize::MAX - 3);
        assert_eq!(w.bytes, []);
    }

    #[test]
    fn writer_and_stream_positions_fail_at_the_bound_instead_of_wrapping() {
        let mut w = BitWriter::new();
        w.bit_pos = usize::MAX - 3;
        assert_eq!(
            w.write_bits(0, 8).unwrap_err().kind,
            ErrorKind::PositionOverflow
        );
        assert_eq!(w.bit_pos, usize::MAX - 3);

        let mut r = StreamBitReader::new(&[0xFFu8][..]);
        r.pos = usize::MAX - 3;
        assert_eq!(
            r.read_bits(8).unwrap_err().kind,
            ErrorKind::PositionOverflow
        );
        assert_eq!(r.pos, usize::MAX - 3);
    }

    #[test]
    fn unaligned_round_trip() {
        let mut w = BitWriter::new();
        w.write(u4::new(0xA)).unwrap();
        w.write(u12::new(0xBCD)).unwrap();
        assert_eq!(w.bit_len(), 16);
        let bytes = w.into_bytes();
        assert_eq!(bytes, [0xAB, 0xCD]);

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
        assert_eq!(r.read::<u12>().unwrap(), u12::new(0xBCD));
        assert_eq!(r.remaining_bits(), 0);
    }

    #[test]
    fn eof_is_an_error_not_a_panic() {
        let mut r = BitReader::new(&[0xFF]);
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xF));
        let err = r.read_bits(8).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 4
            }
        );
        assert_eq!(err.at, 4, "error records the bit offset");
        assert!(err.field.is_none(), "no field context at the reader level");
    }

    #[test]
    fn too_wide_is_rejected() {
        let mut r = BitReader::new(&[0u8; 32]);
        let err = r.read_bits(129).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TooWide { width: 129 });
    }

    #[test]
    fn stream_reader_matches_slice_up_to_128_bits() {
        // The `Source` contract allows reads up to 128 bits; the forward streaming
        // reader must agree with the slice reader across the whole range, including
        // wide (> 64-bit) and byte-straddling reads.
        let bytes: Vec<u8> = (0u8..16).collect(); // 0x00 01 02 … 0F

        // A single 128-bit read.
        let mut s = StreamBitReader::new(&bytes[..]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(s.read_bits(128).unwrap(), r.read_bits(128).unwrap());

        // A 100-bit then 28-bit split (each crosses byte boundaries and the second
        // starts mid-byte, exercising the leftover-bits path).
        let mut s = StreamBitReader::new(&bytes[..]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(s.read_bits(100).unwrap(), r.read_bits(100).unwrap());
        assert_eq!(s.read_bits(28).unwrap(), r.read_bits(28).unwrap());

        // Over-wide is rejected at 128 now, not 64.
        let mut s = StreamBitReader::new(&bytes[..]);
        assert_eq!(
            s.read_bits(65).unwrap(),
            BitReader::new(&bytes).read_bits(65).unwrap(),
            "a 65-bit read used to be rejected"
        );
        let mut s = StreamBitReader::new(&bytes[..]);
        assert_eq!(
            s.read_bits(129).unwrap_err().kind,
            ErrorKind::TooWide { width: 129 }
        );
    }

    #[test]
    fn stream_reader_honors_every_layout() {
        // The streaming reader must agree with the slice reader in *all four* bit/byte
        // orders — not just the default `Msb`/`Big`. (An earlier `read_bits` hardcoded
        // MSB-first extraction, so `Lsb` layouts decoded wrong here alone; and the
        // inherent `read` skipped byte order, so it disagreed with `T::decode`.)
        let bytes: Vec<u8> = (0u8..16).collect();
        for bit in [BitOrder::Msb, BitOrder::Lsb] {
            for byte in [ByteOrder::Big, ByteOrder::Little] {
                let layout = Layout { bit, byte };

                // `read_bits` parity across widths + a mid-byte split (byte-aligned and not).
                let mut s = StreamBitReader::with_layout(&bytes[..], layout);
                let mut r = BitReader::with_layout(&bytes, layout);
                for n in [12u32, 20, 96] {
                    assert_eq!(
                        s.read_bits(n).unwrap(),
                        r.read_bits(n).unwrap(),
                        "read_bits({n}) diverged for {bit:?}/{byte:?}"
                    );
                }

                // The inherent `read` must apply byte order, matching the slice reader's
                // typed read (and therefore `T::decode`).
                let mut s = StreamBitReader::with_layout(&bytes[..], layout);
                let mut r = BitReader::with_layout(&bytes, layout);
                assert_eq!(
                    s.read::<u16>().unwrap(),
                    r.read::<u16>().unwrap(),
                    "read::<u16> diverged for {bit:?}/{byte:?}"
                );
                assert_eq!(
                    s.read::<u32>().unwrap(),
                    r.read::<u32>().unwrap(),
                    "read::<u32> diverged for {bit:?}/{byte:?}"
                );
            }
        }
    }

    // --- BitError: Display for every ErrorKind, and the offset/field suffix --------

    use alloc::string::{String, ToString};

    #[test]
    fn display_unexpected_eof() {
        let e = BitError::new(
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8,
            },
            0,
        );
        assert_eq!(
            e.to_string(),
            "unexpected end of input: needed 16 bits, 8 remain at bit 0"
        );
    }

    #[test]
    fn display_incomplete_with_and_without_hint() {
        assert_eq!(
            BitError::new(ErrorKind::Incomplete { needed: Some(3) }, 8).to_string(),
            "incomplete: need at least 3 more bytes at bit 8",
        );
        assert_eq!(
            BitError::new(ErrorKind::Incomplete { needed: None }, 8).to_string(),
            "incomplete: need more bytes at bit 8",
        );
    }

    #[test]
    fn display_trailing_too_wide_not_seekable_buffer_full() {
        assert_eq!(
            BitError::new(ErrorKind::TrailingBytes { remaining: 2 }, 16).to_string(),
            "2 trailing bytes after the message at bit 16",
        );
        assert_eq!(
            BitError::new(ErrorKind::TooWide { width: 129 }, 0).to_string(),
            "field width 129 exceeds the 128-bit carrier at bit 0",
        );
        assert_eq!(
            BitError::new(ErrorKind::NotSeekable, 4).to_string(),
            "seek to a position this source cannot reach at bit 4",
        );
        assert_eq!(
            BitError::new(ErrorKind::BufferFull { cap: 64 }, 0).to_string(),
            "buffered source exceeded its 64-byte cap at bit 0",
        );
    }

    #[test]
    fn display_bad_magic_and_convert() {
        assert_eq!(
            BitError::bad_magic(0xCAFE, 0x0000, 0).to_string(),
            "bad magic: expected 0xcafe, found 0x0 at bit 0",
        );
        assert_eq!(
            BitError::convert(String::from("nope"), 8).to_string(),
            "conversion failed: nope at bit 8",
        );
    }

    #[test]
    fn display_no_matching_variant() {
        let miss = |observed| {
            BitError::no_matching_variant(
                TypeId::of::<u8>(),
                &("Kind", DispatchKind::Magic),
                observed,
                8,
            )
            .to_string()
        };
        assert_eq!(
            miss(Some(DispatchValue::Integer(0x0a))),
            "unrecognized Kind discriminant: 0xa at bit 8",
        );
        // Byte-string magics render as one prefixed hex run, never bare decimal-looking bytes.
        assert_eq!(
            miss(Some(DispatchValue::Bytes(Vec::from(*b"XY")))),
            "unrecognized Kind discriminant: 0x5859 at bit 8",
        );
        assert_eq!(miss(None), "unrecognized Kind discriminant at bit 8");
    }

    #[test]
    fn display_appends_field_span_when_set() {
        let e = BitError::new(ErrorKind::TooWide { width: 200 }, 12).in_field("payload");
        assert_eq!(
            e.to_string(),
            "field width 200 exceeds the 128-bit carrier at bit 12 (field `payload`)"
        );
    }

    #[test]
    fn display_io_kind() {
        let e = BitError::new(ErrorKind::Io(std::io::ErrorKind::BrokenPipe), 0);
        assert!(e.to_string().starts_with("I/O error:"));
    }

    // --- BitError constructors and the two From bridges ----------------------------

    #[test]
    fn in_field_records_only_the_innermost() {
        let e = BitError::new(ErrorKind::NotSeekable, 0)
            .in_field("inner")
            .in_field("outer"); // ignored — inner already set
        assert_eq!(e.field, Some("inner"));
    }

    #[test]
    fn a_magic_mismatch_keeps_both_sides_of_a_narrow_magic_and_what_was_read_of_a_wide_one() {
        let narrow = MagicMismatch::new(0xC0DE, u128::from(u64::MAX));
        assert_eq!(narrow.expected(), Some(MagicValue::Int(0xC0DE)));
        assert_eq!(narrow.found(), Some(MagicValue::Int(u128::from(u64::MAX))));
        assert_eq!(
            narrow.to_string(),
            "expected 0xc0de, found 0xffffffffffffffff"
        );
        // One bit past 64 on either side: the constant is the type's own and is not kept.
        let wide_value = (0xAB_u128 << 100) | 0x1234_5678_9ABC_DEF0;
        for wide in [
            MagicMismatch::new(1 << 64, wide_value),
            MagicMismatch::new(7, wide_value),
        ] {
            assert_eq!(wide.expected(), None);
            assert_eq!(wide.found(), Some(MagicValue::Int(wide_value)));
            assert_eq!(wide.to_string(), format!("found {wide_value:#x}"));
        }
        // A wide constant against a narrow read (a zeroed buffer): still only what was read.
        let zeroed = MagicMismatch::new(1 << 64, 0);
        assert_eq!(
            (zeroed.expected(), zeroed.found()),
            (None, Some(MagicValue::Int(0)))
        );
        assert_eq!(zeroed, MagicMismatch::new((1 << 64) + 1, 0));
        assert_ne!(MagicMismatch::new(1, 2), MagicMismatch::new(1, 3));
        assert_ne!(MagicMismatch::new(1, 2), MagicMismatch::new(2, 2));
        let bytes = MagicMismatch::bytes(b"AB");
        assert_eq!(bytes.expected(), Some(MagicValue::Bytes(b"AB")));
        assert_eq!(bytes.found(), None);
        assert_eq!(bytes.to_string(), "expected 0x4142");
        assert_eq!(
            MagicMismatch::bytes(&[0x00, 0x0A]).to_string(),
            "expected 0x000a"
        );
        assert_eq!(
            format!("{narrow:?}"),
            "MagicMismatch { expected: Some(Int(49374)), found: Some(Int(18446744073709551615)) }"
        );
        assert_eq!(
            BitError::bad_magic_bytes(b"AB", 16).to_string(),
            "bad magic: expected 0x4142 at bit 16"
        );
    }

    #[test]
    fn a_conversion_carries_its_message_and_a_tag_only_if_one_was_attached() {
        let plain = BitError::convert("out of range", 8);
        assert_eq!(plain.tag(), None);
        assert_eq!(
            plain.kind,
            ErrorKind::Convert(Conversion::new("out of range"))
        );
        assert_eq!(
            plain.to_string(),
            "conversion failed: out of range at bit 8"
        );
        let tagged = BitError::convert(String::from("out of range"), 8).with_tag("crate.range");
        assert_eq!(tagged.tag(), Some("crate.range"));
        let ErrorKind::Convert(conversion) = &tagged.kind else {
            panic!("{tagged:?}")
        };
        assert_eq!(
            (conversion.message(), conversion.tag()),
            ("out of range", Some("crate.range"))
        );
        assert_eq!(
            format!("{conversion:?}"),
            r#"Conversion { message: "out of range", tag: Some("crate.range") }"#
        );
        // Equal by message and tag, whatever the message was built from.
        assert_eq!(
            *conversion,
            Conversion::new("out of range").with_tag("crate.range")
        );
        assert_ne!(tagged.kind, plain.kind);
        assert_ne!(
            *conversion,
            Conversion::new("other").with_tag("crate.range")
        );
        // Any other kind of error has no tag and takes none.
        let other = BitError::new(ErrorKind::NotSeekable, 0).with_tag("crate.range");
        assert_eq!((other.tag(), &other.kind), (None, &ErrorKind::NotSeekable));
    }

    #[test]
    fn each_failure_of_the_crates_own_says_what_it_is() {
        for (kind, text) in [
            (
                ErrorKind::AssertFailed {
                    message: Cow::Borrowed("`level < 12`"),
                },
                "assertion failed: `level < 12`",
            ),
            (
                ErrorKind::InvalidUtf8 { valid_up_to: 3 },
                "invalid UTF-8 after 3 valid bytes of a string",
            ),
            (
                ErrorKind::ValueOutOfRange { bits: 4 },
                "value does not fit in 4 bits",
            ),
            (
                ErrorKind::CountOutOfRange {
                    count: 300,
                    bits: 8,
                },
                "count 300 does not fit in 8 bits",
            ),
            (
                ErrorKind::CountOutOfRange {
                    count: usize::MAX,
                    bits: usize::BITS,
                },
                "count does not fit in a usize",
            ),
            (
                ErrorKind::VarintOverflow { bits: 32 },
                "variable-length integer does not fit in 32 bits",
            ),
            (
                ErrorKind::EmbeddedNul { index: 5 },
                "embedded NUL at byte 5: a NUL-terminated string cannot represent it",
            ),
        ] {
            assert_eq!(
                BitError::new(kind, 8).to_string(),
                format!("{text} at bit 8")
            );
        }
        assert!(
            BitError::new(ErrorKind::UnresolvedWireLen, 0)
                .to_string()
                .starts_with("unresolved `WireLen::Auto`")
        );
    }

    #[test]
    fn an_assert_message_is_borrowed_unless_it_interpolates() {
        assert!(matches!(
            assert_message(format_args!("too deep")),
            Cow::Borrowed("too deep")
        ));
        let depth = 9;
        let formatted = assert_message(format_args!("depth {depth}"));
        assert!(matches!(&formatted, Cow::Owned(text) if text == "depth 9"));
    }

    #[test]
    fn a_retry_signal_gives_its_hint_and_any_other_error_comes_back_whole() {
        let retry = BitError::new(ErrorKind::Incomplete { needed: Some(3) }, 8);
        assert_eq!(retry.into_retry(), Ok(Some(3)));
        let unknown = BitError::new(ErrorKind::Incomplete { needed: None }, 8);
        assert_eq!(unknown.into_retry(), Ok(None));
        let other = BitError::convert("no", 8).in_field("f");
        assert_eq!(other.clone().into_retry(), Err(other));
        // Out of input for good is not a retry.
        let at_eof = BitError::new(ErrorKind::IncompleteAtEof { needed: Some(3) }, 8);
        assert_eq!(at_eof.clone().into_retry(), Err(at_eof));
    }

    #[test]
    fn is_incomplete_is_true_only_for_incomplete() {
        assert!(BitError::new(ErrorKind::Incomplete { needed: None }, 0).is_incomplete());
        assert!(!BitError::new(ErrorKind::NotSeekable, 0).is_incomplete());
    }

    #[test]
    fn construction_error_bridges_to_a_value_out_of_range_error() {
        let e: BitError = crate::error::WidthError::ValueTooLarge { value: 99, bits: 4 }.into();
        assert_eq!(e.kind, ErrorKind::ValueOutOfRange { bits: 4 });
        let negative: BitError = crate::error::WidthError::Negative { value: -1, bits: 7 }.into();
        assert_eq!(negative.kind, ErrorKind::ValueOutOfRange { bits: 7 });
        assert_eq!(e.at, 0);
        assert!(e.to_string().contains("does not fit in 4 bits"));
    }

    #[test]
    fn io_error_bridges_to_an_io_kind() {
        let e: BitError = std::io::Error::new(std::io::ErrorKind::TimedOut, "x").into();
        assert_eq!(e.kind, ErrorKind::Io(std::io::ErrorKind::TimedOut));
        assert_eq!(e.at, 0);
    }

    // --- BitWriter: the LSB-order constructor and the over-wide guard ---------------

    #[test]
    fn writer_with_order_lsb_packs_first_field_in_the_low_bits() {
        let mut w = BitWriter::with_order(BitOrder::Lsb);
        w.write(u4::new(0xA)).unwrap(); // -> low nibble
        w.write(u4::new(0xB)).unwrap(); // -> high nibble
        assert_eq!(w.into_bytes(), [0xBA]);
    }

    #[test]
    fn cursor_layout_matrix_bit_and_byte_order_are_independent() {
        // The two axes — `bit` (how a sub-byte field packs) and `byte` (how a byte-multiple
        // value serializes) — must compose independently at the cursor level (`extract_bits`/
        // `emit_bits`/`apply_byte_order`). Write a nibble pair (bit-order sensitive) then a u16
        // word (byte-order sensitive) under each of the four layouts.
        let combos = [
            (BitOrder::Msb, ByteOrder::Big),
            (BitOrder::Msb, ByteOrder::Little),
            (BitOrder::Lsb, ByteOrder::Big),
            (BitOrder::Lsb, ByteOrder::Little),
        ];
        let mut encs = Vec::new();
        for (bit, byte) in combos {
            let layout = Layout { bit, byte };
            let mut w = BitWriter::with_layout(layout);
            w.write(u4::new(0xA)).unwrap();
            w.write(u4::new(0xB)).unwrap();
            w.write(0x1234u16).unwrap();
            let bytes = w.into_bytes();
            // Every layout round-trips through a reader with the same layout.
            let mut r = BitReader::with_layout(&bytes, layout);
            assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
            assert_eq!(r.read::<u4>().unwrap(), u4::new(0xB));
            assert_eq!(r.read::<u16>().unwrap(), 0x1234);
            encs.push(bytes);
        }
        // All four corners are golden under the natural-layout rule (`apply_byte_order`):
        // each bit order has a natural byte layout — big-endian under MSB, little-endian
        // under LSB (the DBC-Intel layout) — and the byte-order knob swaps a byte-multiple
        // value only when it differs from that natural layout.
        assert_eq!(encs[0], [0xAB, 0x12, 0x34]); // msb / big    (natural — no swap)
        assert_eq!(encs[1], [0xAB, 0x34, 0x12]); // msb / little (differs — word swaps)
        assert_eq!(encs[2], [0xBA, 0x12, 0x34]); // lsb / big    (differs — word swaps)
        assert_eq!(encs[3], [0xBA, 0x34, 0x12]); // lsb / little (natural — no swap)
        // Flipping byte order changes only the word's bytes, never the nibble byte.
        assert_eq!(encs[0][0], encs[1][0]);
        assert_ne!(encs[0][1..], encs[1][1..]);
        assert_eq!(encs[2][0], encs[3][0]);
        assert_ne!(encs[2][1..], encs[3][1..]);
        // All four corners are pairwise distinct — neither axis aliases the other.
        for i in 0..encs.len() {
            for j in (i + 1)..encs.len() {
                assert_ne!(encs[i], encs[j], "layout corners {i} and {j} alias");
            }
        }
    }

    #[test]
    fn write_bits_rejects_over_128() {
        let mut w = BitWriter::new();
        assert_eq!(
            w.write_bits(0, 129).unwrap_err().kind,
            ErrorKind::TooWide { width: 129 }
        );
    }

    // --- Source/Sink trait DEFAULT methods, via minimal in-test impls --------------

    /// A forward-only `Source` that overrides only the two required methods, so calling
    /// the rest exercises the trait's default `byte_order`/`seek_to_bit`/`read`.
    struct TinySource<'a> {
        bytes: &'a [u8],
        pos: usize,
        depth: u16,
    }
    impl sealed::Sealed for TinySource<'_> {}

    impl Source for TinySource<'_> {
        fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
            let n = n as usize;
            let total = self.bytes.len() * 8;
            if self.pos + n > total {
                return Err(BitError::new(
                    ErrorKind::UnexpectedEof {
                        needed: n,
                        remaining: total - self.pos,
                    },
                    self.pos,
                ));
            }
            let mut acc = 0u128;
            for k in 0..n {
                let p = self.pos + k;
                acc = (acc << 1) | u128::from((self.bytes[p >> 3] >> (7 - (p & 7))) & 1);
            }
            self.pos += n;
            Ok(acc)
        }
        fn bit_pos(&self) -> usize {
            self.pos
        }
        fn __bnb_depth(&mut self) -> &mut u16 {
            &mut self.depth
        }
    }

    #[test]
    fn source_default_byte_order_is_big() {
        let s = TinySource {
            bytes: &[0],
            pos: 0,
            depth: 0,
        };
        assert_eq!(s.byte_order(), ByteOrder::Big);
    }

    #[test]
    fn source_default_seek_is_not_seekable() {
        let mut s = TinySource {
            bytes: &[0, 0],
            pos: 0,
            depth: 0,
        };
        assert_eq!(s.seek_to_bit(8).unwrap_err().kind, ErrorKind::NotSeekable);
    }

    #[test]
    fn source_default_read_dispatches_through_read_bits() {
        let mut s = TinySource {
            bytes: &[0xAB, 0xCD],
            pos: 0,
            depth: 0,
        };
        assert_eq!(s.read::<u8>().unwrap(), 0xAB);
        assert_eq!(s.read::<u8>().unwrap(), 0xCD);
    }

    #[test]
    fn source_default_byte_runs_read_per_byte() {
        let mut s = TinySource {
            bytes: &[0x0A, 0x0B, 0x0C, 0x0D, 0x0E],
            pos: 4,
            depth: 0,
        };
        // Unaligned: each byte straddles two input bytes.
        assert_eq!(s.read_bytes(2).unwrap(), [0xA0, 0xB0]);
        let mut buf = [0u8; 2];
        s.read_into(&mut buf).unwrap();
        assert_eq!(buf, [0xC0, 0xD0]);
        assert_eq!(s.bit_pos(), 36);
        let err = s.read_bytes(1).unwrap_err();
        assert!(matches!(
            err.kind,
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 4
            }
        ));
        assert!(s.read_into(&mut buf).is_err());
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

    #[test]
    fn decode_exact_with_accepts_input_the_closure_consumes_exactly() {
        let v = decode_exact_with(&[0xAB, 0xCD], Layout::default(), |r| r.read_bits(16));
        assert_eq!(v.unwrap(), 0xABCD);
    }

    #[test]
    fn decode_exact_with_rejects_a_trailing_byte() {
        let err = decode_exact_with(&[0xAB, 0xCD, 0xEE], Layout::default(), |r| r.read_bits(16))
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::TrailingBytes { remaining: 1 });
        assert_eq!(err.at, 16, "the error points at the first unconsumed bit");
    }

    #[test]
    fn decode_exact_with_accepts_the_unread_bits_of_a_final_partial_byte() {
        // Only *whole* trailing bytes are an error: 12 bits read out of 2 bytes is exact.
        let v = decode_exact_with(&[0xAB, 0xC0], Layout::default(), |r| r.read_bits(12));
        assert_eq!(v.unwrap(), 0xABC);
    }

    #[test]
    fn bit_reader_borrows_exact_aligned_input_spans() {
        let bytes = [0x30, 0x03, 0x02, 0x01, 0x07];
        let mut source = BitReader::new(&bytes);
        assert_eq!(source.read_slice(2).unwrap(), &bytes[..2]);
        assert_eq!(source.read_slice(3).unwrap(), &bytes[2..]);
        assert_eq!(source.remaining_bits(), 0);

        let mut unaligned = BitReader::new(&bytes);
        unaligned.read_bits(1).unwrap();
        assert_eq!(
            unaligned.read_slice(1).unwrap_err().kind,
            ErrorKind::NotByteAligned
        );
    }

    /// A `Sink` that overrides only the required methods, exercising the default
    /// `byte_order`/`write`.
    struct TinySink {
        out: Vec<u8>,
        bit: usize,
    }
    impl sealed::Sealed for TinySink {}

    impl Sink for TinySink {
        fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
            let n = n as usize;
            for k in 0..n {
                let p = self.bit + k;
                if p >> 3 == self.out.len() {
                    self.out.push(0);
                }
                if (value >> (n - 1 - k)) & 1 != 0 {
                    self.out[p >> 3] |= 1 << (7 - (p & 7));
                }
            }
            self.bit += n;
            Ok(())
        }
        fn bit_pos(&self) -> usize {
            self.bit
        }
    }

    #[test]
    fn sink_default_byte_order_is_big() {
        let s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        assert_eq!(s.byte_order(), ByteOrder::Big);
    }

    #[test]
    fn sink_default_write_dispatches_through_write_bits() {
        let mut s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        s.write(0xABu8).unwrap();
        s.write(0xCDu8).unwrap();
        assert_eq!(s.out, [0xAB, 0xCD]);
    }

    #[test]
    fn sink_default_byte_runs_write_per_byte() {
        let mut s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        s.write_bits(0xF, 4).unwrap();
        s.write_bytes(&[0xAB, 0xCD]).unwrap();
        assert_eq!(s.out, [0xFA, 0xBC, 0xD0]);
        assert_eq!(s.bit, 20);
    }

    #[test]
    fn sink_default_lends_no_tail_so_an_integer_run_is_written_each() {
        let mut s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        assert!(s.aligned_tail(4).is_none());
        u16::encode_slice(&[0x0102, 0x0304], &mut s).unwrap();
        assert_eq!(s.out, [1, 2, 3, 4]);
        assert_eq!(s.bit, 32);
    }

    // --- BitEncode/DecodeWith defaults for a leaf type -----------------------------

    #[test]
    fn leaf_canonical_encode_defaults_to_verbatim() {
        let mut a = BitWriter::new();
        let mut b = BitWriter::new();
        BitEncode::bit_encode(&0xABCDu16, &mut a).unwrap();
        BitEncode::canonical_bit_encode(&0xABCDu16, &mut b).unwrap();
        assert_eq!(a.into_bytes(), b.into_bytes());
    }

    #[test]
    fn leaf_decode_with_and_encode_with_unit_args() {
        let mut r = BitReader::new(&[0xAB, 0xCD]);
        assert_eq!(
            <u16 as DecodeWith<()>>::decode_with(&mut r, ()).unwrap(),
            0xABCD
        );
        let mut w = BitWriter::new();
        EncodeWith::encode_with(&0xABCDu16, &mut w, ()).unwrap();
        assert_eq!(w.into_bytes(), [0xAB, 0xCD]);
    }

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

    #[test]
    fn count_prefix_primitive_boundaries() {
        // In-range: exact fit at the top of the range.
        assert_eq!(u8::try_from_count(255).unwrap(), 255u8);
        // Out of range: checked, not wrapped.
        assert_eq!(
            u8::try_from_count(256).unwrap_err(),
            crate::error::WidthError::ValueTooLarge {
                value: 256,
                bits: 8
            }
        );
        assert_eq!(u16::try_from_count(65_535).unwrap(), 65_535u16);
        assert_eq!(u8::to_count(200), 200);
        assert_eq!(u32::to_count(70_000), 70_000);
    }

    #[test]
    fn count_prefix_uint_boundaries() {
        // u12: 4095 fits, 4096 does not.
        assert_eq!(u12::try_from_count(4095).unwrap(), u12::new(4095));
        assert_eq!(
            u12::try_from_count(4096).unwrap_err(),
            crate::error::WidthError::ValueTooLarge {
                value: 4096,
                bits: 12
            }
        );
        assert_eq!(u12::new(4095).to_count(), 4095);
    }

    #[test]
    fn count_prefix_uint_never_truncates_before_the_check() {
        // 300 as u8 would wrap to 44 — a masked `from_raw`/`try_new(len as u8)` path
        // would then accept it. The widen-then-compare must reject instead.
        assert_eq!(
            u4::try_from_count(300).unwrap_err(),
            crate::error::WidthError::ValueTooLarge {
                value: 300,
                bits: 4
            }
        );
    }

    #[test]
    fn count_prefix_round_trips() {
        for len in [0usize, 1, 15, 255, 4095] {
            assert_eq!(u16::try_from_count(len).unwrap().to_count(), len);
            assert_eq!(u12::try_from_count(len).unwrap().to_count(), len);
        }
    }

    #[test]
    fn bulk_bytes_round_trip() {
        let mut w = BitWriter::new();
        w.write_bytes(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        let bytes = w.into_bytes();
        assert_eq!(bytes, [0xDE, 0xAD, 0xBE, 0xEF]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bytes(4).unwrap(), [0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(r.remaining_bits(), 0);
    }

    #[test]
    fn positioning_amounts_keep_low_32_bits_without_debug_only_panics() {
        assert_eq!(u32::MAX.bytes(), 0xffff_fff8);
        assert_eq!((-1i32).bits(), u32::MAX);
        assert_eq!((-1i32).bytes(), 0xffff_fff8);
        assert_eq!((1u64 << 32).bits(), 0);
        assert_eq!((1u64 << 29).bytes(), 0);
    }

    #[test]
    fn oversized_custom_bits_reports_width_error_before_byte_swap() {
        #[derive(Clone, Copy)]
        struct InvalidWidth;
        impl Bits for InvalidWidth {
            const BITS: u32 = 136;
            fn into_bits(self) -> u128 {
                0
            }
            fn from_bits(_: u128) -> Self {
                Self
            }
        }

        let mut writer = BitWriter::with_layout(Layout {
            bit: BitOrder::Msb,
            byte: ByteOrder::Little,
        });
        assert_eq!(
            writer.write(InvalidWidth).unwrap_err().kind,
            ErrorKind::TooWide { width: 136 }
        );
        assert_eq!(writer.into_bytes(), b"");
    }

    #[test]
    fn bulk_bytes_work_at_a_bit_offset() {
        // Bytes need not be aligned: write a nibble, then bytes straddling.
        let mut w = BitWriter::new();
        w.write(u4::new(0xF)).unwrap();
        w.write_bytes(&[0xAB, 0xCD]).unwrap();
        let bytes = w.into_bytes();
        assert_eq!(bytes, [0xFA, 0xBC, 0xD0]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xF));
        assert_eq!(r.read_bytes(2).unwrap(), [0xAB, 0xCD]);
    }

    #[test]
    fn read_bytes_hostile_length_is_eof_not_alloc() {
        // A huge n against a 2-byte source: push-per-byte means a fast EOF, no
        // pre-allocation from the untrusted length.
        let mut r = BitReader::new(&[0x01, 0x02]);
        let err = r.read_bytes(usize::MAX).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }

    #[test]
    fn read_into_fills_and_errors_short() {
        let mut r = BitReader::new(&[0x0A, 0x0B, 0x0C]);
        let mut buf = [0u8; 3];
        r.read_into(&mut buf).unwrap();
        assert_eq!(buf, [0x0A, 0x0B, 0x0C]);

        let mut r = BitReader::new(&[0x0A]);
        let mut buf = [0u8; 3];
        let err = r.read_into(&mut buf).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }
}

#[cfg(test)]
mod component;
