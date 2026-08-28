//! Signature verification fed arbitrary key, message, and signature bytes must return
//! `Ok`/`Err` and never panic. Covers Ed25519, Ed448, ECDSA P-256, and ECDSA P-384.
//!
//! Run: `cargo +nightly fuzz run signature_verify --fuzz-dir crypto/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rsl_crypto::signature::{
    ecdsa_p256::{EcdsaP256Signature, EcdsaP256VerifyingKey},
    ecdsa_p384::{EcdsaP384Signature, EcdsaP384VerifyingKey},
    ed448::{Ed448Signature, Ed448VerifyingKey},
    ed25519::{Ed25519Signature, Ed25519VerifyingKey},
};

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let selector = data[0] % 4;
    let key_len = [32, 57, 65, 97][usize::from(selector)];
    let sig_len = [64, 114, 64, 96][usize::from(selector)];
    if data.len() < 1 + key_len + sig_len {
        return;
    }
    let key = &data[1..1 + key_len];
    let sig = &data[1 + key_len..1 + key_len + sig_len];
    let message = &data[1 + key_len + sig_len..];

    match selector {
        0 => {
            if let (Ok(key), Ok(sig)) = (
                Ed25519VerifyingKey::try_from(key),
                Ed25519Signature::try_from(sig),
            ) {
                let _ = key.verify(message, &sig);
            }
        }
        1 => {
            if let (Ok(key), Ok(sig)) = (Ed448VerifyingKey::try_from(key), Ed448Signature::try_from(sig)) {
                let _ = key.verify(None, message, &sig);
            }
        }
        2 => {
            if let (Ok(key), Ok(sig)) = (
                EcdsaP256VerifyingKey::try_from(key),
                EcdsaP256Signature::try_from(sig),
            ) {
                let _ = key.verify_sha256(message, &sig);
            }
        }
        _ => {
            if let (Ok(key), Ok(sig)) = (
                EcdsaP384VerifyingKey::try_from(key),
                EcdsaP384Signature::try_from(sig),
            ) {
                let _ = key.verify_sha384(message, &sig);
            }
        }
    }
});
