//! The primitives of spec §3 that take text a gateway chose (the canonical string of an
//! anti-MITM object, the challenge check, key derivation, signature verification, encryption
//! of the login secret for an announced key, and the certificate comparison) must never
//! panic, and a derived key verifies what it signed.
//!
//! The input is lines of text: a modulus, an exponent, a challenge and a fourth value, each
//! used wherever a gateway supplies one. The modulus and exponent are also replaced in turn
//! by valid ones, so the arithmetic is reached with only the other chosen.
//!
//! Run: `cargo +nightly fuzz run sig --fuzz-dir protocols/application/sangfor/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use sangfor_atrust_sig::{
    SignKey, canonical_string, chain_matches, challenge_passes, encrypt_secret,
};
use sangfor_fuzz::Counting;

/// A throwaway 1024-bit RSA modulus.
const MODULUS: &str = concat!(
    "c27a17863e6058c010396eb0867f93dc51753e09ba1b87bed5d3f76c16c6f5adb5974f49d93b8452",
    "3df495898e5734b6d750ccdd4be725153111265c57c854ff8d679658bb467aeee4ffcb3643219392",
    "f44bc1c80d90528ae7ac11654be448934a49008136c8ccf34625fca525955de6bec05da1a67c24b6",
    "f9164a062be8f431",
);

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let lines: Vec<&str> = text.split('\n').collect();
    let line = |index: usize| lines.get(index).copied().unwrap_or_default();
    let (modulus, exponent, challenge, fourth) = (line(0), line(1), line(2), line(3));

    // The object as received, and as one member of an object that is otherwise sound.
    let _ = canonical_string(&text);
    let _ = canonical_string(&format!(r#"{{"enable":1,"a":{text},"mitmSig":"00"}}"#));

    let _ = challenge_passes(modulus, exponent, challenge, fourth);
    if let Ok(key) = SignKey::derive(modulus, exponent, challenge) {
        let signature = key.sign(data).unwrap();
        assert!(
            key.verify(data, &signature).unwrap(),
            "a key must verify what it signed"
        );
        let _ = key.verify(data, fourth);
    }

    for (modulus, exponent) in [(modulus, exponent), (MODULUS, exponent), (modulus, "65537")] {
        let _ = encrypt_secret(
            modulus,
            exponent,
            challenge,
            fourth,
            &mut Counting::default(),
        );
    }
    let _ = chain_matches(&[data], modulus, exponent);
});
