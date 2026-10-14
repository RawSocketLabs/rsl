//! X448 key agreement: the RFC 7748 ladder at the ~224-bit security level.
//!
//! # What X448 does
//!
//! X448 is X25519's sibling over curve448 (`p = 2^448 - 2^224 - 1`, `A = 156326`, base
//! u-coordinate 5). Each peer combines 56 private random bytes with the base coordinate to make
//! a public key; applying the same function to a private key and the peer's public key gives
//! both peers the same 56-byte secret. TLS 1.3 names the group `x448`; SSH uses
//! `curve448-sha512`.
//!
//! X448 does **not** authenticate the peer, generate randomness, derive traffic keys, or frame a
//! key share. The result must enter a protocol-specified KDF and authenticated handshake.
//!
//! # What differs from X25519
//!
//! | Quantity | X25519 | X448 |
//! | --- | --- | --- |
//! | field prime | `2^255 - 19` | `2^448 - 2^224 - 1` (eight 56-bit limbs, fold `2^448 ≡ 2^224 + 1`) |
//! | encoding | 32 bytes, top bit masked | 56 bytes, no unused bit |
//! | scalar preparation | clear bits 0–2 and 255, set 254 | clear bits 0–1, set 447 |
//! | `a24` | 121665 | 39081 |
//! | ladder bits | 255 → 0 | 447 → 0 |
//!
//! The ladder body, the `cswap` construction, the inversion by `p - 2`, and the all-zero
//! rejection are the same. See [`x25519`](crate::agreement::x25519) for the step walkthrough.
//!
//! # Published worked exchange
//!
//! RFC 7748 §6.2 publishes Alice's private bytes and resulting public coordinate:
//!
//! ```
//! use rsl_crypto::agreement::x448::{X448, X448PrivateKey};
//!
//! let alice_private = X448PrivateKey::new([
//!     0x9a, 0x8f, 0x49, 0x25, 0xd1, 0x51, 0x9f, 0x57, 0x75, 0xcf, 0x46, 0xb0, 0x4b, 0x58,
//!     0x00, 0xd4, 0xee, 0x9e, 0xe8, 0xba, 0xe8, 0xbc, 0x55, 0x65, 0xd4, 0x98, 0xc2, 0x8d,
//!     0xd9, 0xc9, 0xba, 0xf5, 0x74, 0xa9, 0x41, 0x97, 0x44, 0x89, 0x73, 0x91, 0x00, 0x63,
//!     0x82, 0xa6, 0xf1, 0x27, 0xab, 0x1d, 0x9a, 0xc2, 0xd8, 0xc0, 0xa5, 0x98, 0x72, 0x6b,
//! ]);
//! let public = X448::public_key(&alice_private);
//! assert_eq!(&public.as_bytes()[..4], &[0x9b, 0x08, 0xf7, 0xcc]);
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Do not truncate or mask the 56-byte encoding; X448 has no spare bit.
//! - Do not use the shared secret directly as a key; feed it to the handshake's KDF.
//! - Do not reject twist or non-canonical inputs; RFC 7748 requires processing them. The
//!   all-zero result is the only rejection.
//!
//! # Readable source map
//!
//! - [`field`] owns `GF(2^448 - 2^224 - 1)` encoding, arithmetic, inversion, and masked swaps.
//! - [`scalar`] owns `decodeScalar448`.
//! - [`ladder`] owns the printed ladder with `a24 = 39081` over 448 bits.
//! - [`api`] owns typed keys, all-zero rejection, and the generic agreement trait.
//!
//! # Evidence and security status
//!
//! RFC 7748 §5.2's two direct X448 vectors, its one- and 1,000-iteration checkpoints, §6.2's
//! complete exchange, non-canonical and boundary inputs, and all 510 Wycheproof `x448` cases.
//! The one-million-iteration checkpoint is an ignored test. No side-channel or audit claim is
//! made.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod field;
mod ladder;
mod scalar;

pub use api::{X448, X448PrivateKey, X448PublicKey, X448SharedSecret};

/// Current project lifecycle classification for X448.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
