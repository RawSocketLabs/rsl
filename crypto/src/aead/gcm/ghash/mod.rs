//! Private GHASH layers for GCM authentication.
//!
//! [NIST SP 800-38D §6.3][sp-800-38d] defines multiplication of two 128-bit blocks in
//! `GF(2^128)`. Section 6.4, Algorithm 2 composes that operation into `GHASH_H` over a sequence of
//! complete blocks. Both independently tested layers are present, but the hierarchy deliberately
//! exposes no standalone public hash API.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

mod field;
mod state;

pub(in crate::aead::gcm) use state::{Ghash, GhashResult, HashSubkey};

#[cfg(test)]
mod differential;
