//! The certificate-identity check of the anti-MITM handshake.

// --- Workspace dependencies ---
use base64::{Engine as _, engine::general_purpose::STANDARD};
use crypto::digest::sha2::sha256::Sha256;

/// Appended to a certificate's base64 text before hashing. Embedded in the vendor client
/// (spec §3.1, `C_cert`).
const CERTIFICATE_CONSTANT: &[u8] = b"@~*&!()-";

/// The digest of a certificate given as base64 text: SHA-256 over the text exactly as
/// written, then a constant (spec §3.3). The text is not decoded or re-encoded, because the
/// server's copy is compared as it arrived.
#[must_use]
pub fn certificate_digest(base64_text: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    // A certificate is far shorter than SHA-256's limit, so these cannot fail.
    hash.update(base64_text).ok();
    hash.update(CERTIFICATE_CONSTANT).ok();
    hash.finalize().into_bytes()
}

/// The digest of a certificate given as DER: that of its standard base64 encoding.
#[must_use]
pub fn der_digest(der: &[u8]) -> [u8; 32] {
    certificate_digest(&STANDARD.encode(der))
}

/// Whether any certificate the peer presented is one the server named (spec §3.5): its
/// digest equals that of `rsa_cert` or of `sm2_enc_cert`, the base64 texts from
/// `antiMITMAttackData`. An empty text names nothing, and the check fails when the chain is
/// empty or both texts are.
#[must_use]
pub fn chain_matches(chain_der: &[impl AsRef<[u8]>], rsa_cert: &str, sm2_enc_cert: &str) -> bool {
    let named: Vec<[u8; 32]> = [rsa_cert, sm2_enc_cert]
        .into_iter()
        .filter(|text| !text.is_empty())
        .map(certificate_digest)
        .collect();
    chain_der
        .iter()
        .any(|der| named.contains(&der_digest(der.as_ref())))
}
