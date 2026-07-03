//! Typed record data (RDATA), dispatched by [`RType`](crate::record::RType).
//!
//! The common record types get structured variants; every other type — including the
//! DNSSEC family and anything unregistered — is preserved as [`RData::Custom`] raw bytes
//! rather than misparsed (the dual-use rule: never reject or corrupt representable input).

use crate::name::Name;
use crate::record::RType;
use bnb::bin;
use std::net::{Ipv4Addr, Ipv6Addr};

/// The SOA (Start Of Authority) record data (RFC 1035 §3.3.13).
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Soa {
    /// The primary name server for the zone.
    #[brw(variable)]
    pub mname: Name,
    /// The mailbox of the zone administrator.
    #[brw(variable)]
    pub rname: Name,
    /// The zone serial number.
    pub serial: u32,
    /// Refresh interval (seconds).
    pub refresh: u32,
    /// Retry interval (seconds).
    pub retry: u32,
    /// Expire interval (seconds).
    pub expire: u32,
    /// Minimum / negative-caching TTL (seconds).
    pub minimum: u32,
}

/// The MX (mail exchange) record data (RFC 1035 §3.3.9).
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mx {
    /// Preference (lower is preferred).
    pub preference: u16,
    /// The mail-exchange host.
    #[brw(variable)]
    pub exchange: Name,
}

/// Typed record data, selected by the owning record's [`RType`].
///
/// Length-driven and unknown variants use `rdlength` (passed as context). Unmatched
/// types land in [`RData::Custom`], carrying the type and the raw bytes.
//~ models rfc1035#3.3 part="Standard RR RDATA formats"
#[bin(big, ctx(rtype: RType, rdlength: u16), tag = rtype)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RData {
    /// An IPv4 host address (A).
    #[bin(tag = RType::A)]
    A(
        #[br(map = |x: u32| Ipv4Addr::from(x))]
        #[bw(map = |a: &Ipv4Addr| u32::from(*a))]
        Ipv4Addr,
    ),
    /// An IPv6 host address (AAAA).
    #[bin(tag = RType::AAAA)]
    Aaaa(
        #[br(map = |x: u128| Ipv6Addr::from(x))]
        #[bw(map = |a: &Ipv6Addr| u128::from(*a))]
        Ipv6Addr,
    ),
    /// An authoritative name server (NS).
    #[bin(tag = RType::NS)]
    Ns(#[brw(variable)] Name),
    /// The canonical name for an alias (CNAME).
    #[bin(tag = RType::CNAME)]
    Cname(#[brw(variable)] Name),
    /// A domain-name pointer (PTR).
    #[bin(tag = RType::PTR)]
    Ptr(#[brw(variable)] Name),
    /// Start of a zone of authority (SOA).
    #[bin(tag = RType::SOA)]
    Soa(#[brw(variable)] Soa),
    /// A mail exchange (MX).
    #[bin(tag = RType::MX)]
    Mx(#[brw(variable)] Mx),
    /// Text strings (TXT) — kept as raw RDATA bytes (one or more length-prefixed
    /// character-strings; split with [`RData::txt_strings`]).
    #[bin(tag = RType::TXT)]
    Txt {
        /// The raw RDATA bytes.
        #[br(count = rdlength)]
        bytes: Vec<u8>,
    },
    /// A service locator (SRV, RFC 2782): priority, weight, port, then the target host
    /// as raw bytes (a domain name that may itself be compressed — kept raw here).
    #[bin(tag = RType::SRV)]
    Srv {
        /// Priority (lower is preferred).
        priority: u16,
        /// Weight for records of equal priority.
        weight: u16,
        /// The service port.
        port: u16,
        /// The target host, as raw RDATA bytes (`rdlength - 6`).
        #[br(count = rdlength - 6)]
        target: Vec<u8>,
    },
    /// A Certification Authority Authorization record (CAA, RFC 8659).
    #[bin(tag = RType::CAA)]
    Caa {
        /// The flags byte (bit 7 = issuer-critical).
        flags: u8,
        /// The length of the `tag` field.
        tag_length: u8,
        /// The property tag (ASCII), `tag_length` bytes.
        #[br(count = tag_length)]
        tag: Vec<u8>,
        /// The property value, `rdlength - tag_length - 2` bytes.
        #[br(count = rdlength - u16::from(tag_length) - 2)]
        value: Vec<u8>,
    },
    /// EDNS(0) OPT pseudo-record RDATA — raw option bytes (RFC 6891). The OPT record's
    /// header fields live in the enclosing record's CLASS/TTL; see the EDNS view.
    #[bin(tag = RType::OPT)]
    Opt {
        /// The raw concatenated EDNS options.
        #[br(count = rdlength)]
        bytes: Vec<u8>,
    },
    /// Any other record type — the raw RDATA bytes, tagged with the type. The dual-use
    /// fallback: unknown/DNSSEC/exotic records are preserved exactly, never misparsed.
    #[catch_all]
    Custom {
        /// The record type that was not structurally decoded.
        rtype: RType,
        /// The raw RDATA bytes.
        #[br(count = rdlength)]
        bytes: Vec<u8>,
    },
}

impl RData {
    /// For a [`RData::Txt`], split the raw bytes into their length-prefixed
    /// character-strings; `None` for any other variant.
    #[must_use]
    pub fn txt_strings(&self) -> Option<Vec<Vec<u8>>> {
        let RData::Txt { bytes } = self else {
            return None;
        };
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let len = usize::from(bytes[i]);
            i += 1;
            let end = (i + len).min(bytes.len());
            out.push(bytes[i..end].to_vec());
            i = end;
        }
        Some(out)
    }
}
