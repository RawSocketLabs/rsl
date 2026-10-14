//! Incremental HMAC-SHA-384 composition, tag representation, and verification.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 198-1 §4, Table 1, steps 4–9][fips-198-1] computes
//! `H((K0 XOR opad) || H((K0 XOR ipad) || text))`. This module represents concatenation as
//! sequential [`Sha384::update`] calls: `inner` is initialized with the complete inner pad block,
//! while `outer` is initialized with the complete outer pad block. Message fragments are then
//! added only to `inner`; finalization feeds the 48-byte inner digest to `outer`.
//!
//! Both seeded SHA-384 states contain secret-derived chaining values. [`Sha384`] zeroizes its
//! internal chaining words and buffered bytes on drop, and the temporary inner digest bytes are
//! explicitly zeroized after the outer state consumes them.
//!
//! [fips-198-1]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.198-1.pdf

use core::fmt;
use zeroize::Zeroize;

use crate::{
    CryptoError, Result,
    digest::{Digest, sha2::sha384::Sha384},
    mac::Mac,
};

use super::key::NormalizedKey;

/// The full HMAC-SHA-384 tag length in bytes.
const TAG_LEN: usize = 48;

/// A finalized, full-length HMAC-SHA-384 authentication tag.
///
/// A distinct type prevents a tag from being accidentally interchanged with an ordinary
/// SHA-384 digest, encryption key, nonce, or unrelated 48-byte value. This first reference API
/// intentionally represents only the complete 384-bit tag; protocol-specific truncation remains
/// outside this type.
///
/// # Examples
///
/// ```
/// use rsl_crypto::mac::hmac::sha384::{HmacSha384, HmacSha384Tag};
///
/// let tag: HmacSha384Tag = HmacSha384::authenticate(b"key", b"message")?;
/// assert_eq!(tag.as_bytes().len(), HmacSha384Tag::LEN);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "an authentication tag should be transmitted, verified, or otherwise consumed"]
pub struct HmacSha384Tag([u8; TAG_LEN]);

impl HmacSha384Tag {
    /// The serialized full-tag length in bytes.
    pub const LEN: usize = TAG_LEN;

    /// Borrow the complete tag as a fixed-size byte array.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; TAG_LEN] {
        &self.0
    }

    /// Consume the tag and return its fixed-size byte array.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; TAG_LEN] {
        self.0
    }
}

