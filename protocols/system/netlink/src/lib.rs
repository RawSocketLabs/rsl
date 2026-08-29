//! Strict Linux netlink codecs and typed route/WireGuard operations.

#![cfg(target_os = "linux")]
// Linux ABI family/table constants are small fixed integers, and the public
// codec API intentionally exposes fallible conversions without repeating an
// identical Errors section on every typed convenience method.
#![allow(
    clippy::cast_possible_truncation,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

pub mod core;
pub mod generic;
pub mod route;
pub mod transport;
pub mod wireguard;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
/// Failure from message processing, transport validation, or the kernel.
pub enum Error {
    /// Socket or readiness failure.
    #[error("netlink I/O: {0}")]
    Io(#[from] std::io::Error),
    /// A value could not be represented on the wire.
    #[error("netlink encode error: {0}")]
    Encode(String),
    /// Input did not contain a structurally valid message.
    #[error("netlink decode error: {0}")]
    Decode(String),
    /// A response violated the request/reply contract.
    #[error("netlink protocol error: {0}")]
    Protocol(String),
    /// The kernel returned `NLMSG_ERROR` with a nonzero errno.
    #[error("kernel rejected netlink request: errno {errno}{suffix}", suffix = extack.as_deref().map(|s| format!(": {s}")).unwrap_or_default())]
    Kernel {
        /// Positive Linux errno value.
        errno: i32,
        /// Optional extended acknowledgement text from the kernel.
        extack: Option<String>,
    },
    /// The running kernel does not expose a requested facility.
    #[error("netlink operation is not supported: {0}")]
    Unsupported(String),
}

/// Result type used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
