//! Public-key and shared-secret parsing fed arbitrary bytes must never panic: SEC 1 points,
//! Edwards points, Montgomery coordinates, and the agreement functions over them.
//!
//! Run: `cargo +nightly fuzz run public_key_parse --fuzz-dir crypto/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rsl_crypto::{
    agreement::{
        ecdh_p256::{EcdhP256, EcdhP256PrivateKey, EcdhP256PublicKey},
        ecdh_p384::{EcdhP384, EcdhP384PrivateKey, EcdhP384PublicKey},
        x448::{X448, X448PrivateKey, X448PublicKey},
        x25519::{X25519, X25519PrivateKey, X25519PublicKey},
    },
    signature::{ed448::Ed448VerifyingKey, ed25519::Ed25519VerifyingKey},
};

fuzz_target!(|data: &[u8]| {
    let _ = Ed25519VerifyingKey::try_from(data);
    let _ = Ed448VerifyingKey::try_from(data);
    if let Ok(public) = EcdhP256PublicKey::try_from(data) {
        let private = EcdhP256PrivateKey::from_bytes([0x42; 32]).unwrap();
        let _ = EcdhP256::agree(&private, &public);
    }
    if let Ok(public) = EcdhP384PublicKey::try_from(data) {
        let private = EcdhP384PrivateKey::from_bytes([0x42; 48]).unwrap();
        let _ = EcdhP384::agree(&private, &public);
    }
    if let Ok(public) = X25519PublicKey::try_from(data) {
        let _ = X25519::agree(&X25519PrivateKey::new([0x42; 32]), &public);
    }
    if let Ok(public) = X448PublicKey::try_from(data) {
        let _ = X448::agree(&X448PrivateKey::new([0x42; 56]), &public);
    }
    if data.len() >= 32 {
        let _ = EcdhP256PrivateKey::from_bytes(data[..32].try_into().unwrap());
    }
    if data.len() >= 48 {
        let _ = EcdhP384PrivateKey::from_bytes(data[..48].try_into().unwrap());
    }
});
