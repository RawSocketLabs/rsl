//! Range, encoding, and rejection evidence for the ECDSA P-384 public boundary.

use rsl_crypto::{
    CryptoError,
    signature::ecdsa_p384::{EcdsaP384Signature, EcdsaP384VerifyingKey},
};

use crate::support;

const UX: &str = "ec3a4e415b4e19a4568618029f427fa5da9a8bc4ae92e02e06aae5286b300c64def8f0ea9055866064a254515480bc13";
const UY: &str = "8015d9b72d7d57244ea8ef9ac0c621896708a59367f9dfb9f54ca84b3f1c9db1288b231c3ae0d4fe7344fd2533264720";
const SAMPLE_R: &str = "94edbb92a5ecb8aad4736e56c691916b3f88140666ce9fa73d64c4ea95ad133c81a648152e44acf96e36dd1e80fabe46";
const SAMPLE_S: &str = "99ef4aeb15f178cea1fe40db2603138f130e740a19624526203b6351d0a3a94fa329c145786e679e7b82c71a38628ac8";
/// SP 800-186 §3.2.1.3 group order `n`.
const ORDER: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC7634D81F4372DDF581A0DB248B0A77AECEC196ACCC52973";

/// FIPS 186-5 §6.4.2 step 1 evidence: `r` and `s` outside `[1, n-1]` are rejected.
#[test]
fn zero_order_and_above_order_scalars_are_rejected() {
    let key = support::verifying_key(UX, UY);
    let n: [u8; 48] = support::decode(ORDER);
    let valid_r: [u8; 48] = support::decode(SAMPLE_R);
    let valid_s: [u8; 48] = support::decode(SAMPLE_S);

    for (r, s) in [
        ([0_u8; 48], valid_s),
        (valid_r, [0_u8; 48]),
        (n, valid_s),
        (valid_r, n),
        ([0xff; 48], valid_s),
        (valid_r, [0xff; 48]),
    ] {
        let mut bytes = [0_u8; 96];
        bytes[..48].copy_from_slice(&r);
        bytes[48..].copy_from_slice(&s);
        assert_eq!(
            key.verify_sha384(b"sample", &EcdsaP384Signature::from_bytes(bytes)),
            Err(CryptoError::InvalidSignature)
        );
    }
}

/// Standard-derived evidence: `(r, n - s)` is also a valid signature; this profile does not
/// impose low-`s` normalization, which is a protocol policy.
#[test]
fn the_complementary_s_value_also_verifies() {
    let key = support::verifying_key(UX, UY);
    let n: [u8; 48] = support::decode(ORDER);
    let s: [u8; 48] = support::decode(SAMPLE_S);
    let mut complement = [0_u8; 48];
    let mut borrow = 0_u16;
    for index in (0..48).rev() {
        let difference = i32::from(n[index]) - i32::from(s[index]) - i32::from(borrow);
        let (value, next_borrow) = if difference < 0 {
            (difference + 256, 1)
        } else {
            (difference, 0)
        };
        complement[index] = u8::try_from(value).unwrap();
        borrow = next_borrow;
    }
    let mut bytes = [0_u8; 96];
    bytes[..48].copy_from_slice(&support::decode::<48>(SAMPLE_R));
    bytes[48..].copy_from_slice(&complement);
    key.verify_sha384(b"sample", &EcdsaP384Signature::from_bytes(bytes))
        .expect("(r, n - s) verifies under the same key and message");
}

/// Standard-derived evidence: every single-bit change to the signature or message fails.
#[test]
fn any_changed_signature_bit_or_message_byte_fails() {
    let key = support::verifying_key(UX, UY);
    let original = support::signature(SAMPLE_R, SAMPLE_S).into_bytes();
    for byte_index in [0, 23, 47, 48, 71, 95] {
        let mut changed = original;
        changed[byte_index] ^= 0x01;
        assert_eq!(
            key.verify_sha384(b"sample", &EcdsaP384Signature::from_bytes(changed)),
            Err(CryptoError::InvalidSignature),
            "byte {byte_index}"
        );
    }
    assert_eq!(
        key.verify_sha384(b"samplf", &EcdsaP384Signature::from_bytes(original)),
        Err(CryptoError::InvalidSignature)
    );
    assert_eq!(
        key.verify_sha384(b"", &EcdsaP384Signature::from_bytes(original)),
        Err(CryptoError::InvalidSignature)
    );
}

/// Standard-derived evidence: a different valid key rejects the signature.
#[test]
fn a_different_valid_key_rejects_the_signature() {
    let other = support::verifying_key(
        "667842d7d180ac2cde6f74f37551f55755c7645c20ef73e31634fe72b4c55ee6de3ac808acb4bdb4c88732aee95f41aa",
        "9482ed1fc0eeb9cafc4984625ccfc23f65032149e0e144ada024181535a0f38eeb9fcff3c2c947dae69b4c634573a81c",
    );
    assert_eq!(
        other.verify_sha384(b"sample", &support::signature(SAMPLE_R, SAMPLE_S)),
        Err(CryptoError::InvalidSignature)
    );
}

