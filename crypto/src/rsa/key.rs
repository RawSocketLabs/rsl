//! Imported RSA component ownership and the RFC 8017 primitive boundary.
//!
//! ## Standards ownership
//!
//! RFC 8017 §3 defines public keys `(n, e)` and private keys `(n, d)`; §4 defines the
//! `I2OSP`/`OS2IP` conversions; §5.1 and §5.2 define the primitives RSAEP, RSADP, RSASP1, and
//! RSAVP1, which are all the same modular exponentiation with different roles. This module owns
//! structural component checks and that exponentiation. Encoding methods (PSS, PKCS #1 v1.5,
//! OAEP) live in their own modules, and key generation, CRT, ASN.1, and certificates are out of
//! scope.

use alloc::vec::Vec;
use core::{cmp::Ordering, fmt};

use crate::{CryptoError, Result};

use super::integer::{BigUint, modpow};

/// An RSA public key imported as its unsigned modulus `n` and public exponent `e`.
///
/// Component bytes use RFC 8017 §4's unsigned, big-endian convention. Leading zero bytes are
/// accepted and normalized. Import verifies only structural requirements needed by this integer
/// engine: `n` is an odd integer greater than one, and `e` is an odd integer in `1 < e < n`.
/// It does not validate primality, provenance, strength, or protocol policy.
pub struct RsaPublicKey {
    modulus: BigUint,
    exponent: BigUint,
}

impl RsaPublicKey {
    /// Import unsigned big-endian RSA public components.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] when the components do not satisfy the documented
    /// structural bounds.
    pub fn from_components(
        modulus: impl AsRef<[u8]>,
        public_exponent: impl AsRef<[u8]>,
    ) -> Result<Self> {
        let modulus = BigUint::from_be_bytes(modulus.as_ref());
        let exponent = BigUint::from_be_bytes(public_exponent.as_ref());

        if !valid_modulus(&modulus)
            || exponent.is_zero()
            || exponent.is_one()
            || !exponent.is_odd()
            || exponent.compare(&modulus) != Ordering::Less
        {
            return Err(CryptoError::InvalidPublicKey);
        }

        Ok(Self { modulus, exponent })
    }

    /// Return the significant modulus size in bits.
    #[must_use]
    pub fn modulus_bits(&self) -> usize {
        self.modulus.bit_len()
    }

    /// Return RFC 8017's octet length `k = ceil(modBits / 8)`.
    #[must_use]
    pub fn modulus_len(&self) -> usize {
        self.modulus.byte_len()
    }

    /// RFC 8017 §5.1.1 RSAEP / §5.2.2 RSAVP1: `m = s^e mod n` over a `k`-byte representative.
    ///
    /// The result is the `k`-byte big-endian encoding of the transformed integer. Public-key
    /// operations act on public data, so the variable-time integer engine is acceptable here.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] when the input is not exactly `k` bytes and
    /// [`CryptoError::InvalidKey`] when it does not encode an integer below `n`.
    pub fn apply(&self, representative: &[u8]) -> Result<Vec<u8>> {
        apply_primitive(&self.modulus, &self.exponent, representative)
    }
}

impl fmt::Debug for RsaPublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RsaPublicKey")
            .field("modulus_bits", &self.modulus_bits())
            .field("components", &"[OMITTED]")
            .finish()
    }
}

/// An RSA private key imported as its unsigned modulus `n` and private exponent `d`.
///
/// This intentionally minimal owner stores no prime factors and performs the unoptimized RFC 8017
/// RSADP/RSASP1 exponentiation directly. The exponent is zeroized on drop by the internal integer
/// owner, but this implementation is variable-time and unblinded. See the [`rsa`](super) module's
/// side-channel warning; this crate exposes no private-key RSA scheme, and `rsl-crypto-legacy`
/// consumes this owner only for explicitly opt-in historical PKCS #1 v1.5 operations.
pub struct RsaPrivateKey {
    modulus: BigUint,
    private_exponent: BigUint,
}

impl RsaPrivateKey {
    /// Import unsigned big-endian RSA private components.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidKey`] unless `n` is odd and greater than one and
    /// `1 < d < n`. This cannot prove that `d` belongs to a mathematically valid RSA key pair;
    /// published-vector and differential tests provide evidence for known pairs.
    pub fn from_components(
        modulus: impl AsRef<[u8]>,
        private_exponent: impl AsRef<[u8]>,
    ) -> Result<Self> {
        let modulus = BigUint::from_be_bytes(modulus.as_ref());
        let private_exponent = BigUint::from_be_bytes(private_exponent.as_ref());

        if !valid_modulus(&modulus)
            || private_exponent.is_zero()
            || private_exponent.is_one()
            || private_exponent.compare(&modulus) != Ordering::Less
        {
            return Err(CryptoError::InvalidKey);
        }

        Ok(Self {
            modulus,
            private_exponent,
        })
    }

    /// Return the significant modulus size in bits.
    #[must_use]
    pub fn modulus_bits(&self) -> usize {
        self.modulus.bit_len()
    }

    /// Return RFC 8017's octet length `k = ceil(modBits / 8)`.
    #[must_use]
    pub fn modulus_len(&self) -> usize {
        self.modulus.byte_len()
    }

    /// RFC 8017 §5.1.2 RSADP / §5.2.1 RSASP1: `m = c^d mod n` over a `k`-byte representative.
    ///
    /// This is the variable-time, unblinded teaching primitive classified
    /// [`EducationalOnly`](crate::security::SecurityStatus::EducationalOnly) by
    /// [`RSA_PRIMITIVE_SECURITY_STATUS`](super::RSA_PRIMITIVE_SECURITY_STATUS).
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] when the input is not exactly `k` bytes and
    /// [`CryptoError::InvalidKey`] when it does not encode an integer below `n`.
    pub fn apply(&self, representative: &[u8]) -> Result<Vec<u8>> {
        apply_primitive(&self.modulus, &self.private_exponent, representative)
    }
}

impl fmt::Debug for RsaPrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RsaPrivateKey")
            .field("modulus_bits", &self.modulus_bits())
            .field("private_components", &"[REDACTED]")
            .finish()
    }
}

fn valid_modulus(modulus: &BigUint) -> bool {
    !modulus.is_zero() && !modulus.is_one() && modulus.is_odd()
}

/// Apply RFC 8017 §5's RSA integer primitive with §4 OS2IP/I2OSP conversions.
fn apply_primitive(modulus: &BigUint, exponent: &BigUint, encoded: &[u8]) -> Result<Vec<u8>> {
    let modulus_len = modulus.byte_len();
    if encoded.len() != modulus_len {
        return Err(CryptoError::InvalidLength {
            name: "RSA representative",
            expected: modulus_len,
            actual: encoded.len(),
        });
    }

    let representative = BigUint::from_be_bytes(encoded);
    if representative.compare(modulus) != Ordering::Less {
        return Err(CryptoError::InvalidKey);
    }

    let transformed = modpow(&representative, exponent, modulus)?;
    transformed
        .to_be_bytes_padded(modulus_len)
        .ok_or(CryptoError::InvalidKey)
}
