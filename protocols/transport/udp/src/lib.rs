//! **udp** — a UDP (RFC 768) datagram-header codec on the [`bnb`] bit-aware codec.
//!
//! From-scratch and dual-use: [`UdpHeader`] decodes/encodes the 8-byte header (four 16-bit
//! fields). `length` and `checksum` are stored **verbatim** — decode never recomputes,
//! verifies, or rejects them, so a forged length or checksum survives a round-trip.
//!
//! This is a **header codec**, not a socket. A checksum-compute helper (over the IPv4/IPv6
//! pseudo-header) and the `rawsock` injection-`Protocol` impl (which makes UDP the socket
//! layer's first on-the-wire consumer) arrive with the `rawsock` composition model.
//!
//! [`bnb`]: https://github.com/RawSocketLabs/bitsandbytes
//!
//! ```
//! use udp::UdpHeader;
//!
//! let h = UdpHeader::for_payload(40000, 53, 29); // a 29-byte DNS query
//! assert_eq!(h.length, 37); // 8-byte header + 29
//! let wire = h.to_bytes().unwrap();
//! assert_eq!(wire.len(), 8);
//! assert_eq!(UdpHeader::decode_exact(&wire).unwrap().payload_len(), 29);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use bnb::bin;

/// The rawsock injection layer — the `inject` feature.
#[cfg(feature = "inject")]
pub mod inject;
#[cfg(feature = "inject")]
pub use inject::{Udp, udp_checksum};

/// A UDP datagram header (RFC 768): source port, destination port, length, and checksum —
/// four 16-bit fields, an 8-byte fixed header.
//~ models rfc768 part="UDP header format"
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UdpHeader {
    /// Source port (0 means "no reply expected").
    pub src_port: u16,
    /// Destination port.
    pub dst_port: u16,
    /// Length in bytes of the header **plus** data (minimum 8). Stored verbatim — a datagram
    /// with a `length` that disagrees with its actual size round-trips unchanged (dual-use).
    pub length: u16,
    /// The checksum (stored verbatim — not recomputed or verified on decode; 0 means "none"
    /// for IPv4).
    pub checksum: u16,
}

impl UdpHeader {
    /// The fixed UDP header length in bytes.
    pub const HEADER_LEN: u16 = 8;

    /// A header for a datagram carrying `payload_len` bytes: `length` is `8 + payload_len`
    /// (saturating), `checksum` is 0. Compute and set the checksum separately (a helper lands
    /// with `rawsock`'s compose model); to forge a `length`, set the field directly.
    #[must_use]
    pub fn for_payload(src_port: u16, dst_port: u16, payload_len: u16) -> Self {
        Self {
            src_port,
            dst_port,
            length: Self::HEADER_LEN.saturating_add(payload_len),
            checksum: 0,
        }
    }

    /// The declared payload length in bytes (`length - 8`), `saturating_sub` so a malformed
    /// `length < 8` yields 0 rather than underflowing.
    #[must_use]
    pub fn payload_len(&self) -> u16 {
        self.length.saturating_sub(Self::HEADER_LEN)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn for_payload_computes_length() {
        let h = UdpHeader::for_payload(40000, 53, 29);
        assert_eq!(h.length, 37);
        assert_eq!(h.payload_len(), 29);
        assert_eq!(h.checksum, 0);
    }

    #[test]
    fn payload_len_saturates_on_a_short_length() {
        // A malformed length below the 8-byte header must not underflow.
        let h = UdpHeader {
            src_port: 1,
            dst_port: 2,
            length: 3,
            checksum: 0,
        };
        assert_eq!(h.payload_len(), 0);
    }
}
