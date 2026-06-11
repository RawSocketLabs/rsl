//! Driving a TFTP server: the client side of RFC 1350.
//!
//! Two surfaces, at different levels:
//!
//! - [`Client`] — the high-level client. [`get`](Client::get) downloads a file
//!   (Read Request) and [`put`](Client::put) uploads one (Write Request),
//!   running the full lock-step state machine: a fresh transfer ID per call,
//!   block-by-block ACKs, retransmission on timeout, source-TID checks,
//!   `netascii`/`octet` modes, and — with the `options` feature — `blksize` /
//!   `timeout` / `tsize` negotiation.
//! - [`RawClient`] — the mid-level, validation-free surface for
//!   sending and receiving individual packets with no orchestration, plus
//!   [`raw::malformed`] generators for deliberately spec-violating traffic.
//!
//! # Example
//!
//! ```no_run
//! use tftp::client::Client;
//!
//! # fn main() -> Result<(), tftp::error::TftpError> {
//! let client = Client::new("203.0.113.10:69".parse()?);
//! let bytes = client.get("kernel")?;          // download
//! client.put("upload.bin", &bytes)?;          // upload it back under a new name
//! # Ok(())
//! # }
//! ```

mod client;
pub mod raw;

pub use client::Client;
pub use raw::RawClient;
