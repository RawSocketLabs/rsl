//! **arp** — an ARP (RFC 826) packet codec on the [`bnb`] bit-aware codec.
//!
//! From-scratch and dual-use: [`ArpPacket`] decodes/encodes the 28-byte IPv4-over-Ethernet ARP
//! packet. Every field is stored **verbatim** — the parser rejects nothing representable, so a
//! packet with an unusual operation or a mismatched `hlen`/`plen` round-trips unchanged. The
//! `inject` feature makes `ArpPacket` a `rawsock::Protocol` (an Ethernet payload, EtherType
//! `0x0806`); it has no checksum or length field, so nothing is derived on encode.
//!
//! Reuses the [`ethertype`] crate for the protocol-type field and `bnb`'s native
//! [`Ipv4Addr`](std::net::Ipv4Addr) codec for the sender/target IPs.
//!
//! [`bnb`]: https://github.com/RawSocketLabs/bitsandbytes
//!
//! ```
//! use arp::{ArpPacket, Operation};
//! use std::net::Ipv4Addr;
//!
//! // "who has 10.0.0.2? tell 10.0.0.1"
//! let req = ArpPacket::request([0x02, 0, 0, 0, 0, 1], Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2));
//! assert_eq!(req.oper, Operation::Request);
//! let wire = req.to_bytes().unwrap();
//! assert_eq!(wire.len(), 28);
//! assert_eq!(ArpPacket::decode_exact(&wire).unwrap(), req);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use bnb::{BitEnum, bin};
use ethertype::EtherType;
use std::net::Ipv4Addr;

/// The rawsock injection layer — the `inject` feature.
#[cfg(feature = "inject")]
pub mod inject;

/// The ARP operation code.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u16, bytes = be)]
#[repr(u16)]
pub enum Operation {
    /// Request (1) — "who has `tpa`? tell `spa`".
    Request = 1,
    /// Reply (2) — "`tpa` is at `sha`".
    Reply = 2,
    /// Any other operation (RARP, InARP, …), preserved verbatim (dual-use).
    #[catch_all]
    Other(u16),
}

/// An ARP packet (RFC 826), IPv4-over-Ethernet: the 28-byte fixed layout.
///
/// This models the ubiquitous Ethernet/IPv4 case (hardware addresses `[u8; 6]`, protocol
/// addresses [`Ipv4Addr`]); the general variable-length form (arbitrary `hlen`/`plen`) is a
/// later refinement. All fields are stored verbatim — a packet whose `hlen`/`plen` disagree
/// with the address widths still round-trips.
//~ models rfc826 part="ARP packet format (IPv4 over Ethernet)"
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArpPacket {
    /// Hardware type (1 = Ethernet).
    pub htype: u16,
    /// Protocol type — an EtherType (`0x0800` = IPv4).
    pub ptype: EtherType,
    /// Hardware address length (6 for a MAC).
    pub hlen: u8,
    /// Protocol address length (4 for IPv4).
    pub plen: u8,
    /// The operation (request / reply).
    pub oper: Operation,
    /// Sender hardware (MAC) address.
    pub sha: [u8; 6],
    /// Sender protocol (IPv4) address.
    pub spa: Ipv4Addr,
    /// Target hardware (MAC) address.
    pub tha: [u8; 6],
    /// Target protocol (IPv4) address.
    pub tpa: Ipv4Addr,
}

impl ArpPacket {
    /// The Ethernet hardware type.
    pub const HTYPE_ETHERNET: u16 = 1;

    fn ipv4_over_ethernet(
        oper: Operation,
        sha: [u8; 6],
        spa: Ipv4Addr,
        tha: [u8; 6],
        tpa: Ipv4Addr,
    ) -> Self {
        Self {
            htype: Self::HTYPE_ETHERNET,
            ptype: EtherType::IPv4,
            hlen: 6,
            plen: 4,
            oper,
            sha,
            spa,
            tha,
            tpa,
        }
    }

    /// An ARP **request**: "who has `target_ip`? tell `sender_ip`". The target hardware address
    /// is unknown, so it is zeroed.
    #[must_use]
    pub fn request(sender_mac: [u8; 6], sender_ip: Ipv4Addr, target_ip: Ipv4Addr) -> Self {
        Self::ipv4_over_ethernet(Operation::Request, sender_mac, sender_ip, [0; 6], target_ip)
    }

    /// An ARP **reply**: "`sender_ip` is at `sender_mac`", addressed to `target_mac`/`target_ip`.
    #[must_use]
    pub fn reply(
        sender_mac: [u8; 6],
        sender_ip: Ipv4Addr,
        target_mac: [u8; 6],
        target_ip: Ipv4Addr,
    ) -> Self {
        Self::ipv4_over_ethernet(
            Operation::Reply,
            sender_mac,
            sender_ip,
            target_mac,
            target_ip,
        )
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn request_and_reply_set_the_fixed_fields() {
        let req = ArpPacket::request(
            [0x02, 0, 0, 0, 0, 1],
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(10, 0, 0, 2),
        );
        assert_eq!(req.htype, ArpPacket::HTYPE_ETHERNET);
        assert_eq!(req.ptype, EtherType::IPv4);
        assert_eq!((req.hlen, req.plen), (6, 4));
        assert_eq!(req.oper, Operation::Request);
        assert_eq!(req.tha, [0; 6]); // unknown target MAC

        let reply = ArpPacket::reply(
            [0x02, 0, 0, 0, 0, 2],
            Ipv4Addr::new(10, 0, 0, 2),
            [0x02, 0, 0, 0, 0, 1],
            Ipv4Addr::new(10, 0, 0, 1),
        );
        assert_eq!(reply.oper, Operation::Reply);
        assert_eq!(reply.tha, [0x02, 0, 0, 0, 0, 1]);
    }
}
