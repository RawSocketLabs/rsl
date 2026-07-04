//! **ip** — an IPv4 (RFC 791) header codec on the [`bnb`] bit-aware codec.
//!
//! From-scratch and dual-use: [`Ipv4Header`] decodes/encodes the 20-byte fixed header plus any
//! options, preserving representable input exactly. `total_length` and `header_checksum` are
//! stored **verbatim** — decode never recomputes, verifies, or rejects them, so a forged
//! length or checksum survives a round-trip. The `inject` feature adds a `rawsock::Protocol`
//! layer (`Ip`) that wraps an L4 payload, **supplies it the pseudo-header** for its checksum,
//! and computes the IPv4 header checksum + `total_length` — the piece that lets a full
//! `Ip(Udp(..))` / `Ip(Tcp(..))` stack emit a correct packet.
//!
//! [`bnb`]: https://github.com/RawSocketLabs/bitsandbytes
//!
//! ```
//! use ip::Ipv4Header;
//! use std::net::Ipv4Addr;
//!
//! let h = Ipv4Header::datagram(Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2), 17, 12);
//! assert_eq!(h.total_length, 32); // 20-byte header + 12
//! let wire = h.to_bytes().unwrap();
//! assert_eq!(Ipv4Header::decode_exact(&wire).unwrap().dst_addr(), Ipv4Addr::new(10, 0, 0, 2));
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use bnb::{bin, bitfield, u4, u13};
use std::net::Ipv4Addr;

/// The rawsock injection layer — the `inject` feature.
#[cfg(feature = "inject")]
pub mod inject;
#[cfg(feature = "inject")]
pub use inject::Ip;

/// Version + IHL — byte 0 of the header, a flat `#[bitfield(u8)]`.
//~ models rfc791#3.1 part="Version + IHL"
#[bitfield(u8, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionIhl {
    /// IP version (4 for IPv4).
    pub version: u4,
    /// Internet Header Length in 32-bit words (5 = a 20-byte header, no options).
    pub ihl: u4,
}

/// Flags + fragment offset — bytes 6–7, a flat `#[bitfield(u16)]`.
//~ models rfc791#3.1 part="Flags + Fragment Offset"
#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlagsFragment {
    /// Reserved (must be zero).
    pub reserved: bool,
    /// Don't Fragment.
    pub dont_fragment: bool,
    /// More Fragments.
    pub more_fragments: bool,
    /// Fragment offset, in 8-byte units.
    pub fragment_offset: u13,
}

/// An IPv4 header (RFC 791 §3.1): the 20-byte fixed header plus any options.
//~ models rfc791#3.1 part="Internet Header Format"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ipv4Header {
    /// Version + IHL.
    pub version_ihl: VersionIhl,
    /// Differentiated services + ECN (the former Type of Service byte).
    pub dscp_ecn: u8,
    /// Total datagram length in bytes (header + data). Stored verbatim.
    pub total_length: u16,
    /// Identification (for fragment reassembly).
    pub identification: u16,
    /// Flags + fragment offset.
    pub flags_fragment: FlagsFragment,
    /// Time to live.
    pub ttl: u8,
    /// The encapsulated protocol (17 = UDP, 6 = TCP, 1 = ICMP).
    pub protocol: u8,
    /// The header checksum (stored verbatim — not recomputed or verified on decode).
    pub header_checksum: u16,
    /// Source address, as raw big-endian bits (see [`src_addr`](Self::src_addr)).
    pub src: u32,
    /// Destination address (see [`dst_addr`](Self::dst_addr)).
    pub dst: u32,
    /// Options, as raw bytes: `(ihl - 5) * 4`. `saturating_sub` keeps a malformed `ihl < 5`
    /// from underflow-panicking on untrusted input (it reads zero option bytes).
    #[br(count = usize::from(u8::from(version_ihl.ihl()).saturating_sub(5)) * 4)]
    pub options: Vec<u8>,
}

impl Ipv4Header {
    /// A standard datagram header (version 4, IHL 5, TTL 64, DF set, no options) carrying
    /// `payload_len` bytes of `protocol` from `src` to `dst`. `total_length` is `20 +
    /// payload_len`; `header_checksum` is 0 — the compliant `inject` encode fills it (or set
    /// it yourself). To forge fields, edit the struct directly.
    #[must_use]
    pub fn datagram(src: Ipv4Addr, dst: Ipv4Addr, protocol: u8, payload_len: u16) -> Self {
        Self {
            version_ihl: VersionIhl::new()
                .with_version(u4::new(4))
                .with_ihl(u4::new(5)),
            dscp_ecn: 0,
            total_length: 20u16.saturating_add(payload_len),
            identification: 0,
            flags_fragment: FlagsFragment::new().with_dont_fragment(true),
            ttl: 64,
            protocol,
            header_checksum: 0,
            src: src.to_bits(),
            dst: dst.to_bits(),
            options: Vec::new(),
        }
    }

    /// The source address.
    #[must_use]
    pub fn src_addr(&self) -> Ipv4Addr {
        Ipv4Addr::from_bits(self.src)
    }

    /// The destination address.
    #[must_use]
    pub fn dst_addr(&self) -> Ipv4Addr {
        Ipv4Addr::from_bits(self.dst)
    }

    /// The header length in bytes (`IHL * 4`).
    #[must_use]
    pub fn header_len(&self) -> usize {
        usize::from(u8::from(self.version_ihl.ihl())) * 4
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn datagram_sets_standard_fields() {
        let h = Ipv4Header::datagram(
            Ipv4Addr::new(1, 2, 3, 4),
            Ipv4Addr::new(5, 6, 7, 8),
            17,
            100,
        );
        assert_eq!(u8::from(h.version_ihl.version()), 4);
        assert_eq!(h.header_len(), 20);
        assert_eq!(h.total_length, 120);
        assert_eq!(h.ttl, 64);
        assert!(h.flags_fragment.dont_fragment());
        assert_eq!(h.src_addr(), Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(h.dst_addr(), Ipv4Addr::new(5, 6, 7, 8));
    }
}
