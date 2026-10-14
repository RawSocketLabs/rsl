//! ECDH over P-384, taught from a private scalar to a validated shared secret.
//!
//! # What ECDH P-384 does
//!
//! Each peer multiplies the P-384 generator by a private scalar `d` to publish `Q = [d]G`.
//! Applying the same multiplication to the peer's public point gives both sides the point
//! `[d_A d_B]G`; its `x`-coordinate is the 48-byte shared secret `Z`. SP 800-56A Rev. 3 §5.7.1.2
//! names this the ECC CDH primitive. TLS 1.3 calls the group `secp384r1`; SSH calls the method
//! `ecdh-sha2-nistp384`.
//!
//! ECDH does **not** authenticate the peer, generate randomness, derive traffic keys, or frame
//! a key share. `Z` must enter a protocol-specified KDF together with the transcript.
//!
//! # Inputs, output, and checked behavior
//!
//! - [`EcdhP384PrivateKey`] owns one scalar in `[1, n-1]`; construction rejects zero and
//!   values `>= n`. Generation follows SP 800-56A §5.6.1.2.2 candidate testing.
//! - [`EcdhP384PublicKey`] owns a 97-byte uncompressed SEC 1 point. Construction performs full
//!   public-key validation: prefix `0x04`, both coordinates below `p`, and the curve equation.
//! - [`EcdhP384SharedSecret`] owns the 48-byte big-endian `x`-coordinate of `[d]Q` and exposes
//!   it only explicitly.
//! - [`EcdhP384::agree`] returns [`CryptoError::InvalidPublicKey`](crate::CryptoError::InvalidPublicKey) if the product is the point
//!   at infinity, which a valid input pair cannot produce.
//!
//! # Standards notation in Rust
//!
//! | SP 800-56A / SEC 1 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `d` | [`EcdhP384PrivateKey`] | The private scalar. |
//! | `Q = [d]G` | [`EcdhP384PublicKey`] | The public point, `04 || x || y`. |
//! | `P = [h d]Q_peer`, `h = 1` | `ProjectivePoint::multiply` | The ECC CDH primitive. |
//! | `Z = x_P` | [`EcdhP384SharedSecret`] | Field element to 48-byte octet string. |
//!
//! # Published worked exchange
//!
//! RFC 5903 §8.2 publishes an initiator private key and its public point:
//!
//! ```
//! use rsl_crypto::agreement::ecdh_p384::{EcdhP384, EcdhP384PrivateKey};
//!
//! let initiator = EcdhP384PrivateKey::from_bytes([
//!     0x09, 0x9f, 0x3c, 0x70, 0x34, 0xd4, 0xa2, 0xc6, 0x99, 0x88, 0x4d, 0x73, 0xa3, 0x75, 0xa6,
//!     0x7f, 0x76, 0x24, 0xef, 0x7c, 0x6b, 0x3c, 0x0f, 0x16, 0x06, 0x47, 0xb6, 0x74, 0x14, 0xdc,
//!     0xe6, 0x55, 0xe3, 0x5b, 0x53, 0x80, 0x41, 0xe6, 0x49, 0xee, 0x3f, 0xae, 0xf8, 0x96, 0x78,
//!     0x3a, 0xb1, 0x94,
//! ])?;
//! let public = EcdhP384::public_key(&initiator);
//! assert_eq!(public.as_bytes()[0], 0x04);
//! assert_eq!(
//!     &public.as_bytes()[1..9],
//!     &[0x66, 0x78, 0x42, 0xd7, 0xd1, 0x80, 0xac, 0x2c],
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Complete two-party exchange
//!
//! ```
//! use rsl_crypto::agreement::ecdh_p384::{EcdhP384, EcdhP384PrivateKey};
//!
//! // Fixed bytes keep the example reproducible; real keys need an approved random source.
//! let alice = EcdhP384PrivateKey::from_bytes([0x11; 48])?;
//! let bob = EcdhP384PrivateKey::from_bytes([0x22; 48])?;
//! let alice_shared = EcdhP384::agree(&alice, &EcdhP384::public_key(&bob))?;
//! let bob_shared = EcdhP384::agree(&bob, &EcdhP384::public_key(&alice))?;
//! assert_eq!(alice_shared.expose_secret(), bob_shared.expose_secret());
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Do not skip public-key validation. An off-curve point turns the fixed-structure ladder
//!   into an oracle for private-key bits (the invalid-curve attack).
//! - Do not use `Z` directly as a key. Feed it into HKDF or the protocol's key schedule.
//! - Do not reuse fixed example keys, and do not assume this crate selects an RNG.
//! - Compressed points, P-384, P-521, and cofactor curves are outside this profile.
//!
//! # Readable source map
//!
//! - [`crate::curve::p384`] owns the field, scalar, and point arithmetic.
//! - [`api`] owns typed keys, validation, candidate-testing generation, and the generic
//!   [`KeyAgreement`](crate::agreement::KeyAgreement) contract.
//!
//! # Evidence and security status
//!
//! Public tests cover RFC 5903 §8.2's complete exchange, all 25 NIST CAVP ECC CDH P-384
//! primitive cases, the CAVP PKV P-384 accept/reject cases, exact wire length, range rejection,
//! and development-only differential comparison with the `p256` crate 0.14.0. Passing those is not
//! an audit or a side-channel certification.

#![allow(rustdoc::private_intra_doc_links)]

mod api;

pub use api::{EcdhP384, EcdhP384PrivateKey, EcdhP384PublicKey, EcdhP384SharedSecret};

/// Current project lifecycle classification for ECDH over P-384.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
