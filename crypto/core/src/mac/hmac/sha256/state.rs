//! Incremental HMAC-SHA-256 composition, tag representation, and verification.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 198-1 §4, Table 1, steps 4–9][fips-198-1] computes
//! `H((K0 XOR opad) || H((K0 XOR ipad) || text))`. This module represents concatenation as
//! sequential [`Sha256::update`] calls: `inner` is initialized with the complete inner pad block,
//! while `outer` is initialized with the complete outer pad block. Message fragments are then
//! added only to `inner`; finalization feeds the 32-byte inner digest to `outer`.
//!
//! Both seeded SHA-256 states contain secret-derived chaining values. [`Sha256`] zeroizes its
//! internal chaining words and buffered bytes on drop, and the temporary inner digest bytes are
//! explicitly zeroized after the outer state consumes them.
//!
//! [fips-198-1]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.198-1.pdf

use core::fmt;
use zeroize::Zeroize;

use crate::{
    CryptoError, Result,
    digest::{Digest, sha2::sha256::Sha256},
    mac::Mac,
};

use super::key::NormalizedKey;

/// The full HMAC-SHA-256 tag length in bytes.
const TAG_LEN: usize = 32;

/// A finalized, full-length HMAC-SHA-256 authentication tag.
///
/// A distinct type prevents a tag from being accidentally interchanged with an ordinary
/// SHA-256 digest, encryption key, nonce, or unrelated 32-byte value. This first reference API
/// intentionally represents only the complete 256-bit tag; protocol-specific truncation remains
/// outside this type.
///
/// # Examples
///
/// ```
/// use rsl_crypto::mac::hmac::sha256::{HmacSha256, HmacSha256Tag};
///
/// let tag: HmacSha256Tag = HmacSha256::authenticate(b"key", b"message")?;
/// assert_eq!(tag.as_bytes().len(), HmacSha256Tag::LEN);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Eq, Hash, PartialEq)]
#[must_use = "an authentication tag should be transmitted, verified, or otherwise consumed"]
pub struct HmacSha256Tag([u8; TAG_LEN]);

