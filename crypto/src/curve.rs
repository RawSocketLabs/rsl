//! Elliptic-curve group arithmetic shared by agreement and signature schemes.
//!
//! A prime-order short Weierstrass curve such as P-256 is used by both ECDH and ECDSA. The
//! field, scalar, and point layers therefore live here, once, and the scheme modules
//! ([`crate::agreement::ecdh_p256`] and [`crate::signature::ecdsa_p256`]) own only their typed
//! keys, encodings, validation policy, and the steps their standard adds on top of the group.
//!
//! Nothing in this module is a protocol. Curve arithmetic has no notion of a key share,
//! certificate, transcript, or nonce; it computes points from integers.

pub mod p256;
