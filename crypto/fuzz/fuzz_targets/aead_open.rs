//! Authenticated decryption fed arbitrary nonce, AAD, ciphertext, and tag bytes must return
//! `Ok`/`Err` and never panic, read out of bounds, or release plaintext when the tag is wrong.
//! The target also checks that a genuine seal/open round trip recovers the plaintext.
//!
//! Run: `cargo +nightly fuzz run aead_open --fuzz-dir crypto/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rsl_crypto::aead::{
    Aead,
    chacha20poly1305::{
        ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce, ChaCha20Poly1305Tag,
    },
    gcm::{
        Aes128Gcm, Aes128GcmKey, Aes128GcmNonce, Aes128GcmTag, Aes256Gcm, Aes256GcmKey,
        Aes256GcmNonce, Aes256GcmTag,
    },
};

fn split(data: &[u8]) -> Option<([u8; 32], [u8; 12], [u8; 16], usize, &[u8])> {
    if data.len() < 32 + 12 + 16 + 1 {
        return None;
    }
    let key: [u8; 32] = data[..32].try_into().ok()?;
    let nonce: [u8; 12] = data[32..44].try_into().ok()?;
    let tag: [u8; 16] = data[44..60].try_into().ok()?;
    let aad_len = usize::from(data[60]);
    Some((key, nonce, tag, aad_len, &data[61..]))
}

fuzz_target!(|data: &[u8]| {
    let Some((key, nonce, tag, aad_len, rest)) = split(data) else {
        return;
    };
    let aad_len = aad_len.min(rest.len());
    let (aad, payload) = rest.split_at(aad_len);

    // Arbitrary tag over arbitrary ciphertext: must be rejected or, vanishingly rarely, accepted
    // — never a panic.
    let gcm128 = Aes128Gcm::new(Aes128GcmKey::new(key[..16].try_into().unwrap()));
    let _ = gcm128.open(&Aes128GcmNonce::new(nonce), aad, payload, &Aes128GcmTag::new(tag));
    let gcm256 = Aes256Gcm::new(Aes256GcmKey::new(key));
    let _ = gcm256.open(&Aes256GcmNonce::new(nonce), aad, payload, &Aes256GcmTag::new(tag));
    let chacha = ChaCha20Poly1305::new(ChaCha20Poly1305Key::new(key));
    let _ = chacha.open(
        &ChaCha20Poly1305Nonce::new(nonce),
        aad,
        payload,
        &ChaCha20Poly1305Tag::new(tag),
    );

    // Genuine round trips must succeed, and a flipped tag bit must fail.
    let sealed = gcm256.seal(&Aes256GcmNonce::new(nonce), aad, payload).unwrap();
    assert_eq!(
        gcm256
            .open(&Aes256GcmNonce::new(nonce), aad, sealed.ciphertext(), sealed.tag())
            .unwrap(),
        payload
    );
    let mut wrong = sealed.tag().into_bytes();
    wrong[0] ^= 1;
    assert!(
        gcm256
            .open(&Aes256GcmNonce::new(nonce), aad, sealed.ciphertext(), &Aes256GcmTag::new(wrong))
            .is_err()
    );
    let sealed = Aead::seal(&chacha, &ChaCha20Poly1305Nonce::new(nonce), aad, payload).unwrap();
    assert_eq!(
        Aead::open(&chacha, &ChaCha20Poly1305Nonce::new(nonce), aad, sealed.ciphertext(), sealed.tag()).unwrap(),
        payload
    );
});
