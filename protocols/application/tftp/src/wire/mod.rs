//! The TFTP wire format: every packet, as a typed value (RFC 1350; options per
//! RFC 2347/2348/2349).
//!
//! This is the lowest of the crate's three layers, exposed so you can encode and
//! decode TFTP packets directly. Because TFTP runs over UDP, a packet is always
//! one complete datagram — every type offers `decode(&[u8])` (parse a whole
//! datagram) and `encode() -> Vec<u8>`, and [`Packet`] unifies them behind an
//! opcode-tagged enum.
//!
//! # Message map
//!
//! | Type | Opcode | Packet |
//! |---|---|---|
//! | [`Request`] (RRQ/WRQ) | 1 / 2 | filename, mode, options |
//! | [`Data`] | 3 | block #, payload |
//! | [`Ack`] | 4 | block # |
//! | [`ErrorPacket`] | 5 | error code, message |
//! | [`OptionAck`] | 6 | accepted options (RFC 2347, `options` feature) |
//!
//! Supporting value types: [`Opcode`], [`TransferMode`], [`ErrorCode`], and
//! [`RequestKind`]; with the `options` feature, [`TftpOption`].
//!
//! # Compliant by default, but liberal
//!
//! Every code field keeps unassigned values as a `Custom(..)` variant rather
//! than rejecting them, mode strings are matched case-insensitively and unknown
//! modes preserved, and payload/length limits are *not* imposed at this layer —
//! an over-long DATA block is representable for dual-use. Only the physically
//! unparseable (a truncated field, an unterminated string, an opcode with no
//! typed body) is an error.
//!
//! # Example
//!
//! ```
//! use tftp::wire::{Packet, Request, TransferMode};
//!
//! # fn main() -> Result<(), tftp::error::TftpError> {
//! let rrq = Request::read("boot.img", TransferMode::Octet);
//! let bytes = rrq.encode();
//! assert_eq!(Packet::decode(&bytes)?, Packet::Request(rrq));
//! # Ok(())
//! # }
//! ```

mod ack;
mod codec;
mod data;
mod error_code;
mod error_packet;
mod mode;
#[cfg(feature = "options")]
mod oack;
mod opcode;
#[cfg(feature = "options")]
mod option;
mod packet;
mod request;

pub use ack::Ack;
pub use data::Data;
pub use error_code::ErrorCode;
pub use error_packet::ErrorPacket;
pub use mode::TransferMode;
pub use opcode::Opcode;
pub use packet::Packet;
pub use request::{Request, RequestKind};

#[cfg(feature = "options")]
pub use oack::OptionAck;
#[cfg(feature = "options")]
pub use option::{TftpOption, blksize, timeout_secs, tsize};
