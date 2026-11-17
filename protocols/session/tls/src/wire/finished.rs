//! The message that ends each side's handshake.

// --- Workspace dependencies ---
use bnb::bin;

// --- Internal modules ---
use super::list::{read_to_end, write_bytes};

/// The body of a `Finished` message: the sender's check value over the handshake so far
/// (RFC 5246 §7.4.9).
///
/// The check value is the whole body. Its length is set by the cipher suite and is
/// [`Finished::VERIFY_DATA_LEN`] unless a suite says otherwise; any length decodes and
/// encodes, and the handshake (not the codec) rejects a wrong one.
//~ models rfc5246#7.4.9
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Finished {
    /// The check value.
    #[br(parse_with = read_to_end)]
    #[bw(write_with = write_bytes)]
    pub verify_data: Vec<u8>,
}

impl Finished {
    /// The check value's length in bytes for every suite that does not define its own.
    pub const VERIFY_DATA_LEN: usize = 12;
}
