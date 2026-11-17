//! The server's reply to a `ClientHello`.

// --- Workspace dependencies ---
use bnb::bin;

// --- Internal modules ---
use super::{
    CipherSuite, Extension, ExtensionType, ProtocolVersion,
    list::{read_trailing_list16, write_trailing_list16},
};

/// The body of a `ServerHello` (RFC 5246 §7.4.1.3).
///
/// Dual-use: every field is stored verbatim and none is checked on decode. `extensions` is
/// `None` when the message ends after `compression_method`.
///
/// Decode it with `decode_exact` from exactly the bytes its handshake header counts. Whether
/// the extension block is present is decided by whether bytes remain, so a longer buffer or
/// a streaming source would give the wrong answer or an error.
//~ models rfc5246#7.4.1.3
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServerHello {
    /// The version the server chose.
    pub server_version: ProtocolVersion,
    /// The server's 32 random bytes.
    pub random: [u8; 32],
    /// The session id the server assigned, or empty.
    #[brw(count_prefix = u8)]
    pub session_id: Vec<u8>,
    /// The suite the server chose.
    pub cipher_suite: CipherSuite,
    /// The compression method the server chose.
    pub compression_method: u8,
    /// The extension block, when present.
    #[br(parse_with = read_trailing_list16)]
    #[bw(write_with = write_trailing_list16)]
    pub extensions: Option<Vec<Extension>>,
}

impl ServerHello {
    /// The first extension of this type, if the server sent one.
    #[must_use]
    pub fn extension(&self, extension_type: ExtensionType) -> Option<&Extension> {
        self.extensions
            .iter()
            .flatten()
            .find(|extension| extension.extension_type == extension_type)
    }
}
