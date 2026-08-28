//! HMAC-based Extract-and-Expand Key Derivation Function (HKDF) implementations.
//!
//! [RFC 5869][rfc-5869] defines HKDF as two deliberately separable stages. Extract turns input
//! keying material into a fixed-length pseudorandom key; Expand uses that key, context-specific
//! `info`, and an explicit output length to derive output keying material. Concrete hash
//! instantiations remain in child modules so hash and HMAC sizes cannot become implicit.
//!
//! [`sha256`] is the implemented HKDF-SHA-256 instantiation. Its guide explains salt, input keying
//! material, the pseudorandom key, context information, bounded output, and domain separation.
//!
//! [rfc-5869]: https://www.rfc-editor.org/rfc/rfc5869.html

pub mod sha256;
