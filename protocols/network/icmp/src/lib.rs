//! **icmp** — an ICMP (RFC 792) message-header codec on the [`bnb`] bit-aware codec.
//!
//! From-scratch and dual-use: [`IcmpHeader`] decodes/encodes the 8-byte header (type, code,
//! checksum, rest-of-header). `checksum` is stored **verbatim** — decode never recomputes,
//! verifies, or rejects it, so a forged checksum survives a round-trip. The `inject` feature
//! adds a `rawsock::Protocol` layer (`Icmp`) that wraps the message data and computes the
//! checksum over the **whole** message.
//!
//! Unlike UDP/TCP, the ICMP checksum is **self-contained** — it covers the ICMP message alone,
//! with no IP pseudo-header — so the `inject` layer ignores the enclosing pseudo-header.
//!
//! [`bnb`]: https://github.com/RawSocketLabs/bitsandbytes
//!
//! ```
//! use icmp::IcmpHeader;
//!
//! let echo = IcmpHeader::echo_request(0x1234, 1);
//! assert_eq!(echo.icmp_type, IcmpHeader::ECHO_REQUEST);
//! assert_eq!((echo.identifier(), echo.sequence()), (0x1234, 1));
//! let wire = echo.to_bytes().unwrap();
//! assert_eq!(wire, [0x08, 0x00, 0x00, 0x00, 0x12, 0x34, 0x00, 0x01]);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use bnb::bin;

/// A typed message view (`IcmpMessage`) over the flat header.
pub mod message;
pub use message::IcmpMessage;

/// The rawsock injection layer — the `inject` feature.
#[cfg(feature = "inject")]
pub mod inject;
#[cfg(feature = "inject")]
pub use inject::Icmp;

/// An ICMP message header (RFC 792): type, code, checksum, and the 4-byte rest-of-header — an
/// 8-byte fixed header. The rest-of-header's meaning is type-specific (for Echo it is the
/// identifier and sequence number).
//~ models rfc792 part="ICMP message header format"
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IcmpHeader {
    /// The message type (8 = Echo Request, 0 = Echo Reply, 3 = Destination Unreachable, …).
    pub icmp_type: u8,
    /// The code — a type-specific subtype (e.g. the Destination Unreachable reason).
    pub code: u8,
    /// The checksum over the whole ICMP message (header + data). Stored verbatim — not
    /// recomputed or verified on decode.
    pub checksum: u16,
    /// The rest-of-header (bytes 4–7); type-specific. For Echo it is
    /// `identifier << 16 | sequence` — see [`identifier`](Self::identifier)/[`sequence`](Self::sequence).
    pub rest_of_header: u32,
}

impl IcmpHeader {
    /// Echo Reply (type 0).
    pub const ECHO_REPLY: u8 = 0;
    /// Destination Unreachable (type 3).
    pub const DEST_UNREACHABLE: u8 = 3;
    /// Echo Request (type 8).
    pub const ECHO_REQUEST: u8 = 8;
    /// Time Exceeded (type 11).
    pub const TIME_EXCEEDED: u8 = 11;

    /// An Echo Request (type 8) header carrying `identifier`/`sequence` in the rest-of-header.
    /// `checksum` is 0 — the `inject` encode fills it over the whole message (or set it).
    #[must_use]
    pub fn echo_request(identifier: u16, sequence: u16) -> Self {
        Self::echo(Self::ECHO_REQUEST, identifier, sequence)
    }

    /// An Echo Reply (type 0) header carrying `identifier`/`sequence`.
    #[must_use]
    pub fn echo_reply(identifier: u16, sequence: u16) -> Self {
        Self::echo(Self::ECHO_REPLY, identifier, sequence)
    }

    fn echo(icmp_type: u8, identifier: u16, sequence: u16) -> Self {
        Self {
            icmp_type,
            code: 0,
            checksum: 0,
            rest_of_header: (u32::from(identifier) << 16) | u32::from(sequence),
        }
    }

    /// The Echo `identifier` — the high half of the rest-of-header (meaningful for Echo types).
    #[must_use]
    pub fn identifier(&self) -> u16 {
        (self.rest_of_header >> 16) as u16
    }

    /// The Echo `sequence` number — the low half of the rest-of-header.
    #[must_use]
    pub fn sequence(&self) -> u16 {
        self.rest_of_header as u16
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn echo_constructors_pack_identifier_and_sequence() {
        let req = IcmpHeader::echo_request(0xABCD, 7);
        assert_eq!(req.icmp_type, 8);
        assert_eq!(req.code, 0);
        assert_eq!(req.identifier(), 0xABCD);
        assert_eq!(req.sequence(), 7);

        let reply = IcmpHeader::echo_reply(0xABCD, 7);
        assert_eq!(reply.icmp_type, 0);
        assert_eq!(reply.rest_of_header, req.rest_of_header);
    }
}
