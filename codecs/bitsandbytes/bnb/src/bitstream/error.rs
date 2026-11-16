//! The codec's error type, [`BitError`], and the detail it carries.

#[cfg(doc)]
use super::{BitBuf, BitDecode, BitReader, Source};
#[cfg(doc)]
use crate::field::Bits;
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::TypeId;
use core::fmt;

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

#[cfg(test)]
mod unit {
    use alloc::string::{String, ToString};

    use super::*;

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
}
