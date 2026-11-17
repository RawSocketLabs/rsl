//! The certificate-chain handshake message.

// --- Workspace dependencies ---
use bnb::{bin, u24};

// --- Internal modules ---
use super::list::{read_list24, write_list24};

/// One certificate in a [`Certificate`] message: its DER bytes behind a 24-bit length
/// (RFC 5246 §7.4.2, `ASN.1Cert`). The bytes are not parsed here; `x509` does that.
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Asn1Cert {
    /// The certificate's DER encoding, uninterpreted.
    #[brw(count_prefix = u24)]
    pub der: Vec<u8>,
}

/// The body of a `Certificate` message: the sender's chain, its own certificate first
/// (RFC 5246 §7.4.2). Convert it into a [`PeerChain`](crate::PeerChain) to use the chain.
//~ models rfc5246#7.4.2
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Certificate {
    /// The chain, as sent.
    #[br(parse_with = read_list24)]
    #[bw(write_with = write_list24)]
    pub certificate_list: Vec<Asn1Cert>,
}
