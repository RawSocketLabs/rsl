//! AES-256: the fourteen-round AES profile used by `TLS_AES_256_GCM_SHA384`.
//!
//! # What changes from AES-128
//!
//! FIPS 197 defines one cipher with three key sizes. The state, the four transformations, the
//! S-box, and the block size are identical; only the key expansion (`Nk = 8`, with an extra
//! `SUBWORD()` step when `i mod 8 = 4`) and the round count (`Nr = 14`, Table 3) differ. This
//! module therefore reuses every private AES-128 layer and adds only [`key_schedule`]; the shared
//! `CIPHER()` and `INVCIPHER()` bodies are generic over the round-key source.
//!
//! # Published example
//!
//! NIST's `AES_Core256.pdf` encrypts the first ECB block under the Appendix A.3 key:
//!
//! ```
//! use rsl_crypto::cipher::aes::aes256::{Aes256, Aes256Block, Aes256Key};
//!
//! let key = Aes256Key::new([
//!     0x60, 0x3d, 0xeb, 0x10, 0x15, 0xca, 0x71, 0xbe, 0x2b, 0x73, 0xae, 0xf0, 0x85, 0x7d, 0x77, 0x81,
//!     0x1f, 0x35, 0x2c, 0x07, 0x3b, 0x61, 0x08, 0xd7, 0x2d, 0x98, 0x10, 0xa3, 0x09, 0x14, 0xdf, 0xf4,
//! ]);
//! let mut block = Aes256Block::new([
//!     0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93, 0x17, 0x2a,
//! ]);
//! let cipher = Aes256::new(key);
//! cipher.encrypt_block(&mut block);
//! assert_eq!(&block.as_bytes()[..4], &[0xf3, 0xee, 0xd1, 0xbd]);
//! cipher.decrypt_block(&mut block);
//! assert_eq!(&block.as_bytes()[..4], &[0x6b, 0xc1, 0xbe, 0xe2]);
//! ```
//!
//! This is a raw block permutation, not message encryption; use
//! [`Aes256Gcm`](crate::aead::gcm::Aes256Gcm) for records.
//!
//! # Evidence and security status
//!
//! All 60 Appendix A.3 expanded words, all four `AES_Core256.pdf` ECB blocks in both directions,
//! and differential comparison with the `aes` crate. The same source-level side-channel caveats
//! as AES-128 apply: calculated S-boxes, no secret-indexed tables, no audit.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
#[cfg(test)]
mod appendix_a3;
mod key_schedule;

pub use api::{Aes256, Aes256Block, Aes256Key};

/// Current project lifecycle classification for AES-256.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
