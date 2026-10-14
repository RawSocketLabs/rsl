//! Range, encoding, and rejection evidence for the ECDSA P-256 public boundary.

use rsl_crypto::{
    CryptoError,
    signature::ecdsa_p256::{EcdsaP256Signature, EcdsaP256VerifyingKey},
};

use crate::support;

const UX: &str = "60FED4BA255A9D31C961EB74C6356D68C049B8923B61FA6CE669622E60F29FB6";
const UY: &str = "7903FE1008B8BC99A41AE9E95628BC64F2F1B20C2D7E9F5177A3C294D4462299";
const SAMPLE_R: &str = "EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716";
const SAMPLE_S: &str = "F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8";
/// SP 800-186 §3.2.1.3 group order `n`.
const ORDER: &str = "FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551";

/// FIPS 186-5 §6.4.2 step 1 evidence: `r` and `s` outside `[1, n-1]` are rejected.
#[test]
fn zero_order_and_above_order_scalars_are_rejected() {
    let key = support::verifying_key(UX, UY);
    let n: [u8; 32] = support::decode(ORDER);
    let valid_r: [u8; 32] = support::decode(SAMPLE_R);
    let valid_s: [u8; 32] = support::decode(SAMPLE_S);

    for (r, s) in [
        ([0_u8; 32], valid_s),
        (valid_r, [0_u8; 32]),
        (n, valid_s),
        (valid_r, n),
        ([0xff; 32], valid_s),
        (valid_r, [0xff; 32]),
    ] {
        let mut bytes = [0_u8; 64];
        bytes[..32].copy_from_slice(&r);
        bytes[32..].copy_from_slice(&s);
        assert_eq!(
            key.verify_sha256(b"sample", &EcdsaP256Signature::from_bytes(bytes)),
            Err(CryptoError::InvalidSignature)
        );
    }
}

/// Standard-derived evidence: `(r, n - s)` is also a valid signature; this profile does not
/// impose low-`s` normalization, which is a protocol policy.
#[test]
fn the_complementary_s_value_also_verifies() {
    let key = support::verifying_key(UX, UY);
    let n: [u8; 32] = support::decode(ORDER);
    let s: [u8; 32] = support::decode(SAMPLE_S);
    let mut complement = [0_u8; 32];
    let mut borrow = 0_u16;
    for index in (0..32).rev() {
        let difference = i32::from(n[index]) - i32::from(s[index]) - i32::from(borrow);
        let (value, next_borrow) = if difference < 0 {
            (difference + 256, 1)
        } else {
            (difference, 0)
        };
        complement[index] = u8::try_from(value).unwrap();
        borrow = next_borrow;
    }
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(&support::decode::<32>(SAMPLE_R));
    bytes[32..].copy_from_slice(&complement);
    key.verify_sha256(b"sample", &EcdsaP256Signature::from_bytes(bytes))
        .expect("(r, n - s) verifies under the same key and message");
}

/// Standard-derived evidence: every single-bit change to the signature or message fails.
#[test]
fn any_changed_signature_bit_or_message_byte_fails() {
    let key = support::verifying_key(UX, UY);
    let original = support::signature(SAMPLE_R, SAMPLE_S).into_bytes();
    for byte_index in [0, 15, 31, 32, 47, 63] {
        let mut changed = original;
        changed[byte_index] ^= 0x01;
        assert_eq!(
            key.verify_sha256(b"sample", &EcdsaP256Signature::from_bytes(changed)),
            Err(CryptoError::InvalidSignature),
            "byte {byte_index}"
        );
    }
    assert_eq!(
        key.verify_sha256(b"samplf", &EcdsaP256Signature::from_bytes(original)),
        Err(CryptoError::InvalidSignature)
    );
    assert_eq!(
        key.verify_sha256(b"", &EcdsaP256Signature::from_bytes(original)),
        Err(CryptoError::InvalidSignature)
    );
}

/// Standard-derived evidence: a different valid key rejects the signature.
#[test]
fn a_different_valid_key_rejects_the_signature() {
    let other = support::verifying_key(
        "DAD0B65394221CF9B051E1FECA5787D098DFE637FC90B9EF945D0C3772581180",
        "5271A0461CDB8252D61F1C456FA3E59AB1F45B33ACCF5F58389E0577B8990BB3",
    );
    assert_eq!(
        other.verify_sha256(b"sample", &support::signature(SAMPLE_R, SAMPLE_S)),
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
    x_too_large[1..33].fill(0xff);

    for bytes in [wrong_prefix, off_curve, x_too_large] {
        assert_eq!(
            EcdsaP256VerifyingKey::from_bytes(bytes),
            Err(CryptoError::InvalidPublicKey)
        );
    }
    assert_eq!(
        EcdsaP256VerifyingKey::try_from(&valid[..64]),
        Err(CryptoError::InvalidLength {
            name: "ECDSA P-256 public key",
            expected: 65,
            actual: 64,
        })
    );
    let key = EcdsaP256VerifyingKey::try_from(valid.as_slice()).unwrap();
    assert_eq!(key.into_bytes(), valid);
}

/// Regression evidence: signatures are parsed by exact length and preserve bytes.
#[test]
fn signature_wire_parsing_preserves_bytes_and_checks_length() {
    let bytes: [u8; 64] = core::array::from_fn(|index| u8::try_from(index).unwrap());
    let signature = EcdsaP256Signature::try_from(bytes.as_slice()).unwrap();
    assert_eq!(signature.as_bytes(), &bytes);
    assert_eq!(
        EcdsaP256Signature::try_from(&bytes[..63]),
        Err(CryptoError::InvalidLength {
            name: "ECDSA P-256 signature",
            expected: 64,
            actual: 63,
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
    use rsl_crypto::signature::ecdsa_p256::EcdsaP256SigningKey;

    let n: [u8; 32] = support::decode(ORDER);
    assert!(EcdsaP256SigningKey::from_bytes([0; 32]).is_err());
    assert!(EcdsaP256SigningKey::from_bytes(n).is_err());
    let mut n_minus_one = n;
    n_minus_one[31] -= 1;
    assert!(EcdsaP256SigningKey::from_bytes(n_minus_one).is_ok());

    let mut source = CountingSource {
        fills: vec![0x10, 0xff],
    };
    let generated = EcdsaP256SigningKey::generate(&mut source).unwrap();
    let mut expected = [0x10_u8; 32];
    expected[31] = 0x11;
    assert_eq!(
        generated.verifying_key(),
        EcdsaP256SigningKey::from_bytes(expected)
            .unwrap()
            .verifying_key()
    );
    assert_eq!(
        EcdsaP256SigningKey::generate(&mut CountingSource { fills: vec![] }).err(),
        Some(CryptoError::EntropyUnavailable)
    );
}

/// Regression evidence: the generic `Signer` path ignores randomness and matches the inherent
/// deterministic path; the prehashed path matches the message path.
#[test]
fn generic_signer_and_prehashed_signing_match_the_message_path() {
    use rsl_crypto::signature::{Signer, ecdsa_p256::EcdsaP256SigningKey};

    let key = EcdsaP256SigningKey::from_bytes([0x42; 32]).unwrap();
    let inherent = key.sign_sha256(b"message").unwrap();
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
    let digest = rsl_crypto::digest::sha2::sha256::Sha256::digest(b"message").unwrap();
    assert_eq!(key.sign_sha256_digest(&digest).unwrap(), inherent);
    assert_ne!(key.sign_sha256(b"messagf").unwrap(), inherent);
}
