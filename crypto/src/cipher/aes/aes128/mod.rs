//! AES-128, the readable 128-bit block permutation.
//!
//! # What AES-128 does
//!
//! AES-128 transforms exactly one 16-byte block under one 16-byte secret key. The inverse
//! transformation recovers that block. [NIST FIPS 197][fips-197] specifies the byte state,
//! finite-field arithmetic, key expansion, and ten encryption rounds.
//!
//! Raw AES is a **permutation**, not a safe arbitrary-length encryption scheme. It supplies no
//! nonce, message framing, padding, chaining, or authentication. Most users want
//! [`Aes128Gcm`](crate::aead::gcm::Aes128Gcm), which composes the forward AES permutation into an
//! authenticated construction.
//!
//! # Standard notation in Rust
//!
//! | FIPS 197 concept | Rust representation |
//! | --- | --- |
//! | 128-bit AES key | [`Aes128Key`] |
//! | 128-bit input/output block | [`Aes128Block`] |
//! | state byte `s[r,c]` | private `State.rows[r][c]` |
//! | input byte position | `input[r + 4 * c]` |
//! | byte field `GF(2^8)` | calculated `u8` operations |
//! | 44 expanded AES-128 words | private zeroizing `KeySchedule` |
//!
//! The state is four rows by four columns. Sequential block bytes fill columns: bytes 0–3 form
//! column 0, bytes 4–7 form column 1, and so on. Making this mapping explicit prevents the common
//! mistake of reading the standard's state diagrams as row-major byte arrays.
//!
//! # Key expansion walkthrough
//!
//! The 16-byte key supplies words `w[0]..w[3]`. Each later word XORs the word four positions
//! earlier with a temporary derived from the previous word. At every fourth word, that temporary
//! is rotated by one byte, substituted byte by byte, and combined with the next round constant
//! using XOR.
//! Forty-four words form eleven round keys: one initial key and ten round keys.
//!
//! # Encryption walkthrough
//!
//! 1. Map the input bytes into the state and apply the initial `ADDROUNDKEY()`.
//! 2. Perform nine full rounds of `SUBBYTES()`, `SHIFTROWS()`, `MIXCOLUMNS()`, and
//!    `ADDROUNDKEY()`.
//! 3. Perform the tenth round without `MIXCOLUMNS()`.
//! 4. Map state columns back into sequential block bytes.
//!
//! `SUBBYTES()` calculates inversion in `GF(2^8)` followed by the published affine transform;
//! the readable path does not index a secret-dependent production S-box table. `MIXCOLUMNS()`
//! multiplies each four-byte column by the fixed polynomial matrix. Decryption applies the inverse
//! operations and round keys in reverse order.
//!
//! # Published one-block example
//!
//! FIPS 197 Appendix B publishes this complete AES-128 transformation:
//!
//! ```
//! use rsl_crypto::cipher::aes::aes128::{Aes128, Aes128Block, Aes128Key};
//!
//! let cipher = Aes128::new(Aes128Key::new([
//!     0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6,
//!     0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f, 0x3c,
//! ]));
//! let plaintext = [
//!     0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d,
//!     0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37, 0x07, 0x34,
//! ];
//! let mut block = Aes128Block::new(plaintext);
//!
//! cipher.encrypt_block(&mut block);
//! assert_eq!(
//!     block.as_bytes(),
//!     &[
//!         0x39, 0x25, 0x84, 0x1d, 0x02, 0xdc, 0x09, 0xfb,
//!         0xdc, 0x11, 0x85, 0x97, 0x19, 0x6a, 0x0b, 0x32,
//!     ]
//! );
//! cipher.decrypt_block(&mut block);
//! assert_eq!(block.into_bytes(), plaintext);
//! ```
//!
//! # Common mistakes
//!
//! - Never encrypt a multi-block message by applying AES independently to each block. Equal
//!   plaintext blocks would produce equal ciphertext blocks, and no integrity would be present.
//! - Do not invent a mode, padding rule, or nonce construction. Use the construction mandated by
//!   the protocol, normally an AEAD.
//! - AES decryption does not authenticate ciphertext. Unauthenticated plaintext must not become
//!   caller-visible.
//! - The key and block are the same size but different semantic types; this API makes accidental
//!   interchange a compile-time error.
//!
//! # Readable source map
//!
//! `state.rs` owns byte/state mapping; `field.rs` owns `GF(2^8)` arithmetic; `substitution.rs` owns
//! the calculated S-boxes; `transforms.rs` owns individual round operations; `key_schedule.rs`
//! owns expansion; and `forward.rs`/`inverse.rs` compose complete directions. `api.rs` is only the
//! typed public boundary.
//!
//! # Evidence and security status
//!
//! Evidence includes every published S-box entry, every Appendix A.1 expanded word, Appendix B
//! intermediate states, supplementary NIST encryption/decryption blocks, exhaustive inverse
//! properties, round trips, and development-only differential comparison. Calculation-only
//! substitution and fixed source loops are not a compiler-level constant-time guarantee. The
//! crate-level [security status](crate#security-status) applies.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

mod api;
mod field;
mod forward;
mod inverse;
mod key;
mod key_schedule;
mod state;
mod substitution;
mod transforms;

pub use api::{Aes128, Aes128Block, Aes128Key};

/// Current project lifecycle classification for the AES-128 block primitive.
///
/// This does not make raw block encryption a complete protected-message construction.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
