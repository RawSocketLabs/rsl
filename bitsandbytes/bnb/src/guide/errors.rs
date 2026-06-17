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
//! The [`Display`](std::fmt::Display) impl renders all of it:
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
//! - `Convert { message }` — a `try_map` converter failed.
//! - `Incomplete { needed }` — a stream ran out mid-message (read more and retry).
//! - `NotSeekable` / `BufferFull` / `TooWide` / `Io` — seek-on-a-stream, buffer cap,
//!   over-128-bit field, and underlying `io::Error`.
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
//! When a forward stream runs out partway through a message, the error is
//! `Incomplete` — a signal to read more bytes and retry the decode, as opposed to a
//! definitive failure. [`is_incomplete`](crate::BitError::is_incomplete) distinguishes
//! the two:
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
//! let err = Quad::decode_from(&mut s).unwrap_err();
//! assert!(err.is_incomplete()); // buffer more and try again — not a parse error
//! ```
//!
//! # Two error types
//!
//! Decoding/encoding yields [`BitError`](crate::BitError). The separate
//! [`Error`](crate::Error) covers *construction* (`UInt::try_new` and the `TryFrom`
//! impls). A `From<Error> for BitError` bridges them, so a construction failure inside
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
