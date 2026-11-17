//! **http1** — HTTP/1.1 message syntax for a client (RFC 9112): building requests and
//! decoding responses, with no sockets, clocks or runtimes.
//!
//! - [`Request`] is a request as bytes: build one with the guided constructors, which
//!   refuse anything that would split the message, or fill its public fields directly to
//!   send whatever you like.
//! - [`ResponseDecoder`] turns the bytes a server sends into a [`Response`], however they
//!   are split across reads. It owns every framing decision: where the head ends, whether
//!   there is a body, how long it is, and whether the connection can be used again.
//!
//! This crate is not a client. It opens no connections, follows no redirects, and keeps no
//! cookies or pool; a caller does that with these parts.
//!
//! ```
//! use http1::{Limits, Request, ResponseDecoder};
//!
//! let request = Request::get("example.test", "/status")?;
//! assert_eq!(
//!     request.encode(),
//!     b"GET /status HTTP/1.1\r\nHost: example.test\r\n\r\n"
//! );
//!
//! let mut decoder = ResponseDecoder::new(&request.method, Limits::default());
//! let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
//! assert_eq!(decoder.receive(wire)?, wire.len());
//! let response = decoder.finish()?;
//! assert_eq!(response.head.status, 200);
//! assert_eq!(response.body, b"ok");
//! # Ok::<(), http1::Error>(())
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod limits;
pub mod wire;

pub use error::Error;
pub use limits::Limits;
pub use wire::{
    BodyFraming, ChunkedDecoder, Header, Headers, Request, Response, ResponseDecoder, ResponseHead,
};
