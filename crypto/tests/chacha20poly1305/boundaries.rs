//! Authentication-failure, empty-input, and generic-contract evidence.

use rsl_crypto::{
    CryptoError, RandomSource, Result,
    aead::{
        Aead,
        chacha20poly1305::{
            ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce, ChaCha20Poly1305Tag,
        },
    },
};

struct FixedSource(u8);

impl RandomSource for FixedSource {
    fn fill_bytes(&mut self, output: &mut [u8]) -> Result<()> {
        output.fill(self.0);
        Ok(())
    }
}

/// Standard-derived evidence: every changed nonce, AAD, ciphertext, or tag byte is rejected.
#[test]
fn every_changed_input_byte_fails_authentication() {
    let algorithm = ChaCha20Poly1305::new(ChaCha20Poly1305Key::new([0x42; 32]));
    let nonce = ChaCha20Poly1305Nonce::new([0x24; 12]);
    let aad = b"associated";
    let sealed = algorithm
        .seal(&nonce, aad, b"a payload spanning more than one block?")
        .unwrap();
    let (ciphertext, tag) = sealed.into_parts();

    for index in 0..ciphertext.len() {
        let mut changed = ciphertext.clone();
        changed[index] ^= 0x01;
        assert_eq!(
            algorithm.open(&nonce, aad, &changed, &tag),
            Err(CryptoError::AuthenticationFailed),
            "ciphertext byte {index}"
        );
    }
    for index in 0..16 {
        let mut changed = tag.into_bytes();
        changed[index] ^= 0x80;
        assert_eq!(
            algorithm.open(&nonce, aad, &ciphertext, &ChaCha20Poly1305Tag::new(changed)),
            Err(CryptoError::AuthenticationFailed),
            "tag byte {index}"
        );
    }
    assert!(
        algorithm
            .open(
                &ChaCha20Poly1305Nonce::new([0x25; 12]),
                aad,
                &ciphertext,
                &tag
            )
            .is_err()
    );
    assert!(
        algorithm
            .open(&nonce, b"associatee", &ciphertext, &tag)
            .is_err()
    );
    assert!(
        algorithm
            .open(&nonce, aad, &ciphertext[..ciphertext.len() - 1], &tag)
            .is_err()
    );
}

/// Standard-derived evidence: empty AAD and empty plaintext are valid inputs.
#[test]
fn empty_inputs_seal_and_open() {
    let algorithm = ChaCha20Poly1305::new(ChaCha20Poly1305Key::new([0x11; 32]));
    let nonce = ChaCha20Poly1305Nonce::new([0; 12]);
    let sealed = algorithm.seal(&nonce, b"", b"").unwrap();
    assert!(sealed.ciphertext().is_empty());
    assert_eq!(algorithm.open(&nonce, b"", b"", sealed.tag()).unwrap(), b"");
}

/// Regression evidence: generation uses the caller's source and generic dispatch matches.
#[test]
fn generation_and_generic_dispatch() {
    let mut source = FixedSource(0x5a);
    let key = ChaCha20Poly1305Key::generate(&mut source).unwrap();
    let nonce = ChaCha20Poly1305Nonce::generate(&mut source).unwrap();
    assert_eq!(nonce.as_bytes(), &[0x5a; 12]);
    let algorithm = ChaCha20Poly1305::new(key);
    let generic = Aead::seal(&algorithm, &nonce, b"h", b"p").unwrap();
    let inherent = algorithm.seal(&nonce, b"h", b"p").unwrap();
    assert_eq!(generic, inherent);
    assert_eq!(
        Aead::open(
            &algorithm,
            &nonce,
            b"h",
            generic.ciphertext(),
            generic.tag()
        )
        .unwrap(),
        b"p"
    );
}
