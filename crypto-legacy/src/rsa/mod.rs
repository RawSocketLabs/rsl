//! Historical RSA and PKCS #1 v1.5 encodings, isolated for study and interoperability.
//!
//! > **These operations are not suitable for new protection.**
//!
//! RSA is an integer permutation: an encoded message representative `m` is transformed to
//! `m^e mod n`, and a private operation applies `c^d mod n`. RSA does not itself say how arbitrary
//! bytes become that integer. [RFC 8017] defines the byte/integer conversions, RSA primitives,
//! and encoding methods reproduced here.
//!
//! # The visible layers
//!
//! 1. [`RsaPublicKey`] and [`RsaPrivateKey`] import unsigned big-endian `n`, `e`, and `d`
//!    components. This package does not hide key generation, primality testing, CRT parameters,
//!    or ASN.1 behind a convenient constructor.
//! 2. The private `integer` module implements unsigned integers and Montgomery modular
//!    multiplication with small, named operations.
//! 3. [`pkcs1v15`] applies the historical encryption and signature encodings before the RSA
//!    permutation.
//!
//! The split is important: PKCS #1 padding is not a transport record format, and RSA is not a TLS
//! cipher suite or an SSH key blob. Protocol packages must own those formats and their acceptance
//! policy.
//!
//! # Side-channel boundary
//!
//! The integer engine branches on exponent bits, uses data-dependent vector lengths, performs no
//! RSA blinding, and has not undergone compiler-level timing analysis. The underlying private-key
//! operation is consequently [`EducationalOnly`](crate::SecurityStatus::EducationalOnly), even
//! where a historical encoding is classified separately. The uniform public error returned by
//! decryption prevents callers from learning a named padding defect; it does **not** make this
//! implementation resistant to Bleichenbacher-style timing or protocol oracles.
//!
//! # Deliberate exclusions
//!
//! This slice does not implement key generation, CRT acceleration, multi-prime RSA, OAEP, PSS,
//! DER/PEM, X.509, TLS premaster-secret checks, SSH wire encodings, certificate validation, or
//! protocol fallback. Those are separate algorithms or protocol responsibilities.
//!
//! [RFC 8017]: https://www.rfc-editor.org/rfc/rfc8017.html

mod integer;
mod key;

pub mod pkcs1v15;

pub use key::{RsaPrivateKey, RsaPublicKey};

/// Lifecycle status of this package's readable, variable-time RSA primitive.
pub const RSA_PRIMITIVE_SECURITY_STATUS: crate::SecurityStatus =
    crate::SecurityStatus::EducationalOnly;
