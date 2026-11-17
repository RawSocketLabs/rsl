//! The server's request for a client certificate.

// --- Workspace dependencies ---
use bnb::bin;

// --- Internal modules ---
use super::{
    SignatureScheme,
    list::{read_list16, write_list16},
};

/// The body of a `CertificateRequest` (RFC 5246 §7.4.4): the kinds of certificate, the
/// signature algorithms and the certificate authorities the server would accept.
///
/// Dual-use: every field is stored verbatim. The authorities are kept as the raw bytes of
/// their list, a sequence of DER distinguished names each behind a 16-bit length, because
/// nothing here interprets them.
//~ models rfc5246#7.4.4
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CertificateRequest {
    /// The `ClientCertificateType` codes the server accepts.
    #[brw(count_prefix = u8)]
    pub certificate_types: Vec<u8>,
    /// The signature algorithms the server accepts, most preferred first.
    #[br(parse_with = read_list16)]
    #[bw(write_with = write_list16)]
    pub supported_signature_algorithms: Vec<SignatureScheme>,
    /// The acceptable certificate authorities, uninterpreted.
    #[brw(count_prefix = u16)]
    pub certificate_authorities: Vec<u8>,
}
