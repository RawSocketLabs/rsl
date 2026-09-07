use super::{Command, Endpoint, ReplyCode, VERSION};
use bnb::bin;

/// A client command request: `VER`, `CMD`, `RSV`, `ATYP`, destination, and port (RFC 1928 §4).
//~ models rfc1928#4 part="request"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The protocol version. The builder defaults to [`VERSION`].
    #[reserved_with(VERSION)]
    pub version: u8,
    /// The requested operation.
    pub command: Command,
    /// The reserved byte. The builder defaults to zero.
    #[reserved]
    pub reserved: u8,
    /// The destination endpoint, including its address-type byte.
    #[brw(variable)]
    pub destination: Endpoint,
}

/// A server command reply: `VER`, `REP`, `RSV`, `ATYP`, bound address, and port (RFC 1928 §6).
//~ models rfc1928#6 part="reply"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// The protocol version. The builder defaults to [`VERSION`].
    #[reserved_with(VERSION)]
    pub version: u8,
    /// The request result.
    pub code: ReplyCode,
    /// The reserved byte. The builder defaults to zero.
    #[reserved]
    pub reserved: u8,
    /// The server-bound endpoint, including its address-type byte.
    #[brw(variable)]
    pub bound: Endpoint,
}