/// SEC 1 §2.3.4 evidence: malformed verifying keys cannot be constructed.
#[test]
fn malformed_verifying_keys_are_rejected() {
    let valid = support::uncompressed(UX, UY);
    let mut wrong_prefix = valid;
    wrong_prefix[0] = 0x03;
    let mut off_curve = valid;
    off_curve[40] ^= 0x80;
    let mut x_too_large = valid;
    x_too_large[1..49].fill(0xff);

    for bytes in [wrong_prefix, off_curve, x_too_large] {
        assert_eq!(
            EcdsaP384VerifyingKey::from_bytes(bytes),
            Err(CryptoError::InvalidPublicKey)
        );
    }
    assert_eq!(
        EcdsaP384VerifyingKey::try_from(&valid[..64]),
        Err(CryptoError::InvalidLength {
            name: "ECDSA P-384 public key",
            expected: 97,
            actual: 64,
        })
    );
    let key = EcdsaP384VerifyingKey::try_from(valid.as_slice()).unwrap();
    assert_eq!(key.into_bytes(), valid);
}

/// Regression evidence: signatures are parsed by exact length and preserve bytes.
#[test]
fn signature_wire_parsing_preserves_bytes_and_checks_length() {
    let bytes: [u8; 96] = core::array::from_fn(|index| u8::try_from(index).unwrap());
    let signature = EcdsaP384Signature::try_from(bytes.as_slice()).unwrap();
    assert_eq!(signature.as_bytes(), &bytes);
    assert_eq!(
        EcdsaP384Signature::try_from(&bytes[..95]),
        Err(CryptoError::InvalidLength {
            name: "ECDSA P-384 signature",
            expected: 96,
            actual: 95,
        })
    );
}

struct CountingSource {
    fills: Vec<u8>,
}

impl rsl_crypto::RandomSource for CountingSource {
    fn fill_bytes(&mut self, output: &mut [u8]) -> rsl_crypto::Result<()> {
        let value = self.fills.pop().ok_or(CryptoError::EntropyUnavailable)?;
        output.fill(value);
        Ok(())
    }
}

/// Standard-derived evidence: signing scalars must lie in `[1, n-1]`, and candidate testing
/// skips `c > n - 2` then returns `d = c + 1`.
#[test]
fn signing_key_range_and_candidate_testing_generation() {
    use rsl_crypto::signature::ecdsa_p384::EcdsaP384SigningKey;

    let n: [u8; 48] = support::decode(ORDER);
    assert!(EcdsaP384SigningKey::from_bytes([0; 48]).is_err());
    assert!(EcdsaP384SigningKey::from_bytes(n).is_err());
    let mut n_minus_one = n;
    n_minus_one[47] -= 1;
    assert!(EcdsaP384SigningKey::from_bytes(n_minus_one).is_ok());

    let mut source = CountingSource {
        fills: vec![0x10, 0xff],
    };
    let generated = EcdsaP384SigningKey::generate(&mut source).unwrap();
    let mut expected = [0x10_u8; 48];
    expected[47] = 0x11;
    assert_eq!(
        generated.verifying_key(),
        EcdsaP384SigningKey::from_bytes(expected)
            .unwrap()
            .verifying_key()
    );
    assert_eq!(
        EcdsaP384SigningKey::generate(&mut CountingSource { fills: vec![] }).err(),
        Some(CryptoError::EntropyUnavailable)
    );
}

/// Regression evidence: the generic `Signer` path ignores randomness and matches the inherent
/// deterministic path; the prehashed path matches the message path.
#[test]
fn generic_signer_and_prehashed_signing_match_the_message_path() {
    use rsl_crypto::signature::{Signer, ecdsa_p384::EcdsaP384SigningKey};

    let key = EcdsaP384SigningKey::from_bytes([0x42; 48]).unwrap();
    let inherent = key.sign_sha384(b"message").unwrap();
    let mut source = CountingSource {
        fills: vec![0x01, 0x02, 0x03],
    };
    let generic = Signer::sign(&key, &mut source, b"message").unwrap();
    assert_eq!(generic, inherent);
    assert_eq!(
        source.fills.len(),
        3,
        "deterministic signing consumed no entropy"
    );
    let digest = rsl_crypto::digest::sha2::sha384::Sha384::digest(b"message").unwrap();
    assert_eq!(key.sign_sha384_digest(&digest).unwrap(), inherent);
    assert_ne!(key.sign_sha384(b"messagf").unwrap(), inherent);
}
