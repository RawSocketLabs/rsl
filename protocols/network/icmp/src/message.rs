//! A typed **view** over an ICMP message — the flat [`IcmpHeader`](crate::IcmpHeader) plus its
//! data, classified into the common message kinds.
//!
//! Like the `tcp` crate's `TcpOption`, this is a lens, not the codec: [`IcmpHeader`] stays the
//! source of truth on the wire, and [`IcmpMessage`] parses one into typed fields (echo
//! identifier/sequence, an error's code + embedded datagram) or builds one back. It sidesteps
//! bnb's `ctx`-dispatch gap by being a plain Rust parse/build, not a `#[bin]` union.

use crate::IcmpHeader;

/// A typed ICMP message: the header's type selects the variant; the rest-of-header and data are
/// interpreted accordingly. Unknown types are preserved as [`Other`](IcmpMessage::Other).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IcmpMessage {
    /// Echo Request (type 8) — a ping. `data` is the (echoed) payload.
    EchoRequest {
        /// Echo identifier.
        identifier: u16,
        /// Sequence number.
        sequence: u16,
        /// The ping payload.
        data: Vec<u8>,
    },
    /// Echo Reply (type 0) — a pong.
    EchoReply {
        /// Echo identifier (matches the request).
        identifier: u16,
        /// Sequence number (matches the request).
        sequence: u16,
        /// The echoed payload.
        data: Vec<u8>,
    },
    /// Destination Unreachable (type 3) — `code` gives the reason (net/host/port/…); `data` is
    /// the start of the original datagram that couldn't be delivered.
    DestinationUnreachable {
        /// The unreachable reason code.
        code: u8,
        /// The original datagram (IP header + first 8 bytes), as far as it fits.
        data: Vec<u8>,
    },
    /// Time Exceeded (type 11) — TTL hit 0 in transit or fragment reassembly timed out; `data`
    /// is the original datagram.
    TimeExceeded {
        /// 0 = TTL exceeded in transit, 1 = fragment reassembly time exceeded.
        code: u8,
        /// The original datagram.
        data: Vec<u8>,
    },
    /// Any other message — the raw type/code/rest-of-header + data, preserved (dual-use).
    Other {
        /// The ICMP type.
        icmp_type: u8,
        /// The code.
        code: u8,
        /// The 4-byte rest-of-header.
        rest_of_header: u32,
        /// The message body.
        data: Vec<u8>,
    },
}

impl IcmpMessage {
    /// Classify an [`IcmpHeader`] + its `data` into a typed message.
    #[must_use]
    pub fn parse(header: &IcmpHeader, data: &[u8]) -> Self {
        let data = data.to_vec();
        match header.icmp_type {
            IcmpHeader::ECHO_REQUEST => Self::EchoRequest {
                identifier: header.identifier(),
                sequence: header.sequence(),
                data,
            },
            IcmpHeader::ECHO_REPLY => Self::EchoReply {
                identifier: header.identifier(),
                sequence: header.sequence(),
                data,
            },
            IcmpHeader::DEST_UNREACHABLE => Self::DestinationUnreachable {
                code: header.code,
                data,
            },
            IcmpHeader::TIME_EXCEEDED => Self::TimeExceeded {
                code: header.code,
                data,
            },
            icmp_type => Self::Other {
                icmp_type,
                code: header.code,
                rest_of_header: header.rest_of_header,
                data,
            },
        }
    }

    /// The [`IcmpHeader`] for this message (with `checksum` 0 — the `inject` layer or a manual
    /// checksum fills it over the header + data).
    #[must_use]
    pub fn header(&self) -> IcmpHeader {
        match self {
            Self::EchoRequest {
                identifier,
                sequence,
                ..
            } => IcmpHeader::echo_request(*identifier, *sequence),
            Self::EchoReply {
                identifier,
                sequence,
                ..
            } => IcmpHeader::echo_reply(*identifier, *sequence),
            Self::DestinationUnreachable { code, .. } => IcmpHeader {
                icmp_type: IcmpHeader::DEST_UNREACHABLE,
                code: *code,
                checksum: 0,
                rest_of_header: 0,
            },
            Self::TimeExceeded { code, .. } => IcmpHeader {
                icmp_type: IcmpHeader::TIME_EXCEEDED,
                code: *code,
                checksum: 0,
                rest_of_header: 0,
            },
            Self::Other {
                icmp_type,
                code,
                rest_of_header,
                ..
            } => IcmpHeader {
                icmp_type: *icmp_type,
                code: *code,
                checksum: 0,
                rest_of_header: *rest_of_header,
            },
        }
    }

    /// This message's data (the echo payload, or an error's embedded datagram).
    #[must_use]
    pub fn data(&self) -> &[u8] {
        match self {
            Self::EchoRequest { data, .. }
            | Self::EchoReply { data, .. }
            | Self::DestinationUnreachable { data, .. }
            | Self::TimeExceeded { data, .. }
            | Self::Other { data, .. } => data,
        }
    }
}

impl IcmpHeader {
    /// Classify this header + `data` into a typed [`IcmpMessage`] view.
    #[must_use]
    pub fn message(&self, data: &[u8]) -> IcmpMessage {
        IcmpMessage::parse(self, data)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn parses_an_echo_request_and_round_trips_the_header() {
        let header = IcmpHeader::echo_request(0x1234, 7);
        let msg = header.message(b"pingpong");
        assert_eq!(
            msg,
            IcmpMessage::EchoRequest {
                identifier: 0x1234,
                sequence: 7,
                data: b"pingpong".to_vec(),
            }
        );
        // The view rebuilds the same header (checksum aside — it's 0 here).
        assert_eq!(msg.header(), header);
        assert_eq!(msg.data(), b"pingpong");
    }

    #[test]
    fn classifies_errors_and_preserves_unknowns() {
        let unreach = IcmpHeader {
            icmp_type: IcmpHeader::DEST_UNREACHABLE,
            code: 3, // port unreachable
            checksum: 0,
            rest_of_header: 0,
        };
        assert!(matches!(
            unreach.message(&[0x45, 0x00]),
            IcmpMessage::DestinationUnreachable { code: 3, .. }
        ));

        let weird = IcmpHeader {
            icmp_type: 42,
            code: 9,
            checksum: 0,
            rest_of_header: 0xDEAD_BEEF,
        };
        assert_eq!(
            weird.message(&[]),
            IcmpMessage::Other {
                icmp_type: 42,
                code: 9,
                rest_of_header: 0xDEAD_BEEF,
                data: Vec::new(),
            }
        );
    }
}
