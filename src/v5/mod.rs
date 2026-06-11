//! The SOCKS5 wire format: every message, as a typed value.
//!
//! This is the low-level layer the [`client`](crate::client) and
//! [`server`](crate::server) are built on, exposed so you can encode and decode
//! SOCKS5 messages directly. Reading and writing is done with [`binrw`]; each
//! type carries a `read_from` constructor that consumes exactly one message
//! from a stream (no over-read), and writes via [`binrw::BinWrite`].
//!
//! # Message map
//!
//! | Type | Message | Reference |
//! |---|---|---|
//! | [`Identifier`] | client greeting: VER, NMETHODS, METHODS | RFC 1928 §3 |
//! | [`Method`] | one authentication method code | RFC 1928 §3 |
//! | [`Offer`] | server method selection: VER, METHOD | RFC 1928 §3 |
//! | [`Request`] | client command: VER, CMD, RSV, ATYP, DST | RFC 1928 §4 |
//! | [`Command`] | the CMD field (CONNECT/BIND/UDP ASSOCIATE) | RFC 1928 §4 |
//! | [`AddressType`] / [`Address`] | the ATYP field and its address | RFC 1928 §4 |
//! | [`Reply`] | server reply: VER, REP, RSV, ATYP, BND | RFC 1928 §6 |
//! | [`Response`] | the REP reply code | RFC 1928 §6 |
//! | [`UdpHeader`] | UDP datagram header: RSV, FRAG, ATYP, DST | RFC 1928 §7 |
//! | [`UserPassRequest`] / [`UserPassResponse`] | RFC 1929 credential exchange | RFC 1929 §2 |
//!
//! # Compliant by default, but liberal
//!
//! Builders default to RFC-correct values (`version = 5`, `RSV = 0`), but every
//! field stays `pub` so a caller can write a non-conformant message on purpose.
//! Parsers are correspondingly liberal: a code the RFC leaves unassigned is
//! preserved as a `Custom(..)` variant rather than rejected, so reading never
//! loses information about what was actually on the wire. Only the physically
//! unencodable (e.g. a domain too long for its one-byte length prefix) is an
//! error.
//!
//! # Example
//!
//! Parse a CONNECT request straight from its bytes:
//!
//! ```
//! use socks::v5::{Command, Request};
//!
//! # fn main() -> Result<(), socks::error::SocksError> {
//! // VER=5, CMD=CONNECT, RSV=0, ATYP=IPv4, 127.0.0.1, port 443.
//! let bytes = [5u8, 1, 0, 1, 127, 0, 0, 1, 0x01, 0xBB];
//! let request = Request::read_from(&mut bytes.as_slice())?;
//!
//! assert_eq!(request.command, Command::Connect);
//! assert_eq!(request.dest_addr.to_socket_string(request.dest_port), "127.0.0.1:443");
//! # Ok(())
//! # }
//! ```
//!
//! [`binrw`]: https://docs.rs/binrw

mod address;
mod command;
mod identifier;
mod method;
mod offer;
mod reply;
mod request;
mod response;
mod udp_header;
mod user_pass_request;
mod user_pass_response;

pub use address::*;
pub use command::*;
pub use identifier::*;
pub use method::*;
pub use offer::*;
pub use reply::*;
pub use request::*;
pub use response::*;
pub use udp_header::*;
pub use user_pass_request::*;
pub use user_pass_response::*;
