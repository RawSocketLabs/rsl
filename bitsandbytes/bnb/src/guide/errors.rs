//! Errors — position-aware, and the streaming `Incomplete` signal.
//!
//! The codec's error type is [`BitError`](crate::BitError): a [`kind`](crate::ErrorKind),
//! the absolute **bit offset** `at` where it happened, and the **field** being
//! processed when it did (the innermost one wins, like a span). That makes a failure
//! point at the exact place rather than just "parse error".
//!
//! ```
//! use bnb::{bin, ErrorKind};
//!
//! #[bin(big)]
//! #[derive(Debug)]
//! struct Header { a: u16, b: u16 }
//!
//! // Only one byte: reading `a` (needs 16 bits) runs off the end at bit 0.
//! let err = Header::decode_exact(&[0x00]).unwrap_err();
//! assert!(matches!(err.kind, ErrorKind::UnexpectedEof { needed: 16, remaining: 8 }));
//! assert_eq!(err.at, 0);
//! assert_eq!(err.field, Some("a"));        // the field gives the span
//! ```
//!
//! The [`Display`](core::fmt::Display) impl renders all of it:
//!
//! ```
//! # use bnb::bin;
//! # #[bin(big)] #[derive(Debug)] struct Header { a: u16, b: u16 }
//! let err = Header::decode_exact(&[0x00]).unwrap_err();
//! assert_eq!(
//!     err.to_string(),
//!     "unexpected end of input: needed 16 bits, 8 remain at bit 0 (field `a`)",
//! );
//! ```
//!
//! # The variants you'll see
//!
//! [`ErrorKind`](crate::ErrorKind) is non-exhaustive; the common ones:
//!
//! - `UnexpectedEof { needed, remaining }` — ran off the end of a finite slice.
//! - `TrailingBytes { remaining }` — `decode_exact` left whole bytes unconsumed.
//! - `BadMagic { expected, found }` — a `magic` constant didn't match.
//! - `Convert { message }` — a conversion or guard rejected the value: a `try_map` /
//!   `try_wire` converter, a `#[br(assert(...))]` guard, a `WireLen` / `count_prefix`
//!   length that overflowed its prefix type, invalid UTF-8 in a string field, or a
//!   `WidthError` bridged in from checked construction (`try_new`).
//! - `Incomplete { needed }` — a stream ran out mid-message. `BitBuf` supports retry;
//!   a direct forward-only read may already have consumed input.
//! - `IncompleteAtEof { needed }` — a custom codec still requested input in a finite
//!   `BitBuf`/`BinCodec` attempt. Other finite custom-codec entry points must report
//!   their own definitive errors.
//! - `NoProgress` — a streamed message or counted element consumed no bits.
//! - `NotSeekable` / `BufferFull` / `TooWide` / `Io` — seek-on-a-stream, buffer cap,
//!   over-128-bit field, and an I/O failure (carrying just the `std::io::ErrorKind`, not
//!   the full `io::Error`). `NotSeekable`/`BufferFull` are exactly what you hit moving from
//!   slices to streams (a `restore_position` on a forward stream; a message larger than a
//!   `BufSource` cap, with `std`).
//!
//! ```
//! use bnb::{bin, ErrorKind};
//!
//! #[bin(big)]
//! #[derive(Debug)]
//! struct One { v: u8 }
//!
//! #[bin(big, magic = 0xCAFEu16)]
//! #[derive(Debug)]
//! struct Framed { v: u8 }
//!
//! // Trailing bytes after a complete message.
//! let err = One::decode_exact(&[0x01, 0x02]).unwrap_err();
//! assert!(matches!(err.kind, ErrorKind::TrailingBytes { remaining: 1 }));
//!
//! // A magic mismatch points at the magic.
//! let err = Framed::decode_exact(&[0x00, 0x00, 0x00]).unwrap_err();
//! assert!(matches!(err.kind, ErrorKind::BadMagic { expected: 0xCAFE, found: 0x0000 }));
//! assert_eq!(err.field, Some("magic"));
//! ```
//!
//! # Streaming: `Incomplete` means "retry", not "fail"
//!
//! Buffered attempts return `Incomplete` for physical input shortage, not for a logical
//! region boundary or a custom codec's hard error. Its `needed` is additional **bytes**
//! for the next blocked operation (`UnexpectedEof` uses **bits**). It is not the final
//! frame length. `Some(n > 0)` is a lower bound: appending fewer than `n` bytes cannot
//! produce either success or a terminal error for the same retained prefix, numeric attempt
//! cursor, layout, context, and codec-visible state. Compaction/rebasing invalidates that
//! hint and requires an immediate retry; the whole-message readers handle this internally.
//! Custom/speculative codecs **must** return `None` if
//! they cannot prove that bound. `None`/`Some(0)` mean retry after receiving at least one
//! additional byte. EOF always requires an immediate finite attempt. Overstated custom hints
//! can stall a reader; they are not a memory-safety issue. This is a behavioral change from
//! 0.5's best-effort estimates, so custom codec authors must audit their hints when upgrading.
//! [`is_incomplete`](crate::BitError::is_incomplete) distinguishes retry
//! from a definitive failure. Direct forward readers do not roll back consumed input:
//!
//! ```
//! use bnb::{bin, StreamBitReader};
//!
//! #[bin(big)]
//! #[derive(Debug)]
//! struct Quad { v: u32 }
//!
//! // Only 2 of the 4 needed bytes are available so far.
//! let mut s = StreamBitReader::new(&[0x12, 0x34][..]);
//! let err = Quad::decode(&mut s).unwrap_err();
//! assert!(err.is_incomplete()); // shortage, but the forward source has consumed its prefix
//! // For retryable messages, retain the original input and use BitBuf::try_pull instead.
//! ```
//!
//! # Two error types
//!
//! Decoding/encoding yields [`BitError`](crate::BitError). The separate
//! [`WidthError`](crate::WidthError) covers *construction* (`UInt::try_new` and the `TryFrom`
//! impls). A `From<WidthError> for BitError` bridges them, so a construction failure inside
//! a custom `parse_with`/converter can `?`-propagate:
//!
//! ```
//! use bnb::{u4, BitError};
//! fn make(raw: u8) -> Result<u4, BitError> {
//!     let v = u4::try_new(raw)?; // construction Error -> BitError via `?`
//!     Ok(v)
//! }
//! assert!(make(3).is_ok());
//! assert!(make(99).is_err());
//! ```
