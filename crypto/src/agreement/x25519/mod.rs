//! X25519 key agreement, taught from field bytes to a shared secret.
//!
//! # What X25519 does
//!
//! X25519 is a scalar-multiplication function over the Montgomery form of Curve25519. Two peers
//! each combine private random bytes with the public base coordinate to produce a public key.
//! Applying the same function to a private key and the other peer's public key gives both peers
//! the same 32-byte secret.
//!
//! X25519 does **not** authenticate the peer, generate randomness, derive traffic keys, frame a
//! TLS key share, or sign an SSH exchange. A man-in-the-middle can replace unauthenticated public
//! keys. The result must enter a protocol-specified KDF and authenticated handshake.
//!
//! # Inputs, output, and checked behavior
//!
//! - [`X25519PrivateKey`] owns exactly 32 input bytes and clears them on drop.
//! - [`X25519PublicKey`] owns exactly 32 public coordinate bytes. Non-canonical coordinates and
//!   coordinates on the twist are accepted as RFC 7748 requires.
//! - [`X25519SharedSecret`] owns a nonzero 32-byte result and exposes it only explicitly.
//! - [`X25519::agree`] rejects the all-zero result so a small-order input cannot be represented as
//!   a validated shared secret.
//!
//! # RFC notation in Rust
//!
//! | RFC 7748 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `p = 2^255 - 19` | private [`field::FieldElement`] | Five little-endian radix-`2^51` limbs. |
//! | `k` | [`X25519PrivateKey`] then private [`scalar::PreparedScalar`] | The input bytes and their bit-prepared scalar. |
//! | `u`, `x_1` | [`X25519PublicKey`] then [`field::FieldElement`] | The peer's Montgomery u-coordinate. |
//! | `(x_2, z_2)`, `(x_3, z_3)` | local field elements in [`ladder::scalar_multiply`] | Projective coordinates carried by the ladder. |
//! | `k_t` | `scalar_bit` | Scalar bit `t`, read from 254 down through zero. |
//! | `swap XOR k_t` | `swap_control ^= scalar_bit` | Verified Errata 7625's unambiguous operation. |
//! | `a24` | private constant `121665` | `(486662 - 2) / 4`. |
//! | `x_2 * z_2^(p-2)` | multiplication by [`field::FieldElement::invert`] | Projective-to-affine recovery. |
//!
//! # Algorithm walkthrough
//!
//! 1. Prepare the scalar by clearing its low three bits, clearing bit 255, and setting bit 254.
//! 2. Decode the public coordinate little-endian, ignoring its top bit and accepting every field
//!    encoding as a residue modulo `p`.
//! 3. Initialize `(x_2, z_2) = (1, 0)` and `(x_3, z_3) = (u, 1)`.
//! 4. For every scalar bit from 254 down to zero, conditionally swap the coordinate pairs and
//!    perform the RFC's named `A`, `AA`, `B`, `BB`, `E`, `C`, `D`, `DA`, and `CB` field steps.
//! 5. Perform the final conditional swap, recover `x_2 / z_2` using `z_2^(p-2)`, and serialize
//!    the canonical little-endian coordinate.
//! 6. At the public agreement boundary, OR every result byte and reject an all-zero value.
//!
//! # Published worked exchange
//!
//! RFC 7748 §6.1 publishes Alice's private bytes and resulting public coordinate:
//!
//! ```
//! use rsl_crypto::agreement::x25519::{X25519, X25519PrivateKey};
//!
//! let alice_private = X25519PrivateKey::new([
//!     0x77, 0x07, 0x6d, 0x0a, 0x73, 0x18, 0xa5, 0x7d,
//!     0x3c, 0x16, 0xc1, 0x72, 0x51, 0xb2, 0x66, 0x45,
//!     0xdf, 0x4c, 0x2f, 0x87, 0xeb, 0xc0, 0x99, 0x2a,
//!     0xb1, 0x77, 0xfb, 0xa5, 0x1d, 0xb9, 0x2c, 0x2a,
//! ]);
//! let expected_public = [
//!     0x85, 0x20, 0xf0, 0x09, 0x89, 0x30, 0xa7, 0x54,
//!     0x74, 0x8b, 0x7d, 0xdc, 0xb4, 0x3e, 0xf7, 0x5a,
//!     0x0d, 0xbf, 0x3a, 0x0d, 0x26, 0x38, 0x1a, 0xf4,
//!     0xeb, 0xa4, 0xa9, 0x8e, 0xaa, 0x9b, 0x4e, 0x6a,
//! ];
//!
//! assert_eq!(X25519::public_key(&alice_private).into_bytes(), expected_public);
//! ```
//!
//! # Complete two-party exchange
//!
//! ```
//! use rsl_crypto::agreement::x25519::{X25519, X25519PrivateKey};
//!
//! // These fixed bytes make the example reproducible. Real ephemeral keys require an approved
//! // cryptographic random source owned by the protocol integration.
//! let alice_private = X25519PrivateKey::new([0x11; 32]);
//! let bob_private = X25519PrivateKey::new([0x22; 32]);
//! let alice_public = X25519::public_key(&alice_private);
//! let bob_public = X25519::public_key(&bob_private);
//!
//! let alice_shared = X25519::agree(&alice_private, &bob_public)?;
//! let bob_shared = X25519::agree(&bob_private, &alice_public)?;
//! assert_eq!(alice_shared.expose_secret(), bob_shared.expose_secret());
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Do not use a shared secret directly as an AES key; feed it into the handshake's specified
//!   KDF with the required public keys, transcript, and labels.
//! - Do not treat X25519 as peer authentication. Certificates, signatures, or an authenticated
//!   pre-shared-key mechanism establish identity.
//! - Do not reject a public key merely because its encoding is non-canonical or lies on the twist;
//!   RFC 7748 deliberately requires those values to be processed.
//! - Do not reuse fixed example keys. This crate does not silently select an operating-system RNG.
//! - Do not infer that fixed loop structure proves constant-time machine code. Compiler, target,
//!   cache, instruction-timing, and power analysis remain separate assurance work.
//! - X448, public-key identifiers, TLS key-share encoding, SSH exchange hashes, and protocol key
//!   lifetimes are intentionally outside this first profile.
//!
//! # Readable source map
//!
//! - [`field`] owns coordinate encoding, modular arithmetic, inversion, and masked swaps.
//! - [`scalar`] owns the RFC scalar-bit preparation rules.
//! - [`ladder`] owns the printed Montgomery ladder in its published order.
//! - [`api`] owns typed keys, all-zero rejection, secret exposure, and the generic agreement trait.
//!
//! # Evidence and security status
//!
//! The implementation is checked against RFC 7748 §5.2's two direct X25519 vectors, its one- and
//! 1,000-iteration vectors, and §6.1's complete Alice/Bob exchange. It also has field-boundary,
//! high-bit, non-canonical-coordinate, all-zero rejection, and development-only differential
//! tests. Passing those tests is not an audit or a side-channel certification.

// This teaching page intentionally links into the private executable-specification layers.
// `package.metadata.docs.rs` and the documented local build command both enable private items.
#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod field;
mod ladder;
mod scalar;

pub use api::{X25519, X25519PrivateKey, X25519PublicKey, X25519SharedSecret};

/// Current project lifecycle classification for X25519.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
