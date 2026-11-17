//! The signature-scheme registry.

// --- Workspace dependencies ---
use bnb::BitEnum;

/// A signature algorithm as a two-byte code. TLS 1.2 defines the pair as
/// `SignatureAndHashAlgorithm` (RFC 5246 §7.4.1.4.1); the names and values here are the
/// `SignatureScheme` registry that replaced it (RFC 8446 §4.2.3), which keeps the same wire
/// encoding. Dual-use: any other value is preserved as [`SignatureScheme::Other`].
//~ models rfc8446#4.2.3
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u16, bytes = big)]
#[repr(u16)]
pub enum SignatureScheme {
    /// RSASSA-PKCS1-v1_5 with SHA-1.
    RsaPkcs1Sha1 = 0x0201,
    /// ECDSA with SHA-1.
    EcdsaSha1 = 0x0203,
    /// RSASSA-PKCS1-v1_5 with SHA-256.
    RsaPkcs1Sha256 = 0x0401,
    /// ECDSA on P-256 with SHA-256.
    EcdsaSecp256r1Sha256 = 0x0403,
    /// RSASSA-PKCS1-v1_5 with SHA-384.
    RsaPkcs1Sha384 = 0x0501,
    /// ECDSA on P-384 with SHA-384.
    EcdsaSecp384r1Sha384 = 0x0503,
    /// RSASSA-PKCS1-v1_5 with SHA-512.
    RsaPkcs1Sha512 = 0x0601,
    /// ECDSA on P-521 with SHA-512.
    EcdsaSecp521r1Sha512 = 0x0603,
    /// RSASSA-PSS with SHA-256, for an `rsaEncryption` key.
    RsaPssRsaeSha256 = 0x0804,
    /// RSASSA-PSS with SHA-384, for an `rsaEncryption` key.
    RsaPssRsaeSha384 = 0x0805,
    /// RSASSA-PSS with SHA-512, for an `rsaEncryption` key.
    RsaPssRsaeSha512 = 0x0806,
    /// Ed25519.
    Ed25519 = 0x0807,
    /// Ed448.
    Ed448 = 0x0808,
    /// RSASSA-PSS with SHA-256, for an `id-RSASSA-PSS` key.
    RsaPssPssSha256 = 0x0809,
    /// RSASSA-PSS with SHA-384, for an `id-RSASSA-PSS` key.
    RsaPssPssSha384 = 0x080a,
    /// RSASSA-PSS with SHA-512, for an `id-RSASSA-PSS` key.
    RsaPssPssSha512 = 0x080b,
    /// Any other scheme, preserved verbatim.
    #[catch_all]
    Other(u16),
}
