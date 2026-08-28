//! Smoke test: the owned-library namespaces resolve for the feature slice CI builds.
//! Not exhaustive — it proves the re-export paths exist and unify, not the crates' behavior.

#[test]
fn owned_paths_resolve() {
    // owned: codec is bnb
    #[cfg(feature = "codec")]
    fn _codec_is_bnb() {
        let _ = core::any::type_name::<rsl::codec::Error>();
    }

    #[cfg(feature = "crypto")]
    fn _crypto() {
        let _ = core::any::type_name::<rsl::crypto::SecretBytes<32>>();
        let _ = core::any::type_name::<rsl::crypto::digest::sha2::sha256::Sha256>();
        let _ = core::any::type_name::<rsl::crypto::digest::sha2::sha512::Sha512>();
        let _ = core::any::type_name::<rsl::crypto::mac::hmac::sha256::HmacSha256>();
        let _ = core::any::type_name::<rsl::crypto::kdf::hkdf::sha256::HkdfSha256Prk>();
        let _ = core::any::type_name::<rsl::crypto::cipher::aes::aes128::Aes128>();
        let _ = core::any::type_name::<rsl::crypto::aead::gcm::Aes128Gcm>();
        let _ = core::any::type_name::<rsl::crypto::agreement::x25519::X25519>();
        let _ = core::any::type_name::<rsl::crypto::signature::ed25519::Ed25519SigningKey>();
        let _ = rsl::crypto::signature::ed25519::SECURITY_STATUS;
    }

    #[cfg(feature = "legacy-crypto")]
    fn _legacy_crypto_is_explicit() {
        let _ = rsl::crypto_legacy::PACKAGE_SECURITY_FLOOR;
        let _ = core::any::type_name::<rsl::crypto_legacy::rsa::RsaPublicKey>();
        let _ = rsl::crypto_legacy::rsa::pkcs1v15::RSAES_SECURITY_STATUS;
    }

    #[cfg(feature = "compression")]
    fn _compression() {
        let _ = rsl::compression::Flush::None;
    }

    #[cfg(feature = "error-correction")]
    fn _error_correction() {
        let _ = rsl::error_correction::Correction::Clean;
    }

    // owned: protocol crates live under rsl::proto::*
    #[cfg(feature = "proto-dns")]
    fn _proto_dns() {
        let _ = core::any::type_name::<rsl::proto::dns::Message>();
    }

    // prelude glob compiles
    #[allow(unused_imports)]
    use rsl::prelude::*;
}
