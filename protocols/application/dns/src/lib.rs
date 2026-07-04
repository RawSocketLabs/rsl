//! **dns** — a DNS (RFC 1034/1035) message codec on the [`bnb`] bit-aware codec.
//!
//! Dual-use and from-scratch: the guided path emits/parses RFC-correct messages, while
//! unknown values (record types, opcodes, classes) are preserved as `Custom`/`Other`
//! rather than rejected, and unknown RDATA is kept as raw bytes rather than misparsed.
//!
//! The **pure codec**: decode (following compression pointers inline), plus both encode
//! forms — [`to_bytes`](Message::to_bytes) (uncompressed) and
//! [`to_compressed_bytes`](Message::to_compressed_bytes) (RFC 1035 §4.1.4 name compression).
//! A network client (a resolver) is the remaining piece, pending the external `rawsock`.
//!
//! ```
//! use dns::Message;
//!
//! // A `www.example.com` A-record response (uncompressed on the wire).
//! let wire = [
//!     0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x03,
//!     b'w', b'w', b'w', 0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c',
//!     b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01, 0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01,
//!     0x00, 0x00, 0x00, 0x3c, 0x00, 0x04, 0x5d, 0xb8, 0xd8, 0x22,
//! ];
//! let msg = Message::decode_exact(&wire).unwrap();
//! assert_eq!(msg.questions[0].name.to_string(), "www.example.com");
//! assert!(msg.header.is_response());
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod header;
pub mod message;
pub mod name;
pub mod question;
pub mod rdata;
pub mod record;

/// Re-exported from `bnb`: the auto-deriving, overridable length type used for the header
/// section counts and `rdlength` (`auto()` to derive, `set(n)` to forge — dual-use).
pub use bnb::WireLen;
pub use error::{DnsError, Result};
pub use header::{Header, Op, RCode, State};
pub use message::Message;
pub use name::{CompressionDict, Name};
pub use question::{QClass, QType, Question};
pub use rdata::{Mx, RData, Soa};
pub use record::{RClass, RType, Record};
