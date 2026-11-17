//! **tls** — a from-scratch TLS client stack on the [`bnb`] bit-aware codec.
//!
//! The parts of a TLS 1.2 client exist; the connection that drives them does not yet:
//!
//! - [`wire`] — the TLS 1.2 wire types: the record header, alerts, and the handshake messages
//!   of an ECDHE exchange. Dual-use: every field is stored verbatim.
//! - [`certificate`] — the peer's chain ([`PeerChain`]) and the client's trust decision
//!   ([`ServerVerification`]). Certificates are parsed by [`x509`] and paths are validated by
//!   [`pki`]; this crate contains no ASN.1 or path-validation code of its own.
//! - [`schedule`] — the TLS 1.2 PRF, the master secret, and the `Finished` check values and
//!   record keys derived from it.
//! - [`record`] — AES-128-GCM record protection.
//!
//! Every primitive comes from [`crypto`]. The sans-I/O connection is not built yet, so the
//! crate cannot open a connection; see its `DESIGN.md` for the plan.
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
pub mod handshake;
pub mod record;
pub mod schedule;
pub mod wire;

pub use certificate::{CertificateError, PeerChain, ServerVerification};