impl AsRef<[u8]> for HmacSha384Tag {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for HmacSha384Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HmacSha384Tag(")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// Incremental HMAC-SHA-384 state.
///
/// Construction copies the supplied key into secret-bearing temporary storage, derives the inner
/// and outer padded blocks, and seeds two independent SHA-384 states. The original borrowed key is
/// never retained; its source storage remains the caller's responsibility. This type deliberately
/// does not implement [`Clone`] or [`Debug`] because its states are equivalent to secret key
/// material.
///
/// # Examples
///
/// ```
/// use rsl_crypto::mac::hmac::sha384::HmacSha384;
///
/// let mut state = HmacSha384::new(b"shared secret")?;
/// state.update(b"one ")?;
/// state.update(b"message")?;
/// let tag = state.finalize();
///
/// let mut verifier = HmacSha384::new(b"shared secret")?;
/// verifier.update(b"one message")?;
/// verifier.verify(tag.as_bytes())?;
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct HmacSha384 {
    /// SHA-384 state after `K0 XOR ipad`, followed by message bytes accepted so far.
    inner: Sha384,
    /// SHA-384 state after the complete `K0 XOR opad` block.
    outer: Sha384,
}

impl HmacSha384 {
    /// Construct a new HMAC-SHA-384 state from arbitrary key bytes.
    ///
    /// FIPS 198-1 §4 accepts keys shorter than, equal to, or longer than SHA-384's 128-byte input
    /// block and normalizes each case explicitly. Applications remain responsible for choosing a
    /// key with appropriate entropy and strength. In particular, accepting a short or empty byte
    /// string at this primitive boundary is not a recommendation to generate such an HMAC key.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if a key is too long for SHA-384 to hash during
    /// FIPS 198-1 key normalization. See [`HmacSha384`] for a complete incremental example.
    pub fn new(key: &[u8]) -> Result<Self> {
        let normalized = NormalizedKey::from_key(key)?;
        let (inner_pad, outer_pad) = normalized.into_padded_blocks();

        let mut inner = Sha384::new();
        inner.update(inner_pad.expose_secret())?;

        let mut outer = Sha384::new();
        outer.update(outer_pad.expose_secret())?;

        Ok(Self { inner, outer })
    }

    /// Incorporate more authenticated message bytes.
    ///
    /// Common borrowed and owned byte-like values are accepted without defining an implicit
    /// serialization. Text is authenticated as its exact UTF-8 byte representation.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] before changing the state if the inner SHA-384
    /// message-length bound would be exceeded. The bound includes HMAC's 128-byte inner pad. See
    /// [`HmacSha384`] for a complete example.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.inner.update(input)
    }

    /// Complete the inner and outer hashes and return the full 384-bit tag.
    ///
    /// # Panics
    ///
    /// Panics only if a private invariant is broken and the outer SHA-384 state, which contains
    /// exactly 128 bytes, rejects the fixed 48-byte inner digest as too long. Callers cannot create
    /// or modify that state directly.
    pub fn finalize(self) -> HmacSha384Tag {
        let Self { inner, mut outer } = self;
        let mut inner_digest = inner.finalize().into_bytes();

        // `outer` contains exactly one 128-byte prefix and this update is exactly 48 bytes, so it
        // cannot approach SHA-384's 2^128-bit message-length limit. Failure would indicate a broken
        // private state invariant rather than caller-controlled input.
        <Sha384 as Digest>::update(&mut outer, &inner_digest)
            .expect("a 128-byte SHA-384 prefix can always accept a 48-byte inner digest");
        inner_digest.zeroize();

        HmacSha384Tag(outer.finalize().into_bytes())
    }

    /// Authenticate one complete message with one key.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::mac::hmac::sha384::HmacSha384;
    ///
    /// let tag = HmacSha384::authenticate(b"shared secret", "UTF-8 message")?;
    /// assert_eq!(tag.as_bytes().len(), 48);
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if key normalization or the message would exceed
    /// the underlying SHA-384 message-length bound.
    pub fn authenticate(key: &[u8], message: impl AsRef<[u8]>) -> Result<HmacSha384Tag> {
        let mut state = Self::new(key)?;
        state.update(message)?;
        Ok(state.finalize())
    }

    /// Verify a supplied full-length tag without returning the computed tag.
    ///
    /// Every computed tag byte participates in one XOR/OR accumulator; tag contents never cause
    /// an early return. The supplied slice's length affects indexing and is not secret. This
    /// straightforward source shape is designed for review, but has not received compiler-level
    /// constant-time analysis and is not a production-security claim.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::AuthenticationFailed`] for every wrong value or wrong length.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::{CryptoError, mac::hmac::sha384::HmacSha384};
    ///
    /// let mut verifier = HmacSha384::new(b"right key")?;
    /// verifier.update(b"message")?;
    /// assert_eq!(
    ///     verifier.verify([0_u8; 32]),
    ///     Err(CryptoError::AuthenticationFailed),
    /// );
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    pub fn verify(self, expected: impl AsRef<[u8]>) -> Result<()> {
        let computed = self.finalize();

        if tags_match(computed.as_bytes(), expected.as_ref()) {
            Ok(())
        } else {
            Err(CryptoError::AuthenticationFailed)
        }
    }
}

impl Mac for HmacSha384 {
    type Tag = HmacSha384Tag;

    fn new(key: &[u8]) -> Result<Self> {
        Self::new(key)
    }

    fn update(&mut self, input: &[u8]) -> Result<()> {
        self.update(input)
    }

    fn finalize(self) -> Self::Tag {
        self.finalize()
    }

