//! ECDH over P-256, taught from a private scalar to a validated shared secret.
//!
//! # What ECDH P-256 does
//!
//! Each peer multiplies the P-256 generator by a private scalar `d` to publish `Q = [d]G`.
//! Applying the same multiplication to the peer's public point gives both sides the point
//! `[d_A d_B]G`; its `x`-coordinate is the 32-byte shared secret `Z`. SP 800-56A Rev. 3 §5.7.1.2
//! names this the ECC CDH primitive. TLS 1.3 calls the group `secp256r1`; SSH calls the method
//! `ecdh-sha2-nistp256`.
//!
//! ECDH does **not** authenticate the peer, generate randomness, derive traffic keys, or frame
//! a key share. `Z` must enter a protocol-specified KDF together with the transcript.
//!
//! # Inputs, output, and checked behavior
//!
//! - [`EcdhP256PrivateKey`] owns one scalar in `[1, n-1]`; construction rejects zero and
//!   values `>= n`. Generation follows SP 800-56A §5.6.1.2.2 candidate testing.
//! - [`EcdhP256PublicKey`] owns a 65-byte uncompressed SEC 1 point. Construction performs full
//!   public-key validation: prefix `0x04`, both coordinates below `p`, and the curve equation.
//! - [`EcdhP256SharedSecret`] owns the 32-byte big-endian `x`-coordinate of `[d]Q` and exposes
//!   it only explicitly.
//! - [`EcdhP256::agree`] returns [`CryptoError::InvalidPublicKey`](crate::CryptoError::InvalidPublicKey) if the product is the point
//!   at infinity, which a valid input pair cannot produce.
//!
//! # Standards notation in Rust
//!
//! | SP 800-56A / SEC 1 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `d` | [`EcdhP256PrivateKey`] | The private scalar. |
//! | `Q = [d]G` | [`EcdhP256PublicKey`] | The public point, `04 || x || y`. |
//! | `P = [h d]Q_peer`, `h = 1` | `ProjectivePoint::multiply` | The ECC CDH primitive. |
//! | `Z = x_P` | [`EcdhP256SharedSecret`] | Field element to 32-byte octet string. |
//!
//! # Published worked exchange
//!
//! RFC 5903 §8.1 publishes an initiator private key and its public point:
//!
//! ```
//! use rsl_crypto::agreement::ecdh_p256::{EcdhP256, EcdhP256PrivateKey};
//!
//! let initiator = EcdhP256PrivateKey::from_bytes([
//!     0xc8, 0x8f, 0x01, 0xf5, 0x10, 0xd9, 0xac, 0x3f, 0x70, 0xa2, 0x92, 0xda, 0xa2, 0x31, 0x6d,
//!     0xe5, 0x44, 0xe9, 0xaa, 0xb8, 0xaf, 0xe8, 0x40, 0x49, 0xc6, 0x2a, 0x9c, 0x57, 0x86, 0x2d,
//!     0x14, 0x33,
//! ])?;
//! let public = EcdhP256::public_key(&initiator);
//! assert_eq!(public.as_bytes()[0], 0x04);
//! assert_eq!(
//!     &public.as_bytes()[1..9],
//!     &[0xda, 0xd0, 0xb6, 0x53, 0x94, 0x22, 0x1c, 0xf9],
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Complete two-party exchange
//!
//! ```
//! use rsl_crypto::agreement::ecdh_p256::{EcdhP256, EcdhP256PrivateKey};
//!
//! // Fixed bytes keep the example reproducible; real keys need an approved random source.
//! let alice = EcdhP256PrivateKey::from_bytes([0x11; 32])?;
//! let bob = EcdhP256PrivateKey::from_bytes([0x22; 32])?;
//! let alice_shared = EcdhP256::agree(&alice, &EcdhP256::public_key(&bob))?;
//! let bob_shared = EcdhP256::agree(&bob, &EcdhP256::public_key(&alice))?;
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
//! - [`crate::curve::p256`] owns the field, scalar, and point arithmetic.
//! - [`api`] owns typed keys, validation, candidate-testing generation, and the generic
//!   [`KeyAgreement`](crate::agreement::KeyAgreement) contract.
//!
//! # Evidence and security status
//!
//! Public tests cover RFC 5903 §8.1's complete exchange, all 25 NIST CAVP ECC CDH P-256
//! primitive cases, the CAVP PKV P-256 accept/reject cases, exact wire length, range rejection,
//! and development-only differential comparison with the `p256` crate 0.14.0. Passing those is not
//! an audit or a side-channel certification.

#![allow(rustdoc::private_intra_doc_links)]

mod api;

pub use api::{EcdhP256, EcdhP256PrivateKey, EcdhP256PublicKey, EcdhP256SharedSecret};

/// Current project lifecycle classification for ECDH over P-256.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
