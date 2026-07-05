//! The 12-byte DNS message header (RFC 1035 §4.1.1).

use bnb::{BitEnum, WireLen, bin, bitfield, u3, u4};

/// The DNS operation code (RFC 1035 §4.1.1 OPCODE).
///
/// Dual-use: an unregistered opcode (e.g. NOTIFY=4, UPDATE=5) is preserved as
/// [`Op::Other`] rather than rejected.
//~ models rfc1035#4.1.1 part="OPCODE"
#[derive(BitEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[bit_enum(u4)]
pub enum Op {
    /// A standard query (QUERY).
    #[default]
    Query,
    /// An inverse query (IQUERY, obsolete).
    IQuery,
    /// A server status request (STATUS).
    Status,
    /// Any other opcode value, preserved verbatim.
    #[catch_all]
    Other(u4),
}

/// The 4-bit response code (RFC 1035 §4.1.1 RCODE).
///
/// Dual-use: an unregistered rcode is preserved as [`RCode::Other`].
//~ models rfc1035#4.1.1 part="RCODE"
#[derive(BitEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[bit_enum(u4)]
pub enum RCode {
    /// No error.
    #[default]
    NoError,
    /// Format error — the server could not interpret the query.
    FormErr,
    /// Server failure.
    ServFail,
    /// Name error — the domain does not exist (NXDOMAIN).
    NxDomain,
    /// The requested query kind is not implemented.
    NotImp,
    /// The server refuses to perform the operation.
    Refused,
    /// Any other rcode value, preserved verbatim.
    #[catch_all]
    Other(u4),
}

/// The second 16-bit word of the header — QR/OPCODE/AA/TC/RD/RA/Z/RCODE, exactly the
/// RFC 1035 §4.1.1 bit diagram (MSB-first). `bnb` bitfields carry a byte-width backing,
/// so the sub-byte OPCODE/flags groupings are flattened into these leaf fields.
#[bitfield(u16, bits = msb, bytes = big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    /// QR: `false` = query, `true` = response.
    pub response: bool,
    /// The 4-bit operation code.
    pub op: Op,
    /// Authoritative Answer.
    pub authoritative: bool,
    /// TrunCation.
    pub truncated: bool,
    /// Recursion Desired.
    pub recursion_desired: bool,
    /// Recursion Available.
    pub recursion_available: bool,
    /// The reserved Z bits (must be zero per the RFC; retained verbatim — dual-use).
    pub reserved: u3,
    /// The 4-bit response code.
    pub rcode: RCode,
}

/// The 12-byte DNS message header (RFC 1035 §4.1.1).
///
/// The four section counts are [`WireLen<u16>`]: left [`auto()`](WireLen::auto) (the
/// default) they derive from their sections when the [`Message`](crate::Message) is
/// encoded, so a freshly-built message is correct without a sync step. Set one explicitly
/// with [`WireLen::set`] to forge a count that *disagrees* with its section (dual-use). A
/// decoded header carries each count as [`Set`](WireLen::Set), so `decode → to_bytes`
/// round-trips byte-for-byte (a forged count survives).
//~ models rfc1035#4.1.1 part="Header section format"
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// A 16-bit identifier assigned by the requester.
    pub id: u16,
    /// The packed QR/OPCODE/flags/RCODE word.
    pub state: State,
    /// Number of entries in the question section (auto-derives from `Message::questions`).
    pub qdcount: WireLen<u16>,
    /// Number of resource records in the answer section (auto-derives from `answers`).
    pub ancount: WireLen<u16>,
    /// Number of name-server records in the authority section (auto-derives from `authorities`).
    pub nscount: WireLen<u16>,
    /// Number of resource records in the additional section (auto-derives from `additional`).
    pub arcount: WireLen<u16>,
}

impl Header {
    /// The operation code.
    #[must_use]
    pub fn op(&self) -> Op {
        self.state.op()
    }

    /// Whether this is a response (QR bit set).
    #[must_use]
    pub fn is_response(&self) -> bool {
        self.state.response()
    }

    /// The response code.
    #[must_use]
    pub fn rcode(&self) -> RCode {
        self.state.rcode()
    }
}

/// Pure bitfield/enum logic — no message codec.
#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn state_packs_the_rfc_bit_order() {
        // AA + NXDOMAIN on an authoritative query response: QR=1, AA=1, rcode=3.
        let s = State::new()
            .with_response(true)
            .with_op(Op::Query)
            .with_authoritative(true)
            .with_rcode(RCode::NxDomain);
        // QR(1) op(0000) AA(1) TC(0) RD(0) RA(0) Z(000) RCODE(0011) = 1000 0100 0000 0011.
        assert_eq!(s.to_be_bytes(), [0x84, 0x03]);
    }

    #[test]
    fn unknown_opcode_and_rcode_are_preserved() {
        // opcode=5 (UPDATE, unnamed) and rcode=9 (unnamed) survive as Other(..).
        let s = State::new()
            .with_op(Op::Other(u4::new(5)))
            .with_rcode(RCode::Other(u4::new(9)));
        let bytes = s.to_be_bytes();
        let back = State::from_be_bytes(bytes);
        assert_eq!(back.op(), Op::Other(u4::new(5)));
        assert_eq!(back.rcode(), RCode::Other(u4::new(9)));
    }

    #[test]
    fn op_and_rcode_defaults() {
        assert_eq!(Op::default(), Op::Query);
        assert_eq!(RCode::default(), RCode::NoError);
    }
}

/// The `Header` wire codec through the bnb `Source`/`Sink` seam.
#[cfg(test)]
mod component {
    use super::*;

    #[test]
    fn header_round_trips_through_the_codec() {
        // id=0x1234, flags word=0x8180 (QR=1, opcode=0, RD=1, RA=1, rcode=0), qd=1 an=1.
        let wire = [
            0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        ];
        let h = Header::decode_exact(&wire).unwrap();
        assert_eq!(h.id, 0x1234);
        assert!(h.is_response());
        assert_eq!(h.op(), Op::Query);
        assert!(h.state.recursion_desired());
        assert!(h.state.recursion_available());
        assert!(!h.state.authoritative());
        assert_eq!(h.rcode(), RCode::NoError);
        assert_eq!(
            (
                h.qdcount.to_count(),
                h.ancount.to_count(),
                h.nscount.to_count(),
                h.arcount.to_count()
            ),
            (1, 1, 0, 0)
        );
        assert_eq!(h.to_bytes().unwrap(), wire);
    }
}
