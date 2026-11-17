//! **sangfor-atrust-sig** — the primitives of the Sangfor aTrust client protocol that are
//! pure computation: the sign key and its HMAC signatures, the anti-MITM checks, encryption of
//! the login secret, the nonce generator, and the identifiers a client presents.
//!
//! Nothing here does I/O or knows about HTTP. Each item implements one rule of §3 of the
//! behaviour specification, `../SPEC-atrust.md`, and is tested against that document's
//! vectors.
//!
//! These are the vendor's constructions, reproduced for interoperability. Several rest on
//! primitives that are no longer recommended (MD5, CBC without authentication, RSA
//! PKCS #1 v1.5 encryption); none of this is a design to copy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod canonical;
mod certificate;
mod challenge;
mod error;
mod hex;
pub mod identifiers;
mod nonce;
mod secret;
mod sign_key;

pub use canonical::canonical_string;
pub use certificate::{certificate_digest, chain_matches, der_digest};
pub use challenge::challenge_passes;
pub use error::Error;
pub use nonce::NonceGenerator;
pub use secret::encrypt_secret;
pub use sign_key::SignKey;
