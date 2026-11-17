//! The client's opening handshake message.

// --- Workspace dependencies ---
use bnb::bin;

// --- Internal modules ---
use super::{
    CipherSuite, Extension, ProtocolVersion,
    list::{read_list16, read_trailing_list16, write_list16, write_trailing_list16},
};

/// The body of a `ClientHello` (RFC 5246 §7.4.1.2).
///
/// Dual-use: every field is stored verbatim and none is checked on decode, so a caller can
/// send any session id, suite list or extension block. `extensions` is `None` when the
/// message ends after `compression_methods`, which is different on the wire from an empty
/// extension block.
///
/// Decode it with `decode_exact` from exactly the bytes its handshake header counts. Whether
/// the extension block is present is decided by whether bytes remain, so a longer buffer or
/// a streaming source would give the wrong answer or an error.
//~ models rfc5246#7.4.1.2
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClientHello {
    /// The highest version the client supports.
    pub client_version: ProtocolVersion,
    /// The client's 32 random bytes.
    pub random: [u8; 32],
    /// The session to resume, or empty.
    #[brw(count_prefix = u8)]
    pub session_id: Vec<u8>,
    /// The suites the client offers, most preferred first.
    #[br(parse_with = read_list16)]
    #[bw(write_with = write_list16)]
    pub cipher_suites: Vec<CipherSuite>,
    /// The compression methods the client offers.
    #[brw(count_prefix = u8)]
    pub compression_methods: Vec<u8>,
    /// The extension block, when present.
    #[br(parse_with = read_trailing_list16)]
    #[bw(write_with = write_trailing_list16)]
    pub extensions: Option<Vec<Extension>>,
}

impl ClientHello {
    /// The `null` compression method (RFC 5246 §7.4.1.2), the only one a compliant hello
    /// offers.
    pub const COMPRESSION_NULL: u8 = 0;

    /// A TLS 1.2 hello offering `cipher_suites` and `extensions`, with no session to resume
    /// and no compression.
    #[must_use]
    pub fn new(
        random: [u8; 32],
        cipher_suites: Vec<CipherSuite>,
        extensions: Vec<Extension>,
    ) -> Self {
        Self {
            client_version: ProtocolVersion::TLS_1_2,
            random,
            session_id: Vec::new(),
            cipher_suites,
            compression_methods: vec![Self::COMPRESSION_NULL],
            extensions: Some(extensions),
        }
    }
}
