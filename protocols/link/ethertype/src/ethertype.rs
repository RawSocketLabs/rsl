//! The `EtherType` value.

use bnb::BitEnum;

/// The 16-bit **EtherType**: the field in an Ethernet II frame that names the
/// encapsulated protocol (IEEE 802.3 / the IANA ethertype registry).
///
/// A `bnb` [`BitEnum`] at a byte-aligned `u16` width, big-endian (network order). The
/// `#[catch_all]` `Custom` variant makes this **dual-use**: an unknown ethertype is
/// preserved as `Custom(raw)` rather than rejected — only the guided path emits the
/// named values, while any 16-bit value round-trips. Because the width is byte-aligned,
/// the derive also emits `From<EtherType> for u16` and (thanks to the catch-all) an
/// infallible `From<u16> for EtherType`.
///
/// # Examples
///
/// ```
/// use ethertype::EtherType;
///
/// assert_eq!(u16::from(EtherType::IPv4), 0x0800);
/// assert_eq!(EtherType::from(0x0806), EtherType::ARP);
/// // Unknown values are preserved, never rejected (dual-use):
/// assert_eq!(EtherType::from(0x1234), EtherType::Custom(0x1234));
/// ```
//~ models rfc0894#front part="EtherType field of an Ethernet II frame"
#[derive(BitEnum, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = be)]
#[repr(u16)]
pub enum EtherType {
    /// Internet Protocol version 4.
    #[default]
    IPv4 = 0x0800,
    /// Address Resolution Protocol.
    ARP = 0x0806,
    /// Wake-on-LAN.
    WakeOnLAN = 0x0842,
    /// Internet Protocol version 6.
    IPv6 = 0x86DD,
    /// IEEE 802.1Q VLAN-tagged frame.
    VlanTaggedFrame = 0x8100,
    /// MPLS unicast.
    Mpls = 0x8847,
    /// MPLS multicast (with upstream-assigned label).
    MplsWithControl = 0x8848,
    /// Any value not named above — preserved verbatim (dual-use: never rejected).
    #[catch_all]
    Custom(u16),
}

#[cfg(test)]
mod unit {
    use super::*;
    use bnb::{BitReader, BitWriter, Sink, Source};

    /// Encode one `EtherType` to its two big-endian wire bytes.
    fn to_bytes(e: EtherType) -> [u8; 2] {
        let mut w = BitWriter::new();
        w.write(e).unwrap();
        let v = w.into_bytes();
        [v[0], v[1]]
    }

    /// Decode one `EtherType` from two wire bytes.
    fn from_bytes(b: [u8; 2]) -> EtherType {
        BitReader::new(&b).read::<EtherType>().unwrap()
    }

    #[test]
    fn golden_bytes_network_order() {
        assert_eq!(to_bytes(EtherType::IPv4), [0x08, 0x00]);
        assert_eq!(to_bytes(EtherType::IPv6), [0x86, 0xDD]);
        assert_eq!(to_bytes(EtherType::ARP), [0x08, 0x06]);
    }

    #[test]
    fn round_trips_every_named_value() {
        for e in [
            EtherType::IPv4,
            EtherType::ARP,
            EtherType::WakeOnLAN,
            EtherType::IPv6,
            EtherType::VlanTaggedFrame,
            EtherType::Mpls,
            EtherType::MplsWithControl,
        ] {
            assert_eq!(from_bytes(to_bytes(e)), e);
        }
    }

    #[test]
    fn unknown_value_is_preserved_not_rejected() {
        // The dual-use property: an unregistered ethertype decodes as `Custom`, and
        // re-encodes to the same bytes (round-trip, never an error).
        let wire = [0x12, 0x34];
        let decoded = from_bytes(wire);
        assert_eq!(decoded, EtherType::Custom(0x1234));
        assert_eq!(to_bytes(decoded), wire);
    }

    #[test]
    fn int_conversions() {
        assert_eq!(u16::from(EtherType::ARP), 0x0806);
        assert_eq!(EtherType::from(0x86DD), EtherType::IPv6);
        assert_eq!(EtherType::from(0x9999), EtherType::Custom(0x9999));
    }

    #[test]
    fn default_is_ipv4() {
        assert_eq!(EtherType::default(), EtherType::IPv4);
    }
}