impl HmacSha256Tag {
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

impl AsRef<[u8]> for HmacSha256Tag {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for HmacSha256Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HmacSha256Tag(")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// Incremental HMAC-SHA-256 state.
///
/// Construction copies the supplied key into secret-bearing temporary storage, derives the inner
/// and outer padded blocks, and seeds two independent SHA-256 states. The original borrowed key is
/// never retained; its source storage remains the caller's responsibility. This type deliberately
/// does not implement [`Clone`] or [`Debug`] because its states are equivalent to secret key
/// material.
///
/// # Examples
///
/// ```
/// use rsl_crypto::mac::hmac::sha256::HmacSha256;
///
/// let mut state = HmacSha256::new(b"shared secret")?;
/// state.update(b"one ")?;
/// state.update(b"message")?;
/// let tag = state.finalize();
///
/// let mut verifier = HmacSha256::new(b"shared secret")?;
/// verifier.update(b"one message")?;
/// verifier.verify(tag.as_bytes())?;
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct HmacSha256 {
    /// SHA-256 state after `K0 XOR ipad`, followed by message bytes accepted so far.
    inner: Sha256,
    /// SHA-256 state after the complete `K0 XOR opad` block.
    outer: Sha256,
}

impl HmacSha256 {
    /// Construct a new HMAC-SHA-256 state from arbitrary key bytes.
    ///
    /// FIPS 198-1 §4 accepts keys shorter than, equal to, or longer than SHA-256's 64-byte input
    /// block and normalizes each case explicitly. Applications remain responsible for choosing a
    /// key with appropriate entropy and strength. In particular, accepting a short or empty byte
    /// string at this primitive boundary is not a recommendation to generate such an HMAC key.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if a key is too long for SHA-256 to hash during
    /// FIPS 198-1 key normalization. See [`HmacSha256`] for a complete incremental example.
    pub fn new(key: &[u8]) -> Result<Self> {
        let normalized = NormalizedKey::from_key(key)?;
        let (inner_pad, outer_pad) = normalized.into_padded_blocks();

        let mut inner = Sha256::new();
        inner.update(inner_pad.expose_secret())?;

        let mut outer = Sha256::new();
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
    /// Returns [`CryptoError::MessageTooLong`] before changing the state if the inner SHA-256
    /// message-length bound would be exceeded. The bound includes HMAC's 64-byte inner pad. See
    /// [`HmacSha256`] for a complete example.
    pub fn update(&mut self, input: impl AsRef<[u8]>) -> Result<()> {
        self.inner.update(input)
    }

    /// Complete the inner and outer hashes and return the full 256-bit tag.
    ///
    /// # Panics
    ///
    /// Panics only if a private invariant is broken and the outer SHA-256 state, which contains
    /// exactly 64 bytes, rejects the fixed 32-byte inner digest as too long. Callers cannot create
    /// or modify that state directly.
    pub fn finalize(self) -> HmacSha256Tag {
        let Self { inner, mut outer } = self;
        let mut inner_digest = inner.finalize().into_bytes();

        // `outer` contains exactly one 64-byte prefix and this update is exactly 32 bytes, so it
        // cannot approach SHA-256's 2^64-bit message-length limit. Failure would indicate a broken
        // private state invariant rather than caller-controlled input.
        <Sha256 as Digest>::update(&mut outer, &inner_digest)
            .expect("a 64-byte SHA-256 prefix can always accept a 32-byte inner digest");
        inner_digest.zeroize();

        HmacSha256Tag(outer.finalize().into_bytes())
    }

    /// Authenticate one complete message with one key.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::mac::hmac::sha256::HmacSha256;
    ///
    /// let tag = HmacSha256::authenticate(b"shared secret", "UTF-8 message")?;
    /// assert_eq!(tag.as_bytes().len(), 32);
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] if key normalization or the message would exceed
    /// the underlying SHA-256 message-length bound.
    pub fn authenticate(key: &[u8], message: impl AsRef<[u8]>) -> Result<HmacSha256Tag> {
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
    /// use rsl_crypto::{CryptoError, mac::hmac::sha256::HmacSha256};
    ///
    /// let mut verifier = HmacSha256::new(b"right key")?;
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

impl Mac for HmacSha256 {
    type Tag = HmacSha256Tag;

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

    use super::{HmacSha256, HmacSha256Tag, TAG_LEN, tags_match};
    use crate::CryptoError;

    /// Published known-answer evidence from RFC 4231 §4.2, Test Case 1.
    #[test]
    fn rfc_4231_test_case_1() {
        let key = [0x0b; 20];
        let tag = HmacSha256::authenticate(&key, b"Hi There")
            .expect("the RFC 4231 fixture is within SHA-256 limits");

        assert_eq!(
            tag.into_bytes(),
            [
                0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
                0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
                0x2e, 0x32, 0xcf, 0xf7,
            ]
        );
    }

    /// Regression evidence that arbitrary message fragmentation preserves the authenticated data.
    #[test]
    fn one_byte_fragments_match_one_shot_authentication() {
        let key = b"readable key";
        let message = b"authenticated in deliberately tiny pieces";
        let expected = HmacSha256::authenticate(key, message).expect("the fixture is short");
        let mut state = HmacSha256::new(key).expect("the fixture key is short");

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
        let state = HmacSha256::new(b"key").expect("the fixture key is short");

        assert_eq!(
            state.verify([0; TAG_LEN]),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    /// Regression evidence for the tag newtype's exact byte access and visible debug form.
    #[test]
    fn tag_value_exposes_exact_public_bytes_and_names_itself() {
        let bytes = [0xab; HmacSha256Tag::LEN];
        let tag = HmacSha256Tag(bytes);

        assert_eq!(tag.as_bytes(), &bytes);
        assert_eq!(tag.as_ref(), bytes.as_slice());
        assert_eq!(
            format!("{tag:?}"),
            "HmacSha256Tag(abababababababababababababababababababababababababababababababab)"
        );
    }
}
