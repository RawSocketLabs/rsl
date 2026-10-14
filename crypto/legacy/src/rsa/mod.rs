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
//!    components. They are re-exported from [`rsl_crypto::rsa`], which owns the integer engine
//!    and the RFC 8017 primitives so that contemporary RSASSA-PSS verification and this
//!    historical package share one exponentiation. Nothing hides key generation, primality
//!    testing, CRT parameters, or ASN.1 behind a convenient constructor.
//! 2. [`pkcs1v15`] applies the historical encryption and signature encodings before the RSA
//!    permutation.
//!
//! The split is important: PKCS #1 padding is not a transport record format, and RSA is not a TLS
//! cipher suite or an SSH key blob. Protocol packages must own those formats and their acceptance
//! policy.
//!
//! # Side-channel boundary
//!
//! The shared integer engine branches on exponent bits, uses data-dependent vector lengths,
//! performs no RSA blinding, and has not undergone compiler-level timing analysis. The
//! underlying private-key operation is consequently
//! [`EducationalOnly`](crate::SecurityStatus::EducationalOnly), even where a historical encoding
//! is classified separately. The uniform public error returned by
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

pub mod pkcs1v15;

pub use rsl_crypto::rsa::{RSA_PRIMITIVE_SECURITY_STATUS, RsaPrivateKey, RsaPublicKey};
