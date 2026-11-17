//! What a node answers to a connect request.

// --- Workspace dependencies ---
use bnb::bin;
use socks::v5::{Endpoint, ReplyCode};

/// The first two bytes of the node's answer to a connect request (spec §9.1). When the code
/// is not [`ReplyCode::Succeeded`], a node may send nothing more, so these are decoded on
/// their own and a [`ConnectReply`] is read only after a success.
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReplyHead {
    /// The protocol version, 5.
    pub version: u8,
    /// The result.
    pub code: ReplyCode,
}

/// The rest of a successful answer to a connect request, after its [`ReplyHead`]
/// (spec §9.1).
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectReply {
    /// 1 when the node agrees to end the stream with a close frame and keep the connection.
    pub flag: u8,
    /// The address the node bound, which a client reads and discards. A compliant node sends
    /// an IPv4 or IPv6 address; a name has the SOCKS5 layout, so it decodes, and refusing it
    /// is the client's job.
    #[brw(variable)]
    pub bound: Endpoint,
}

impl ConnectReply {
    /// The flag value that means the node will end the stream with a close frame.
    pub const FLAG_REUSABLE: u8 = 0x01;
}
