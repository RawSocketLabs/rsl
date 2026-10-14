//! HKDF-SHA-256 Extract and its secret pseudorandom-key output.
//!
//! ## Standards ownership
//!
//! [RFC 5869 §2.2][rfc-5869] defines `PRK = HMAC-Hash(salt, IKM)`. For this instantiation,
//! `HMAC-Hash` is HMAC-SHA-256 and the `PRK` is exactly 32 bytes. If the caller has no salt, the
//! RFC substitutes exactly `HashLen` zero octets as the HMAC key. A present zero-length salt is
//! passed through as a zero-length key; HMAC normalization makes the two cases mathematically
//! equivalent while the API keeps their semantic distinction visible.
//!
//! The input keying material is the HMAC *message*, not its key. This argument order is called out
//! explicitly by RFC 5869 §2.1 and remains visible in [`extract`].
//!
//! [rfc-5869]: https://www.rfc-editor.org/rfc/rfc5869.html

use crate::{Result, SecretBytes, mac::hmac::sha256::HmacSha256};

use super::HASH_LEN;

/// The RFC 5869 §2.2 replacement when no salt is provided.
const ABSENT_SALT: [u8; HASH_LEN] = [0; HASH_LEN];

/// A 32-byte pseudorandom key produced by HKDF-SHA-256 Extract.
///
/// This value is secret keying material, not a public authentication tag. It deliberately does
/// not implement [`Clone`] or [`Debug`], stores its bytes in zeroizing memory, and requires an
/// explicit exposure call when the Expand stage needs to use it.
///
/// # Examples
///
/// ```
/// use rsl_crypto::kdf::hkdf::sha256::{HkdfSha256Prk, extract};
///
/// let prk: HkdfSha256Prk = extract(Some(b"public salt"), b"secret input")?;
/// assert_eq!(prk.expose_secret().len(), HkdfSha256Prk::LEN);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct HkdfSha256Prk(SecretBytes<HASH_LEN>);

impl HkdfSha256Prk {
    /// The exact pseudorandom-key length in bytes.
    pub const LEN: usize = HASH_LEN;

    /// Explicitly borrow the secret PRK bytes.
    #[must_use]
    pub fn expose_secret(&self) -> &[u8; HASH_LEN] {
        self.0.expose_secret()
    }
}

/// Extract a fixed-length pseudorandom key from input keying material.
///
/// `None` means that no salt is available and applies RFC 5869's 32-zero-byte substitution.
/// `Some(&[])` means that the application explicitly supplied an empty salt. The salt is commonly
/// non-secret, while `input_key_material` and the returned PRK are secret-bearing values whose
/// source storage remains the caller's responsibility.
///
/// # Errors
///
/// Returns [`crate::CryptoError::MessageTooLong`] if the input keying material exceeds the
/// underlying HMAC-SHA-256 message-length bound.
///
/// # Examples
///
/// ```
/// use rsl_crypto::kdf::hkdf::sha256::extract;
///
/// let salted = extract(Some(b"public salt"), b"secret input keying material")?;
/// let unsalted = extract(None, b"secret input keying material")?;
/// assert_ne!(salted.expose_secret(), unsalted.expose_secret());
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub fn extract(salt: Option<&[u8]>, input_key_material: &[u8]) -> Result<HkdfSha256Prk> {
    let hmac_key = salt.unwrap_or(&ABSENT_SALT);
    let prk = HmacSha256::authenticate(hmac_key, input_key_material)?.into_bytes();

    Ok(HkdfSha256Prk(SecretBytes::new(prk)))
}

#[cfg(test)]
mod unit {
    use super::extract;

    /// Published intermediate-value evidence from RFC 5869 Appendix A.1.
    #[test]
    fn test_case_1_extracts_the_published_prk() {
        let ikm = [0x0b; 22];
        let salt: [u8; 13] = core::array::from_fn(|index| {
            u8::try_from(index).expect("RFC 5869 Test Case 1 salt indices fit u8")
        });
        let expected = [
            0x07, 0x77, 0x09, 0x36, 0x2c, 0x2e, 0x32, 0xdf, 0x0d, 0xdc, 0x3f, 0x0d, 0xc4, 0x7b,
            0xba, 0x63, 0x90, 0xb6, 0xc7, 0x3b, 0xb5, 0x0f, 0x9c, 0x31, 0x22, 0xec, 0x84, 0x4a,
            0xd7, 0xc2, 0xb3, 0xe5,
        ];
        let prk = extract(Some(&salt), &ikm).expect("the RFC fixture is within HMAC limits");

        assert_eq!(prk.expose_secret(), &expected);
    }

    /// Published intermediate-value evidence from RFC 5869 Appendix A.2.
    #[test]
    fn test_case_2_extracts_the_published_long_input_prk() {
        let ikm: [u8; 80] = core::array::from_fn(|index| {
            u8::try_from(index).expect("RFC 5869 Test Case 2 IKM indices fit u8")
        });
        let salt: [u8; 80] = core::array::from_fn(|index| {
            0x60_u8.wrapping_add(
                u8::try_from(index).expect("RFC 5869 Test Case 2 salt indices fit u8"),
            )
        });
        let expected = [
            0x06, 0xa6, 0xb8, 0x8c, 0x58, 0x53, 0x36, 0x1a, 0x06, 0x10, 0x4c, 0x9c, 0xeb, 0x35,
            0xb4, 0x5c, 0xef, 0x76, 0x00, 0x14, 0x90, 0x46, 0x71, 0x01, 0x4a, 0x19, 0x3f, 0x40,
            0xc1, 0x5f, 0xc2, 0x44,
        ];
        let prk = extract(Some(&salt), &ikm).expect("the RFC fixture is within HMAC limits");

        assert_eq!(prk.expose_secret(), &expected);
    }

    /// Published intermediate-value evidence from RFC 5869 Appendix A.3.
    #[test]
    fn explicit_empty_and_absent_salt_match_the_published_prk() {
        let ikm = [0x0b; 22];
        let expected = [
            0x19, 0xef, 0x24, 0xa3, 0x2c, 0x71, 0x7b, 0x16, 0x7f, 0x33, 0xa9, 0x1d, 0x6f, 0x64,
            0x8b, 0xdf, 0x96, 0x59, 0x67, 0x76, 0xaf, 0xdb, 0x63, 0x77, 0xac, 0x43, 0x4c, 0x1c,
            0x29, 0x3c, 0xcb, 0x04,
        ];
        let empty = extract(Some(&[]), &ikm).expect("the RFC fixture is within HMAC limits");
        let absent = extract(None, &ikm).expect("the RFC fixture is within HMAC limits");

        assert_eq!(empty.expose_secret(), &expected);
        assert_eq!(absent.expose_secret(), &expected);
    }
}
