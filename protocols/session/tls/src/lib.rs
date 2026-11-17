//! **tls** — a from-scratch TLS client stack on the [`bnb`] bit-aware codec.
//!
//! A sans-I/O TLS 1.2 [`Client`] and the parts it is built from. There is no socket adapter
//! yet: a caller moves the bytes itself.
//!
//! - [`client`] — the client state machine: one cipher suite
//!   (`TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`), X25519, RSA-PSS signatures, and
//!   `extended_master_secret` required.
//! - [`wire`] — the TLS 1.2 wire types: the record header, alerts, and the handshake messages
//!   of an ECDHE exchange. Dual-use: every field is stored verbatim.
//! - [`certificate`] — the peer's chain ([`PeerChain`]) and the client's trust decision
//!   ([`ServerVerification`]). Certificates are parsed by [`x509`] and paths are validated by
//!   [`pki`]; this crate contains no ASN.1 or path-validation code of its own.
//! - [`schedule`] — the TLS 1.2 PRF, the master secret, and the `Finished` check values and
//!   record keys derived from it.
//! - [`record`] — AES-128-GCM record protection.
//!
//! - [`handshake`] — joining handshake messages out of records, and the transcript hash.
//!
//! Every primitive comes from [`crypto`], which describes itself as an unaudited early
//! implementation; so is this crate. See its `DESIGN.md` for decisions, gaps and risks.
//!
//! ```
//! use tls::wire::{ContentType, ProtocolVersion, RecordHeader};
//!
//! let header = RecordHeader::new(ContentType::Handshake, ProtocolVersion::TLS_1_2, 512);
//! assert_eq!(header.to_bytes().unwrap(), [22, 3, 3, 2, 0]);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod certificate;
pub mod client;
pub mod handshake;
pub mod record;
pub mod schedule;
pub mod wire;

pub use certificate::{CertificateError, PeerChain, ServerVerification};
pub use client::{Client, ClientConfig, Error};
