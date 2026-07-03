//! The question section (RFC 1035 §4.1.2).

use crate::name::Name;
use bnb::{BitEnum, bin};

/// A question's QTYPE — the record types plus the query-only pseudo-types (RFC 1035
/// §3.2.2–3.2.3). Dual-use: any other value is preserved as [`QType::Custom`].
//~ models rfc1035#4.1.2 part="QTYPE"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = be)]
#[repr(u16)]
#[allow(missing_docs)] // registry names, self-documenting
pub enum QType {
    A = 1,
    NS = 2,
    CNAME = 5,
    SOA = 6,
    PTR = 12,
    MX = 15,
    TXT = 16,
    AAAA = 28,
    SRV = 33,
    OPT = 41,
    CAA = 257,
    /// A request for a zone transfer (AXFR).
    AXFR = 252,
    /// A request for mailbox-related records (MAILB).
    MAILB = 253,
    /// A request for mail-agent records (MAILA, obsolete).
    MAILA = 254,
    /// A request for all records (`*` / ANY).
    ANY = 255,
    /// Any other QTYPE value, preserved verbatim.
    #[catch_all]
    Custom(u16),
}

/// A question's QCLASS (RFC 1035 §3.2.4–3.2.5). Dual-use: any other value is preserved.
//~ models rfc1035#4.1.2 part="QCLASS"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = be)]
#[repr(u16)]
pub enum QClass {
    /// The Internet class (IN).
    Internet = 1,
    /// The CHAOS class.
    Chaos = 3,
    /// The Hesiod class.
    Hesiod = 4,
    /// Any class (`*`).
    Any = 255,
    /// Any other QCLASS value, preserved verbatim.
    #[catch_all]
    Custom(u16),
}

/// A single entry in the question section: a name, a QTYPE, and a QCLASS.
//~ models rfc1035#4.1.2 part="Question section format"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    /// The queried domain name.
    #[brw(variable)]
    pub name: Name,
    /// The type of records requested.
    pub qtype: QType,
    /// The class of records requested.
    pub qclass: QClass,
}

/// Pure QTYPE/QCLASS enum ⇄ integer logic.
#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn qtype_covers_records_and_pseudo_types() {
        assert_eq!(u16::from(QType::A), 1);
        assert_eq!(u16::from(QType::ANY), 255);
        assert_eq!(u16::from(QType::AXFR), 252);
        assert_eq!(QType::from(41), QType::OPT);
        assert_eq!(QType::from(7777), QType::Custom(7777));
    }

    #[test]
    fn qclass_round_trips_and_preserves_unknown() {
        assert_eq!(u16::from(QClass::Internet), 1);
        assert_eq!(QClass::from(255), QClass::Any);
        assert_eq!(QClass::from(9), QClass::Custom(9));
    }
}

/// A question entry through the bnb codec seam.
#[cfg(test)]
mod component {
    use super::*;

    #[test]
    fn question_round_trips() {
        // "example.com" A IN
        let wire = [
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00,
            0x01, 0x00, 0x01,
        ];
        let q = Question::decode_exact(&wire).unwrap();
        assert_eq!(q.name.to_string(), "example.com");
        assert_eq!(q.qtype, QType::A);
        assert_eq!(q.qclass, QClass::Internet);
        assert_eq!(q.to_bytes().unwrap(), wire);
    }
}
