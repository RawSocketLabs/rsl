//! RSA component import and RSASSA-PSS verification fed arbitrary bytes must never panic or
//! loop unboundedly, whatever the modulus and exponent look like.
//!
//! Run: `cargo +nightly fuzz run rsa_pss_verify --fuzz-dir crypto/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rsl_crypto::{
    rsa::RsaPublicKey,
    signature::rsa_pss::{RsaPssSha256VerifyingKey, RsaPssSignature},
};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let modulus_len = usize::from(u16::from_le_bytes([data[0], data[1]])).min(data.len() - 2);
    let exponent_len = usize::from(data[2]).min(data.len() - 2 - modulus_len);
    let modulus = &data[3..3 + modulus_len.min(data.len() - 3)];
    let rest = &data[3 + modulus.len()..];
    let exponent = &rest[..exponent_len.min(rest.len())];
    let signature = &rest[exponent.len()..];

    let _ = RsaPublicKey::from_components(modulus, exponent);
    if let Ok(key) = RsaPssSha256VerifyingKey::from_components(modulus, exponent) {
        let _ = key.verify_sha256(b"message", &RsaPssSignature::from(signature));
        let _ = key.verify_sha256_with_salt_len(signature, &RsaPssSignature::from(signature), 0);
    }
});
