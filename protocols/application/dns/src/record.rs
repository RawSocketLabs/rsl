//! Resource-record framing: the [`RType`]/[`RClass`] registries and the [`Record`]
//! wrapper (RFC 1035 §4.1.3).

use crate::name::Name;
use crate::rdata::{RData, RDataCtx};
use bnb::{BitEnum, bin};

/// The DNS resource-record TYPE (the IANA RR-type registry).
///
/// Dual-use: an unregistered type is preserved as [`RType::Custom`], and its RDATA is
/// kept as raw bytes ([`RData::Custom`](crate::rdata::RData::Custom)) rather than
/// misparsed.
//~ models rfc1035#3.2.2 part="TYPE values"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = be)]
#[repr(u16)]
#[allow(missing_docs)] // the variants are the IANA registry names; self-documenting
pub enum RType {
    A = 1,
    NS = 2,
    MD = 3,
    MF = 4,
    CNAME = 5,
    SOA = 6,
    MB = 7,
    MG = 8,
    MR = 9,
    NULL = 10,
    WKS = 11,
    PTR = 12,
    HINFO = 13,
    MINFO = 14,
    MX = 15,
    TXT = 16,
    RP = 17,
    AFSDB = 18,
    SIG = 24,
    KEY = 25,
    AAAA = 28,
    LOC = 29,
    SRV = 33,
    NAPTR = 35,
    KX = 36,
    CERT = 37,
    DNAME = 39,
    OPT = 41,
    APL = 42,
    DS = 43,
    SSHFP = 44,
    IPSECKEY = 45,
    RRSIG = 46,
    NSEC = 47,
    DNSKEY = 48,
    DHCID = 49,
    NSEC3 = 50,
    NSEC3PARAM = 51,
    TLSA = 52,
    SMIMEA = 53,
    HIP = 55,
    NINFO = 56,
    CDS = 59,
    CDNSKEY = 60,
    OPENPGPKEY = 61,
    CSYNC = 62,
    ZONEMD = 63,
    SVCB = 64,
    HTTPS = 65,
    SPF = 99,
    EUI48 = 108,
    EUI64 = 109,
    TKEY = 249,
    TSIG = 250,
    IXFR = 251,
    URI = 256,
    CAA = 257,
    DOA = 259,
    TA = 32768,
    DLV = 32769,
    /// Any TYPE value not named above, preserved verbatim.
    #[catch_all]
    Custom(u16),
}

/// The DNS resource-record CLASS (RFC 1035 §3.2.4).
///
/// Dual-use: an unregistered class is preserved as [`RClass::Custom`]. For an EDNS(0)
/// OPT record this 16-bit field is reinterpreted as the requester's UDP payload size.
//~ models rfc1035#3.2.4 part="CLASS values"
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = be)]
#[repr(u16)]
pub enum RClass {
    /// The Internet class (IN).
    Internet = 1,
    /// The CSNET class (obsolete).
    Csnet = 2,
    /// The CHAOS class.
    Chaos = 3,
    /// The Hesiod class.
    Hesiod = 4,
    /// NONE (RFC 2136).
    None = 254,
    /// Any class value not named above, preserved verbatim.
    #[catch_all]
    Custom(u16),
}

/// A resource record (RFC 1035 §4.1.3): owner name, type, class, TTL, and typed RDATA.
///
/// The RDATA is dispatched by `rtype`, with `rdlength` passed down so length-driven and
/// unknown records size correctly (unknown types keep their raw bytes — dual-use).
//~ models rfc1035#4.1.3 part="Resource record format"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The owner domain name (compression pointers are followed on decode).
    #[brw(variable)]
    pub name: Name,
    /// The record type.
    pub rtype: RType,
    /// The record class.
    pub class: RClass,
    /// Time-to-live, in seconds.
    pub ttl: u32,
    /// The RDATA length in bytes (`rdlength`).
    pub rdlength: u16,
    /// The typed record data, dispatched by `rtype`.
    #[br(ctx { rtype, rdlength })]
    pub data: RData,
}
