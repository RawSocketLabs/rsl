//! Sans-I/O SOCKS5 handshakes: protocol state without sockets, runtimes, or deadlines.
//!
//! A driver moves bytes between its transport and the machine. [`Client::advance`] says
//! which direction is needed next; the machine owns sequencing, validation, and every
//! input byte it has accepted, so unread application bytes survive the handoff.

mod client;

// --- Internal modules ---
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub(crate) use client::MAX_FRAME_LEN;
pub use client::{Client, Step};
