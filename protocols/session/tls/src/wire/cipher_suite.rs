//! The cipher-suite registry.

// --- Workspace dependencies ---
use bnb::BitEnum;

/// A two-byte cipher-suite code (RFC 5246 §7.4.1.2). Only the suites this crate acts on are
/// named; every other code is preserved as [`CipherSuite::Other`], so a hello offering or
/// choosing it still round-trips.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = big)]
#[repr(u16)]
pub enum CipherSuite {
    /// `TLS_EMPTY_RENEGOTIATION_INFO_SCSV` (RFC 5746 §3.3): not a suite, but a signal that the
    /// client supports secure renegotiation.
    EmptyRenegotiationInfoScsv = 0x00ff,
    /// `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256` (RFC 5289 §3.2).
    EcdheRsaWithAes128GcmSha256 = 0xc02f,
    /// Any other suite, preserved verbatim.
    #[catch_all]
    Other(u16),
}
