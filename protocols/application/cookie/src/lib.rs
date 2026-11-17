//! **cookie** — HTTP cookies for a client (a subset of RFC 6265).
//!
//! [`Cookie::parse`] reads one `Set-Cookie` header value the way the RFC's user-agent
//! algorithm does, and a [`Jar`] keeps the cookies of **one scheme and host** and writes the
//! `Cookie` header for a request to it.
//!
//! What is left out: the `Domain` attribute and everything about which host a cookie
//! belongs to (the caller keeps one jar per scheme and host), and the `Secure` and
//! `HttpOnly` flags.
//! There is no clock: the caller passes the current time.
//!
//! ```
//! use cookie::Jar;
//!
//! let now = 1_700_000_000; // seconds since the Unix epoch
//! let mut jar = Jar::new();
//! jar.store(b"SID=31d4d96e407aad42; Path=/; Secure; HttpOnly", b"/login", now);
//! jar.store(b"lang=en-US; Path=/", b"/login", now);
//! assert_eq!(
//!     jar.header(b"/account", now).as_deref(),
//!     Some(&b"SID=31d4d96e407aad42; lang=en-US"[..])
//! );
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

mod cookie;
mod date;
mod jar;

pub use cookie::Cookie;
pub use jar::Jar;
