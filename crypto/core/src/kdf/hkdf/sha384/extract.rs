//! HKDF-SHA-384 Extract and its secret pseudorandom-key output.
//!
//! ## Standards ownership
//!
//! [RFC 5869 §2.2][rfc-5869] defines `PRK = HMAC-Hash(salt, IKM)`. For this instantiation,
//! `HMAC-Hash` is HMAC-SHA-384 and the `PRK` is exactly 48 bytes. If the caller has no salt, the
//! RFC substitutes exactly `HashLen` zero octets as the HMAC key. A present zero-length salt is
//! passed through as a zero-length key; HMAC normalization makes the two cases mathematically
//! equivalent while the API keeps their semantic distinction visible.
//!
//! The input keying material is the HMAC *message*, not its key. This argument order is called out
//! explicitly by RFC 5869 §2.1 and remains visible in [`extract`].
//!
//! [rfc-5869]: https://www.rfc-editor.org/rfc/rfc5869.html

use crate::{Result, SecretBytes, mac::hmac::sha384::HmacSha384};

use super::HASH_LEN;

/// The RFC 5869 §2.2 replacement when no salt is provided.
const ABSENT_SALT: [u8; HASH_LEN] = [0; HASH_LEN];

/// A 48-byte pseudorandom key produced by HKDF-SHA-384 Extract.
///
/// This value is secret keying material, not a public authentication tag. It deliberately does
/// not implement [`Clone`] or [`Debug`], stores its bytes in zeroizing memory, and requires an
/// explicit exposure call when the Expand stage needs to use it.
///
/// # Examples
///
/// ```
/// use rsl_crypto::kdf::hkdf::sha384::{HkdfSha384Prk, extract};
///
/// let prk: HkdfSha384Prk = extract(Some(b"public salt"), b"secret input")?;
/// assert_eq!(prk.expose_secret().len(), HkdfSha384Prk::LEN);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct HkdfSha384Prk(SecretBytes<HASH_LEN>);

impl HkdfSha384Prk {
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
/// underlying HMAC-SHA-384 message-length bound.
///
/// # Examples
///
/// ```
/// use rsl_crypto::kdf::hkdf::sha384::extract;
///
/// let salted = extract(Some(b"public salt"), b"secret input keying material")?;
/// let unsalted = extract(None, b"secret input keying material")?;
/// assert_ne!(salted.expose_secret(), unsalted.expose_secret());
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub fn extract(salt: Option<&[u8]>, input_key_material: &[u8]) -> Result<HkdfSha384Prk> {
    let hmac_key = salt.unwrap_or(&ABSENT_SALT);
    let prk = HmacSha384::authenticate(hmac_key, input_key_material)?.into_bytes();

    Ok(HkdfSha384Prk(SecretBytes::new(prk)))
}

#[cfg(test)]
mod unit {
    use super::{HASH_LEN, extract};
    use crate::mac::hmac::sha384::HmacSha384;

    /// Standard-derived evidence for RFC 5869 §2.2: `PRK = HMAC-Hash(salt, IKM)` with SHA-384.
    ///
    /// RFC 5869 publishes no SHA-384 vectors; the public Wycheproof suite supplies published
    /// end-to-end evidence, and this test pins the Extract stage to the HMAC it is defined by.
    #[test]
    fn extract_is_hmac_sha384_keyed_by_the_salt() {
        let ikm = [0x0b; 22];
        let salt: [u8; 13] = core::array::from_fn(|index| u8::try_from(index).unwrap());
        let expected = HmacSha384::authenticate(&salt, ikm).unwrap().into_bytes();
        let prk = extract(Some(&salt), &ikm).expect("the fixture is within HMAC limits");

        assert_eq!(prk.expose_secret(), &expected);
        assert_eq!(prk.expose_secret().len(), HASH_LEN);
    }

    /// Standard-derived evidence for RFC 5869 §2.2: an absent salt is `HashLen` zero bytes, so
    /// `None` and an explicitly empty salt produce the same PRK.
    #[test]
    fn explicit_empty_and_absent_salt_use_hash_len_zero_bytes() {
        let ikm = [0x0b; 22];
        let expected = HmacSha384::authenticate(&[0; HASH_LEN], ikm)
            .unwrap()
            .into_bytes();
        let empty = extract(Some(&[]), &ikm).expect("the fixture is within HMAC limits");
        let absent = extract(None, &ikm).expect("the fixture is within HMAC limits");

        assert_eq!(empty.expose_secret(), &expected);
        assert_eq!(absent.expose_secret(), &expected);
    }
}
