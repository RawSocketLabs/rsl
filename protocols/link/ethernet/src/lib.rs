//! **ethernet** — an Ethernet II (IEEE 802.3) frame-header codec on the [`bnb`] bit-aware codec.
//!
//! From-scratch and dual-use: [`EthernetHeader`] decodes/encodes the 14-byte frame header
//! (destination MAC, source MAC, EtherType). The `inject` feature adds a `rawsock::Protocol`
//! layer (`Ethernet`) that frames an L3 payload for L2 injection — the top of the stack.
//!
//! The 4-byte FCS (frame check sequence) is **not** part of this codec: on transmit the NIC /
//! kernel computes and appends it (that's how `AF_PACKET` injection works).
//!
//! [`bnb`]: https://github.com/RawSocketLabs/bitsandbytes
//!
//! ```
//! use ethernet::{EthernetHeader, BROADCAST};
//! use ethertype::EtherType;
//!
//! let h = EthernetHeader { dst: BROADCAST, src: [0x02, 0, 0, 0, 0, 1], ethertype: EtherType::ARP };
//! let wire = h.to_bytes().unwrap();
//! assert_eq!(wire.len(), 14);
//! assert_eq!(&wire[12..14], &[0x08, 0x06]); // ARP
//! assert_eq!(EthernetHeader::decode_exact(&wire).unwrap(), h);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use bnb::bin;
use ethertype::EtherType;

/// The rawsock injection layer — the `inject` feature.
#[cfg(feature = "inject")]
pub mod inject;
#[cfg(feature = "inject")]
pub use inject::Ethernet;

/// The broadcast MAC address (`ff:ff:ff:ff:ff:ff`).
pub const BROADCAST: [u8; 6] = [0xff; 6];

/// An Ethernet II frame header (IEEE 802.3): destination MAC, source MAC, and the EtherType —
/// a 14-byte header. The payload and the 4-byte FCS follow (the FCS is added by the NIC on
/// transmit, so it is not part of this codec).
//~ models ieee802.3 part="Ethernet II frame header"
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EthernetHeader {
    /// Destination MAC address.
    pub dst: [u8; 6],
    /// Source MAC address.
    pub src: [u8; 6],
    /// The EtherType — the encapsulated protocol (`0x0800` IPv4, `0x0806` ARP, `0x86DD` IPv6, …).
    pub ethertype: EtherType,
}

impl EthernetHeader {
    /// The fixed Ethernet II header length in bytes (no 802.1Q VLAN tag).
    pub const HEADER_LEN: usize = 14;
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn header_round_trips_with_the_ethertype_enum() {
        let h = EthernetHeader {
            dst: [0x01, 0x02, 0x03, 0x04, 0x05, 0x06],
            src: [0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f],
            ethertype: EtherType::IPv4,
        };
        let wire = h.to_bytes().unwrap();
        assert_eq!(wire.len(), EthernetHeader::HEADER_LEN);
        assert_eq!(&wire[12..14], &[0x08, 0x00]); // IPv4
        assert_eq!(EthernetHeader::decode_exact(&wire).unwrap(), h);
    }
}
