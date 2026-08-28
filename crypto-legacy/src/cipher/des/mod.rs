//! DES and Triple-DES as readable 64-bit block permutations.
//!
//! > **DES is broken, and Triple-DES is withdrawn. Neither protects new data.**
//!
//! DES takes one eight-byte block and an eight-byte encoded key. Every key byte contains one
//! parity bit, so only 56 key bits affect the permutation. It applies an initial bit permutation,
//! sixteen Feistel rounds, swaps the two 32-bit halves, and applies the inverse permutation. Each
//! round expands 32 bits to 48, XORs one rotated/permuted subkey, substitutes through eight
//! S-boxes, and permutes the resulting 32 bits.
//!
//! Triple-DES does not invent a new round function. It composes DES three times in
//! encrypt-decrypt-encrypt (EDE) order. [`TripleDesEde2`] uses `K1, K2, K1`; [`TripleDesEde3`]
//! uses three independently encoded keys.
//!
//! # One DES block
//!
//! ```
//! use rsl_crypto_legacy::cipher::des::{Des, DesBlock, DesKey};
//!
//! let cipher = Des::new(DesKey::new([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]));
//! let plaintext = [0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96];
//! let mut block = DesBlock::new(plaintext);
//! cipher.encrypt_block(&mut block);
//! assert_eq!(block.as_bytes(), &[0x72, 0x77, 0xa0, 0x0d, 0xc1, 0xc1, 0xc3, 0x6b]);
//! cipher.decrypt_block(&mut block);
//! assert_eq!(block.into_bytes(), plaintext);
//! ```
//!
//! A block permutation supplies no chaining, IV, padding, authentication, usage limit, or record
//! framing. The separately layered CBC primitive supplies only chaining; TLS and SSH repositories
//! must still own their exact MAC/padding/record rules and explicit legacy policy.
//!
//! # Parity, weak keys, and review limits
//!
//! [`DesKey::new`] preserves FIPS 46-3 compatibility: it accepts all 64 encoded bits and the key
//! schedule drops positions 8, 16, …, 64 as parity. It does not normalize parity and deliberately
//! accepts weak/semi-weak keys so historical bytes can be reproduced exactly. This choice would
//! be inappropriate for a modern protection API.
//!
//! The implementation uses direct bit permutations and direct S-box lookups to mirror the
//! withdrawn standards. These secret-dependent table accesses are not constant-time. Key owners
//! and expanded schedules are non-`Clone`, redacted, and zeroizing, but lifecycle hygiene does not
//! repair the algorithms' security limits.
//!
//! FIPS 46-3 and withdrawn NIST SP 800-67 Rev. 2 control the mechanics. NIST's official TDES
//! intermediate-value file supplies test evidence. Exact links, section mapping, current
//! withdrawal status, evidence, and exclusions are recorded in the package `STANDARDS.md`.

mod api;
mod constants;
mod permutation;
mod round;
mod schedule;
mod triple;

pub use api::{Des, DesBlock, DesKey};
pub use triple::{TripleDesEde2, TripleDesEde2Key, TripleDesEde3, TripleDesEde3Key};

/// DES lifecycle status: its 56 effective key bits permit practical exhaustive search.
pub const DES_SECURITY_STATUS: crate::SecurityStatus = crate::SecurityStatus::Broken;

/// Triple-DES lifecycle status: retained only to process historical data under explicit policy.
pub const TRIPLE_DES_SECURITY_STATUS: crate::SecurityStatus = crate::SecurityStatus::Legacy;
