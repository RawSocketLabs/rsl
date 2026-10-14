//! Historical ciphers and primitive modes.
//!
//! [`rc4`] is the first implemented historical cipher. DES, Triple-DES, and narrowly scoped CBC
//! primitives will follow here. Cipher-suite names, record layouts, padding validation, MAC
//! ordering, downgrade behavior, and negotiation remain outside this package in the protocol
//! that defines them.

pub mod cbc;
pub mod des;
pub mod rc4;
