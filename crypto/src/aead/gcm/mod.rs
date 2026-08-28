//! AES-128-GCM authenticated encryption, taught layer by layer.
//!
//! # What GCM protects
//!
//! Galois/Counter Mode (GCM) is an authenticated-encryption-with-associated-data construction
//! specified by [NIST SP 800-38D][sp-800-38d]. This profile combines AES-128 with:
//!
//! - a dedicated 16-byte secret [`Aes128GcmKey`];
//! - a 12-byte [`Aes128GcmNonce`] that must be unique for every encryption under that key; and
//! - a complete 16-byte [`Aes128GcmTag`].
//!
//! Plaintext becomes same-length ciphertext. Additional authenticated data (AAD) stays cleartext,
//! but any change to it invalidates the tag. This lets a protocol transmit selected fields such as
//! record headers unencrypted while still binding their exact encoded bytes to the protected
//! payload.
//!
//! # Outputs and security properties
//!
//! [`Aes128Gcm::seal`] returns [`crate::aead::Sealed`], containing detached ciphertext and tag.
//! [`Aes128Gcm::open`] authenticates the nonce, AAD, ciphertext, and tag before allocating or
//! transforming plaintext. It returns only plaintext or a uniform
//! [`crate::CryptoError::AuthenticationFailed`] error.
//!
//! GCM does not by itself prevent replay. It also cannot know whether a nonce was used earlier.
//! TLS, SSH, or another protocol context must own sequence numbers, nonce construction, key
//! lifetime, exhaustion, replay handling, wire framing, and encryption activation state.
//!
//! # Standard notation in Rust
//!
//! | SP 800-38D notation | Meaning | Implementation boundary |
//! | --- | --- | --- |
//! | `K` | AES-128 key | [`Aes128GcmKey`] |
//! | `IV` | 96-bit initialization vector/nonce | [`Aes128GcmNonce`] |
//! | `H = CIPH_K(0^128)` | secret GHASH subkey | private `HashSubkey` |
//! | `J0` | pre-counter block | private `PreCounterBlock` |
//! | `inc32` | increment rightmost 32 bits | private `CounterBlock` |
//! | `GCTR` | counter-mode byte transform | private `gctr` layer |
//! | `A` | cleartext associated data | `&[u8]` |
//! | `P` / `C` | plaintext / ciphertext | borrowed input / owned output |
//! | `S` | GHASH authentication result | private `GhashResult` |
//! | `T` | complete authentication tag | [`Aes128GcmTag`] |
//!
//! GHASH uses multiplication in `GF(2^128)`. SP 800-38D's displayed-bit convention makes the
//! block beginning with `0x80` the field identity; the private implementation calls out that
//! counterintuitive mapping rather than hiding it behind native-endian integers.
//!
//! # Authenticated encryption walkthrough
//!
//! [`Aes128Gcm::seal`] follows Algorithm 4:
//!
//! 1. Derive the secret hash subkey `H = AES_K(0^128)`.
//! 2. For the supported 96-bit nonce, construct `J0 = IV || 0^31 || 1`.
//! 3. Encrypt plaintext with `GCTR_K(inc32(J0), P)` to produce ciphertext `C`.
//! 4. Zero-pad AAD and ciphertext independently to 128-bit boundaries.
//! 5. Append their original 64-bit bit lengths and apply GHASH to produce `S`.
//! 6. Mask `S` with `GCTR_K(J0, S)` to produce the full tag `T`.
//!
//! The tag authenticates the nonce indirectly through `J0`, plus the exact AAD, ciphertext,
//! ordering, padding boundaries, and original lengths.
//!
//! # Authenticated decryption walkthrough
//!
//! Algorithm 5 prints plaintext computation before tag comparison, but the standard explicitly
//! permits verification to happen first. This implementation uses that safer equivalent order:
//!
//! 1. Validate supported input lengths.
//! 2. Derive `H` and `J0`.
//! 3. Calculate `S` over the received AAD and ciphertext.
//! 4. Calculate and compare the candidate tag across all sixteen bytes.
//! 5. Only after success, allocate plaintext and apply GCTR to the ciphertext.
//!
//! Consequently, an authentication-failure path never creates a plaintext owner.
//!
//! # Published empty-input example
//!
//! NIST's `AES_GCM.pdf` Example 1 publishes this key, nonce, empty ciphertext, and tag:
//!
//! ```
//! use rsl_crypto::aead::gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce, Aes128GcmTag};
//!
//! let algorithm = Aes128Gcm::new(Aes128GcmKey::new([
//!     0xfe, 0xff, 0xe9, 0x92, 0x86, 0x65, 0x73, 0x1c,
//!     0x6d, 0x6a, 0x8f, 0x94, 0x67, 0x30, 0x83, 0x08,
//! ]));
//! let nonce = Aes128GcmNonce::new([
//!     0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce,
//!     0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
//! ]);
//! let sealed = algorithm.seal(&nonce, &[], &[])?;
//! assert!(sealed.ciphertext().is_empty());
//! assert_eq!(
//!     sealed.tag(),
//!     &Aes128GcmTag::new([
//!         0x32, 0x47, 0x18, 0x4b, 0x3c, 0x4f, 0x69, 0xa4,
//!         0x4d, 0xbc, 0xd2, 0x28, 0x87, 0xbb, 0xb4, 0x18,
//!     ]),
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Cleartext AAD on the wire
//!
//! The header below is not transformed. A sender may write `header || ciphertext || tag`; the
//! receiver supplies the parsed header bytes as AAD when opening.
//!
//! ```
//! use rsl_crypto::{CryptoError, aead::gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce}};
//!
//! let algorithm = Aes128Gcm::new(Aes128GcmKey::new([0x42; 16]));
//! // Fixed only for this isolated example. A protocol must never reuse a key/nonce pair.
//! let nonce = Aes128GcmNonce::new([0x24; 12]);
//! let header = *b"visible header";
//! let sealed = algorithm.seal(&nonce, &header, b"protected body")?;
//! assert_eq!(header, *b"visible header");
//!
//! let plaintext = algorithm.open(&nonce, &header, sealed.ciphertext(), sealed.tag())?;
//! assert_eq!(plaintext, b"protected body");
//!
//! let mut changed_header = header;
//! changed_header[0] ^= 1;
//! assert_eq!(
//!     algorithm.open(&nonce, &changed_header, sealed.ciphertext(), sealed.tag()),
//!     Err(CryptoError::AuthenticationFailed),
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Parsing detached wire values
//!
//! Exact-size conversions keep a truncated or extended nonce/tag from reaching cryptographic
//! work:
//!
//! ```
//! use rsl_crypto::{CryptoError, aead::gcm::{Aes128GcmNonce, Aes128GcmTag}};
//!
//! let nonce_bytes = [0x11; 12];
//! let tag_bytes = [0x22; 16];
//! let nonce = Aes128GcmNonce::try_from(nonce_bytes.as_slice())?;
//! let tag = Aes128GcmTag::try_from(tag_bytes.as_slice())?;
//! assert_eq!(nonce.as_bytes(), &nonce_bytes);
//! assert_eq!(tag.as_bytes(), &tag_bytes);
//! assert!(matches!(
//!     Aes128GcmTag::try_from(&tag_bytes[..15]),
//!     Err(CryptoError::InvalidLength { .. }),
//! ));
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and unsupported profiles
//!
//! - **Never reuse a nonce for encryption under the same key.** A single reuse can destroy both
//!   confidentiality and authentication. The value type enforces size, not history.
//! - Do not release plaintext after authentication failure. This API returns owned plaintext only
//!   after verification.
//! - AAD must be reproduced byte-for-byte by the receiver. Semantic equivalence is insufficient
//!   if encodings differ.
//! - Do not omit cleartext fields that influence how plaintext is interpreted from the AAD unless
//!   the controlling protocol explicitly leaves them unauthenticated.
//! - This first profile intentionally supports only 96-bit nonces and 128-bit tags. It does not
//!   expose SP 800-38D's variable-IV branch or tag truncation.
//! - GCM does not prevent replay; record or packet sequence policy belongs above it.
//!
//! # Readable source map
//!
//! `counter.rs` teaches `inc32`; `ghash/field.rs` teaches field multiplication;
//! `ghash/state.rs` teaches the GHASH recurrence; `gctr.rs` teaches counter-mode transformation;
//! `setup.rs` derives `H` and `J0`; `authentication.rs` constructs padded authentication input;
//! `limits.rs` owns supported lengths; `tag.rs` masks and compares full tags; and `seal.rs` /
//! `open.rs` compose Algorithms 4 and 5. `api.rs` supplies only the typed public boundary.
//!
//! GHASH remains private because SP 800-38D §5.3 approves it only inside GCM and requires its
//! subkey and intermediate values to remain secret.
//!
//! # Evidence and security status
//!
//! Layer tests connect all five published NIST full-tag examples through `H`, `J0`, counters,
//! ciphertext, `S`, and `T`. Public tests cover end-to-end examples, AAD behavior, exact wire
//! parsing, every tag-byte and ciphertext-byte change, and 32 development-only differential
//! cases. The source is structured for review but has not received independent audit, formal NIST
//! validation, or complete side-channel analysis. The crate-level
//! [security status](crate#security-status) applies.
//!
//! NIST is preparing Revision 1, but as of 2026-08-27 it has issued preliminary calls for comment
//! and no replacement specification. The repository traceability ledger records that revision
//! state so a future document is not silently treated as equivalent.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

mod api;
mod authentication;
mod counter;
mod gctr;
mod ghash;
mod limits;
mod open;
mod seal;
mod setup;
mod tag;

pub use api::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce, Aes128GcmTag};

/// Current project lifecycle classification for the supported full-tag AES-128-GCM profile.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
