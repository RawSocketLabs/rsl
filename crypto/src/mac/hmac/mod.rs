//! Hash-based message authentication code (HMAC) implementations.
//!
//! HMAC combines a secret key with an iterative digest. The generic construction is specified by
//! [NIST FIPS 198-1][fips-198-1] and [RFC 2104][rfc-2104]. Concrete hash instantiations remain in
//! separate child modules so their block size, output size, key representation, and published
//! evidence stay explicit.
//!
//! [`sha256`] is the implemented HMAC-SHA-256 instantiation. Its guide contrasts a digest with a
//! MAC and maps key normalization and the inner/outer hash construction to readable layers.
//!
//! [fips-198-1]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.198-1.pdf
//! [rfc-2104]: https://www.rfc-editor.org/rfc/rfc2104.html

pub mod sha256;