    fn verify(self, expected: &[u8]) -> Result<()> {
        self.verify(expected)
    }
}

/// Compare a complete computed tag with an arbitrary supplied slice without value-based exit.
#[must_use]
fn tags_match(computed: &[u8; TAG_LEN], expected: &[u8]) -> bool {
    let mut difference = computed.len() ^ expected.len();

    for (index, computed_byte) in computed.iter().copied().enumerate() {
        let expected_byte = expected.get(index).copied().unwrap_or(0);
        difference |= usize::from(computed_byte ^ expected_byte);
    }

    difference == 0
}

#[cfg(test)]
mod unit {
    use alloc::format;

    use super::{HmacSha384, HmacSha384Tag, TAG_LEN, tags_match};
    use crate::CryptoError;

    /// Published known-answer evidence from RFC 4231 §4.2, Test Case 1.
    #[test]
    fn rfc_4231_test_case_1_full_tag() {
        let key = [0x0b; 20];
        let tag = HmacSha384::authenticate(&key, b"Hi There")
            .expect("the RFC 4231 fixture is within SHA-384 limits");
        let expected: [u8; TAG_LEN] = [
            0xaf, 0xd0, 0x39, 0x44, 0xd8, 0x48, 0x95, 0x62, 0x6b, 0x08, 0x25, 0xf4, 0xab, 0x46,
            0x90, 0x7f, 0x15, 0xf9, 0xda, 0xdb, 0xe4, 0x10, 0x1e, 0xc6, 0x82, 0xaa, 0x03, 0x4c,
            0x7c, 0xeb, 0xc5, 0x9c, 0xfa, 0xea, 0x9e, 0xa9, 0x07, 0x6e, 0xde, 0x7f, 0x4a, 0xf1,
            0x52, 0xe8, 0xb2, 0xfa, 0x9c, 0xb6,
        ];
        assert_eq!(tag.into_bytes(), expected);
    }

    /// Regression evidence that arbitrary message fragmentation preserves the authenticated data.
    #[test]
    fn one_byte_fragments_match_one_shot_authentication() {
        let key = b"readable key";
        let message = b"authenticated in deliberately tiny pieces";
        let expected = HmacSha384::authenticate(key, message).expect("the fixture is short");
        let mut state = HmacSha384::new(key).expect("the fixture key is short");

        for byte in message {
            state.update([*byte]).expect("the fixture is short");
        }

        assert_eq!(state.finalize(), expected);
    }

    /// Negative evidence for wrong values at every position and wrong lengths.
    #[test]
    fn tag_comparison_checks_every_byte_and_the_exact_length() {
        let computed = [0xa5; TAG_LEN];

        assert!(tags_match(&computed, &computed));
        assert!(!tags_match(&computed, &computed[..TAG_LEN - 1]));

        let mut longer = [0xa5; TAG_LEN + 1];
        assert!(!tags_match(&computed, &longer));

        for index in 0..TAG_LEN {
            longer[..TAG_LEN].copy_from_slice(&computed);
            longer[index] ^= 1;
            assert!(!tags_match(&computed, &longer[..TAG_LEN]));
        }
    }

    /// Negative public-contract evidence for a mismatched tag.
    #[test]
    fn verification_returns_only_the_uniform_authentication_error() {
        let state = HmacSha384::new(b"key").expect("the fixture key is short");

        assert_eq!(
            state.verify([0; TAG_LEN]),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    /// Regression evidence for the tag newtype's exact byte access and visible debug form.
    #[test]
    fn tag_value_exposes_exact_public_bytes_and_names_itself() {
        let bytes = [0xab; HmacSha384Tag::LEN];
        let tag = HmacSha384Tag(bytes);

        assert_eq!(tag.as_bytes(), &bytes);
        assert_eq!(tag.as_ref(), bytes.as_slice());
        assert_eq!(
            format!("{tag:?}"),
            "HmacSha384Tag(abababababababababababababababababababababababababababababababababababababababababababababababab)"
        );
    }
}
