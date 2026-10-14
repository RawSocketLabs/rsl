//! RSA integer primitives, taught from imported components to a modular exponentiation.
//!
//! # What RSA is
//!
//! RSA is an integer permutation: with public key `(n, e)` and private key `(n, d)`, a
//! representative `m` below `n` maps to `m^e mod n` and back through `c^d mod n`. RSA does not
//! itself say how message bytes become that integer; that is the job of an *encoding method*.
//! [RFC 8017] defines the byte/integer conversions (§4), the four named primitives (§5), and
//! the encoding methods. This module owns the components and the primitives only.
//!
//! # The visible layers
//!
//! 1. [`RsaPublicKey`] and [`RsaPrivateKey`] import unsigned big-endian `n`, `e`, and `d`
//!    components. Nothing is hidden behind key generation, primality testing, CRT parameters, or
//!    ASN.1.
//! 2. The private [`integer`] module implements unsigned integers and Montgomery modular
//!    multiplication with small, named operations.
//! 3. Encoding methods consume the primitives: [`RSASSA-PSS`](crate::signature::rsa_pss)
//!    verification lives in this crate; historical PKCS #1 v1.5 lives in `rsl-crypto-legacy`.
//!
//! # RFC 8017 notation in Rust
//!
//! | RFC 8017 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `(n, e)` | [`RsaPublicKey`] | Modulus and public exponent. |
//! | `(n, d)` | [`RsaPrivateKey`] | Modulus and private exponent. |
//! | `k` | [`RsaPublicKey::modulus_len`] | `ceil(modBits / 8)`, the octet length of `n`. |
//! | `OS2IP`, `I2OSP` | `integer::BigUint::from_be_bytes`, `to_be_bytes_padded` | Unsigned big-endian conversion. |
//! | RSAEP, RSAVP1 | [`RsaPublicKey::apply`] | `m^e mod n`. |
//! | RSADP, RSASP1 | [`RsaPrivateKey::apply`] | `c^d mod n`. |
//!
//! # Side-channel boundary
//!
//! The integer engine branches on exponent bits, uses data-dependent vector lengths, performs no
//! RSA blinding, and has not undergone compiler-level timing analysis. Public-key operations
//! process only public data and are unaffected. The private-key primitive is consequently
//! [`EducationalOnly`](crate::security::SecurityStatus::EducationalOnly), and this crate exposes
//! no scheme that uses it.
//!
//! # Deliberate exclusions
//!
//! Key generation, CRT acceleration, multi-prime RSA, OAEP, PKCS #1 v1.5, DER/PEM, X.509, and
//! protocol policy are separate algorithms or protocol responsibilities.
//!
//! [RFC 8017]: https://www.rfc-editor.org/rfc/rfc8017.html

#![allow(rustdoc::private_intra_doc_links)]

pub(crate) mod integer;
mod key;

pub use key::{RsaPrivateKey, RsaPublicKey};

/// Lifecycle status of the readable, variable-time private-key RSA primitive.
pub const RSA_PRIMITIVE_SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::EducationalOnly;
