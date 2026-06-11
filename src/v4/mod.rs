//! The SOCKS4 / 4A wire format: every message, as a typed value.
//!
//! This is the low-level layer the [`client::v4`](crate::client::v4) and
//! [`server::v4`](crate::server::v4) types are built on, exposed so you can
//! encode and decode SOCKS4 messages directly. Reading and writing use
//! [`binrw`]; each message type carries a `read_from` constructor that consumes
//! exactly one message from a stream (no over-read) and writes via
//! [`binrw::BinWrite`].
//!
//! # Message map
//!
//! | Type | Message | Field |
//! |---|---|---|
//! | [`Request`] | client request: VN, CD, DSTPORT, DSTIP, USERID, \[DOMAIN\] | — |
//! | [`Command`] | the CD field of a request (CONNECT / BIND) | CD |
//! | [`Reply`] | server reply: VN, CD, DSTPORT, DSTIP | — |
//! | [`ReplyCode`] | the CD field of a reply (90–93) | CD |
//!
//! SOCKS4 has **no method negotiation and no authentication subnegotiation**:
//! the entire client-to-server exchange is one [`Request`] and one [`Reply`].
//! Identity, such as it is, travels in the request's `USERID` field.
//!
//! # SOCKS4 vs SOCKS4A
//!
//! SOCKS4A is the same wire format with one addition: when the client cannot
//! resolve the destination itself, it sets `DSTIP` to the inadmissible
//! `0.0.0.x` marker (`x` non-zero) and appends the destination host name after
//! `USERID`. The [`Request::domain`] field and that parsing branch are compiled
//! in only under the `v4a` feature; [`is_unresolved_marker`] is the predicate
//! that recognizes the marker.
//!
//! # Compliant by default, but liberal
//!
//! Builders default to conformant values (`version = 4` on a request, `0` on a
//! reply). Every field stays `pub` and `version` is a plain `u8`, so a caller
//! can construct a deliberately non-conformant message. Parsers preserve
//! unknown command and reply codes as `Custom(..)` rather than rejecting them.
//!
//! # Example
//!
//! ```
//! use std::net::Ipv4Addr;
//! use socks::v4::{Command, Request};
//!
//! # fn main() -> Result<(), socks::error::SocksError> {
//! // VN=4, CD=CONNECT, port 80, 93.184.216.34, userid "alice".
//! let bytes = [4u8, 1, 0, 80, 93, 184, 216, 34, b'a', b'l', b'i', b'c', b'e', 0];
//! let request = Request::read_from(&mut bytes.as_slice())?;
//!
//! assert_eq!(request.command, Command::Connect);
//! assert_eq!(request.dest_ip, Ipv4Addr::new(93, 184, 216, 34));
//! assert_eq!(request.userid.to_string(), "alice");
//! # Ok(())
//! # }
//! ```
//!
//! [`binrw`]: https://docs.rs/binrw

mod command;
mod reply;
mod reply_code;
mod request;

pub use command::Command;
pub use reply::{Reply, ReplyBuilder, ReplyBuilderError};
pub use reply_code::ReplyCode;
pub use request::{is_unresolved_marker, Request, RequestBuilder, RequestBuilderError};
