//! **tls** — a from-scratch TLS client stack on the [`bnb`] bit-aware codec.
//!
//! Skeleton stage. Two pieces exist:
//!
//! - [`wire`] — the RFC 5246 record-layer header ([`RecordHeader`], [`ContentType`],
//!   [`ProtocolVersion`]). Dual-use: every field is stored verbatim.
//! - [`certificate`] — the peer's chain ([`PeerChain`]) and the client's trust decision
//!   ([`ServerVerification`]). Certificates are parsed by [`x509`] and paths are validated by
//!   [`pki`]; this crate contains no ASN.1 or path-validation code of its own.
//!
//! The handshake, record protection and the sans-I/O connection are not built yet; see the
//! crate's `DESIGN.md` for the plan.
//!
//! ```
//! use tls::{ContentType, ProtocolVersion, RecordHeader};
//!
//! let header = RecordHeader::new(ContentType::Handshake, ProtocolVersion::TLS_1_2, 512);
//! assert_eq!(header.to_bytes().unwrap(), [22, 3, 3, 2, 0]);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod certificate;
pub mod wire;

pub use certificate::{CertificateError, PeerChain, ServerVerification};
pub use wire::{ContentType, ProtocolVersion, RecordHeader};
